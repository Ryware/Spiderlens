//! The crawl loop (research R7, R8, R11).
//!
//! One coordinator task owns the crawl file and every scheduling decision;
//! fetches run as separate tasks and report back over a channel. This is the
//! actor pattern (one owner of mutable state, messages in), which keeps the
//! store single-threaded without locks.

use std::collections::{BTreeMap, HashMap};
use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use reqwest::Client;
use serde::Serialize;
use spiderlens_config::{
    CrawlConfig, LinkProblem, Mode, ModeInput, RobotsMode, StoredCrawlConfig, normalize_url,
};
use tokio::sync::{mpsc, watch};
use url::Url;

use super::links::{self, PageLinks};
use super::reason::{DiscoverySource, LinkKind, NotCrawledReason, UrlState};
use super::scope::{Candidate, Nofollow, ScopeRules, Verdict, problem_verdict};
use super::sitemap::{self, MAX_INDEX_DEPTH, Sitemap};
use super::store::{
    Counts, CrawlStore, Discovery, HostRow, NewLink, Placement, QueueEntry, Recorded, Response, SitemapRow,
    StoreError,
};
use super::window::QueueWindow;
use crate::fetch::{build_client_with, fetch_once};
use crate::robots::{self, RobotsFetch, RobotsPolicy, RobotsTxt, RobotsVerdict};
use crate::{CheckError, CheckOptions, RequestHeader, Title};

const WINDOW: usize = 4096;
const PROGRESS_EVERY: Duration = Duration::from_millis(250);
const LONG_CRAWL_DELAY: Duration = Duration::from_secs(60);

#[derive(Debug, thiserror::Error)]
pub enum CrawlError {
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error("{0}")]
    Config(String),
    #[error("could not start the HTTP client: {0}")]
    Client(CheckError),
}

