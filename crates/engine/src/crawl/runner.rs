//! The crawl loop (research R7, R8, R11).
//!
//! One coordinator task owns the crawl file and every scheduling decision;
//! fetches run as separate tasks and report back over a channel. This is the
//! actor pattern (one owner of mutable state, messages in), which keeps the
//! store single-threaded without locks.

use std::collections::{BTreeMap, HashMap, VecDeque};
use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use reqwest::Client;
use serde::Serialize;
use spiderlens_config::{
    CrawlConfig, LinkProblem, Mode, ModeInput, RobotsMode, SpeedSettings, StoredCrawlConfig, normalize_url,
};
use tokio::sync::{mpsc, watch};
use url::Url;

use super::links::{self, PageLinks};
use super::reason::{DiscoverySource, LinkKind, NotCrawledReason, UrlState};
use super::scope::{Candidate, Nofollow, ScopeRules, Verdict, problem_verdict};
use super::sitemap::{self, MAX_INDEX_DEPTH, Sitemap};
use super::speed::{Signal, SiteSpeed, parse_retry_after};
use super::store::{
    Counts, CrawlStore, Discovery, HostRow, NewLink, Placement, QueueEntry, QueueKey, Recorded, Response,
    SitemapRow, StateCounts, StoreError,
};
use super::window::QueueWindow;
use crate::fetch::{build_client_with, fetch_once};
use crate::robots::{self, RobotsFetch, RobotsPolicy, RobotsTxt, RobotsVerdict};
use crate::{CheckError, CheckOptions, RequestHeader, Title};

const WINDOW: usize = 4096;
const PROGRESS_EVERY: Duration = Duration::from_millis(250);
/// The "current" speed is averaged over this long (spec 008 assumption).
const RATE_WINDOW: Duration = Duration::from_secs(5);
const LONG_CRAWL_DELAY: Duration = Duration::from_secs(60);
/// Results are written in one transaction committed this often, so a crash
/// loses at most this much work (spec 008 plan D2, FR-024).
const COMMIT_EVERY: Duration = Duration::from_millis(200);
const COMMIT_WRITES: u32 = 500;

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

#[derive(Debug, Clone, PartialEq, Serialize)]
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
    /// Requests finished per second over the last few seconds (spec 008
    /// FR-027), external checks included.
    pub urls_per_sec: f64,
    /// Requests finished per second since this run started.
    pub avg_urls_per_sec: f64,
    /// Seconds until the URLs queued now are done at the current speed. In
    /// Spider mode more URLs are usually found on the way, so this is a
    /// floor that rises while discovery outpaces crawling. None while the
    /// speed is still unknown.
    pub eta_secs: Option<u64>,
}

#[derive(Debug, Clone, PartialEq)]
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
    /// External checks skipped because robots.txt does not allow them.
    pub external_blocked: u64,
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
        external_blocked,
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
        external_blocked,
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
    /// No request before this: crawl-delay spacing or a Retry-After pause.
    next_start: Instant,
    delay: Option<Duration>,
    speed: SiteSpeed,
}

impl Host {
    fn new(speed: &SpeedSettings) -> Self {
        Self {
            robots: RobotsState::Unknown,
            in_flight: 0,
            next_start: Instant::now(),
            delay: None,
            speed: SiteSpeed::new(speed.max_per_host, speed.adaptive),
        }
    }

    /// Requests this host may have open now.
    fn limit(&self) -> u32 {
        if self.delay.is_some() {
            1
        } else {
            self.speed.allowance()
        }
    }
}