/// What the user asked the crawl to do (FR-022).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Control {
    Run,
    /// Finish in-flight requests, keep the queue, save as paused.
    Pause,
    /// Finish in-flight requests, record the queue as "Crawl stopped".
    Stop,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Progress {
    pub discovered: u64,
    pub crawled: u64,
    pub in_flight: u32,
    /// Waiting to be requested (in flight not included).
    pub queued: u64,
    /// Found but not crawled, for any reason (external checks included).
    pub not_crawled: u64,
    /// Crawled pages that failed or answered 4xx/5xx.
    pub errors: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CrawlEvent {
    Progress(Progress),
    Warning(String),
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct HostSummary {
    pub host: String,
    pub robots: String,
    pub crawl_delay_secs: Option<f64>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SitemapSummary {
    pub url: String,
    pub urls: i64,
    pub error: Option<String>,
}

/// The crawl summary (contracts/crawl-summary.schema.json).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CrawlSummary {
    pub status: String,
    pub discovered: u64,
    pub crawled: u64,
    pub not_crawled: u64,
    pub queued: u64,
    pub by_reason: BTreeMap<NotCrawledReason, u64>,
    pub external_checked: u64,
    pub url_limit_reached: bool,
    pub hosts: Vec<HostSummary>,
    pub sitemaps: Vec<SitemapSummary>,
    pub warnings: Vec<String>,
}

impl CrawlSummary {
    pub fn read(path: &Path) -> Result<Self, CrawlError> {
        summarize(&CrawlStore::open(path)?)
    }
}

fn summarize(store: &CrawlStore) -> Result<CrawlSummary, CrawlError> {
    let Counts {
        by_state,
        by_reason,
        external_checked,
    } = store.counts()?;
    let count = |s: UrlState| by_state.iter().find(|(x, _)| *x == s).map_or(0, |(_, n)| *n);
    let hosts = store
        .hosts()?
        .into_iter()
        .map(|h| HostSummary {
            host: h.host_key,
            robots: h.robots_status,
            crawl_delay_secs: h.crawl_delay_ms.map(|ms| ms as f64 / 1000.0),
            error: h.robots_error,
        })
        .collect();
    let sitemaps = store
        .sitemaps()?
        .into_iter()
        .map(|s| SitemapSummary {
            url: s.url,
            urls: s.url_count,
            error: s.error,
        })
        .collect();
    let warnings = store
        .meta("warnings")?
        .and_then(|w| serde_json::from_str(&w).ok())
        .unwrap_or_default();
    Ok(CrawlSummary {
        status: store.meta("status")?.unwrap_or_else(|| "running".into()),
        discovered: by_state.iter().map(|(_, n)| n).sum(),
        crawled: count(UrlState::Crawled),
        not_crawled: count(UrlState::NotCrawled),
        queued: count(UrlState::Queued) + count(UrlState::InFlight),
        by_reason: by_reason.into_iter().collect(),
        external_checked,
        url_limit_reached: store.meta("url_limit_reached")?.as_deref() == Some("true"),
        hosts,
        sitemaps,
        warnings,
    })
}

/// `scheme://host:port`: the unit for robots.txt and politeness (FR-023).
/// The crawl file name used when none is given: `<host>-<UTC time>.spiderlens`,
/// or `list-<UTC time>.spiderlens` in List mode (contracts/cli.md).
pub fn default_file_name(input: &ModeInput) -> String {
    let stamp = jiff::Timestamp::now().strftime("%Y%m%dT%H%M%SZ").to_string();
    let name = match input {
        ModeInput::Spider { start_url } => normalize_url(start_url)
            .ok()
            .and_then(|u| u.host_str().map(str::to_string))
            .unwrap_or_else(|| "crawl".into()),
        ModeInput::List(_) => "list".into(),
    };
    format!("{name}-{stamp}.spiderlens")
}

pub fn host_key(url: &Url) -> String {
    format!(
        "{}://{}:{}",
        url.scheme(),
        url.host_str().unwrap_or(""),
        url.port_or_known_default().unwrap_or(0)
    )
}

#[derive(Debug, Clone)]
enum RobotsState {
    Unknown,
    Fetching,
    Known(Arc<RobotsPolicy>, Option<String>),
}

#[derive(Debug)]
struct Host {
    robots: RobotsState,
    in_flight: u32,
    next_start: Instant,
    delay: Option<Duration>,
}

impl Host {
    fn new() -> Self {
        Self {
            robots: RobotsState::Unknown,
            in_flight: 0,
            next_start: Instant::now(),
            delay: None,
        }
    }
}

/// What a fetch task reports.
enum Msg {
    Page {
        entry: QueueEntry,
        internal: bool,
        robots_rule: Option<String>,
        result: Result<PageOutcome, CheckError>,
    },
    Robots {
        host_key: String,
        fetch: RobotsFetch,
    },
    SitemapUrls {
        sitemap: Url,
        urls: Vec<String>,
    },
    SitemapDone(SitemapRow),
    SitemapsFinished,
}

struct PageOutcome {
    status: u16,
    content_type: Option<String>,
    location: Option<String>,
    page: Option<PageLinks>,
}

/// A crawl bound to its crawl file.
pub struct Crawler {
    store: CrawlStore,
    stored: StoredCrawlConfig,
    config: CrawlConfig,
    rules: ScopeRules,
    start: Option<Url>,
    resolve: Vec<(String, SocketAddr)>,
    retry_base: Duration,
}

impl Crawler {
    /// Creates the crawl file and records the seeds. The configuration must
    /// already be validated (spec 002); invalid patterns are refused here too.
    pub fn create(path: &Path, config: CrawlConfig, input: &ModeInput) -> Result<Self, CrawlError> {
        let seeds: Vec<Url> = match input {
            ModeInput::Spider { start_url } => {
                vec![normalize_url(start_url).map_err(|e| CrawlError::Config(format!("start URL: {e}")))?]
            }
            ModeInput::List(list) => list.urls.iter().filter_map(|u| normalize_url(u).ok()).collect(),
        };
        if seeds.is_empty() {
            return Err(CrawlError::Config("there are no URLs to crawl".into()));
        }
        let rules = ScopeRules::new(&config, &seeds).map_err(CrawlError::Config)?;
        let mut store = CrawlStore::create(path)?;

        // List URLs live in the urls table; storing them again in the
        // configuration would double a large list (data-model.md).
        let stored_input = match input {
            ModeInput::List(list) => {
                let mut l = list.clone();
                l.urls.clear();
                ModeInput::List(l)
            }
            other => other.clone(),
        };
        let stored = StoredCrawlConfig::new(
            config.clone(),
            stored_input,
            rules.internal_hosts().to_vec(),
            jiff::Timestamp::now(),
        );
        store.set_meta(
            "config",
            &serde_json::to_string(&stored).expect("config serializes"),
        )?;
        let start = (config.mode == Mode::Spider).then(|| rules.normalize_seed(&seeds[0]));
        if let Some(s) = &start {
            store.set_meta("start_url", s.as_str())?;
        }

        store.begin()?;
        let source = match config.mode {
            Mode::Spider => DiscoverySource::Start,
            Mode::List => DiscoverySource::List,
        };
        for seed in &seeds {
            let url = rules.normalize_seed(seed);
            let verdict = rules.classify(
                &url,
                &Candidate {
                    depth: 0,
                    seed: true,
                    nofollow: None,
                },
            );
            let placement = placement_for(verdict, false);
            store.record(&Discovery {
                url: url.as_str(),
                host_key: &host_key(&url),
                internal: true,
                depth: 0,
                source,
                source_id: None,
                in_sitemap: false,
                placement,
            })?;
        }
        store.commit()?;
        Ok(Self {
            store,
            stored,
            config,
            rules,
            start,
            resolve: Vec::new(),
            retry_base: Duration::from_secs(1),
        })
    }

    /// Opens a crawl file to resume it with its stored configuration.
    pub fn open(path: &Path) -> Result<Self, CrawlError> {
        let store = CrawlStore::open(path)?;
        let stored: StoredCrawlConfig = store
            .meta("config")?
            .and_then(|c| serde_json::from_str(&c).ok())
            .ok_or_else(|| CrawlError::Config("the crawl file has no readable configuration".into()))?;
        let config = stored.effective();
        let start = store.meta("start_url")?.and_then(|u| Url::parse(&u).ok());
        let seeds: Vec<Url> = match &start {
            Some(s) => vec![s.clone()],
            None => store
                .list_urls()?
                .iter()
                .filter_map(|u| Url::parse(u).ok())
                .collect(),
        };
        let mut rules = ScopeRules::new(&config, &seeds).map_err(CrawlError::Config)?;
        rules.restore_hosts(&stored.resolved_hosts);
        Ok(Self {
            store,
            stored,
            config,
            rules,
            start,
            resolve: Vec::new(),
            retry_base: Duration::from_secs(1),
        })
    }

    /// Pins host names to addresses (tests serve `.test` hosts locally).
    pub fn with_resolve(mut self, resolve: Vec<(String, SocketAddr)>) -> Self {
        self.resolve = resolve;
        self
    }

    /// First wait between robots.txt attempts; doubles each time.
    pub fn with_retry_base(mut self, base: Duration) -> Self {
        self.retry_base = base;
        self
    }

    pub fn config(&self) -> &CrawlConfig {
        &self.config
    }

    /// Runs until the queue is empty or `control` asks to pause or stop.
    pub async fn run(
        self,
        control: watch::Receiver<Control>,
        events: Option<mpsc::UnboundedSender<CrawlEvent>>,
    ) -> Result<CrawlSummary, CrawlError> {
        let opts = CheckOptions::from(&self.config);
        let client = build_client_with(&opts, &self.resolve).map_err(CrawlError::Client)?;
        let mut run = Run::new(self, opts, client, control, events)?;
        run.main_loop().await?;
        summarize(&run.c.store)
    }
}

fn placement_for(verdict: Verdict, limit_reached: bool) -> Placement {
    match verdict {
        Verdict::Crawl { .. } if limit_reached => Placement::NotCrawled {
            reason: NotCrawledReason::OverUrlLimit,
            detail: None,
        },
        Verdict::Crawl { via_nofollow } => Placement::Queue {
            check_only: false,
            via_nofollow,
        },
        Verdict::External { check: true } => Placement::Queue {
            check_only: true,
            via_nofollow: false,
        },
        Verdict::External { check: false } => Placement::NotCrawled {
            reason: NotCrawledReason::External,
            detail: None,
        },
        Verdict::NotCrawled { reason, detail } => Placement::NotCrawled { reason, detail },
    }
}

fn now_ms() -> i64 {
    jiff::Timestamp::now().as_millisecond()
}

/// State of one running crawl.
struct Run {
    c: Crawler,
    opts: CheckOptions,
    client: Client,
    headers: Arc<Vec<RequestHeader>>,
    control: watch::Receiver<Control>,
    control_closed: bool,
    events: Option<mpsc::UnboundedSender<CrawlEvent>>,
    tx: mpsc::UnboundedSender<Msg>,
    rx: mpsc::UnboundedReceiver<Msg>,
    window: QueueWindow,
    hosts: HashMap<String, Host>,
    in_flight: u32,
    next_global: Instant,
    pages_started: u64,
    limit_reached: bool,
    sitemaps_pending: bool,
    discovered: u64,
    crawled: u64,
    errors: u64,
    last_progress: Instant,
    warnings: Vec<String>,
}

enum Action {
    Start(QueueEntry, Option<String>),
    Block(QueueEntry, NotCrawledReason, String),
}

impl Run {
    fn new(
        c: Crawler,
        opts: CheckOptions,
        client: Client,
        control: watch::Receiver<Control>,
        events: Option<mpsc::UnboundedSender<CrawlEvent>>,
    ) -> Result<Self, CrawlError> {
        let (tx, rx) = mpsc::unbounded_channel();
        c.store.requeue_for_resume()?;
        c.store.set_meta("status", "running")?;
        let warnings: Vec<String> = c
            .store
            .meta("warnings")?
            .and_then(|w| serde_json::from_str(&w).ok())
            .unwrap_or_default();
        let mut hosts = HashMap::new();
        for h in c.store.hosts()? {
            let policy = match h.robots_status.as_str() {
                "ok" => RobotsTxt::parse(h.robots_txt.as_deref().unwrap_or("").as_bytes())
                    .for_agent(&opts.robots_token),
                "unavailable" => RobotsPolicy::disallow_all(),
                _ => RobotsPolicy::allow_all(),
            };
            let mut host = Host::new();
            host.delay = honoured_delay(&c.config, policy.crawl_delay);
            host.robots = RobotsState::Known(Arc::new(policy), h.robots_error);
            hosts.insert(h.host_key, host);
        }
        let pages_started = c.store.pages_started()?;
        let limit_reached = c.store.meta("url_limit_reached")?.as_deref() == Some("true");
        let by_state = c.store.counts()?.by_state;
        let discovered = by_state.iter().map(|(_, n)| n).sum();
        let crawled = by_state
            .iter()
            .find(|(s, _)| *s == UrlState::Crawled)
            .map_or(0, |(_, n)| *n);
        let errors = c.store.page_errors()?;
        Ok(Self {
            headers: Arc::new(opts.headers.clone()),
            c,
            opts,
            client,
            control,
            control_closed: false,
            events,
            tx,
            rx,
            window: QueueWindow::new(WINDOW),
            hosts,
            in_flight: 0,
            next_global: Instant::now(),
            pages_started,
            limit_reached,
            sitemaps_pending: false,
            discovered,
            crawled,
            errors,
            last_progress: Instant::now() - PROGRESS_EVERY,
            warnings,
        })
    }

    fn emit(&self, e: CrawlEvent) {
        if let Some(tx) = &self.events {
            let _ = tx.send(e);
        }
    }

    fn warn(&mut self, message: String) -> Result<(), CrawlError> {
        if self.warnings.contains(&message) {
            return Ok(());
        }
        self.emit(CrawlEvent::Warning(message.clone()));
        self.warnings.push(message);
        self.c.store.set_meta(
            "warnings",
            &serde_json::to_string(&self.warnings).expect("strings serialize"),
        )?;
        Ok(())
    }

    fn progress(&mut self, force: bool) {
        if force || self.last_progress.elapsed() >= PROGRESS_EVERY {
            self.last_progress = Instant::now();
            if self.events.is_none() {
                return;
            }
            // One indexed GROUP BY, a few times a second: cheaper and less
            // fragile than keeping a counter at every place a state changes.
            // A failed read only costs this one update.
            let (mut queued, mut not_crawled) = (0, 0);
            if let Ok(counts) = self.c.store.state_counts() {
                for (state, n) in counts {
                    match state {
                        UrlState::Queued => queued = n,
                        UrlState::NotCrawled => not_crawled = n,
                        UrlState::InFlight | UrlState::Crawled => {}
                    }
                }
            }
            self.emit(CrawlEvent::Progress(Progress {
                discovered: self.discovered,
                crawled: self.crawled,
                in_flight: self.in_flight,
                queued,
                not_crawled,
                errors: self.errors,
            }));
        }
    }

    fn state(&self) -> Control {
        *self.control.borrow()
    }

    async fn main_loop(&mut self) -> Result<(), CrawlError> {
        self.maybe_start_sitemaps()?;
        loop {
            let wake = if self.state() == Control::Run {
                self.dispatch()?
            } else {
                None
            };
            self.progress(false);
            if self.in_flight == 0 {
                match self.state() {
                    Control::Pause => {
                        self.c.store.set_meta("status", "paused")?;
                        break;
                    }
                    Control::Stop => {
                        self.c.store.stop_queue()?;
                        self.c.store.set_meta("status", "stopped")?;
                        break;
                    }
                    Control::Run => {
                        self.window.refill(&self.c.store)?;
                        if self.window.is_empty() && self.window.holds_everything() && !self.sitemaps_pending
                        {
                            self.c.store.set_meta("status", "completed")?;
                            break;
                        }
                    }
                }
            }
            let sleep = async {
                match wake {
                    Some(t) => tokio::time::sleep_until(t.into()).await,
                    None => std::future::pending().await,
                }
            };
            tokio::select! {
                Some(msg) = self.rx.recv() => self.handle(msg)?,
                () = sleep => {}
                r = self.control.changed(), if !self.control_closed => {
                    if r.is_err() {
                        self.control_closed = true;
                    }
                }
            }
        }
        self.progress(true);
        Ok(())
    }

    /// Starts every request that may start now, in queue order (FR-020,
    /// FR-021). Returns when to look again if something is waiting on time.
    fn dispatch(&mut self) -> Result<Option<Instant>, CrawlError> {
        let mut wake: Option<Instant> = None;
        let soonest = |t: Instant, wake: &mut Option<Instant>| {
            *wake = Some(wake.map_or(t, |w| w.min(t)));
        };
        loop {
            if self.in_flight >= self.c.config.speed.max_concurrent {
                break;
            }
            self.window.refill(&self.c.store)?;
            let now = Instant::now();
            let per_host = self.c.config.speed.max_per_host;
            let robots_mode = self.c.config.robots.mode;
            let mut need_robots = Vec::new();
            let mut action = None;
            let mut rate_blocked = false;
            for e in self.window.iter() {
                let host = self.hosts.entry(e.host_key.clone()).or_insert_with(Host::new);
                let (policy, robots_error) = match &host.robots {
                    RobotsState::Unknown => {
                        if robots_mode == RobotsMode::Ignore {
                            host.robots = RobotsState::Known(Arc::new(RobotsPolicy::allow_all()), None);
                            need_robots.push((e.host_key.clone(), false));
                        } else if self.in_flight + (need_robots.len() as u32)
                            < self.c.config.speed.max_concurrent
                        {
                            host.robots = RobotsState::Fetching;
                            need_robots.push((e.host_key.clone(), true));
                        }
                        continue;
                    }
                    RobotsState::Fetching => continue,
                    RobotsState::Known(p, err) => (p.clone(), err.clone()),
                };
                let url = Url::parse(&e.url).expect("stored URLs parse");
                let mut robots_rule = None;
                if let RobotsVerdict::Disallowed(rule) = policy.check(&url) {
                    if robots_mode == RobotsMode::Respect {
                        let (reason, detail) = if policy.is_disallow_all() {
                            (NotCrawledReason::RobotsUnavailable, robots_error.unwrap_or(rule))
                        } else {
                            (NotCrawledReason::BlockedByRobots, rule)
                        };
                        action = Some(Action::Block(e.clone(), reason, detail));
                        break;
                    }
                    robots_rule = Some(rule);
                }
                let limit = if host.delay.is_some() { 1 } else { per_host };
                if host.in_flight >= limit {
                    continue;
                }
                if host.next_start > now {
                    soonest(host.next_start, &mut wake);
                    continue;
                }
                if self.next_global > now {
                    soonest(self.next_global, &mut wake);
                    rate_blocked = true;
                    break;
                }
                action = Some(Action::Start(e.clone(), robots_rule));
                break;
            }
            for (key, fetch) in need_robots {
                if fetch {
                    self.spawn_robots(key);
                } else {
                    self.c.store.upsert_host(&HostRow {
                        host_key: key,
                        robots_status: "ignored".into(),
                        robots_http_status: None,
                        robots_error: None,
                        crawl_delay_ms: None,
                        robots_txt: None,
                    })?;
                }
            }
            match action {
                Some(Action::Block(e, reason, detail)) => {
                    self.window.remove(e.key());
                    if e.check_only {
                        self.c.store.drop_check(e.id)?;
                    } else {
                        self.c.store.set_not_crawled(e.id, reason, Some(&detail))?;
                        if e.source == DiscoverySource::Start {
                            self.warn(format!(
                                "The start URL {} is blocked by robots.txt ({detail}), so nothing else can be found.",
                                e.url
                            ))?;
                        }
                    }
                }
                Some(Action::Start(e, rule)) => self.start(e, rule, now)?,
                None => break,
            }
            if rate_blocked {
                break;
            }
        }
        Ok(wake)
    }

    fn start(&mut self, e: QueueEntry, robots_rule: Option<String>, now: Instant) -> Result<(), CrawlError> {
        self.window.remove(e.key());
        self.c.store.mark_in_flight(e.id)?;
        let host = self.hosts.get_mut(&e.host_key).expect("host known before start");
        host.in_flight += 1;
        if let Some(d) = host.delay {
            host.next_start = now + d;
        }
        if let Some(rate) = self.c.config.speed.max_urls_per_sec {
            self.next_global = now + Duration::from_secs_f64(1.0 / rate);
        }
        self.in_flight += 1;

        let url = Url::parse(&e.url).expect("stored URLs parse");
        let internal = !e.check_only && self.c.rules.is_internal(&url);
        let client = self.client.clone();
        let headers = self.headers.clone();
        let max_body = self.opts.max_body_bytes;
        let token = self.opts.robots_token.clone();
        let tx = self.tx.clone();
        let check_only = e.check_only;
        tokio::spawn(async move {
            let result = fetch_once(&client, &url, &headers, internal, !check_only, max_body)
                .await
                .map(|o| PageOutcome {
                    page: o.body.as_deref().map(|b| {
                        links::extract(
                            &url,
                            o.charset.as_deref(),
                            b,
                            &token,
                            &o.x_robots_tag,
                            &o.link_header,
                        )
                    }),
                    status: o.status,
                    content_type: o.content_type,
                    location: o.location,
                });
            let _ = tx.send(Msg::Page {
                entry: e,
                internal,
                robots_rule,
                result,
            });
        });

        if !check_only {
            self.pages_started += 1;
            if let Some(max) = self.c.config.limits.max_urls
                && self.pages_started >= max
                && !self.limit_reached
            {
                self.apply_url_limit()?;
            }
        }
        Ok(())
    }

    /// FR-019: no new page requests; every queued page gets "Over URL limit",
    /// unless robots.txt (earlier in the FR-038 order) already blocks it.
    fn apply_url_limit(&mut self) -> Result<(), CrawlError> {
        self.limit_reached = true;
        let store = &self.c.store;
        store.begin()?;
        store.set_meta("url_limit_reached", "true")?;
        let mut after = 0;
        loop {
            let batch = store.queued_pages_after(after, 1000)?;
            let Some(last) = batch.last() else { break };
            after = last.id;
            for e in &batch {
                let robots = match self.hosts.get(&e.host_key).map(|h| &h.robots) {
                    Some(RobotsState::Known(p, _)) if self.c.config.robots.mode == RobotsMode::Respect => {
                        match p.check(&Url::parse(&e.url).expect("stored URLs parse")) {
                            RobotsVerdict::Disallowed(rule) => Some(rule),
                            RobotsVerdict::Allowed => None,
                        }
                    }
                    _ => None,
                };
                match robots {
                    Some(rule) => {
                        store.set_not_crawled(e.id, NotCrawledReason::BlockedByRobots, Some(&rule))?
                    }
                    None => store.set_not_crawled(e.id, NotCrawledReason::OverUrlLimit, None)?,
                }
            }
        }
        store.commit()?;
        self.window.retain(|e| e.check_only);
        Ok(())
    }

    fn spawn_robots(&mut self, key: String) {
        self.in_flight += 1;
        let client = self.client.clone();
        let tx = self.tx.clone();
        let retry = self.c.config.retry;
        let base = self.c.retry_base;
        let origin = Url::parse(&format!("{key}/")).expect("host keys are URLs");
        tokio::spawn(async move {
            let mut attempt = 1;
            let fetch = loop {
                let f = robots::fetch(&client, &origin).await;
                let retryable = match &f {
                    RobotsFetch::ServerError { status: 429 } => retry.retry_429,
                    RobotsFetch::ServerError { .. } => retry.retry_5xx,
                    RobotsFetch::Unreachable(_) => true,
                    _ => false,
                };
                if !retryable || attempt >= retry.attempts {
                    break f;
                }
                let wait = base.mul_f64(2f64.powi(attempt as i32 - 1));
                tokio::time::sleep(wait.min(Duration::from_secs_f64(retry.max_wait_secs))).await;
                attempt += 1;
            };
            let _ = tx.send(Msg::Robots { host_key: key, fetch });
        });
    }

    fn maybe_start_sitemaps(&mut self) -> Result<(), CrawlError> {
        if !self.c.config.scope.use_sitemaps || self.c.store.meta("sitemaps_read")?.as_deref() == Some("true")
        {
            return Ok(());
        }
        let Some(start) = &self.c.start else { return Ok(()) };
        let key = host_key(start);
        let Some(Host {
            robots: RobotsState::Known(policy, _),
            ..
        }) = self.hosts.get(&key)
        else {
            return Ok(());
        };
        let roots: Vec<Url> = policy
            .sitemaps
            .iter()
            .filter_map(|s| Url::parse(s).ok())
            .collect();
        self.c.store.set_meta("sitemaps_read", "true")?;
        if roots.is_empty() {
            return Ok(());
        }
        self.sitemaps_pending = true;
        let client = self.client.clone();
        let tx = self.tx.clone();
        tokio::spawn(async move {
            let mut todo: Vec<(Url, Option<String>, u32)> =
                roots.into_iter().rev().map(|u| (u, None, 0)).collect();
            let mut seen = std::collections::HashSet::new();
            while let Some((url, parent, level)) = todo.pop() {
                if !seen.insert(url.clone()) {
                    continue;
                }
                let mut row = SitemapRow {
                    url: url.to_string(),
                    parent,
                    url_count: 0,
                    error: None,
                };
                match sitemap::fetch(&client, &url)
                    .await
                    .and_then(|b| sitemap::parse(&b))
                {
                    Ok(Sitemap::Urls(urls)) => {
                        row.url_count = urls.len() as i64;
                        let _ = tx.send(Msg::SitemapUrls {
                            sitemap: url.clone(),
                            urls,
                        });
                    }
                    Ok(Sitemap::Index(children)) => {
                        row.url_count = children.len() as i64;
                        if level + 1 >= MAX_INDEX_DEPTH {
                            row.error = Some(format!(
                                "sitemap index nested deeper than {MAX_INDEX_DEPTH} levels"
                            ));
                        } else {
                            for child in children.iter().rev() {
                                match url.join(child) {
                                    Ok(c) => todo.push((c, Some(url.to_string()), level + 1)),
                                    Err(_) => row.error = Some(format!("invalid sitemap address {child}")),
                                }
                            }
                        }
                    }
                    Err(e) => row.error = Some(e),
                }
                let _ = tx.send(Msg::SitemapDone(row));
            }
            let _ = tx.send(Msg::SitemapsFinished);
        });
        Ok(())
    }

    fn handle(&mut self, msg: Msg) -> Result<(), CrawlError> {
        match msg {
            Msg::Robots { host_key, fetch } => self.on_robots(host_key, fetch),
            Msg::Page {
                entry,
                internal,
                robots_rule,
                result,
            } => {
                self.in_flight -= 1;
                if let Some(h) = self.hosts.get_mut(&entry.host_key) {
                    h.in_flight -= 1;
                }
                self.c.store.begin()?;
                self.on_page(entry, internal, robots_rule, result)?;
                self.c.store.commit()?;
                Ok(())
            }
            Msg::SitemapUrls { sitemap, urls } => {
                self.c.store.begin()?;
                for loc in urls {
                    let parsed = self.c.rules.normalize(&sitemap, &loc);
                    self.discover(None, &loc, LinkKind::Anchor, parsed, 1, None, false, true)?;
                }
                self.c.store.commit()?;
                Ok(())
            }
            Msg::SitemapDone(row) => {
                if let Some(e) = &row.error {
                    self.warn(format!("Sitemap {} could not be used: {e}", row.url))?;
                }
                Ok(self.c.store.upsert_sitemap(&row)?)
            }
            Msg::SitemapsFinished => {
                self.sitemaps_pending = false;
                Ok(())
            }
        }
    }

    fn on_robots(&mut self, key: String, fetch: RobotsFetch) -> Result<(), CrawlError> {
        self.in_flight -= 1;
        let policy = fetch.policy(&self.opts.robots_token);
        let (status, http, error, text) = match &fetch {
            RobotsFetch::Found {
                status,
                text,
                warning,
            } => {
                if let Some(w) = warning {
                    self.warn(format!("{key}: {w}"))?;
                }
                let s = if warning.is_some() { "not_found" } else { "ok" };
                (s, Some(*status), None, Some(text.clone()))
            }
            RobotsFetch::NotFound { status } => ("not_found", Some(*status), None, None),
            RobotsFetch::ServerError { status } => (
                "unavailable",
                Some(*status),
                Some(format!("robots.txt answered {status}")),
                None,
            ),
            RobotsFetch::Unreachable(e) => (
                "unavailable",
                None,
                Some(format!("robots.txt could not be fetched: {}", e.message)),
                None,
            ),
        };
        let delay = honoured_delay(&self.c.config, policy.crawl_delay);
        if let Some(d) = policy.crawl_delay
            && d >= LONG_CRAWL_DELAY
        {
            let per_hour = 3600.0 / d.as_secs_f64();
            let note = if delay.is_some() {
                format!(
                    "{key} asks for a crawl-delay of {} s, so it is crawled at most {per_hour:.0} URLs per hour.",
                    d.as_secs()
                )
            } else {
                format!(
                    "{key} asks for a crawl-delay of {} s; it is ignored by your settings.",
                    d.as_secs()
                )
            };
            self.warn(note)?;
        }
        self.c.store.upsert_host(&HostRow {
            host_key: key.clone(),
            robots_status: status.into(),
            robots_http_status: http,
            robots_error: error.clone(),
            crawl_delay_ms: policy.crawl_delay.map(|d| d.as_millis() as i64),
            robots_txt: text,
        })?;
        let host = self.hosts.entry(key).or_insert_with(Host::new);
        host.delay = delay;
        host.robots = RobotsState::Known(Arc::new(policy), error);
        self.maybe_start_sitemaps()
    }

    fn on_page(
        &mut self,
        entry: QueueEntry,
        internal: bool,
        robots_rule: Option<String>,
        result: Result<PageOutcome, CheckError>,
    ) -> Result<(), CrawlError> {
        let mut response = Response {
            robots_rule,
            ..Response::default()
        };
        if !entry.check_only {
            self.crawled += 1;
        }
        match result {
            Err(e) => {
                response.error_class = Some(e.class.as_str().into());
                response.error_message = Some(e.message);
            }
            Ok(o) => {
                response.status = Some(o.status);
                response.content_type = o.content_type;
                let url = Url::parse(&entry.url).expect("stored URLs parse");
                if let Some(loc) = o.location.as_deref()
                    && !entry.check_only
                {
                    let target = self.c.rules.normalize(&url, loc);
                    let mut seed = false;
                    if entry.source == DiscoverySource::Start
                        && let Ok(t) = &target
                        && t.host_str() != url.host_str()
                        && self.c.rules.adopt_start_redirect(t)
                    {
                        seed = true;
                        self.c.stored.resolved_hosts = self.c.rules.internal_hosts().to_vec();
                        self.c.store.set_meta(
                            "config",
                            &serde_json::to_string(&self.c.stored).expect("config serializes"),
                        )?;
                    }
                    if entry.source == DiscoverySource::Start
                        && let Ok(t) = &target
                        && !seed
                        && !self.c.rules.is_internal(t)
                    {
                        self.warn(format!(
                            "The start URL redirects to another site ({t}). That site is not crawled; \
                             start a new crawl from it if you meant to crawl it."
                        ))?;
                    }
                    response.redirect_to = self.discover(
                        Some(entry.id),
                        loc,
                        LinkKind::Redirect,
                        target,
                        entry.depth + 1,
                        None,
                        seed,
                        false,
                    )?;
                }
                if let Some(page) = o.page {
                    if let Title::Text(t) = &page.title {
                        response.title = Some(t.clone());
                    }
                    if internal && !entry.check_only {
                        for link in &page.links {
                            let nofollow = match link.kind {
                                LinkKind::Anchor if link.nofollow => Some(Nofollow::Link),
                                LinkKind::Anchor if page.page_nofollow => Some(Nofollow::Page),
                                _ => None,
                            };
                            let parsed = self.c.rules.normalize(&page.base, &link.raw);
                            let to = self.discover(
                                Some(entry.id),
                                &link.raw,
                                link.kind,
                                parsed,
                                entry.depth + 1,
                                nofollow,
                                false,
                                false,
                            )?;
                            let _ = (to, page.page_nofollow);
                        }
                    }
                }
            }
        }
        if !entry.check_only && (response.error_class.is_some() || response.status.is_some_and(|s| s >= 400))
        {
            self.errors += 1;
        }
        self.c
            .store
            .record_response(entry.id, entry.check_only, &response, now_ms())?;
        Ok(())
    }

    /// Records one discovered reference and its link row. Returns the target
    /// row id, or None for a self reference.
    #[allow(clippy::too_many_arguments)]
    fn discover(
        &mut self,
        from: Option<i64>,
        raw: &str,
        kind: LinkKind,
        parsed: Result<Url, LinkProblem>,
        depth: u32,
        nofollow: Option<Nofollow>,
        seed: bool,
        in_sitemap: bool,
    ) -> Result<Option<i64>, CrawlError> {
        let source = if in_sitemap {
            DiscoverySource::Sitemap
        } else {
            kind.source()
        };
        let (url, host, internal, placement) = match parsed {
            Ok(url) => {
                let verdict = self.c.rules.classify(
                    &url,
                    &Candidate {
                        depth,
                        seed,
                        nofollow,
                    },
                );
                let internal = seed || self.c.rules.is_internal(&url);
                let host = host_key(&url);
                let placement = placement_for(verdict, self.limit_reached);
                (url.to_string(), host, internal, placement)
            }
            Err(problem) => {
                let Some((stored, Verdict::NotCrawled { reason, detail })) = problem_verdict(&problem) else {
                    return Ok(None);
                };
                (
                    stored,
                    String::new(),
                    false,
                    Placement::NotCrawled { reason, detail },
                )
            }
        };
        let followed = matches!(
            placement,
            Placement::Queue {
                check_only: false,
                ..
            }
        );
        let recorded = self.c.store.record(&Discovery {
            url: &url,
            host_key: &host,
            internal,
            depth,
            source,
            source_id: from,
            in_sitemap,
            placement,
        })?;
        match &recorded {
            Recorded::New { queued, .. } => {
                self.discovered += 1;
                if let Some(e) = queued {
                    self.window.on_queued(e.clone());
                }
            }
            Recorded::Requeued { entry, old_key, .. } => {
                if let Some(k) = old_key {
                    self.window.remove(*k);
                }
                self.window.on_queued(entry.clone());
            }
            Recorded::Seen { .. } => {}
        }
        let id = recorded.id();
        if let Some(from) = from {
            let link_nofollow = nofollow == Some(Nofollow::Link);
            let page_nofollow = nofollow == Some(Nofollow::Page);
            self.c.store.add_link(&NewLink {
                from,
                to: id,
                raw,
                kind,
                nofollow: link_nofollow,
                page_nofollow,
                followed,
            })?;
        }
        Ok(Some(id))
    }
}

/// The crawl-delay that applies, unless the user chose to ignore it (FR-027,
/// FR-028).
fn honoured_delay(config: &CrawlConfig, delay: Option<Duration>) -> Option<Duration> {
    delay.filter(|d| config.robots.honour_crawl_delay && !d.is_zero())
}