/// What a fetch task reports.
// Page is nearly every message, so boxing it would only add an allocation.
#[allow(clippy::large_enum_variant)]
enum Msg {
    Page {
        entry: QueueEntry,
        internal: bool,
        robots_rule: Option<String>,
        /// From sending the request to the end of the body.
        took: Duration,
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
    retry_after: Option<String>,
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
    /// An open write transaction: when it began and how many results it holds.
    write_tx: Option<(Instant, u32)>,
    /// Requests finished in this run, pages and checks alike.
    finished: u64,
    started_at: Instant,
    /// (time, finished) samples over the last `RATE_WINDOW`, oldest first.
    rate_samples: VecDeque<(Instant, u64)>,
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
            let mut host = Host::new(&c.config.speed);
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
            write_tx: None,
            finished: 0,
            started_at: Instant::now(),
            rate_samples: VecDeque::new(),
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
            // Kept by the store as rows change state: counting the table here
            // grows with the crawl and stalls the loop on large crawls.
            let StateCounts {
                queued, not_crawled, ..
            } = self.c.store.live_counts();
            let (urls_per_sec, avg_urls_per_sec) = self.rates();
            // Under a URL limit, only the pages it still allows are left.
            let left = match self.c.config.limits.max_urls {
                Some(max) if !self.limit_reached => queued.min(max.saturating_sub(self.pages_started)),
                Some(_) => 0,
                None => queued,
            };
            let eta_secs = (urls_per_sec > 0.0).then(|| (left as f64 / urls_per_sec).ceil() as u64);
            self.emit(CrawlEvent::Progress(Progress {
                discovered: self.discovered,
                crawled: self.crawled,
                in_flight: self.in_flight,
                queued,
                not_crawled,
                errors: self.errors,
                urls_per_sec,
                avg_urls_per_sec,
                eta_secs,
            }));
        }
    }

    /// Current (last few seconds) and average requests finished per second.
    fn rates(&mut self) -> (f64, f64) {
        let now = Instant::now();
        self.rate_samples.push_back((now, self.finished));
        while self
            .rate_samples
            .get(1)
            .is_some_and(|(t, _)| now.duration_since(*t) >= RATE_WINDOW)
        {
            self.rate_samples.pop_front();
        }
        let per_sec = |n: u64, d: Duration| {
            let secs = d.as_secs_f64();
            if secs >= 0.5 { n as f64 / secs } else { 0.0 }
        };
        let (t0, n0) = self.rate_samples[0];
        (
            per_sec(self.finished - n0, now.duration_since(t0)),
            per_sec(self.finished, now.duration_since(self.started_at)),
        )
    }

    fn state(&self) -> Control {
        *self.control.borrow()
    }

    /// Every write in the loop goes into one transaction (plan D2).
    fn open_tx(&mut self) -> Result<(), CrawlError> {
        if self.write_tx.is_none() {
            self.c.store.begin()?;
            self.write_tx = Some((Instant::now(), 0));
        }
        Ok(())
    }

    fn commit(&mut self) -> Result<(), CrawlError> {
        if self.write_tx.take().is_some() {
            self.c.store.commit()?;
        }
        Ok(())
    }

    /// Commits when the transaction is old or large enough.
    fn commit_if_due(&mut self) -> Result<(), CrawlError> {
        if let Some((since, writes)) = self.write_tx
            && (since.elapsed() >= COMMIT_EVERY || writes >= COMMIT_WRITES)
        {
            self.commit()?;
        }
        Ok(())
    }

    async fn main_loop(&mut self) -> Result<(), CrawlError> {
        self.maybe_start_sitemaps()?;
        loop {
            self.open_tx()?;
            let mut wake = if self.state() == Control::Run {
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
            if let Some((since, _)) = self.write_tx {
                let due = since + COMMIT_EVERY;
                wake = Some(wake.map_or(due, |w| w.min(due)));
            }
            let sleep = async {
                match wake {
                    Some(t) => tokio::time::sleep_until(t.into()).await,
                    None => std::future::pending().await,
                }
            };
            tokio::select! {
                Some(msg) = self.rx.recv() => {
                    self.handle(msg)?;
                    if let Some((_, writes)) = &mut self.write_tx {
                        *writes += 1;
                    }
                }
                () = sleep => {}
                r = self.control.changed(), if !self.control_closed => {
                    if r.is_err() {
                        self.control_closed = true;
                    }
                }
            }
            self.commit_if_due()?;
        }
        self.commit()?;
        #[cfg(debug_assertions)]
        {
            let mut counted = StateCounts::default();
            for (state, n) in self.c.store.state_counts()? {
                match state {
                    UrlState::Queued => counted.queued = n,
                    UrlState::InFlight => counted.in_flight = n,
                    UrlState::Crawled => counted.crawled = n,
                    UrlState::NotCrawled => counted.not_crawled = n,
                }
            }
            debug_assert_eq!(self.c.store.live_counts(), counted, "live counts match the file");
        }
        self.progress(true);
        Ok(())
    }

    /// Starts every request that may start now, in queue order (FR-020,
    /// FR-021). Returns when to look again if something is waiting on time.
    ///
    /// Walks hosts, not URLs (spec 008 plan D1): a host at its limit, waiting
    /// for robots.txt or inside a delay is skipped in one step, and among the
    /// hosts that can start, the one whose next URL comes first in the queue
    /// goes next. So breadth-first order holds and one busy host never hides
    /// the others.
    fn dispatch(&mut self) -> Result<Option<Instant>, CrawlError> {
        let mut wake: Option<Instant> = None;
        let soonest = |t: Instant, wake: &mut Option<Instant>| {
            *wake = Some(wake.map_or(t, |w| w.min(t)));
        };
        let speed = self.c.config.speed;
        let max_concurrent = speed.max_concurrent;
        let robots_mode = self.c.config.robots.mode;
        loop {
            if self.in_flight >= max_concurrent {
                break;
            }
            self.window.refill(&self.c.store)?;
            let now = Instant::now();

            // Hosts seen for the first time: fetch robots.txt (or skip it).
            let mut need_robots = Vec::new();
            for (key, _) in self.window.host_heads() {
                let unknown = self
                    .hosts
                    .get(key)
                    .is_none_or(|h| matches!(h.robots, RobotsState::Unknown));
                if unknown {
                    need_robots.push(key.to_string());
                }
            }
            for key in need_robots {
                if robots_mode == RobotsMode::Ignore {
                    let host = self.hosts.entry(key.clone()).or_insert_with(|| Host::new(&speed));
                    host.robots = RobotsState::Known(Arc::new(RobotsPolicy::allow_all()), None);
                    self.c.store.upsert_host(&HostRow {
                        host_key: key,
                        robots_status: "ignored".into(),
                        robots_http_status: None,
                        robots_error: None,
                        crawl_delay_ms: None,
                        robots_txt: None,
                    })?;
                } else if self.in_flight < max_concurrent {
                    let host = self.hosts.entry(key.clone()).or_insert_with(|| Host::new(&speed));
                    host.robots = RobotsState::Fetching;
                    self.spawn_robots(key);
                }
            }

            // The earliest queued URL among hosts that may start now.
            let mut best: Option<QueueKey> = None;
            for (key, head) in self.window.host_heads() {
                let Some(host) = self.hosts.get(key) else { continue };
                if !matches!(host.robots, RobotsState::Known(..)) {
                    continue;
                }
                if host.in_flight >= host.limit() {
                    continue;
                }
                if host.next_start > now {
                    soonest(host.next_start, &mut wake);
                    continue;
                }
                if best.is_none_or(|b| head < b) {
                    best = Some(head);
                }
            }
            let Some(best) = best else { break };
            let e = self.window.get(best).expect("head is in the window").clone();
            let RobotsState::Known(policy, robots_error) = &self.hosts[&e.host_key].robots else {
                unreachable!("only hosts with known robots.txt are picked");
            };
            let url = Url::parse(&e.url).expect("stored URLs parse");
            let mut robots_rule = None;
            if let RobotsVerdict::Disallowed(rule) = policy.check(&url) {
                if robots_mode == RobotsMode::Respect {
                    let (reason, detail) = if policy.is_disallow_all() {
                        (
                            NotCrawledReason::RobotsUnavailable,
                            robots_error.clone().unwrap_or(rule),
                        )
                    } else {
                        (NotCrawledReason::BlockedByRobots, rule)
                    };
                    self.block(e, reason, detail)?;
                    continue;
                }
                robots_rule = Some(rule);
            }
            if self.next_global > now {
                soonest(self.next_global, &mut wake);
                break;
            }
            self.start(e, robots_rule, now)?;
        }
        Ok(wake)
    }

    fn block(&mut self, e: QueueEntry, reason: NotCrawledReason, detail: String) -> Result<(), CrawlError> {
        self.window.remove(e.key());
        if e.check_only {
            let rule = (reason == NotCrawledReason::BlockedByRobots).then_some(detail.as_str());
            self.c.store.drop_check(e.id, reason, rule)?;
        } else {
            self.c.store.set_not_crawled(e.id, reason, Some(&detail))?;
            if e.source == DiscoverySource::Start {
                self.warn(format!(
                    "The start URL {} is blocked by robots.txt ({detail}), so nothing else can be found.",
                    e.url
                ))?;
            }
        }
        Ok(())
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
            let sent = Instant::now();
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
                    retry_after: o.retry_after,
                });
            let took = sent.elapsed();
            let _ = tx.send(Msg::Page {
                entry: e,
                internal,
                robots_rule,
                took,
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
        // Runs inside the loop's open transaction.
        let store = &self.c.store;
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
                took,
                result,
            } => {
                self.in_flight -= 1;
                self.finished += 1;
                if let Some(h) = self.hosts.get_mut(&entry.host_key) {
                    h.in_flight -= 1;
                    let signal = signal_of(&result);
                    let now = Instant::now();
                    if let Some(until) = h.speed.on_response(took, signal, now) {
                        h.next_start = h.next_start.max(until);
                    }
                }
                self.on_page(entry, internal, robots_rule, result)
            }
            Msg::SitemapUrls { sitemap, urls } => {
                for loc in urls {
                    let parsed = self.c.rules.normalize(&sitemap, &loc);
                    self.discover(None, &loc, LinkKind::Anchor, parsed, 1, None, false, true)?;
                }
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
            RobotsFetch::NotFound {
                status: s @ (401 | 403),
            } => ("forbidden", Some(*s), None, None),
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
        let speed = self.c.config.speed;
        let host = self.hosts.entry(key).or_insert_with(|| Host::new(&speed));
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
                // External checks record their redirect target too; it is
                // queued as another check, so the chain ends where it lands.
                if let Some(loc) = o.location.as_deref() {
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

/// What a finished request says about its site's load (spec 008 FR-013,
/// FR-014).
fn signal_of(result: &Result<PageOutcome, CheckError>) -> Signal {
    match result {
        Ok(o) if o.status == 429 || o.status == 503 => Signal::Overloaded {
            retry_after: o
                .retry_after
                .as_deref()
                .and_then(|v| parse_retry_after(v, jiff::Timestamp::now())),
        },
        Ok(_) => Signal::Answered,
        Err(e) if e.class == crate::ErrorClass::Timeout => Signal::TimedOut,
        Err(_) => Signal::Other,
    }
}

/// The crawl-delay that applies, unless the user chose to ignore it (FR-027,
/// FR-028).
fn honoured_delay(config: &CrawlConfig, delay: Option<Duration>) -> Option<Duration> {
    delay.filter(|d| config.robots.honour_crawl_delay && !d.is_zero())
}
