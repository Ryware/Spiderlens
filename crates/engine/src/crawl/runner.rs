//! The crawl loop (research R7, R8, R11).
//!
//! One coordinator task owns the crawl file and every scheduling decision;
//! fetches run as separate tasks and report back over a channel. This is the
//! actor pattern (one owner of mutable state, messages in), which keeps the
//! store single-threaded without locks.

use std::cmp::Reverse;
use std::collections::{BTreeMap, BinaryHeap, HashMap, HashSet, VecDeque};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use reqwest::Client;
use serde::Serialize;
use spiderlens_config::{
    A11yLevel, Change, CrawlConfig, HeaderValue, LinkProblem, Mode, ModeInput, RobotsMode, StoredCrawlConfig,
    normalize_url,
};
use tokio::sync::{Semaphore, mpsc, oneshot, watch};
use url::Url;

use super::file::{self, LocalError, LockError, WriterLock};
use super::health::Transition;
use super::hosts::{Host, RobotsState};
use super::pace::{self, CrawlView, LoadMeter, SiteEvents, SitePace, SiteView, SpeedLimit};
use super::reason::{DiscoverySource, LinkKind, NotCrawledReason, UrlState};
use super::render::a11y_probe::{AXE_VERSION, FullOutcome, MobileOutcome};
use super::render::cache::{DEFAULT_CAPACITY, ResourceCache};
use super::render::net::{NetMsg, NetShared, Note, Permits, Slots as HostSlots};
use super::render::progress::{QUEUE_HIGH, QUEUE_LOW, RenderProgress};
use super::render::stage::{Eligibility, MAX_ATTEMPTS, RenderStage, Rendered};
use super::retry::{self, Attempt, Decision};
use super::scope::{Candidate, Nofollow, ScopeRules, Verdict, problem_verdict};
use super::site_files::{self, SiteFileRow};
use super::sitemap::{self, MAX_INDEX_DEPTH, Root};
use super::speed::{Cut, Signal, parse_retry_after};
use super::store::custom::{EncodedCustom, Slots, encode_custom};
use super::store::{
    ConsoleRow, Counts, CrawlStore, Discovery, HostRow, NewLink, PageRecord, Placement, QueueEntry, QueueKey,
    Recorded, RenderRecord, RenderStatus, RenderedPageRecord, ResourceRow, Response, SitemapEntryRecord,
    SitemapFileRecord, SitemapRow, StateCounts, StoreError,
};
use super::window::QueueWindow;
use crate::custom::CompiledRules;
use crate::extract::compare::Origin;

/// A crawl's compiled rules and the stored slot of each.
struct CustomRun {
    rules: CompiledRules,
    slots: Slots,
}
use crate::extract::directives::page_flags;
use crate::extract::{
    self, ContentRules, Directive, HeaderFacts, PageData, PageHook, PageInput, Ref, RefSource, body,
};
use crate::fetch::{Once, fetch_once};
use crate::net::{AuthNote, LoginSession, NetSession};
use crate::pdf::{self, PdfFacts};
use crate::robots::{self, RobotsFetch, RobotsPolicy, RobotsTxt, RobotsVerdict};
use crate::title::is_html;
use crate::{CheckError, CheckOptions, ErrorClass, ErrorReason, FinalClass, RequestHeader};

const WINDOW: usize = 4096;
const PROGRESS_EVERY: Duration = Duration::from_millis(250);
/// The "current" speed is averaged over this long (spec 008 assumption).
const RATE_WINDOW: Duration = Duration::from_secs(5);
/// How lately the URLs-per-second cap must have held a URL back for the
/// progress to name it as the limit.
const RATE_HELD_RECENT: Duration = Duration::from_secs(2);
const LONG_CRAWL_DELAY: Duration = Duration::from_secs(60);
/// Results are written in one transaction committed this often, so a crash
/// loses at most this much work (spec 008 plan D2, FR-024).
const COMMIT_EVERY: Duration = Duration::from_millis(200);
const COMMIT_WRITES: u32 = 500;
/// How often the crawl lets the write-ahead log start over.
const CATCH_UP_EVERY: Duration = Duration::from_millis(250);
/// At most this much HTML (in KiB) is being extracted at once, which bounds
/// the memory parsed documents take (spec 005 plan D3).
const EXTRACT_BUDGET_KIB: usize = 64 * 1024;
/// The queue window may grow to this many rows when every host in it is
/// paused or waiting (spec 004 plan D11).
const MAX_WINDOW: usize = 64 * 1024;
/// Network failures in a row, across at least two hosts and with no answer
/// in between, that mean the local network is down (spec 004 plan D7).
const NETWORK_DOWN_FAILURES: u32 = 10;
/// Sites listed in the per-site breakdown (spec 008 FR-029).
const SITES_SHOWN: usize = 50;
/// How often free space on the crawl file's drive is checked (spec 006 FR-027).
const DISK_CHECK_EVERY: Duration = Duration::from_secs(2);

#[derive(Debug, thiserror::Error)]
pub enum CrawlError {
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error("{0}")]
    Config(String),
    #[error("could not start the HTTP client: {0}")]
    Client(CheckError),
    #[error(transparent)]
    Lock(#[from] LockError),
    /// The file is an older format: resuming upgrades it (spec 006 FR-036).
    #[error(
        "this crawl file uses crawl format {from}; resuming upgrades it to format {}, after which older SpiderLens versions cannot open it",
        super::store::FORMAT_VERSION
    )]
    NeedsUpgrade { from: i64 },
    /// Header values that are never stored must be entered again (FR-034).
    #[error("enter these secret values to resume: {}", .0.join(", "))]
    NeedsSecrets(Vec<String>),
    /// Read-only file or media: save a copy elsewhere to resume (FR-021).
    #[error("{} cannot be changed here (read-only); save a copy to another folder to resume it", .0.display())]
    ReadOnly(PathBuf),
    /// A disk or write problem paused the crawl; it can be resumed (FR-028).
    #[error("paused: {0}")]
    Local(LocalError),
    /// Rendering is on and no browser could be started (spec 014 FR-045).
    #[error("no usable browser for JavaScript rendering: {0}")]
    Browser(String),
}

/// What to do with an older crawl file on resume (FR-036).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Upgrade {
    KeepCopy,
    NoCopy,
}

/// Options for [`Crawler::open_with`].
#[derive(Debug, Clone, Default)]
pub struct ResumeOptions {
    /// None refuses older files with [`CrawlError::NeedsUpgrade`].
    pub upgrade: Option<Upgrade>,
    /// Speed and URL-limit changes, recorded in the configuration history.
    pub changes: Vec<Change>,
    /// Values for secrets that were not stored, keyed as in
    /// `CrawlConfig::missing_secrets` (a header name, or `cookie:domain/name`).
    pub secrets: Vec<(String, String)>,
}

/// What [`Crawler::open_with`] did to the file before the run.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Opened {
    /// URLs requeued because the crawl was not closed cleanly.
    pub recovered: Option<usize>,
    /// The format it was upgraded from, and the backup copy if kept.
    pub upgraded: Option<(i64, Option<PathBuf>)>,
}

/// Requests a running crawl handles between batches.
#[derive(Debug)]
pub enum RunCommand {
    /// Move the crawl to `dest` and continue there (spec 006 FR-015).
    SaveAs {
        dest: PathBuf,
        reply: oneshot::Sender<Result<PathBuf, String>>,
    },
    /// End a host's pause now and send it a test request (spec 004 US4.5).
    ResumeHost(String),
    /// Apply changed speed or retry settings at once; recorded in the
    /// crawl's stored configuration (spec 002 FR-040, spec 004 plan D13).
    Apply(Change),
    /// Sign in to an origin that asked for it and fetch its 401 URLs again
    /// (spec 025 FR-004). Replies with how many URLs were queued again.
    AddSignIn {
        sign_in: spiderlens_config::SignIn,
        reply: oneshot::Sender<u64>,
    },
}

/// A host that is paused right now (spec 004 FR-028).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PausedHost {
    pub host: String,
    /// Why, in words.
    pub reason: String,
    /// Seconds until the test request; 0 while it is out.
    pub seconds_left: u64,
    pub probing: bool,
    pub given_up: bool,
}

/// Something worth telling the user while the crawl runs (spec 004 FR-035).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Notice {
    pub kind: String,
    pub host: Option<String>,
    pub message: String,
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
    /// Time spent crawling, pauses excluded, across every run of this crawl
    /// file (spec 007 FR-004).
    pub active_ms: u64,
    /// URLs waiting to be retried (spec 004).
    pub waiting: u64,
    /// Hosts paused by the circuit breaker (spec 004 FR-028).
    pub paused_hosts: Vec<PausedHost>,
    /// Nothing can be requested because every remaining URL's host is
    /// paused (US4.4).
    pub waiting_for_host: bool,
    /// What keeps the crawl from going faster, if anything (spec 008
    /// FR-028).
    pub limit: Option<SpeedLimit>,
    /// The sites with work in progress, busiest first, at most
    /// `SITES_SHOWN` of them (FR-029).
    pub sites: Vec<SitePace>,
    /// How many sites have work in progress in all.
    pub sites_total: u32,
    /// Pages whose accessibility was checked so far (spec 026).
    pub a11y_checked: u64,
    /// Average time of a page's full accessibility checks, in milliseconds.
    pub a11y_avg_ms: Option<u32>,
    /// Rendering progress (spec 014 D13); absent for crawls that do not
    /// render, so their progress payload is unchanged.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub render: Option<RenderProgress>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum CrawlEvent {
    Progress(Progress),
    Warning(String),
    /// A host paused, resumed or given up, or a local problem (spec 004).
    Notice(Notice),
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
    /// Crawled URLs that failed, by error class (spec 007 FR-005).
    pub failed_by_class: BTreeMap<String, u64>,
    /// Time spent crawling, pauses excluded (spec 007 FR-004).
    pub active_ms: u64,
    /// Requested URLs by final class, HTTP classes included (spec 004
    /// FR-036). Successful answers are not listed.
    pub by_class: BTreeMap<String, u64>,
    pub transient_errors: u64,
    pub permanent_errors: u64,
    /// Automatic retries made.
    pub retries: u64,
    /// URLs that needed more than one attempt and ended without an error.
    pub recovered: u64,
    /// Hosts the circuit breaker paused at least once.
    pub hosts_paused: u64,
    /// "Retry failed URLs" runs.
    pub manual_retries: u64,
    /// Why the crawl paused itself (`network`), if it did.
    pub pause_reason: Option<String>,
    /// Sites that slowed down, asked to wait or paused, one entry per run
    /// (spec 008 FR-021).
    pub speed_events: Vec<SpeedNote>,
    /// Rendering completeness (spec 014 FR-041); absent for raw crawls, so
    /// their summary is unchanged.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub render: Option<RenderSummary>,
}

/// The browser that rendered a crawl (`meta.renderer`, FR-003).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, serde::Deserialize)]
pub struct RendererInfo {
    pub name: String,
    pub version: String,
    /// `downloaded`, `installed` or `user_path`.
    pub source: String,
}

/// The summary's Rendering block (spec 014 data-model.md). The final
/// states (`rendered`, `timed_out`, `failed`, `not_rendered`) add up to
/// the pages eligible for rendering.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RenderSummary {
    pub by_state: BTreeMap<String, u64>,
    /// Final reasons of failed and not rendered pages.
    pub by_reason: BTreeMap<String, u64>,
    pub avg_render_ms: Option<u64>,
    pub max_render_ms: Option<u64>,
    pub js_errors: u64,
    pub failed_resources: u64,
    pub blocked_resources: u64,
    pub renderer: Option<RendererInfo>,
}

impl RenderSummary {
    /// The window's and the CLI's words for a render failure or skip reason
    /// (`renders.reason`), in the order the summary lists them.
    pub const REASONS: [(&'static str, &'static str); 8] = [
        ("crash", "Browser crash"),
        ("renderer_error", "Renderer error"),
        ("page_too_large", "Page too large"),
        ("navigation_error", "Navigation error"),
        ("outside_rules", "Outside rendering rules"),
        ("crawl_stopped", "Crawl stopped"),
        ("browser_unavailable", "Browser unavailable"),
        ("html_not_stored", "Raw HTML not stored"),
    ];

    /// The reasons with a count, labelled, in listing order.
    pub fn labelled_reasons(&self) -> Vec<(&'static str, &'static str, u64)> {
        Self::REASONS
            .iter()
            .filter_map(|(key, label)| {
                let n = self.by_reason.get(*key).copied().unwrap_or(0);
                (n > 0).then_some((*key, *label, n))
            })
            .collect()
    }
}

/// What slowed one site down during a run (spec 008 FR-021).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SpeedNote {
    pub host: String,
    pub summary: String,
}

impl CrawlSummary {
    pub fn read(path: &Path) -> Result<Self, CrawlError> {
        summarize(&CrawlStore::open_read_only(path)?)
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
    let mut by_class = BTreeMap::new();
    let (mut transient_errors, mut permanent_errors) = (0, 0);
    for (status, class, reason, n) in store.outcomes()? {
        let class = class.as_deref().and_then(ErrorClass::parse);
        let f = FinalClass::of(status, class);
        if !f.is_error() {
            continue;
        }
        *by_class.entry(f.as_str().to_string()).or_insert(0) += n;
        if f.is_transient(status, reason.as_deref().and_then(ErrorReason::parse)) {
            transient_errors += n;
        } else {
            permanent_errors += n;
        }
    }
    Ok(CrawlSummary {
        status: store.meta("status")?.unwrap_or_else(|| "running".into()),
        discovered: by_state.iter().map(|(_, n)| n).sum(),
        crawled: count(UrlState::Crawled),
        not_crawled: count(UrlState::NotCrawled),
        queued: count(UrlState::Queued) + count(UrlState::InFlight) + count(UrlState::Waiting),
        by_reason: by_reason.into_iter().collect(),
        external_checked,
        external_blocked,
        url_limit_reached: store.meta("url_limit_reached")?.as_deref() == Some("true"),
        hosts,
        sitemaps,
        warnings,
        failed_by_class: store.error_classes()?.into_iter().collect(),
        active_ms: store.meta("active_ms")?.and_then(|v| v.parse().ok()).unwrap_or(0),
        by_class,
        transient_errors,
        permanent_errors,
        retries: store.log_count("retry", false)?,
        recovered: store.recovered()?,
        hosts_paused: store.log_count("host_paused", true)?,
        manual_retries: store
            .meta("manual_retries")?
            .and_then(|v| v.parse().ok())
            .unwrap_or(0),
        pause_reason: store.meta("pause_reason")?.filter(|r| !r.is_empty()),
        render: match store.render_totals()? {
            Some(t) => Some(RenderSummary {
                by_state: t
                    .by_state
                    .into_iter()
                    .map(|(s, n)| (s.as_str().to_string(), n))
                    .collect(),
                by_reason: t.by_reason.into_iter().collect(),
                avg_render_ms: t.avg_render_ms,
                max_render_ms: t.max_render_ms,
                js_errors: t.js_errors,
                failed_resources: t.failed_resources,
                blocked_resources: t.blocked_resources,
                renderer: store
                    .meta("renderer")?
                    .and_then(|r| serde_json::from_str(&r).ok()),
            }),
            None => None,
        },
        speed_events: store
            .speed_summaries()?
            .into_iter()
            .map(|(host, summary)| SpeedNote { host, summary })
            .collect(),
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

/// A cut, a requested wait, the allowance after them, and the site's typical
/// and usual response times.
type SpeedEvent = (Option<Cut>, Option<Duration>, u32, (Option<f64>, Option<f64>));

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
    /// One sitemap file read, with every entry up to the limit (spec 011).
    SitemapFile(Box<SitemapRead>),
    SitemapsFinished,
    /// A rendered page's request for a resource (spec 014 plan D4).
    Net(NetMsg),
    /// A render task ended (spec 014 plan D2).
    Rendered(Box<Rendered>),
    /// A PDF read after its response was reported (spec 027 part C).
    Pdf {
        id: i64,
        url: String,
        depth: u32,
        internal: bool,
        read: pdf::pool::Read,
    },
    SiteFile(SiteFileRow),
    SiteFilesFinished,
    /// The signed-out check's answer (spec 025 FR-012).
    SessionCheck {
        signed_out: bool,
    },
}

/// Every this many internal pages, the signed-out check runs (plan D5:
/// half the SC-006 budget of 50 requests).
const SESSION_CHECK_EVERY: u32 = 25;

/// The signed-out check of a crawl with a login session (FR-012).
struct SessionCheck {
    url: Url,
    when: spiderlens_config::CheckCondition,
    since: u32,
    pending: bool,
    /// When the check last passed, in ms since the epoch.
    last_ok_ms: i64,
}

/// The per-host AI site files (spec 015 FR-010), fetched after the last page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SiteFiles {
    NotStarted,
    Pending,
    Done,
}

struct SitemapRead {
    file: SitemapFileRecord,
    /// The location of the index that listed it.
    parent: Option<String>,
    /// What the entries resolve against: the final URL, or the address a
    /// local file will have.
    base: Option<Url>,
    entries: Vec<sitemap::Entry>,
}

/// Where a sitemap comes from (spec 011 FR-001, FR-002).
enum SitemapSource {
    Url(Url),
    Local { path: std::path::PathBuf, base: Url },
}

struct SitemapJob {
    at: SitemapSource,
    source: &'static str,
    parent: Option<String>,
    level: u32,
}

impl SitemapSource {
    fn location(&self) -> String {
        match self {
            Self::Url(u) => u.to_string(),
            Self::Local { path, .. } => format!("file:{}", path.display()),
        }
    }
}

/// Reads one sitemap file. None for a default location that is not there.
async fn read_sitemap(client: &Client, job: &SitemapJob, optional: bool) -> Option<SitemapRead> {
    let mut file = SitemapFileRecord {
        location: job.at.location(),
        source: job.source,
        level: job.level,
        read_at: jiff::Timestamp::now().as_millisecond(),
        ..Default::default()
    };
    let (bytes, base) = match &job.at {
        SitemapSource::Url(url) => match sitemap::fetch(client, url).await {
            Ok(f) => {
                file.http_status = f.status;
                file.final_url = Some(f.final_url.to_string());
                file.redirected = f.redirected;
                (Some(f.bytes), Some(f.final_url))
            }
            Err(_) if optional => return None,
            Err(e) => {
                file.http_status = e.status;
                file.final_url = e.final_url.as_ref().map(Url::to_string);
                file.error_class = Some(e.class.to_string());
                file.error = Some(e.message);
                (None, Some(url.clone()))
            }
        },
        SitemapSource::Local { path, base } => {
            file.base_url = Some(base.to_string());
            let p = path.clone();
            let read = tokio::task::spawn_blocking(move || std::fs::read(p))
                .await
                .unwrap_or_else(|e| Err(std::io::Error::other(e)));
            match read {
                Ok(b) => (Some(b), Some(base.clone())),
                Err(e) => {
                    file.error_class = Some("file_unreadable".into());
                    file.error = Some(format!("could not read {}: {e}", path.display()));
                    (None, Some(base.clone()))
                }
            }
        }
    };
    let mut entries = Vec::new();
    if let Some(bytes) = bytes {
        match sitemap::read(&bytes, |e| entries.push(e)) {
            Ok(facts) => {
                file.kind = Some(match facts.root {
                    Root::Urlset => "urlset",
                    Root::Index => "index",
                });
                file.compressed = facts.compressed;
                file.bytes = facts.bytes;
                file.entry_count = facts.entries;
                file.over_limit = facts.over_limit;
            }
            Err(e) => {
                file.error_class = Some(e.class.as_str().to_string());
                file.error = Some(e.message);
                file.error_line = e.line;
                file.entry_count = e.entries;
                file.bytes = bytes.len() as u64;
            }
        }
    }
    Some(SitemapRead {
        file,
        parent: job.parent.clone(),
        base,
        entries,
    })
}

/// Queues the files an index lists, up to the depth limit.
fn expand_index(read: &mut SitemapRead, job: &SitemapJob, todo: &mut Vec<SitemapJob>) {
    if read.file.kind != Some("index") {
        return;
    }
    if job.level + 1 >= MAX_INDEX_DEPTH {
        read.file.error_class = Some("too_deep".into());
        read.file.error = Some(format!(
            "sitemap index nested deeper than {MAX_INDEX_DEPTH} levels"
        ));
        return;
    }
    let Some(base) = &read.base else { return };
    for e in read.entries.iter().rev() {
        if let Ok(child) = base.join(&e.loc)
            && matches!(child.scheme(), "http" | "https")
        {
            todo.push(SitemapJob {
                at: SitemapSource::Url(child),
                source: job.source,
                parent: Some(read.file.location.clone()),
                level: job.level + 1,
            });
        }
    }
}

struct PageOutcome {
    status: u16,
    content_type: Option<String>,
    location: Option<String>,
    retry_after: Option<String>,
    headers: Vec<(String, String)>,
    content_length: Option<u64>,
    /// Bytes of body read, when it was read.
    body_len: Option<u64>,
    /// Body cut off; what was read is still extracted.
    body_problem: Option<CheckError>,
    header_facts: HeaderFacts,
    page: Option<Extracted>,
    /// A PDF body was handed to the reader pool; a `Msg::Pdf` follows.
    pdf: bool,
    /// A 401 a sign-in did not answer (spec 025).
    auth: Option<AuthNote>,
}

/// What the extraction step produced for an HTML body.
struct Extracted {
    data: Result<PageData, String>,
    /// Rule results, encoded for the store.
    custom: Option<EncodedCustom>,
    /// Compressed body and its size, when raw HTML is stored or the page
    /// may be rendered.
    html: Option<(Vec<u8>, usize)>,
}

/// Stores the static accessibility findings and mobile facts of one page
/// (spec 026), inside the writer's batch.
fn record_static_a11y(
    store: &CrawlStore,
    url_id: i64,
    a: &extract::a11y::A11yStatic,
) -> Result<(), super::store::StoreError> {
    use super::store::{A11yNodeRow, A11yRow, MobileRow};
    use extract::a11y::StaticCheck;
    let rows: Vec<A11yRow<'_>> = a
        .findings
        .iter()
        .map(|f| A11yRow {
            check_id: format!("static.{}", f.check.as_str()).into(),
            kind: "violation",
            source: "static",
            impact: Some(f.check.impact().as_str()),
            wcag: f.check.wcag().into(),
            nodes_total: f.count,
            nodes: f
                .nodes
                .iter()
                .map(|n| A11yNodeRow {
                    selector: n.selector.as_str().into(),
                    excerpt: n.excerpt.as_str().into(),
                    data: None,
                    bbox: None,
                })
                .collect(),
        })
        .collect();
    let viewport_state = match &a.viewport {
        None => "missing",
        Some(v) if v.zoom_disabled => "zoom_disabled",
        Some(v) if !v.device_width => "not_device_width",
        Some(_) => "ok",
    };
    store.record_static_a11y(
        url_id,
        &rows,
        &MobileRow {
            viewport: a.viewport.as_ref().map(|v| v.content.as_str()),
            viewport_state: Some(viewport_state),
            plugins: Some(
                a.finding(StaticCheck::PluginElement)
                    .map_or(0, |f| i64::from(f.count)),
            ),
            ..MobileRow::default()
        },
    )?;
    Ok(())
}

/// Header facts for every response; for HTML bodies, extraction and
/// compression on Tokio's blocking pool under the memory budget, so parsing
/// never stalls network I/O (spec 005 plan D3).
/// `keep_html`: compress the body, to store it or to render it later.
#[allow(clippy::too_many_arguments)]
async fn finish_page(
    o: Once,
    url: Url,
    token: String,
    keep_html: bool,
    custom: Option<Arc<CustomRun>>,
    hook: Option<PageHook>,
    content: Arc<ContentRules>,
    a11y: bool,
    budget: &Semaphore,
) -> PageOutcome {
    let header_facts = extract::header_facts(&o.x_robots_tag, &o.link_header);
    let body_len = o.body.as_ref().map(|b| b.len() as u64);
    let truncated = o.body_problem.is_some();
    let page = match o.body {
        Some(bytes) if is_html(o.content_type.as_deref(), &bytes) => {
            let kib = (bytes.len() / 1024 + 1).min(EXTRACT_BUDGET_KIB) as u32;
            let _permit = budget
                .acquire_many(kib)
                .await
                .expect("the budget is never closed");
            let charset = o.charset.clone();
            let job = tokio::task::spawn_blocking(move || {
                let mut data = extract::extract_page_caught(
                    &PageInput {
                        url: &url,
                        charset: charset.as_deref(),
                        body: &bytes,
                        truncated,
                        robots_token: &token,
                        keep_text: false,
                        // Spec 026: static accessibility checks, internal pages only.
                        a11y,
                        hook: hook.as_ref(),
                        content: Some(&content),
                    },
                    custom.as_deref().map(|c| &c.rules),
                );
                // Encoded here, so the single writer only inserts.
                let custom = match (data.as_mut().ok().and_then(|d| d.custom.take()), &custom) {
                    (Some(out), Some(c)) => encode_custom(&c.slots, &out).ok(),
                    _ => None,
                };
                let html = if keep_html {
                    body::compress(&bytes).ok().map(|packed| (packed, bytes.len()))
                } else {
                    None
                };
                Extracted { data, custom, html }
            });
            Some(job.await.unwrap_or_else(|e| Extracted {
                data: Err(format!("Page extraction failed: {e}")),
                custom: None,
                html: None,
            }))
        }
        _ => None,
    };
    PageOutcome {
        status: o.status,
        content_type: o.content_type,
        location: o.location,
        retry_after: o.retry_after,
        headers: o.headers,
        content_length: o.content_length,
        body_len,
        body_problem: o.body_problem,
        header_facts,
        page,
        pdf: false,
        auth: o.auth,
    }
}

/// Why a reference that resolves to no new URL is not crawlable (FR-024).
fn self_reason(raw: &str) -> &'static str {
    let t = raw.trim_matches(|c: char| c.is_ascii_whitespace());
    if t.is_empty() {
        "empty"
    } else if t.starts_with('#') {
        "fragment"
    } else {
        "self"
    }
}

/// A crawl bound to its crawl file.
pub struct Crawler {
    store: CrawlStore,
    path: PathBuf,
    /// Held while this value may write the file (spec 006 plan D2).
    lock: WriterLock,
    /// Created by this value, so the run is not a resume.
    fresh: bool,
    disk_reserve: Option<u64>,
    opened: Opened,
    stored: StoredCrawlConfig,
    config: CrawlConfig,
    rules: ScopeRules,
    start: Option<Url>,
    resolve: Vec<(String, SocketAddr)>,
    retry_base: Duration,
    /// The browser to render with; found on the machine when not given.
    browser: Option<spiderlens_render::BrowserInfo>,
    /// How to start it; through the guard process when not given.
    render_launch: Option<spiderlens_render::LaunchOptions>,
    /// Back-pressure marks: fetching pauses above the first number of
    /// queued renders and resumes below the second (FR-035).
    render_queue_marks: (u64, u64),
    page_hook: Option<PageHook>,
    /// The login session the crawl signs in with (spec 025 plan D5); held
    /// in memory only.
    login: Option<LoginSession>,
}

impl Crawler {
    /// Crawls signed in with the cookies of a login session (spec 025
    /// US2). Sign-out links are not requested while it is in use.
    pub fn set_login(&mut self, session: LoginSession) {
        self.login = Some(session);
    }

    /// Reads more from every parsed page during this crawl (see
    /// [`PageHook`]); its facts are stored in `page_facts`.
    pub fn set_page_hook(&mut self, hook: PageHook) {
        self.page_hook = Some(hook);
    }

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
        let lock = WriterLock::acquire(path)?;
        let mut store = CrawlStore::create(path)?;
        store.record_event("created", None)?;

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
        // What readers will find in this file (spec 007 plan D3).
        let mut features = String::from(if config.extraction.store_html {
            "extraction,timing,attempts,raw_html,readability,sitemap_entries,agents,near_dup"
        } else {
            "extraction,timing,attempts,readability,sitemap_entries,agents,near_dup"
        });
        if config.extraction.pdf.crawl {
            features.push_str(",pdf");
        }
        if config.extraction.check_amp {
            features.push_str(",amp");
        }
        if config.accessibility.level != A11yLevel::Off {
            features.push_str(",accessibility_static");
        }
        store.set_meta("data_features", &features)?;
        if config.rendering.is_on() {
            let mut features = vec!["rendering"];
            if config.extraction.store_html {
                features.push("rendered_html");
            }
            if config.rendering.screenshots {
                features.push("screenshots");
            }
            // Spec 026: full checks and mobile layout need the render stage.
            if config.accessibility.level == A11yLevel::Full {
                features.push("accessibility_full");
                store.set_meta(
                    "a11y_engine",
                    &format!("{{\"name\":\"axe-core\",\"version\":\"{}\"}}", AXE_VERSION),
                )?;
            }
            if config.accessibility.mobile && config.accessibility.level != A11yLevel::Off {
                features.push("mobile_layout");
            }
            store.add_data_features(&features)?;
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
            path: path.to_path_buf(),
            lock,
            fresh: true,
            disk_reserve: None,
            opened: Opened::default(),
            stored,
            config,
            rules,
            start,
            resolve: Vec::new(),
            retry_base: Duration::from_secs(1),
            browser: None,
            render_launch: None,
            render_queue_marks: (QUEUE_HIGH, QUEUE_LOW),
            page_hook: None,
            login: None,
        })
    }

    /// Opens a crawl file to resume it with its stored configuration.
    /// Refuses older formats; see [`Self::open_with`].
    pub fn open(path: &Path) -> Result<Self, CrawlError> {
        Self::open_with(path, &ResumeOptions::default())
    }

    /// Opens a crawl file to resume it: takes the writer lock, recovers it if
    /// it was not closed cleanly, upgrades an older format (keeping a copy
    /// unless declined) and applies speed and URL-limit changes (spec 006).
    pub fn open_with(path: &Path, options: &ResumeOptions) -> Result<Self, CrawlError> {
        if !path.exists() {
            return Err(CrawlError::Config(format!("{} does not exist", path.display())));
        }
        if !file::writable(path) {
            return Err(CrawlError::ReadOnly(path.to_path_buf()));
        }
        let lock = WriterLock::acquire(path)?;
        let mut opened = Opened::default();
        let store = CrawlStore::open(path)?;
        if store.format_version()? < 2 {
            return Err(CrawlError::Config(
                "This crawl file was made by an earlier SpiderLens build, before page data extraction. \
                 It can still be opened and viewed, but not resumed; start a new crawl instead."
                    .into(),
            ));
        }
        let mut stored: StoredCrawlConfig = store
            .meta("config")?
            .and_then(|c| serde_json::from_str(&c).ok())
            .ok_or_else(|| CrawlError::Config("the crawl file has no readable configuration".into()))?;
        // Secrets are never stored (FR-034): the caller supplies them again.
        let missing = stored.config.fill_secrets(&options.secrets);
        if !missing.is_empty() {
            return Err(CrawlError::NeedsSecrets(missing));
        }
        let version = store.format_version()?;
        if version < super::store::FORMAT_VERSION {
            let Some(upgrade) = options.upgrade else {
                return Err(CrawlError::NeedsUpgrade { from: version });
            };
            opened.recovered = file::recover(&store)?;
            let backup = match upgrade {
                Upgrade::KeepCopy => {
                    let to = file::backup_path(path, version);
                    file::write_atomically(&to, |p| file::copy_database(&store, path, p))?;
                    Some(to)
                }
                Upgrade::NoCopy => None,
            };
            super::migrate::upgrade(store.conn()).map_err(StoreError::from)?;
            store.record_event(
                "upgraded",
                Some(&serde_json::json!({ "from": version, "to": super::store::FORMAT_VERSION, "backup": backup }).to_string()),
            )?;
            opened.upgraded = Some((version, backup));
        } else {
            opened.recovered = file::recover(&store)?;
        }
        if !options.changes.is_empty() {
            let now = jiff::Timestamp::now();
            for change in &options.changes {
                stored.record(now, change.clone());
                store.record_event(
                    "config_changed",
                    Some(&serde_json::to_string(change).expect("changes serialize")),
                )?;
            }
            // The stored copy never holds secret values.
            let mut on_disk = stored.clone();
            for h in &mut on_disk.config.identity.headers {
                if h.value.is_secret() {
                    h.value = HeaderValue::Secret(None);
                }
            }
            store.set_meta(
                "config",
                &serde_json::to_string(&on_disk).expect("config serializes"),
            )?;
        }
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
            path: path.to_path_buf(),
            lock,
            fresh: false,
            disk_reserve: None,
            opened,
            stored,
            config,
            rules,
            start,
            resolve: Vec::new(),
            retry_base: Duration::from_secs(1),
            browser: None,
            render_launch: None,
            render_queue_marks: (QUEUE_HIGH, QUEUE_LOW),
            page_hook: None,
            login: None,
        })
    }

    /// Pins host names to addresses (tests serve `.test` hosts locally).
    pub fn with_resolve(mut self, resolve: Vec<(String, SocketAddr)>) -> Self {
        self.resolve = resolve;
        self
    }

    /// First wait between robots.txt attempts; doubles each time.
    /// The browser a rendering crawl uses, instead of looking for one.
    pub fn with_browser(mut self, browser: spiderlens_render::BrowserInfo) -> Self {
        self.browser = Some(browser);
        self
    }

    /// How a rendering crawl starts its browser. By default through a guard
    /// process (this executable with `--render-guard`), so the browser is
    /// closed even if this process is killed (research R12).
    pub fn with_render_launch(mut self, launch: spiderlens_render::LaunchOptions) -> Self {
        self.render_launch = Some(launch);
        self
    }

    /// Overrides the render queue marks that hold fetching back (tests use
    /// small ones).
    pub fn with_render_queue_marks(mut self, high: u64, low: u64) -> Self {
        self.render_queue_marks = (high, low);
        self
    }

    pub fn with_retry_base(mut self, base: Duration) -> Self {
        self.retry_base = base;
        self
    }

    pub fn config(&self) -> &CrawlConfig {
        &self.config
    }

    /// The crawl file.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// What opening did to the file (recovery, upgrade).
    pub fn opened(&self) -> &Opened {
        &self.opened
    }

    /// Pause when free space on the crawl file's drive drops below `bytes`
    /// (spec 006 FR-027). None turns the check off.
    pub fn with_disk_reserve(mut self, bytes: Option<u64>) -> Self {
        self.disk_reserve = bytes;
        self
    }

    /// Limits the crawl file's size in database pages, so tests can make the
    /// disk "full" (SQLite then fails writes with SQLITE_FULL).
    #[doc(hidden)]
    pub fn with_max_pages(self, pages: u64) -> Result<Self, CrawlError> {
        self.store
            .conn()
            .pragma_update(None, "max_page_count", pages as i64)
            .map_err(StoreError::from)?;
        Ok(self)
    }

    /// Stores a key in the crawl file's `meta` table, for callers that add
    /// their own context, such as the schedule that started a run (spec 021).
    pub fn set_meta(&self, key: &str, value: &str) -> Result<(), CrawlError> {
        Ok(self.store.set_meta(key, value)?)
    }

    /// Runs until the queue is empty or `control` asks to pause or stop.
    pub async fn run(
        self,
        control: watch::Receiver<Control>,
        events: Option<mpsc::UnboundedSender<CrawlEvent>>,
    ) -> Result<CrawlSummary, CrawlError> {
        self.run_with(control, events, None).await
    }

    /// [`Self::run`] that also takes requests such as Save As.
    pub async fn run_with(
        mut self,
        control: watch::Receiver<Control>,
        events: Option<mpsc::UnboundedSender<CrawlEvent>>,
        commands: Option<mpsc::UnboundedReceiver<RunCommand>>,
    ) -> Result<CrawlSummary, CrawlError> {
        let opts = CheckOptions::from(&self.config);
        let mut net = NetSession::with_login(
            &opts,
            &self.config.access,
            self.rules.internal().clone(),
            &self.resolve,
            self.login.as_ref(),
        )
        .map_err(CrawlError::Client)?;
        net.set_ntlm_connections(self.config.speed.max_per_host as usize);
        // The proxy a crawl used is part of its record (FR-024); nothing is
        // written on the default path.
        if self.config.access.proxy.mode != spiderlens_config::ProxyMode::None {
            self.store.set_meta(
                "proxy_in_effect",
                &serde_json::to_string(net.proxy_in_effect()).expect("serializes"),
            )?;
        }
        if let Some(l) = &self.login {
            // Names only, never values (data-model.md).
            let names: Vec<String> = l
                .summary()
                .into_iter()
                .map(|(d, n)| format!("{d}: {}", n.join(", ")))
                .collect();
            // The site signed the last run out: what it fetched after the
            // last good check is login pages, so it is fetched again (SC-006).
            let ended = self.store.meta("pause_reason")?.as_deref() == Some("session_ended");
            if ended
                && let Some(since) = self
                    .store
                    .meta("session_ok_at")?
                    .and_then(|v| v.parse::<i64>().ok())
            {
                let n = self.store.requeue_fetched_since(since)?;
                self.store.record_event(
                    "signed_in_again",
                    Some(
                        &serde_json::json!({ "login_url": l.login_url, "cookies": names, "requeued": n })
                            .to_string(),
                    ),
                )?;
            } else {
                self.store.record_event(
                    "signed_in",
                    Some(&serde_json::json!({ "login_url": l.login_url, "cookies": names }).to_string()),
                )?;
            }
        }
        let mut run = Run::new(self, opts, net, control, events, commands)?;
        if run.c.config.rendering.is_on() {
            run.start_render().await?;
        }
        let ended = run.main_loop().await;
        if let Some(stage) = run.render.take() {
            stage.close().await;
        }
        match ended {
            Ok(()) => {}
            // A write failed: nothing of the open batch reached the file, so
            // no URL is marked crawled without its result (FR-028).
            Err(CrawlError::Store(e)) => return Err(CrawlError::Local(run.fail_locally(&e))),
            Err(e) => return Err(e),
        }
        if let Some(local) = run.local.take() {
            return Err(CrawlError::Local(local));
        }
        summarize(&run.c.store)
    }

    /// "Retry failed URLs" (spec 004 US5, plan D12): opens a paused or
    /// finished crawl and re-queues its transient failures. Run the returned
    /// crawler to fetch them. The count says how many were re-queued; 0 means
    /// there was nothing to retry and nothing changed.
    pub fn retry_failed(path: &Path) -> Result<(Self, u64), CrawlError> {
        let mut c = Self::open(path)?;
        let by_class = c.store.transient_failures()?;
        let n = c.store.requeue_transient()?;
        if n > 0 {
            let detail: Vec<String> = by_class.iter().map(|(k, v)| format!("{k} {v}")).collect();
            c.store.log(
                now_ms(),
                "manual_retry",
                None,
                None,
                &format!(
                    "Retrying {n} URLs that failed for temporary reasons ({}).",
                    detail.join(", ")
                ),
            )?;
        }
        Ok((c, n))
    }
}

/// What "Retry failed URLs" would re-queue, by class (spec 004 FR-031).
pub fn retry_preview(path: &Path) -> Result<Vec<(String, u64)>, CrawlError> {
    Ok(CrawlStore::open(path)?.transient_failures()?)
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
    net: Arc<NetSession>,
    headers: Arc<Vec<RequestHeader>>,
    /// Origins already told about a sign-in they need or refused (spec
    /// 025 FR-004, FR-005): once each.
    auth_noticed: HashSet<String>,
    /// Sign-out patterns while a login session is in use (FR-011).
    sign_out_guard: Option<Vec<String>>,
    session_check: Option<SessionCheck>,
    /// The signed-out check failed: the crawl pauses (FR-012).
    session_ended: bool,
    control: watch::Receiver<Control>,
    control_closed: bool,
    events: Option<mpsc::UnboundedSender<CrawlEvent>>,
    tx: mpsc::UnboundedSender<Msg>,
    rx: mpsc::UnboundedReceiver<Msg>,
    window: QueueWindow,
    hosts: HashMap<String, Host>,
    in_flight: u32,
    next_global: Instant,
    /// When the URLs-per-second cap last held back a ready URL.
    rate_held_at: Option<Instant>,
    /// Rendered pages' resource requests waiting for a slot (spec 014 D4).
    permits: Permits,
    /// A waiting resource holds the global caps: no new page may start.
    permits_hold_global: bool,
    /// The render stage; None unless the crawl renders (spec 014 D2).
    render: Option<RenderStage>,
    /// `meta.fetch_completed_at` was written in this run.
    fetch_completed: bool,
    pages_started: u64,
    limit_reached: bool,
    sitemaps_pending: bool,
    /// Ids of the sitemap files stored so far, by location.
    sitemap_ids: HashMap<String, i64>,
    site_files: SiteFiles,
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
    extract_budget: Arc<Semaphore>,
    /// Main content settings, parsed once (spec 020 FR-003).
    content_rules: Arc<ContentRules>,
    /// Reads PDFs when the crawl reads them (spec 027 part C).
    pdf_pool: Option<pdf::pool::Pool>,
    /// PDFs whose response is recorded but whose reading is not.
    pdf_pending: u32,
    /// PDFs whose X-Robots-Tag says nofollow.
    pdf_nofollow: std::collections::HashSet<i64>,
    /// Custom rules and where their results go (spec 013).
    custom: Option<Arc<CustomRun>>,
    /// Active crawl time of earlier runs of this file.
    active_before_ms: u64,
    last_catch_up: Instant,
    commands: Option<mpsc::UnboundedReceiver<RunCommand>>,
    /// URLs waiting to be retried: (due, row id), soonest first (plan D4).
    retries: BinaryHeap<Reverse<(Instant, i64)>>,
    rng: fastrand::Rng,
    /// Network failures in a row with no answer in between, and the hosts
    /// they came from (plan D7).
    net_failures: u32,
    net_hosts: HashSet<String>,
    /// The window may grow while hosts are paused (plan D11).
    window_cap: usize,
    /// Set while a redirect past the hop limit is being recorded.
    redirect_over_limit: bool,
    /// Set while a reference found only after rendering is discovered: its
    /// position in the rendered page; its link row goes to `rendered_links`.
    rendered_rpos: Option<u32>,
    /// URLs finished per host, earlier runs included (spec 008 FR-029).
    crawled_by_host: HashMap<String, u64>,
    /// Speed events per host in this run (FR-021).
    site_events: HashMap<String, SiteEvents>,
    /// How busy the loop is: computer or saving as the limit (FR-028).
    load: LoadMeter,
    /// Every site is slow at once: requests stay at this many until it
    /// passes (FR-019).
    connection_hold: Option<u32>,
    /// A disk problem that is pausing the crawl (spec 006 FR-027).
    local: Option<LocalError>,
    next_disk_check: Instant,
}

impl Run {
    fn new(
        c: Crawler,
        opts: CheckOptions,
        net: NetSession,
        control: watch::Receiver<Control>,
        events: Option<mpsc::UnboundedSender<CrawlEvent>>,
        commands: Option<mpsc::UnboundedReceiver<RunCommand>>,
    ) -> Result<Self, CrawlError> {
        let (tx, rx) = mpsc::unbounded_channel();
        let mut c = c;
        c.store.note_attempts_feature()?;
        c.store.checkpoint_in_background()?;
        c.store.drop_result_indexes()?;
        c.store.requeue_for_resume()?;
        c.store.requeue_rendering()?;
        c.store.set_meta("status", "running")?;
        // URLs that were waiting for a retry keep their remaining wait
        // (FR-023).
        let now = Instant::now();
        let now_wall = now_ms();
        let retries = c
            .store
            .waiting()?
            .into_iter()
            .map(|(id, at)| {
                Reverse((
                    now + Duration::from_millis(at.saturating_sub(now_wall).max(0) as u64),
                    id,
                ))
            })
            .collect();
        c.store
            .conn()
            .execute("DELETE FROM meta WHERE key = 'pause_reason'", [])
            .map_err(StoreError::from)?;
        if !c.fresh {
            c.store.record_event("resumed", None)?;
        }
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
        let crawled_by_host = c.store.crawled_by_host()?.into_iter().collect();
        let active_before_ms = c
            .store
            .meta("active_ms")?
            .and_then(|v| v.parse().ok())
            .unwrap_or(0);
        // Validation rejected bad selectors before the crawl started.
        let content = &c.config.content;
        let content_rules =
            ContentRules::new(content.content_selector.as_deref(), &content.exclude_selectors)
                .unwrap_or_default();
        let pdf = &c.config.extraction.pdf;
        let pdf_pool = pdf.crawl.then(|| {
            pdf::pool::Pool::new(
                pdf::pool::default_readers(),
                Duration::from_secs(u64::from(pdf.time_limit_secs)),
                c.config.extraction.store_html,
            )
        });
        let custom = if c.config.custom.is_empty() && !c.store.has_custom_rules()? {
            None
        } else {
            if !c.config.custom.is_empty() {
                c.store.sync_custom_rules(&c.config.custom)?;
            }
            let (set, slots) = c.store.stored_rule_set()?;
            match crate::custom::compile(&set) {
                Ok(None) => None,
                Ok(Some(rules)) => {
                    c.store.mark_new_custom_rules_running()?;
                    Some(Arc::new(CustomRun { rules, slots }))
                }
                Err(errors) => {
                    let joined = errors
                        .iter()
                        .map(ToString::to_string)
                        .collect::<Vec<_>>()
                        .join("; ");
                    return Err(CrawlError::Config(format!("custom rules: {joined}")));
                }
            }
        };
        let sign_out_guard = c.login.as_ref().filter(|l| !l.cookies.is_empty()).map(|_| {
            c.config.access.login.as_ref().map_or_else(
                || {
                    spiderlens_config::DEFAULT_SIGN_OUT_PATTERNS
                        .iter()
                        .map(|p| (*p).to_string())
                        .collect()
                },
                |l| l.sign_out_patterns.clone(),
            )
        });
        let session_check = c
            .login
            .as_ref()
            .filter(|l| !l.cookies.is_empty())
            .and_then(|_| c.config.access.login.as_ref()?.check.clone())
            .and_then(|check| {
                Some(SessionCheck {
                    url: Url::parse(check.url.trim()).ok()?,
                    when: check.when,
                    // The first check runs early, so a session that does
                    // not work is seen at once.
                    since: SESSION_CHECK_EVERY - 1,
                    pending: false,
                    last_ok_ms: now_ms(),
                })
            });
        Ok(Self {
            headers: Arc::new(opts.headers.clone()),
            c,
            opts,
            client: net.client().clone(),
            net: Arc::new(net),
            auth_noticed: HashSet::new(),
            sign_out_guard,
            session_check,
            session_ended: false,
            control,
            control_closed: false,
            events,
            tx,
            rx,
            window: QueueWindow::new(WINDOW),
            hosts,
            in_flight: 0,
            next_global: Instant::now(),
            rate_held_at: None,
            permits: Permits::default(),
            permits_hold_global: false,
            render: None,
            fetch_completed: false,
            pages_started,
            limit_reached,
            sitemaps_pending: false,
            sitemap_ids: HashMap::new(),
            site_files: SiteFiles::NotStarted,
            discovered,
            crawled,
            errors,
            last_progress: Instant::now() - PROGRESS_EVERY,
            warnings,
            write_tx: None,
            finished: 0,
            started_at: Instant::now(),
            rate_samples: VecDeque::new(),
            extract_budget: Arc::new(Semaphore::new(EXTRACT_BUDGET_KIB)),
            content_rules: Arc::new(content_rules),
            pdf_pool,
            pdf_pending: 0,
            pdf_nofollow: Default::default(),
            custom,
            active_before_ms,
            last_catch_up: Instant::now(),
            commands,
            retries,
            rng: fastrand::Rng::new(),
            net_failures: 0,
            net_hosts: HashSet::new(),
            window_cap: WINDOW,
            redirect_over_limit: false,
            rendered_rpos: None,
            crawled_by_host,
            site_events: HashMap::new(),
            load: LoadMeter::new(now),
            connection_hold: None,
            local: None,
            next_disk_check: Instant::now(),
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
            self.update_connection_hold(self.last_progress);
            if self.events.is_none() {
                return;
            }
            // Kept by the store as rows change state: counting the table here
            // grows with the crawl and stalls the loop on large crawls.
            let StateCounts {
                queued,
                not_crawled,
                waiting,
                ..
            } = self.c.store.live_counts();
            let (urls_per_sec, avg_urls_per_sec) = self.rates();
            // Under a URL limit, only the pages it still allows are left.
            let left = match self.c.config.limits.max_urls {
                Some(max) if !self.limit_reached => queued.min(max.saturating_sub(self.pages_started)),
                Some(_) => 0,
                None => queued,
            };
            let eta_secs = (urls_per_sec > 0.0).then(|| (left as f64 / urls_per_sec).ceil() as u64);
            let paused_hosts = self.paused_hosts();
            let waiting_for_host = !paused_hosts.is_empty()
                && self.in_flight == 0
                && !self.window.is_empty()
                && self
                    .window
                    .host_heads()
                    .all(|(h, _)| self.hosts.get(h).is_some_and(|x| x.health.is_blocked()));
            let load = self.load.load(Instant::now());
            let (limit, sites, sites_total) = self.pace(urls_per_sec, load);
            let (a11y_checked, a11y_avg_ms) = self.c.store.a11y_live();
            let fetching_done = self.fetch_completed;
            let render = self.render.as_mut().map(|r| r.progress(fetching_done));
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
                active_ms: self.active_ms(),
                waiting,
                paused_hosts,
                waiting_for_host,
                limit,
                sites,
                sites_total,
                a11y_checked,
                a11y_avg_ms,
                render,
            }));
        }
    }

    /// FR-019: when every answering site gets slow at once, the user's
    /// connection is the problem. Requests stay where they are and no site
    /// is cut for slowness until it passes.
    fn update_connection_hold(&mut self, now: Instant) {
        let slow = pace::connection_slow(self.hosts.values().map(|h| (&h.speed, h.last_answer)), now);
        if slow == self.connection_hold.is_some() {
            return;
        }
        self.connection_hold = slow.then_some(self.in_flight.max(1));
        for h in self.hosts.values_mut() {
            h.speed.set_hold(slow);
        }
    }

    fn site_view<'a>(&'a self, key: &'a str, h: &'a Host, waiting: u64) -> SiteView<'a> {
        use super::health::HealthState;
        let now = Instant::now();
        let paused = match h.health.state() {
            HealthState::Paused { until } => {
                Some(until.saturating_duration_since(now).as_secs_f64().ceil() as u64)
            }
            HealthState::Probing { .. } => Some(0),
            HealthState::Active | HealthState::GivenUp => None,
        };
        SiteView {
            host: key,
            speed: &h.speed,
            in_flight: h.in_flight,
            limit: h.limit(),
            delay: h.delay,
            next_start: h.next_start,
            paused,
            crawled: self.crawled_by_host.get(key).copied().unwrap_or(0),
            waiting,
            robots_pending: !matches!(h.robots, RobotsState::Known(..)),
        }
    }

    /// What limits the crawl (FR-028) and the per-site breakdown (FR-029).
    fn pace(&self, urls_per_sec: f64, load: pace::Load) -> (Option<SpeedLimit>, Vec<SitePace>, u32) {
        let now = Instant::now();
        let mut waiting: HashMap<&str, u64> = self.window.host_counts().map(|(h, n)| (h, n as u64)).collect();
        for (key, h) in &self.hosts {
            if h.in_flight > 0 {
                waiting.entry(key.as_str()).or_insert(0);
            }
        }
        let more = !self.window.holds_everything();
        let mut sites: Vec<SitePace> = Vec::with_capacity(waiting.len());
        // The site holding up the most queued URLs, when no site with queued
        // URLs can start a request now.
        let mut blocking: Option<(&str, &Host, u64)> = None;
        let mut any_can_start = false;
        for (&key, &n) in &waiting {
            let Some(h) = self.hosts.get(key) else {
                // robots.txt not asked for yet: about to start.
                any_can_start |= n > 0;
                continue;
            };
            sites.push(pace::site_pace(&self.site_view(key, h, n), now, more && n > 0));
            if n == 0 {
                continue;
            }
            let ready = matches!(h.robots, RobotsState::Known(..));
            if !ready || (h.in_flight < h.limit() && h.next_start <= now) {
                any_can_start = true;
            } else if blocking.is_none_or(|(_, _, m)| n > m) {
                blocking = Some((key, h, n));
            }
        }
        let total = sites.len() as u32;
        sites.sort_by(|a, b| {
            (b.in_flight as u64 + b.waiting)
                .cmp(&(a.in_flight as u64 + a.waiting))
                .then_with(|| a.host.cmp(&b.host))
        });
        sites.truncate(SITES_SHOWN);
        let crawl = CrawlView {
            in_flight: self.in_flight,
            max_concurrent: self.c.config.speed.max_concurrent,
            max_urls_per_sec: self.c.config.speed.max_urls_per_sec,
            urls_per_sec,
            rate_held: self.rate_held_at.is_some_and(|t| t.elapsed() < RATE_HELD_RECENT),
            connection_slow: self.connection_hold.is_some(),
            load,
        };
        let blocking = blocking
            .filter(|_| !any_can_start)
            .map(|(key, h, n)| self.site_view(key, h, n));
        (pace::crawl_limit(&crawl, blocking.as_ref(), now), sites, total)
    }

    fn paused_hosts(&self) -> Vec<PausedHost> {
        use super::health::HealthState;
        let now = Instant::now();
        let mut out: Vec<PausedHost> = self
            .hosts
            .iter()
            .filter_map(|(key, h)| {
                let (seconds_left, probing, given_up) = match h.health.state() {
                    HealthState::Active => return None,
                    HealthState::Paused { until } => (
                        until.saturating_duration_since(now).as_secs_f64().ceil() as u64,
                        false,
                        false,
                    ),
                    HealthState::Probing { .. } => (0, true, false),
                    HealthState::GivenUp => (0, false, true),
                };
                let reason = if given_up {
                    "Given up after repeated failures".to_string()
                } else if probing {
                    "Checking whether it has recovered".to_string()
                } else {
                    "Too many temporary failures in a row".to_string()
                };
                Some(PausedHost {
                    host: key.clone(),
                    reason,
                    seconds_left,
                    probing,
                    given_up,
                })
            })
            .collect();
        out.sort_by(|a, b| a.host.cmp(&b.host));
        out
    }

    /// Runs the signed-out check every [`SESSION_CHECK_EVERY`] internal
    /// pages (FR-012). The check is not a crawled URL: it goes through the
    /// session's client and only its answer comes back.
    fn tick_session_check(&mut self) {
        let Some(check) = &mut self.session_check else {
            return;
        };
        check.since += 1;
        if check.pending || check.since < SESSION_CHECK_EVERY {
            return;
        }
        check.since = 0;
        check.pending = true;
        let client = self.client.clone();
        let url = check.url.clone();
        let when = check.when.clone();
        let tx = self.tx.clone();
        tokio::spawn(async move {
            let signed_out = match client.get(url).send().await {
                Ok(r) => {
                    let status = r.status().as_u16();
                    let location = r
                        .headers()
                        .get(reqwest::header::LOCATION)
                        .and_then(|v| v.to_str().ok())
                        .map(str::to_string);
                    let body = match &when {
                        spiderlens_config::CheckCondition::Contains(_) if location.is_none() => {
                            r.text().await.ok()
                        }
                        _ => None,
                    };
                    crate::net::login::signed_out(
                        &when,
                        &crate::net::login::CheckResponse {
                            status,
                            location: location.as_deref(),
                            body: body.as_deref(),
                        },
                    )
                }
                // A failed check says nothing about the session; the
                // network checks of spec 004 handle failures.
                Err(_) => false,
            };
            let _ = tx.send(Msg::SessionCheck { signed_out });
        });
    }

    /// Tells the user once per origin that it needs a sign-in, or refused
    /// the one given (spec 025 FR-004, FR-005).
    fn auth_notice(&mut self, url: &Url, note: &AuthNote) -> Result<(), CrawlError> {
        let origin = url.origin().ascii_serialization();
        if !self.auth_noticed.insert(origin.clone()) {
            return Ok(());
        }
        let host = url.host_str().unwrap_or_default().to_string();
        let names = |schemes: &[String]| {
            schemes
                .iter()
                .map(|s| match s.as_str() {
                    "ntlm" => "NTLM".to_string(),
                    other => {
                        let mut c = other.chars();
                        c.next()
                            .map(|f| f.to_ascii_uppercase().to_string() + c.as_str())
                            .unwrap_or_default()
                    }
                })
                .collect::<Vec<_>>()
                .join(", ")
        };
        let (kind, message) = match note {
            AuthNote::Required {
                schemes,
                supported: true,
            } => (
                "auth_required",
                format!(
                    "{host} asks for a sign-in ({}). Add one for {origin} and its pages are fetched again.",
                    names(schemes)
                ),
            ),
            AuthNote::Required {
                schemes,
                supported: false,
            } => (
                "auth_unsupported",
                format!(
                    "{host} asks for a kind of sign-in SpiderLens cannot do ({}). Its pages are recorded as 401.",
                    names(schemes)
                ),
            ),
            AuthNote::Rejected { method } => (
                "auth_rejected",
                format!(
                    "{host} refused the sign-in ({}). Check the user name and password; its pages are recorded as 401.",
                    names(&[(*method).to_string()])
                ),
            ),
        };
        self.notice(kind, Some(&origin), None, message)
    }

    /// Records a crawl log row and tells the user (spec 004 FR-035).
    fn notice(
        &mut self,
        kind: &str,
        host: Option<&str>,
        url_id: Option<i64>,
        message: String,
    ) -> Result<(), CrawlError> {
        self.c.store.log(now_ms(), kind, host, url_id, &message)?;
        self.emit(CrawlEvent::Notice(Notice {
            kind: kind.into(),
            host: host.map(str::to_string),
            message,
        }));
        Ok(())
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
        let asked = *self.control.borrow();
        if (self.local.is_some() || self.session_ended) && asked == Control::Run {
            Control::Pause
        } else {
            asked
        }
    }

    /// Pauses the crawl when free space drops below the reserve (FR-027).
    fn check_disk(&mut self) {
        let Some(reserve) = self.c.disk_reserve else {
            return;
        };
        if self.local.is_some() || Instant::now() < self.next_disk_check {
            return;
        }
        self.next_disk_check = Instant::now() + DISK_CHECK_EVERY;
        if let Some(free) = file::free_space(&self.c.path)
            && free < reserve
        {
            self.local = Some(LocalError::disk_low(&self.c.path, free, reserve));
        }
    }

    fn handle_command(&mut self, cmd: RunCommand) -> Result<(), CrawlError> {
        match cmd {
            RunCommand::SaveAs { dest, reply } => {
                self.commit()?;
                let result = self.move_to(&dest);
                let _ = reply.send(result.as_ref().map(|()| dest.clone()).map_err(|e| e.to_string()));
            }
            RunCommand::AddSignIn { sign_in, reply } => {
                self.net.add_sign_in(&sign_in);
                let origin = sign_in.origin_key().unwrap_or_default();
                self.auth_noticed.remove(&origin);
                // The password stays in memory; the stored configuration
                // gets the sign-in without it, so a resume asks for it.
                if !self
                    .c
                    .config
                    .access
                    .sign_ins
                    .iter()
                    .any(|s| s.origin_key().as_deref() == Some(origin.as_str()))
                {
                    self.c.config.access.sign_ins.push(sign_in.clone());
                    self.c.stored.config.access.sign_ins.push(sign_in);
                    self.c.store.set_meta(
                        "config",
                        &serde_json::to_string(&self.c.stored).expect("config serializes"),
                    )?;
                }
                // The open batch first: the re-queue writes outside it.
                self.commit()?;
                let entries = self.c.store.requeue_unauthorized(&origin)?;
                let n = entries.len() as u64;
                for e in entries {
                    self.window.on_queued(e);
                }
                if n > 0 {
                    self.notice(
                        "signed_in",
                        Some(&origin),
                        None,
                        format!("Signed in to {origin}; {n} URLs that answered 401 are fetched again."),
                    )?;
                }
                let _ = reply.send(n);
            }
            RunCommand::ResumeHost(key) => {
                let resumed = self.hosts.get_mut(&key).is_some_and(|h| h.health.resume_now());
                if resumed {
                    self.notice(
                        "host_resumed",
                        Some(&key),
                        None,
                        format!("{key} was resumed by you; one test request is sent first."),
                    )?;
                }
            }
            RunCommand::Apply(change) => {
                match &change {
                    Change::Speed(speed) => {
                        self.c.config.speed = *speed;
                        for h in self.hosts.values_mut() {
                            h.speed.set_max(speed.max_per_host, speed.adaptive);
                        }
                        if speed.max_urls_per_sec.is_none() {
                            self.next_global = Instant::now();
                        }
                    }
                    Change::Retry(r) => self.c.config.retry = *r,
                    Change::MaxUrls(m) => self.c.config.limits.max_urls = *m,
                    // Rendering settings take effect when a finished or
                    // stopped crawl is resumed with them (spec 014 T008);
                    // the render stage of a running crawl is fixed at start.
                    Change::Rendering(_) => return Ok(()),
                }
                self.c.stored.record(jiff::Timestamp::now(), change);
                self.c.store.set_meta(
                    "config",
                    &serde_json::to_string(&self.c.stored).expect("config serializes"),
                )?;
            }
        }
        Ok(())
    }

    /// Save As while crawling (plan D5): between batches, copy the file,
    /// switch the write connection and the lock to the copy, and go on.
    fn move_to(&mut self, dest: &Path) -> Result<(), CrawlError> {
        if file::same_path(&self.c.path, dest) {
            return Ok(());
        }
        let lock = WriterLock::acquire(dest)?;
        let src = self.c.path.clone();
        file::write_atomically(dest, |p| file::copy_database(&self.c.store, &src, p))?;
        self.c.store.reopen_at(dest)?;
        self.c.lock = lock;
        self.c.path = dest.to_path_buf();
        self.c.store.record_event("saved", Some(&file::json_path(dest)))?;
        Ok(())
    }

    /// After a failed write: roll the open batch back and record the pause
    /// as well as the file still allows.
    fn fail_locally(&mut self, e: &StoreError) -> LocalError {
        let local = LocalError::from_store(e, &self.c.path);
        let store = &self.c.store;
        let _ = store.conn().execute_batch("ROLLBACK");
        self.write_tx = None;
        let _ = store.set_meta("status", "paused");
        let _ = store.set_meta("pause_reason", local.kind.as_str());
        let _ = store.record_event(
            "local_error",
            Some(&serde_json::to_string(&local).expect("serializes")),
        );
        local
    }

    /// Every write in the loop goes into one transaction (plan D2).
    fn open_tx(&mut self) -> Result<(), CrawlError> {
        if self.write_tx.is_none() {
            self.c.store.begin()?;
            self.write_tx = Some((Instant::now(), 0));
        }
        Ok(())
    }

    fn active_ms(&self) -> u64 {
        self.active_before_ms + self.started_at.elapsed().as_millis() as u64
    }

    fn commit(&mut self) -> Result<(), CrawlError> {
        if self.write_tx.is_some() {
            // Saved with every batch, so it survives a quit or a crash.
            self.c
                .store
                .set_meta("active_ms", &self.active_ms().to_string())?;
        }
        if self.write_tx.take().is_some() {
            let t = Instant::now();
            self.c.store.commit()?;
            if self.last_catch_up.elapsed() >= CATCH_UP_EVERY && self.c.store.catch_up_checkpoint() {
                self.last_catch_up = Instant::now();
            }
            self.load.saving(t.elapsed());
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
            self.check_disk();
            let running = self.state() == Control::Run;
            // Resources of pages already rendering go first, paused or not:
            // the render that waits for them holds a slot already.
            let permit_wake = self.serve_permits()?;
            let mut wake = if running {
                self.requeue_due_retries()?;
                let wake = self.dispatch()?;
                if self.render.is_some() {
                    self.dispatch_renders()?;
                }
                wake
            } else {
                None
            };
            if let Some(t) = permit_wake {
                wake = Some(wake.map_or(t, |w| w.min(t)));
            }
            if running && let Some(Reverse((due, _))) = self.retries.peek() {
                wake = Some(wake.map_or(*due, |w| w.min(*due)));
            }
            self.progress(false);
            if self.in_flight == 0 && !self.fetch_completed && self.render.is_some() {
                self.note_fetch_completed()?;
            }
            if self.in_flight == 0
                && self.pdf_pending == 0
                && self.render.as_ref().is_none_or(|r| r.open == 0)
            {
                match self.state() {
                    Control::Pause => {
                        let reason = match &self.local {
                            Some(l) => l.kind.as_str(),
                            None if self.session_ended => "session_ended",
                            None => "user",
                        };
                        self.c.store.set_meta("status", "paused")?;
                        self.c.store.set_meta("pause_reason", reason)?;
                        if self.session_ended && self.local.is_none() {
                            let since = self.session_check.as_ref().map_or(0, |c| c.last_ok_ms);
                            self.c.store.set_meta("session_ok_at", &since.to_string())?;
                            self.c.store.record_event(
                                "session_ended",
                                Some(&serde_json::json!({ "since": since }).to_string()),
                            )?;
                            break;
                        }
                        match &self.local {
                            Some(l) => self.c.store.record_event(
                                "local_error",
                                Some(&serde_json::to_string(l).expect("serializes")),
                            )?,
                            None => self.c.store.record_event("paused", None)?,
                        }
                        break;
                    }
                    Control::Stop => {
                        if self.render.is_some() {
                            self.c.store.stop_renders()?;
                        }
                        self.c.store.stop_queue()?;
                        self.c.store.set_meta("status", "stopped")?;
                        self.c.store.record_event("stopped", None)?;
                        break;
                    }
                    Control::Run => {
                        self.window.refill(&self.c.store)?;
                        let fetch_done = self.window.is_empty()
                            && self.window.holds_everything()
                            && !self.sitemaps_pending
                            && self.retries.is_empty();
                        let rendering = match &mut self.render {
                            Some(r) if fetch_done => r.busy(&self.c.store)?,
                            _ => false,
                        };
                        if fetch_done && !rendering {
                            match self.site_files {
                                SiteFiles::NotStarted => self.start_site_files()?,
                                SiteFiles::Pending => {}
                                SiteFiles::Done => {
                                    self.c.store.set_meta("status", "completed")?;
                                    self.c
                                        .store
                                        .conn()
                                        .execute("DELETE FROM meta WHERE key = 'recovered'", [])
                                        .map_err(StoreError::from)?;
                                    self.c.store.record_event("completed", None)?;
                                    break;
                                }
                            }
                        }
                    }
                }
            }
            if let Some((since, _)) = self.write_tx {
                let due = since + COMMIT_EVERY;
                wake = Some(wake.map_or(due, |w| w.min(due)));
            }
            if self.c.disk_reserve.is_some() && self.local.is_none() {
                wake = Some(wake.map_or(self.next_disk_check, |w| w.min(self.next_disk_check)));
            }
            let sleep = async {
                match wake {
                    Some(t) => tokio::time::sleep_until(t.into()).await,
                    None => std::future::pending().await,
                }
            };
            let command = async {
                match self.commands.as_mut() {
                    Some(rx) => rx.recv().await,
                    None => std::future::pending().await,
                }
            };
            // Time spent waiting here is time the loop was not the limit.
            let waiting_since = Instant::now();
            tokio::select! {
                Some(cmd) = command => {
                    self.load.idle(waiting_since.elapsed());
                    self.handle_command(cmd)?;
                }
                Some(msg) = self.rx.recv() => {
                    self.load.idle(waiting_since.elapsed());
                    self.handle(msg)?;
                    if let Some((_, writes)) = &mut self.write_tx {
                        *writes += 1;
                    }
                }
                () = sleep => self.load.idle(waiting_since.elapsed()),
                r = self.control.changed(), if !self.control_closed => {
                    self.load.idle(waiting_since.elapsed());
                    if r.is_err() {
                        self.control_closed = true;
                    }
                }
            }
            self.commit_if_due()?;
        }
        self.record_speed_summaries()?;
        self.commit()?;
        self.c.store.build_result_indexes()?;
        self.c.store.link_sitemap_refs()?;
        #[cfg(debug_assertions)]
        {
            let mut counted = StateCounts::default();
            for (state, n) in self.c.store.state_counts()? {
                match state {
                    UrlState::Queued => counted.queued = n,
                    UrlState::InFlight => counted.in_flight = n,
                    UrlState::Waiting => counted.waiting = n,
                    UrlState::Crawled => counted.crawled = n,
                    UrlState::NotCrawled => counted.not_crawled = n,
                }
            }
            debug_assert_eq!(self.c.store.live_counts(), counted, "live counts match the file");
            // SC-008: a finished crawl leaves no render waiting or running.
            let status = self.c.store.meta("status")?;
            if matches!(status.as_deref(), Some("completed" | "stopped")) {
                let left = self
                    .c
                    .store
                    .render_counts()?
                    .into_iter()
                    .filter(|(s, _)| matches!(s, RenderStatus::Queued | RenderStatus::Rendering))
                    .map(|(_, n)| n)
                    .sum::<u64>();
                debug_assert_eq!(left, 0, "no render left queued or rendering");
            }
        }
        self.progress(true);
        Ok(())
    }

    /// Waiting URLs whose time has come go to the back of the queue
    /// (FR-019).
    fn requeue_due_retries(&mut self) -> Result<(), CrawlError> {
        let now = Instant::now();
        while let Some(Reverse((due, id))) = self.retries.peek().copied() {
            if due > now {
                break;
            }
            self.retries.pop();
            if let Some(entry) = self.c.store.requeue_waiting(id)? {
                self.window.on_queued(entry);
            }
        }
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
        let max_concurrent = speed.max_concurrent.min(self.connection_hold.unwrap_or(u32::MAX));
        let robots_mode = self.c.config.robots.mode;
        loop {
            if self.in_flight >= max_concurrent || self.permits_hold_global {
                break;
            }
            self.window.refill(&self.c.store)?;
            let now = Instant::now();
            // Back-pressure (FR-035): while too many renders wait, only
            // external checks start; robots.txt is fetched below as usual.
            let queue_full = self.render.as_mut().is_some_and(RenderStage::queue_full);

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
                self.ensure_robots(key)?;
            }

            // Hosts given up by the circuit breaker: their URLs end as "Host
            // unavailable" without a request (FR-026).
            let given_up: Vec<String> = self
                .window
                .host_heads()
                .filter(|(k, _)| self.hosts.get(*k).is_some_and(|h| h.health.is_given_up()))
                .map(|(k, _)| k.to_string())
                .collect();
            for key in given_up {
                for e in self.window.take_host(&key) {
                    self.c.store.set_host_unavailable(
                        e.id,
                        e.check_only,
                        &format!("{key} was given up after it kept failing; this URL was not requested."),
                        now_ms(),
                    )?;
                    if !e.check_only {
                        self.crawled += 1;
                        self.errors += 1;
                    }
                }
            }

            // The earliest queued URL among hosts that may start now.
            let mut best: Option<QueueKey> = None;
            let mut held_back = false;
            {
                let (mut all_heads, mut check_heads);
                let heads: &mut dyn Iterator<Item = (&str, QueueKey)> = if queue_full {
                    check_heads = self.window.check_heads();
                    &mut check_heads
                } else {
                    all_heads = self.window.host_heads();
                    &mut all_heads
                };
                for (key, head) in heads {
                    let Some(host) = self.hosts.get_mut(key) else {
                        continue;
                    };
                    if !matches!(host.robots, RobotsState::Known(..)) {
                        continue;
                    }
                    host.health.tick(now);
                    if let Some(until) = host.health.paused_until() {
                        soonest(until, &mut wake);
                        held_back = true;
                        continue;
                    }
                    if host.in_flight >= host.limit() {
                        continue;
                    }
                    if host.next_start > now {
                        soonest(host.next_start, &mut wake);
                        held_back = true;
                        continue;
                    }
                    if best.is_none_or(|b| head < b) {
                        best = Some(head);
                    }
                }
            }
            let Some(best) = best else {
                // Every host in the window is paused or waiting: look further
                // down the queue so other hosts keep moving (plan D11).
                if held_back && !self.window.holds_everything() && self.window_cap < MAX_WINDOW {
                    self.window_cap = (self.window_cap * 2).min(MAX_WINDOW);
                    if self.window.grow(&self.c.store, self.window_cap)? {
                        continue;
                    }
                }
                break;
            };
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
            // A signed-in crawl never requests a sign-out link (FR-011).
            if !e.check_only
                && let Some(patterns) = &self.sign_out_guard
                && let Some(p) = crate::net::login::is_sign_out(&url, patterns)
            {
                self.block(e, NotCrawledReason::SessionGuard, format!("matches \"{p}\""))?;
                continue;
            }
            if self.next_global > now {
                if self.c.config.speed.max_urls_per_sec.is_some() {
                    self.rate_held_at = Some(now);
                }
                soonest(self.next_global, &mut wake);
                break;
            }
            self.start(e, robots_rule, now)?;
        }
        Ok(wake)
    }

    /// Reads robots.txt of a host seen for the first time, or marks it
    /// ignored, when a global slot is free for it.
    fn ensure_robots(&mut self, key: String) -> Result<(), CrawlError> {
        let speed = self.c.config.speed;
        if self.c.config.robots.mode == RobotsMode::Ignore {
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
        } else if self.in_flight < speed.max_concurrent {
            let host = self.hosts.entry(key.clone()).or_insert_with(|| Host::new(&speed));
            host.robots = RobotsState::Fetching;
            self.spawn_robots(key);
        }
        Ok(())
    }

    /// Grants every waiting resource permit that may go now (spec 014 D4).
    /// Returns when to look again if one waits on time.
    fn serve_permits(&mut self) -> Result<Option<Instant>, CrawlError> {
        if self.permits.waiting() == 0 {
            self.permits_hold_global = false;
            return Ok(None);
        }
        let speed = self.c.config.speed;
        let mut slots = HostSlots {
            hosts: &mut self.hosts,
            in_flight: &mut self.in_flight,
            next_global: &mut self.next_global,
            max_concurrent: speed.max_concurrent,
            rate: speed.max_urls_per_sec,
        };
        let served = self
            .permits
            .serve(&mut slots, self.c.config.robots.mode, Instant::now());
        self.permits_hold_global = served.waiting_on_global;
        let mut wake = served.wake;
        if !served.need_robots.is_empty() {
            for key in served.need_robots {
                self.ensure_robots(key)?;
            }
            // Hosts marked ignored can be served at once.
            if self.c.config.robots.mode == RobotsMode::Ignore {
                wake = Some(Instant::now());
            }
        }
        Ok(wake)
    }

    /// Finds and launches the browser and records it in the file (spec 014
    /// D1, D12). Only called when the crawl renders.
    async fn start_render(&mut self) -> Result<(), CrawlError> {
        use spiderlens_render::{BrowserSource, FindOptions, GuardCommand, LaunchOptions};
        let info = match self.c.browser.clone() {
            Some(b) => b,
            None => {
                let data_dir = spiderlens_render::data_dir().unwrap_or_else(std::env::temp_dir);
                spiderlens_render::find_browser(&FindOptions {
                    user_path: None,
                    data_dir,
                })
                .map_err(|e| CrawlError::Browser(format!("{e:?}")))?
            }
        };
        let launch = self.c.render_launch.clone().unwrap_or_else(|| LaunchOptions {
            guard: GuardCommand::current_exe().ok(),
            ..LaunchOptions::default()
        });
        let tx = self.tx.clone();
        let net = Arc::new(NetShared {
            client: self.client.clone(),
            headers: self.headers.clone(),
            max_body: self.opts.max_body_bytes,
            cache: std::sync::Mutex::new(ResourceCache::new(DEFAULT_CAPACITY)),
            send: Box::new(move |m| tx.send(Msg::Net(m)).is_ok()),
        });
        let rules = self.c.rules.clone();
        let internal: spiderlens_render::InternalHosts = Arc::new(move |u: &Url| rules.is_internal(u));
        let stage = RenderStage::start(
            &self.c.config,
            self.opts.user_agent.clone(),
            &info,
            launch,
            net,
            internal,
        )
        .await
        .map_err(|e| CrawlError::Browser(format!("{e:?}")))?;
        let source = match info.source {
            BrowserSource::Downloaded => "downloaded",
            BrowserSource::Installed => "installed",
            BrowserSource::UserPath => "user_path",
        };
        // No file path: the file may be shared (data-model.md).
        let renderer = serde_json::json!({
            "name": info.name.label(),
            "version": info.version,
            "source": source,
        });
        self.c.store.set_renderer(&renderer.to_string())?;
        let mut stage = stage;
        let (high, low) = self.c.render_queue_marks;
        stage.init_counts(&self.c.store, high, low)?;
        self.render = Some(stage);
        Ok(())
    }

    /// Records when the fetch stage finished while renders go on
    /// (`meta.fetch_completed_at`, plan D2).
    fn note_fetch_completed(&mut self) -> Result<(), CrawlError> {
        self.window.refill(&self.c.store)?;
        if !(self.window.is_empty()
            && self.window.holds_everything()
            && !self.sitemaps_pending
            && self.retries.is_empty())
        {
            return Ok(());
        }
        if let Some(r) = &mut self.render
            && r.busy(&self.c.store)?
        {
            // The press is done; the bindery goes on (plan D2).
            self.fetch_completed = true;
            self.c
                .store
                .set_meta("fetch_completed_at", &now_ms().to_string())?;
        }
        Ok(())
    }

    /// Starts queued renders up to the stage's limit (spec 014 D2).
    fn dispatch_renders(&mut self) -> Result<(), CrawlError> {
        let Some(stage) = &mut self.render else {
            return Ok(());
        };
        stage.guard_tick();
        stage.restart_if_needed();
        let store = &self.c.store;
        // A browser that cannot be started again: everything still waiting
        // ends as "Browser unavailable", with one warning (FR-046).
        if stage.unavailable() {
            let mut skipped = 0;
            while let Some(e) = stage.next(store)? {
                store.skip_render(e.url_id, "browser_unavailable", None)?;
                stage.on_started();
                skipped += 1;
            }
            if skipped > 0 {
                self.warn(
                    "The browser stopped and could not be started again. The pages still waiting were not rendered."
                        .into(),
                )?;
            }
            return Ok(());
        }
        while stage.open < stage.limit {
            let Some(mut e) = stage.next(store)? else { break };
            if !store.mark_rendering(e.url_id)? {
                continue;
            }
            stage.on_started();
            e.attempts += 1;
            let (Some((status, headers)), Some(raw)) =
                (store.render_main(e.url_id)?, store.render_input(e.url_id)?)
            else {
                store.skip_render(e.url_id, "html_not_stored", None)?;
                continue;
            };
            let tx = self.tx.clone();
            stage.spawn(
                e,
                status,
                headers,
                raw,
                self.opts.robots_token.clone(),
                self.c.config.extraction.store_html,
                self.extract_budget.clone(),
                move |r| {
                    let _ = tx.send(Msg::Rendered(Box::new(r)));
                },
            );
        }
        Ok(())
    }

    /// Records the result of one render, or queues it again once after a
    /// crash (FR-042). References found only after rendering, and a script
    /// navigation, are discovered like raw ones (FR-013, FR-022). Raw rows
    /// are never updated (FR-020).
    fn on_rendered(&mut self, mut r: Rendered) -> Result<(), CrawlError> {
        use spiderlens_render::{ConsoleLevel, FailReason, FailedReason, RenderState};
        let Some(stage) = &mut self.render else {
            return Ok(());
        };
        stage.open -= 1;
        stage.page_done();
        let id = r.entry.url_id;
        if let RenderState::Failed { retryable: true, .. } = r.outcome.state {
            // The browser may have died with the page (FR-042).
            stage.browser_suspect();
        }
        if let RenderState::Failed { retryable: true, .. } = r.outcome.state
            && r.entry.attempts < MAX_ATTEMPTS
        {
            self.c.store.requeue_render(id)?;
            stage.retry(r.entry);
            return Ok(());
        }
        stage.on_done();
        let o = &r.outcome;
        let (status, reason) = match o.state {
            RenderState::Rendered => (RenderStatus::Rendered, None),
            RenderState::TimedOut => (RenderStatus::TimedOut, None),
            RenderState::Failed { reason, .. } => (
                RenderStatus::Failed,
                Some(match reason {
                    FailedReason::Crash => "crash",
                    FailedReason::RendererError => "renderer_error",
                    FailedReason::PageTooLarge => "page_too_large",
                    FailedReason::NavigationError => "navigation_error",
                }),
            ),
        };

        // Each resource's outcome: what the browser saw, plus what only the
        // engine knows (cache, robots.txt, error class).
        let mut notes: VecDeque<(Url, Note)> = std::mem::take(&mut r.notes).into();
        let mut take_note = |url: &Url| {
            let i = notes.iter().position(|(u, _)| u == url)?;
            notes.remove(i).map(|(_, n)| n)
        };
        struct Res {
            url: String,
            kind: String,
            host: String,
            third_party: bool,
            outcome: &'static str,
            status: Option<u16>,
            error: Option<String>,
            robots_rule: Option<String>,
            bytes: u64,
        }
        let (mut failed, mut blocked) = (0u32, 0u32);
        let res: Vec<Res> = o
            .resources
            .iter()
            .map(|x| {
                let note = take_note(&x.url);
                let (outcome, error, robots_rule) = match (&note, x.failed) {
                    (Some(Note::BlockedByRobots(rule)), _) => ("blocked_robots", None, Some(rule.clone())),
                    // An answer of 4xx or 5xx is a failed resource too (FR-018, FR-043).
                    (_, None) if x.status.is_some_and(|c| c >= 400) => {
                        let code = x.status.unwrap_or_default();
                        ("failed", Some(format!("HTTP {code}")), None)
                    }
                    (Some(Note::Cached), None) => ("cached", None, None),
                    (Some(Note::RobotsIgnored(rule)), None) => ("loaded", None, Some(rule.clone())),
                    (_, None) => ("loaded", None, None),
                    (Some(Note::Error(class)), Some(_)) => ("failed", Some(class.clone()), None),
                    (_, Some(FailReason::BlockedByClient)) => ("blocked_setting", None, None),
                    (_, Some(f)) => ("failed", Some(format!("{f:?}")), None),
                };
                match outcome {
                    "failed" => failed += 1,
                    "blocked_setting" | "blocked_robots" => blocked += 1,
                    _ => {}
                }
                Res {
                    url: x.url.to_string(),
                    kind: format!("{:?}", x.kind).to_ascii_lowercase(),
                    host: host_key(&x.url),
                    third_party: x.third_party,
                    outcome,
                    status: x.status,
                    error,
                    robots_rule,
                    bytes: x.bytes,
                }
            })
            .collect();
        let resources: Vec<ResourceRow> = res
            .iter()
            .map(|x| ResourceRow {
                url: &x.url,
                kind: &x.kind,
                host: &x.host,
                third_party: x.third_party,
                outcome: x.outcome,
                status: x.status,
                error: x.error.as_deref(),
                robots_rule: x.robots_rule.as_deref(),
                bytes: x.bytes,
            })
            .collect();
        let console: Vec<ConsoleRow> = o
            .console
            .iter()
            .map(|c| ConsoleRow {
                level: match c.level {
                    ConsoleLevel::Error => "error",
                    ConsoleLevel::Warning => "warning",
                    ConsoleLevel::Exception => "exception",
                },
                message: &c.message,
                source: c.source.as_ref().map(|(u, l, col)| (u.as_str(), *l, *col)),
            })
            .collect();

        // Rendered meta directives with the header ones, which are the same
        // for both versions (FR-019).
        let rendered = r.rendered.as_ref().and_then(|d| d.as_ref().ok());
        let page_url = Url::parse(&r.entry.url).expect("stored URLs parse");
        let (noindex, nofollow, canonicals) = match rendered {
            Some(d) => {
                let dirs: Vec<Directive> = d
                    .directives
                    .iter()
                    .chain(&r.header_facts.directives)
                    .cloned()
                    .collect();
                let (noindex, nofollow) = page_flags(&dirs, &self.opts.robots_token);
                let canonicals: Vec<String> = d
                    .refs
                    .iter()
                    .map(|x| (x, &d.base))
                    .chain(r.header_facts.refs.iter().map(|x| (x, &page_url)))
                    .filter(|(x, _)| x.kind == LinkKind::Canonical)
                    .filter_map(|(x, b)| b.join(x.raw.trim()).ok())
                    .map(|mut u| {
                        u.set_fragment(None);
                        u.to_string()
                    })
                    .collect();
                (noindex, nofollow, canonicals)
            }
            None => (false, false, Vec::new()),
        };
        let mut distinct = canonicals.clone();
        distinct.sort();
        distinct.dedup();
        let raw_only_links: Vec<u32> = r
            .comparison
            .as_ref()
            .map(|c| c.origins.refs.raw_only.iter().map(|&i| i as u32).collect())
            .unwrap_or_default();
        let page = r.rendered.as_ref().map(|d| RenderedPageRecord {
            data: d.as_ref().ok(),
            extract_error: d.as_ref().err().map(String::as_str),
            raw: r.raw.as_ref(),
            comparison: r.comparison.as_ref(),
            noindex,
            nofollow,
            canonical: canonicals.first().map(String::as_str),
            canonical_count: canonicals.len() as u32,
            canonical_conflict: distinct.len() > 1,
            html: r.html.as_ref().map(|(b, n)| (b.as_slice(), *n)),
            raw_only_links: &raw_only_links,
        });
        let js_redirect_id = self.discover_rendered(&r, &page_url, nofollow)?;
        let o = &r.outcome;
        let js_redirect = o.js_redirect.as_ref().map(Url::as_str);
        self.c.store.record_render(
            id,
            &RenderRecord {
                status,
                reason,
                reason_detail: None,
                render_ms: Some(o.render_ms),
                window: Some((o.window.width, o.window.height)),
                js_redirect,
                js_redirect_id,
                console_totals: o.console_totals,
                resources_failed: failed,
                resources_blocked: blocked,
                bot_challenge: r.bot_challenge,
                rendered_at: now_ms(),
                page,
                resources: &resources,
                console: &console,
                screenshot: o
                    .screenshot
                    .as_ref()
                    .map(|s| (s.webp.as_slice(), s.width, s.height)),
            },
        )?;
        self.record_probes(id, &r)?;
        self.finished += 1;
        Ok(())
    }

    /// Stores what the spec 026 probes found on one rendered page: the full
    /// accessibility result or why there is none, and the mobile layout. The
    /// results were parsed on the render task; this only writes rows. A page
    /// never rendered gets nothing (the UI reads a missing state as not
    /// rendered).
    fn record_probes(&self, id: i64, r: &Rendered) -> Result<(), CrawlError> {
        use super::store::MobileRow;
        use spiderlens_render::RenderState;
        let a = &self.c.config.accessibility;
        let store = &self.c.store;
        if a.level == A11yLevel::Full {
            let rendered = matches!(r.outcome.state, RenderState::Rendered);
            let not_checked =
                |reason: &str| store.set_a11y_state(id, "full", "not_checked", Some(reason), None);
            if !rendered {
                not_checked(match r.outcome.state {
                    RenderState::TimedOut => "render_timed_out",
                    _ => "render_failed",
                })?;
            } else if r.rendered.is_none() || r.outcome.html.is_none() {
                not_checked("render_failed")?;
            } else if r.bot_challenge {
                not_checked("bot_challenge")?;
            } else {
                match &r.probe_report.full {
                    Some(FullOutcome::Checked(f)) => {
                        let rows: Vec<_> = f.results.iter().map(|x| x.as_row()).collect();
                        store.record_a11y(id, "full", &rows)?;
                        store.set_a11y_state(id, "full", "checked", None, Some(f.ms))?;
                    }
                    Some(FullOutcome::TimedOut { ms }) => {
                        store.set_a11y_state(id, "full", "timed_out", None, Some(*ms))?;
                    }
                    Some(FullOutcome::NotChecked(reason)) => not_checked(*reason)?,
                    None => not_checked("probe_failed")?,
                }
            }
        }
        if a.mobile
            && a.level != A11yLevel::Off
            && matches!(r.outcome.state, RenderState::Rendered)
            && !r.bot_challenge
        {
            if !r.outcome.window.mobile {
                store.record_mobile(
                    id,
                    &MobileRow {
                        window: Some("desktop"),
                        measured: Some("desktop_window"),
                        ..MobileRow::default()
                    },
                )?;
            } else {
                match &r.probe_report.mobile {
                    Some(MobileOutcome::Measured(m)) => store.record_mobile(
                        id,
                        &MobileRow {
                            window: Some("mobile"),
                            measured: Some("yes"),
                            content_px: Some(m.content_px),
                            window_px: Some(m.window_px),
                            widest: m.widest.as_deref(),
                            small_text_share: Some(m.small_text_share),
                            small_targets: Some(m.small_targets),
                            examples: Some(m.examples.clone()),
                            ..MobileRow::default()
                        },
                    )?,
                    Some(MobileOutcome::TimedOut) => store.record_mobile(
                        id,
                        &MobileRow {
                            window: Some("mobile"),
                            measured: Some("timed_out"),
                            ..MobileRow::default()
                        },
                    )?,
                    _ => {}
                }
            }
        }
        Ok(())
    }

    /// Discovers the references of a rendered page that its raw HTML does
    /// not have, with the page's depth plus one and the rendered page's own
    /// nofollow (FR-022), then its script navigation, like a redirect
    /// (FR-013). Returns the navigation target's row id.
    fn discover_rendered(
        &mut self,
        r: &Rendered,
        page_url: &Url,
        page_nofollow: bool,
    ) -> Result<Option<i64>, CrawlError> {
        let id = r.entry.url_id;
        let depth = r.entry.depth + 1;
        let rendered = r.rendered.as_ref().and_then(|d| d.as_ref().ok());
        // Rendered-only link rows count on from the page's last raw
        // position: HTML references, then header ones (as in `on_page`).
        let mut pos = r.raw.as_ref().map_or(0, |d| d.refs.len()) + r.header_facts.refs.len();
        if let (Some(d), Some(c)) = (rendered, &r.comparison) {
            for (rpos, (x, origin)) in d.refs.iter().zip(&c.origins.refs.rendered).enumerate() {
                if *origin != Origin::Rendered {
                    continue;
                }
                let nofollow = match x.kind {
                    LinkKind::Anchor if x.nofollow => Some(Nofollow::Link),
                    LinkKind::Anchor if page_nofollow => Some(Nofollow::Page),
                    _ => None,
                };
                let parsed = self.c.rules.normalize(&d.base, &x.raw);
                self.rendered_rpos = Some(rpos as u32);
                let found = self.discover(
                    Some(id),
                    &x.raw,
                    x.kind,
                    parsed,
                    depth,
                    nofollow,
                    false,
                    false,
                    Some((pos as u32, x)),
                );
                self.rendered_rpos = None;
                found?;
                pos += 1;
            }
        }
        let mut target_id = None;
        if let Some(target) = &r.outcome.js_redirect {
            let parsed = self.c.rules.normalize(page_url, target.as_str());
            self.rendered_rpos = Some(rendered.map_or(0, |d| d.refs.len()) as u32);
            let found = self.discover(
                Some(id),
                target.as_str(),
                LinkKind::JsRedirect,
                parsed,
                depth,
                None,
                false,
                false,
                None,
            );
            self.rendered_rpos = None;
            target_id = found?;
        }
        // New URLs to fetch: the fetch stage is not done after all.
        if self.fetch_completed && !self.window.is_empty() {
            self.fetch_completed = false;
        }
        Ok(target_id)
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
        host.health.on_start();
        if let Some(d) = host.delay {
            host.next_start = now + d;
        }
        if let Some(rate) = self.c.config.speed.max_urls_per_sec {
            self.next_global = now + Duration::from_secs_f64(1.0 / rate);
        }
        self.in_flight += 1;

        let url = Url::parse(&e.url).expect("stored URLs parse");
        let internal = !e.check_only && self.c.rules.is_internal(&url);
        let net = Arc::clone(&self.net);
        let headers = self.headers.clone();
        let max_body = self.opts.max_body_bytes;
        let token = self.opts.robots_token.clone();
        let tx = self.tx.clone();
        let check_only = e.check_only;
        let budget = self.extract_budget.clone();
        let store_html = self.c.config.extraction.store_html;
        let pool = self.pdf_pool.clone().filter(|_| !check_only);
        let pdf_max = pool
            .as_ref()
            .map(|_| u64::from(self.c.config.extraction.pdf.max_mb) * 1024 * 1024);
        // Rules run on the pages spec 005 extracts for the results: internal ones.
        let rules = self.custom.as_ref().filter(|_| internal).map(Arc::clone);
        // The hook's facts stand in for stored HTML, so it runs only when the
        // HTML is kept (a crawl without it has no content checks either).
        let hook = self.c.page_hook.clone().filter(|_| store_html);
        // Rendering on: an internal page keeps its body for the render input
        // (one `Option` check when it is off, plan D2).
        let keep_html = store_html || (internal && self.render.is_some());
        let content = self.content_rules.clone();
        // Spec 026: one `bool` per page; Off collects and stores nothing.
        let a11y = internal && self.c.config.accessibility.level != A11yLevel::Off;
        tokio::spawn(async move {
            let sent = Instant::now();
            let fetched = fetch_once(&net, &url, &headers, internal, !check_only, max_body, pdf_max).await;
            // Response time ends with the body, before extraction (FR-001).
            let took = sent.elapsed();
            let (id, depth, page_url) = (e.id, e.depth, url.to_string());
            let mut pdf_body = None;
            let result = match fetched {
                Ok(mut o) => {
                    pdf_body = o.pdf.take();
                    let mut outcome =
                        finish_page(o, url, token, keep_html, rules, hook, content, a11y, &budget).await;
                    outcome.pdf = pdf_body.is_some();
                    Ok(outcome)
                }
                Err(e) => Err(e),
            };
            let _ = tx.send(Msg::Page {
                entry: e,
                internal,
                robots_rule,
                took,
                result,
            });
            // The site slot is free now; reading may take a while.
            if let (Some(body), Some(pool)) = (pdf_body, pool) {
                let read = if body.over_limit {
                    pdf::pool::Read {
                        facts: PdfFacts::over_size_limit(pdf_max.unwrap_or_default()),
                        text: None,
                    }
                } else {
                    pool.read(pool.hold(body.bytes)).await
                };
                let _ = tx.send(Msg::Pdf {
                    id,
                    url: page_url,
                    depth,
                    internal,
                    read,
                });
            }
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
        let internal = self.c.rules.is_internal(&origin);
        let headers: Vec<RequestHeader> = self
            .headers
            .iter()
            .filter(|h| h.all_hosts || internal)
            .cloned()
            .collect();
        tokio::spawn(async move {
            let mut attempt = 1;
            let fetch = loop {
                let f = robots::fetch(&client, &origin, &headers).await;
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

    /// Fetches each internal host's AI site files, one at a time per host
    /// and spaced by its crawl-delay, honouring robots.txt for our own token.
    fn start_site_files(&mut self) -> Result<(), CrawlError> {
        let mut wanted: Vec<&'static str> = Vec::new();
        if self.c.config.ai_checks.site_files {
            wanted.extend(site_files::PATHS);
        }
        if self.c.config.agents.entry_points {
            wanted.extend(site_files::AGENT_PATHS);
        }
        if wanted.is_empty() {
            self.site_files = SiteFiles::Done;
            return Ok(());
        }
        let mut jobs = Vec::new();
        for key in self.c.store.internal_hosts()? {
            let host = self.hosts.get(&key);
            let delay = host.and_then(|h| h.delay).unwrap_or(Duration::ZERO);
            let policy = match host.map(|h| &h.robots) {
                Some(RobotsState::Known(p, _)) => Some(p.clone()),
                _ => None,
            };
            // A host the crawl gave up on gets no more requests (spec 004).
            let given_up = host.is_some_and(|h| h.health.is_given_up());
            let paths: Vec<(String, Option<String>)> = wanted
                .iter()
                .map(|path| {
                    if given_up {
                        return (
                            (*path).to_string(),
                            Some("host unavailable: the crawl gave up on it".to_string()),
                        );
                    }
                    let blocked = policy.as_ref().and_then(|p| {
                        let url = Url::parse(&format!("{key}{path}")).ok()?;
                        match p.check(&url) {
                            RobotsVerdict::Disallowed(rule) => Some(format!("blocked by robots.txt: {rule}")),
                            _ => None,
                        }
                    });
                    ((*path).to_string(), blocked)
                })
                .collect();
            jobs.push((key, delay, paths));
        }
        self.site_files = SiteFiles::Pending;
        let client = self.client.clone();
        let tx = self.tx.clone();
        tokio::spawn(async move {
            for (key, delay, paths) in jobs {
                for (i, (path, blocked)) in paths.into_iter().enumerate() {
                    let row = match blocked {
                        Some(why) => SiteFileRow::skipped(&key, &path, &why),
                        None => {
                            if i > 0 && !delay.is_zero() {
                                tokio::time::sleep(delay).await;
                            }
                            site_files::fetch(&client, &key, &path).await
                        }
                    };
                    let _ = tx.send(Msg::SiteFile(row));
                }
            }
            let _ = tx.send(Msg::SiteFilesFinished);
        });
        Ok(())
    }

    fn maybe_start_sitemaps(&mut self) -> Result<(), CrawlError> {
        if !self.c.config.scope.sitemaps.read
            || self.c.store.meta("sitemaps_read")?.as_deref() == Some("true")
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
        let listed: Vec<Url> = policy
            .sitemaps
            .iter()
            .filter_map(|s| Url::parse(s).ok())
            .collect();
        let settings = &self.c.config.scope.sitemaps;
        let mut roots: Vec<SitemapJob> = listed
            .into_iter()
            .map(|u| SitemapJob {
                at: SitemapSource::Url(u),
                source: "robots",
                parent: None,
                level: 0,
            })
            .collect();
        // Spec 011 FR-001: robots.txt lists none, so try the usual places.
        let defaults: Vec<Url> = if roots.is_empty() {
            ["/sitemap.xml", "/sitemap_index.xml"]
                .iter()
                .filter_map(|p| start.join(p).ok())
                .collect()
        } else {
            Vec::new()
        };
        roots.extend(
            settings
                .urls
                .iter()
                .filter_map(|u| Url::parse(u.trim()).ok())
                .map(|u| SitemapJob {
                    at: SitemapSource::Url(u),
                    source: "user",
                    parent: None,
                    level: 0,
                }),
        );
        roots.extend(settings.files.iter().filter_map(|f| {
            Some(SitemapJob {
                at: SitemapSource::Local {
                    path: f.path.clone().into(),
                    base: Url::parse(f.base_url.trim()).ok()?,
                },
                source: "local",
                parent: None,
                level: 0,
            })
        }));
        self.c.store.set_meta("sitemaps_read", "true")?;
        if roots.is_empty() && defaults.is_empty() {
            return Ok(());
        }
        self.sitemaps_pending = true;
        let client = self.client.clone();
        let tx = self.tx.clone();
        tokio::spawn(async move {
            let mut todo: Vec<SitemapJob> = roots.into_iter().rev().collect();
            let mut seen = std::collections::HashSet::new();
            for url in defaults {
                let job = SitemapJob {
                    at: SitemapSource::Url(url),
                    source: "default",
                    parent: None,
                    level: 0,
                };
                if let Some(mut read) = read_sitemap(&client, &job, true).await {
                    seen.insert(job.at.location());
                    expand_index(&mut read, &job, &mut todo);
                    let _ = tx.send(Msg::SitemapFile(Box::new(read)));
                    break;
                }
            }
            while let Some(job) = todo.pop() {
                if !seen.insert(job.at.location()) {
                    continue;
                }
                let Some(mut read) = read_sitemap(&client, &job, false).await else {
                    continue;
                };
                expand_index(&mut read, &job, &mut todo);
                let _ = tx.send(Msg::SitemapFile(Box::new(read)));
            }
            let _ = tx.send(Msg::SitemapsFinished);
        });
        Ok(())
    }

    /// Stores a sitemap file and its entries; URL entries are discovered as
    /// before (spec 003) and matched to their `urls` rows (spec 011 FR-006).
    fn on_sitemap(&mut self, read: SitemapRead) -> Result<(), CrawlError> {
        let SitemapRead {
            mut file,
            parent,
            base,
            entries,
        } = read;
        file.parent_id = parent.as_ref().and_then(|p| self.sitemap_ids.get(p).copied());
        if let Some(e) = &file.error {
            self.warn(format!("Sitemap {} could not be used: {e}", file.location))?;
        }
        self.c.store.upsert_sitemap(&SitemapRow {
            url: file.location.clone(),
            parent,
            url_count: entries.len() as i64,
            error: file.error.clone(),
        })?;
        let file_id = self.c.store.insert_sitemap_file(&file)?;
        self.sitemap_ids.insert(file.location.clone(), file_id);
        let urls = file.kind != Some("index");
        let mut image_urls = Vec::new();
        let mut alternate_urls = Vec::new();
        for e in &entries {
            let (url_id, invalid) = match &base {
                Some(base) if urls => {
                    let parsed = self.c.rules.normalize(base, &e.loc);
                    let invalid = match &parsed {
                        Ok(_) => None,
                        Err(LinkProblem::SelfReference) => Some("no address".to_string()),
                        Err(LinkProblem::Malformed { message, .. }) => Some(message.clone()),
                        Err(LinkProblem::UnsupportedScheme { scheme, .. }) => {
                            Some(format!("{scheme}: address"))
                        }
                        Err(LinkProblem::TooLong { length, .. }) => Some(format!("{length} characters long")),
                    };
                    let id =
                        self.discover(None, &e.loc, LinkKind::Anchor, parsed, 1, None, false, true, None)?;
                    (id, invalid)
                }
                Some(base) => (None, base.join(&e.loc).err().map(|x| x.to_string())),
                None => (None, None),
            };
            image_urls.clear();
            alternate_urls.clear();
            if let Some(base) = &base {
                let norm = |raw: &str| self.c.rules.normalize(base, raw).ok().map(String::from);
                image_urls.extend(e.images.iter().map(|r| norm(r)));
                alternate_urls.extend(e.alternates.iter().map(|a| norm(&a.href)));
            }
            self.c.store.insert_sitemap_entry(
                file_id,
                &SitemapEntryRecord {
                    entry: e,
                    kind: if urls { "url" } else { "sitemap" },
                    url_id,
                    invalid: invalid.as_deref(),
                    image_urls: &image_urls,
                    alternate_urls: &alternate_urls,
                },
            )?;
        }
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
                if matches!(&result, Ok(o) if o.pdf) {
                    self.pdf_pending += 1;
                }
                self.on_result(entry, internal, robots_rule, took, result)
            }
            Msg::Pdf {
                id,
                url,
                depth,
                internal,
                read,
            } => {
                self.pdf_pending -= 1;
                self.on_pdf(id, &url, depth, internal, read)
            }
            Msg::SitemapFile(read) => self.on_sitemap(*read),
            Msg::SitemapsFinished => {
                self.sitemaps_pending = false;
                Ok(())
            }
            Msg::Rendered(r) => self.on_rendered(*r),
            Msg::Net(NetMsg::Permit(r)) => {
                self.permits.push(r);
                Ok(())
            }
            Msg::Net(NetMsg::Done {
                host_key,
                third_party,
                took,
                signal,
            }) => {
                let speed = self.c.config.speed;
                let mut slots = HostSlots {
                    hosts: &mut self.hosts,
                    in_flight: &mut self.in_flight,
                    next_global: &mut self.next_global,
                    max_concurrent: speed.max_concurrent,
                    rate: speed.max_urls_per_sec,
                };
                self.permits
                    .done(&mut slots, &host_key, third_party, took, signal);
                Ok(())
            }
            Msg::SiteFile(row) => Ok(self.c.store.record_site_file(&row)?),
            Msg::SessionCheck { signed_out } => {
                let Some(check) = &mut self.session_check else {
                    return Ok(());
                };
                check.pending = false;
                if !signed_out {
                    check.last_ok_ms = now_ms();
                    return Ok(());
                }
                if !self.session_ended {
                    self.session_ended = true;
                    self.notice(
                        "session_ended",
                        None,
                        None,
                        "The site signed the crawl out, so it paused. Sign in again and resume: pages fetched \
                         since the last good check are fetched again."
                            .into(),
                    )?;
                }
                Ok(())
            }
            Msg::SiteFilesFinished => {
                self.site_files = SiteFiles::Done;
                Ok(())
            }
        }
    }

    /// One finished request: speed signal, circuit breaker, local network
    /// check, then either a retry (the URL waits) or the recorded result
    /// (spec 004 plan D1, D4, D6, D7).
    fn on_result(
        &mut self,
        entry: QueueEntry,
        internal: bool,
        robots_rule: Option<String>,
        took: Duration,
        result: Result<PageOutcome, CheckError>,
    ) -> Result<(), CrawlError> {
        use super::health::HealthState;
        let now = Instant::now();
        let settings = self.c.config.retry;
        let attempt = attempt_of(&result, entry.round_attempts + 1);
        let transient = attempt.is_transient();
        let max_wait = Duration::from_secs_f64(settings.max_wait_secs);

        let (host_blocked, speed_event) = match self.hosts.get_mut(&entry.host_key) {
            Some(h) => {
                h.in_flight -= 1;
                // Retry-After pauses the whole host, up to the maximum wait
                // (FR-018).
                // A lone 503 that will be retried says little about load:
                // treating it as overload halved the host's concurrency for
                // every flaky page (2% flaky pages took a 100k crawl from 34
                // to 64 s). A 503 that is still there after the last
                // attempt, a 429, and any Retry-After still slow the host
                // down; a real outage trips the host breaker (D6).
                let mut signal = signal_of(&result);
                if let Signal::Overloaded { retry_after: None } = signal
                    && matches!(&result, Ok(o) if o.status == 503)
                    && settings.retry_5xx
                    && attempt.round_attempts < settings.attempts
                {
                    signal = Signal::Other;
                }
                let cut_before = h.speed.last_cut();
                let until = h.speed.on_response(took, signal, now);
                if signal == Signal::Answered {
                    h.last_answer = now;
                }
                let cut = h.speed.last_cut().filter(|c| Some(*c) != cut_before);
                let wait = until.map(|u| u.min(now + max_wait).saturating_duration_since(now));
                if let Some(w) = wait {
                    h.next_start = h.next_start.max(now + w);
                }
                let blocked = matches!(
                    h.health.state(),
                    HealthState::Paused { .. } | HealthState::GivenUp
                );
                let typical = (h.speed.typical_ms(), h.speed.baseline_ms());
                (blocked, (cut.map(|(_, c)| c), wait, h.speed.allowance(), typical))
            }
            None => (false, (None, None, 0, (None, None))),
        };
        self.record_speed_event(&entry, &result, speed_event)?;
        // A request that was in flight when its host paused, or when the
        // network went down, does not use an attempt (FR-027).
        if transient && (host_blocked || self.local.is_some()) {
            return self.return_to_queue(entry);
        }

        if is_network_failure(&result) {
            self.net_failures += 1;
            self.net_hosts.insert(entry.host_key.clone());
            if self.net_failures >= NETWORK_DOWN_FAILURES && self.net_hosts.len() >= 2 {
                let message = format!(
                    "Requests to {} different sites failed in a row, so your network connection may be down. \
                     The crawl was paused; no URL was marked as failed. Resume it when you are back online.",
                    self.net_hosts.len()
                );
                self.notice("local_problem", None, None, message.clone())?;
                self.local = Some(LocalError::network_down(&self.c.path, message));
                return self.return_to_queue(entry);
            }
        } else if result.is_ok() {
            self.net_failures = 0;
            self.net_hosts.clear();
        }

        let transition = match self.hosts.get_mut(&entry.host_key) {
            Some(h) if transient => h.health.on_transient_failure(&settings, now),
            Some(h) => h.health.on_alive(),
            None => None,
        };
        if let Some(t) = transition {
            let key = entry.host_key.clone();
            match t {
                Transition::Paused { pause, failures } => {
                    self.site_events.entry(key.clone()).or_default().pauses += 1;
                    let why = if failures > 0 {
                        format!("{failures} requests in a row failed")
                    } else {
                        "its test request failed".to_string()
                    };
                    self.notice(
                        "host_paused",
                        Some(&key),
                        Some(entry.id),
                        format!(
                            "{key} is paused for {} because {why}; other sites keep being crawled.",
                            human_duration(pause)
                        ),
                    )?;
                }
                Transition::Resumed => {
                    self.notice(
                        "host_resumed",
                        Some(&key),
                        Some(entry.id),
                        format!("{key} answers again; crawling it resumes."),
                    )?;
                }
                Transition::GivenUp => {
                    self.notice(
                        "host_given_up",
                        Some(&key),
                        Some(entry.id),
                        format!(
                            "{key} kept failing after {} pauses, so its remaining URLs are recorded as \
                             \"Host unavailable\". Use Retry failed URLs later to try them again.",
                            settings.host_give_up_after
                        ),
                    )?;
                }
            }
        }

        if let Decision::Retry {
            wait,
            retry_after_used,
            capped_from,
        } = retry::decide(&settings, &attempt, retry::jitter(&mut self.rng))
        {
            let attempts = entry.attempts + 1;
            let wall = now_ms();
            self.c.store.set_waiting(
                entry.id,
                wall + wait.as_millis() as i64,
                attempts,
                attempt.round_attempts,
                wall,
            )?;
            self.retries.push(Reverse((now + wait, entry.id)));
            if retry_after_used && let Some(h) = self.hosts.get_mut(&entry.host_key) {
                // No other request to the host before the retry (US2.3).
                h.next_start = h.next_start.max(now + wait);
            }
            let class = describe(&attempt);
            self.c.store.log(
                wall,
                "retry",
                Some(&entry.host_key),
                Some(entry.id),
                &format!(
                    "Attempt {} of {} failed ({class}); trying again in {}{}.",
                    attempt.round_attempts,
                    settings.attempts,
                    human_duration(wait),
                    if retry_after_used {
                        " as the server asked (Retry-After)"
                    } else {
                        ""
                    }
                ),
            )?;
            if let Some(asked) = capped_from {
                self.c.store.log(
                    wall,
                    "retry_after_capped",
                    Some(&entry.host_key),
                    Some(entry.id),
                    &format!(
                        "The server asked to wait {}; the maximum wait of {} was used instead.",
                        human_duration(asked),
                        human_duration(max_wait)
                    ),
                )?;
            }
            return Ok(());
        }
        let failed = attempt.class().is_error() && (attempt.error.is_some() || attempt.round_attempts > 1);
        if failed {
            self.c.store.log(
                now_ms(),
                "failure",
                Some(&entry.host_key),
                Some(entry.id),
                &format!(
                    "Failed after {} attempt{}: {}.",
                    entry.attempts + 1,
                    if entry.attempts == 0 { "" } else { "s" },
                    describe(&attempt)
                ),
            )?;
        }
        *self.crawled_by_host.entry(entry.host_key.clone()).or_default() += 1;
        self.on_page(entry, internal, robots_rule, took, result, attempt.round_attempts)
    }

    /// Logs a cut or a requested wait, and counts it for the site's summary
    /// (spec 008 FR-021).
    fn record_speed_event(
        &mut self,
        entry: &QueueEntry,
        result: &Result<PageOutcome, CheckError>,
        (cut, wait, allowance, (typical, usual)): SpeedEvent,
    ) -> Result<(), CrawlError> {
        let host = entry.host_key.as_str();
        let wall = now_ms();
        if let Some(cut) = cut {
            let events = self.site_events.entry(host.to_string()).or_default();
            let what = match cut {
                Cut::Overloaded => {
                    events.overloads += 1;
                    let status = result.as_ref().map_or(0, |o| o.status);
                    format!("{host} answered {status} (overloaded)")
                }
                Cut::Slower => {
                    events.slowdowns += 1;
                    format!(
                        "{host} got slower ({} ms, usually {} ms)",
                        typical.unwrap_or(0.0).round(),
                        usual.unwrap_or(0.0).round()
                    )
                }
                Cut::TimedOut => {
                    events.slowdowns += 1;
                    format!("A request to {host} timed out")
                }
            };
            let message = format!("{what}; requests to it at once cut to {allowance}.");
            self.c
                .store
                .log(wall, "speed_cut", Some(host), Some(entry.id), &message)?;
        }
        if let Some(w) = wait.filter(|w| !w.is_zero()) {
            self.site_events.entry(host.to_string()).or_default().wait(w);
            let message = format!(
                "{host} asked to wait {} (Retry-After); no requests to it until then.",
                human_duration(w)
            );
            self.c
                .store
                .log(wall, "speed_wait", Some(host), Some(entry.id), &message)?;
        }
        Ok(())
    }

    /// One line per site that slowed down, waited or paused in this run
    /// (FR-021), so a finished crawl can explain why parts of it were slow.
    fn record_speed_summaries(&mut self) -> Result<(), CrawlError> {
        let mut events: Vec<_> = self.site_events.drain().filter(|(_, e)| !e.is_empty()).collect();
        events.sort_by(|a, b| a.0.cmp(&b.0));
        let wall = now_ms();
        for (host, e) in events {
            let message = format!("{host}: {}.", e.summary());
            self.c
                .store
                .log(wall, "speed_summary", Some(&host), None, &message)?;
        }
        Ok(())
    }

    fn return_to_queue(&mut self, entry: QueueEntry) -> Result<(), CrawlError> {
        self.c.store.return_in_flight(entry.id)?;
        self.window.on_queued(entry);
        Ok(())
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
        took: Duration,
        result: Result<PageOutcome, CheckError>,
        round_attempts: u32,
    ) -> Result<(), CrawlError> {
        let mut response = Response {
            robots_rule,
            attempts: entry.attempts + 1,
            round_attempts,
            ..Response::default()
        };
        if !entry.check_only {
            self.crawled += 1;
        }
        let mut outcome = None;
        match result {
            Err(e) => {
                response.error_class = Some(e.class.as_str().into());
                response.error_reason = e.reason.map(ErrorReason::as_str);
                response.error_message = Some(e.message);
            }
            Ok(o) => {
                response.status = Some(o.status);
                response.content_type.clone_from(&o.content_type);
                response.response_ms = Some(u64::try_from(took.as_millis()).unwrap_or(u64::MAX));
                response.transfer_bytes = o.content_length;
                response.size_bytes = o.body_len.or(o.content_length);
                if let Some(p) = &o.body_problem {
                    response.error_class = Some(p.class.as_str().into());
                    response.error_reason = p.reason.map(ErrorReason::as_str);
                    response.error_message = Some(p.message.clone());
                }
                let url = Url::parse(&entry.url).expect("stored URLs parse");
                if let Some(note) = &o.auth {
                    response.reason_detail = Some(note.detail());
                    self.auth_notice(&url, note)?;
                }
                if internal && !entry.check_only {
                    self.tick_session_check();
                }
                // External checks record their redirect target too; it is
                // queued as another check, so the chain ends where it lands.
                if let Some(loc) = o.location.as_deref() {
                    response.redirect_kind = Some("http");
                    let target = self.c.rules.normalize(&url, loc);
                    if target.is_err() && response.error_class.is_none() {
                        // US3.6: the redirect itself is the problem.
                        response.error_class = Some(ErrorClass::RedirectProblem.as_str().into());
                        response.error_reason = Some(ErrorReason::InvalidTarget.as_str());
                        response.error_message =
                            Some(format!("The server redirected to an invalid address: {loc}"));
                    }
                    // Stop following at the hop limit (US3.2, plan D8).
                    let max = self.c.config.network.max_redirects;
                    let before = if entry.source == DiscoverySource::Redirect {
                        self.c.store.redirect_depth(entry.id, max)?
                    } else {
                        0
                    };
                    self.redirect_over_limit = before + 1 > max;
                    let mut seed = false;
                    if entry.source == DiscoverySource::Start
                        && let Ok(t) = &target
                        && t.host_str() != url.host_str()
                        && self.c.rules.adopt_start_redirect(t)
                    {
                        seed = true;
                        self.net.adopt_host(t);
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
                        None,
                    )?;
                    self.redirect_over_limit = false;
                } else {
                    response.resource_type = Some(if o.page.is_some() {
                        "html"
                    } else if o.pdf {
                        "pdf"
                    } else {
                        "other"
                    });
                }

                let data = o.page.as_ref().and_then(|p| p.data.as_ref().ok());
                let meta_dirs: &[Directive] = data.map_or(&[], |d| &d.directives);
                let all_dirs: Vec<Directive> = meta_dirs
                    .iter()
                    .chain(&o.header_facts.directives)
                    .cloned()
                    .collect();
                let (noindex, page_nofollow) = page_flags(&all_dirs, &self.opts.robots_token);
                response.noindex = Some(noindex);
                response.nofollow = Some(page_nofollow);
                if o.pdf && page_nofollow {
                    self.pdf_nofollow.insert(entry.id);
                }
                if let Some(t) = data.and_then(PageData::title).filter(|t| !t.is_empty()) {
                    response.title = Some(t.to_string());
                }

                // HTML references resolve against the base; header ones
                // against the URL itself.
                let base = data.map_or(&url, |d| &d.base).clone();
                let refs: Vec<(&Ref, &Url)> = data
                    .map_or(&[][..], |d| &d.refs[..])
                    .iter()
                    .map(|r| (r, &base))
                    .chain(o.header_facts.refs.iter().map(|r| (r, &url)))
                    // amphtml links are followed only with AMP checks on
                    // (spec 027 FR-E01); otherwise the crawl is unchanged.
                    .filter(|(r, _)| r.kind != LinkKind::AmpHtml || self.c.config.extraction.check_amp)
                    .collect();

                let canonicals: Vec<String> = refs
                    .iter()
                    .filter(|(r, _)| r.kind == LinkKind::Canonical)
                    .filter_map(|(r, b)| b.join(r.raw.trim()).ok())
                    .map(|mut u| {
                        u.set_fragment(None);
                        u.to_string()
                    })
                    .collect();
                if !canonicals.is_empty() {
                    let mut distinct = canonicals.clone();
                    distinct.sort();
                    distinct.dedup();
                    response.canonical = canonicals.first().cloned();
                    response.canonical_count = Some(canonicals.len() as u32);
                    response.canonical_conflict = Some(distinct.len() > 1);
                }

                // A 503 or 429 page is the server's "try later" page, not the
                // page itself: its links are not followed, so a manual retry
                // that gets the real page leaves nothing behind (spec 004).
                let try_later = FinalClass::of_status(o.status).is_transient(Some(o.status), None);
                if internal && !entry.check_only && !try_later {
                    for (pos, (r, b)) in refs.iter().enumerate() {
                        let nofollow = match r.kind {
                            LinkKind::Anchor if r.nofollow => Some(Nofollow::Link),
                            LinkKind::Anchor if page_nofollow => Some(Nofollow::Page),
                            _ => None,
                        };
                        let parsed = self.c.rules.normalize(b, &r.raw);
                        let to = self.discover(
                            Some(entry.id),
                            &r.raw,
                            r.kind,
                            parsed,
                            entry.depth + 1,
                            nofollow,
                            false,
                            false,
                            Some((pos as u32, r)),
                        )?;
                        if r.kind == LinkKind::MetaRefresh && response.redirect_to.is_none() && to.is_some() {
                            response.redirect_to = to;
                            response.redirect_kind = Some("meta_refresh");
                        }
                    }
                }
                outcome = Some(o);
            }
        }
        if !entry.check_only && (response.error_class.is_some() || response.status.is_some_and(|s| s >= 400))
        {
            self.errors += 1;
        }
        self.c
            .store
            .record_response(entry.id, entry.check_only, &response, now_ms())?;
        if let Some(o) = outcome {
            let store = &self.c.store;
            let store_html = self.c.config.extraction.store_html;
            store.record_headers(entry.id, &o.headers)?;
            // The cookie audit costs nothing on responses without cookies.
            if o.headers.iter().any(|(n, _)| n == "set-cookie")
                && let Ok(url) = Url::parse(&entry.url)
            {
                store.record_cookies(
                    entry.id,
                    &super::cookies::from_headers(&url, &o.headers, now_ms()),
                )?;
            }
            let meta_count = match &o.page {
                Some(p) => {
                    store.record_page(
                        entry.id,
                        &PageRecord {
                            data: p.data.as_ref().ok(),
                            extract_error: p.data.as_ref().err().map(String::as_str),
                            html: p
                                .html
                                .as_ref()
                                .filter(|_| store_html)
                                .map(|(b, n)| (b.as_slice(), *n)),
                            header_agent_links: &o.header_facts.agent_links,
                        },
                    )?;
                    if self.c.config.extraction.check_amp
                        && let Some(amp) = p.data.as_ref().ok().and_then(|d| d.amp.as_ref())
                    {
                        store.record_amp(entry.id, amp)?;
                    }
                    if self.c.config.accessibility.level != A11yLevel::Off
                        && let Some(a) = p.data.as_ref().ok().and_then(|d| d.a11y.as_ref())
                    {
                        record_static_a11y(store, entry.id, a)?;
                    }
                    if let Some(c) = &p.custom {
                        store.record_custom(entry.id, c)?;
                    }
                    p.data.as_ref().map_or(0, |d| d.directives.len())
                }
                None => 0,
            };
            store.record_directives(entry.id, &o.header_facts.directives, meta_count)?;
            if let Some(stage) = &mut self.render {
                let html = o.page.is_some();
                match stage.eligibility(&entry.url, internal && !entry.check_only, html, o.status) {
                    Eligibility::No => {}
                    Eligibility::OutsideRules => store.skip_render(entry.id, "outside_rules", None)?,
                    Eligibility::Queue => {
                        // With "Store page HTML" off the input waits in
                        // `render_inputs` (research R8).
                        let input = o
                            .page
                            .as_ref()
                            .and_then(|p| p.html.as_ref())
                            .filter(|_| !store_html)
                            .map(|(b, n)| (b.as_slice(), *n));
                        if store.enqueue_render(entry.id, input)? {
                            stage.on_enqueued();
                        }
                    }
                }
            }
        }
        Ok(())
    }

    /// A link row: `links`, or `rendered_links` while references found
    /// only after rendering are discovered (spec 014 data-model.md).
    fn put_link(&self, link: &NewLink) -> Result<(), CrawlError> {
        match self.rendered_rpos {
            Some(rpos) => self.c.store.add_rendered_link(link, rpos)?,
            None => self.c.store.add_link(link)?,
        }
        Ok(())
    }

    /// Records what was read from a PDF and its link annotations (FR-C03).
    /// Links are followed only from internal PDFs, and only when the crawl
    /// follows PDF links; otherwise they are stored as not followed.
    fn on_pdf(
        &mut self,
        id: i64,
        url: &str,
        depth: u32,
        internal: bool,
        read: pdf::pool::Read,
    ) -> Result<(), CrawlError> {
        let facts = read.facts;
        self.c
            .store
            .record_pdf(id, &facts, read.text.as_ref().map(|(b, n)| (b.as_slice(), *n)))?;
        let page_nofollow = self.pdf_nofollow.remove(&id);
        if !internal {
            return Ok(());
        }
        let base = Url::parse(url).expect("stored URLs parse");
        let follow = self.c.config.extraction.pdf.follow_links;
        for link in &facts.links {
            let r = Ref::new(LinkKind::Anchor, &link.uri, RefSource::Pdf);
            let parsed = self.c.rules.normalize(&base, &link.uri);
            if follow {
                self.discover(
                    Some(id),
                    &link.uri,
                    LinkKind::Anchor,
                    parsed,
                    depth + 1,
                    page_nofollow.then_some(Nofollow::Page),
                    false,
                    false,
                    Some((link.page, &r)),
                )?;
            } else {
                let target = parsed.as_ref().ok().map(Url::to_string);
                self.c.store.add_link(&NewLink {
                    from: id,
                    to: None,
                    raw: &link.uri,
                    kind: LinkKind::Anchor,
                    nofollow: false,
                    page_nofollow,
                    followed: false,
                    pos: Some(link.page),
                    target: target.as_deref().or(Some(&link.uri)),
                    skip_reason: Some("pdf_links_not_followed"),
                    context: Some(&r),
                })?;
            }
        }
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
        context: Option<(u32, &Ref)>,
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
                let mut placement = placement_for(verdict, self.limit_reached);
                if kind == LinkKind::Redirect
                    && self.redirect_over_limit
                    && matches!(placement, Placement::Queue { .. })
                {
                    placement = Placement::NotCrawled {
                        reason: NotCrawledReason::RedirectLimit,
                        detail: Some(format!(
                            "more than {} redirects in a row",
                            self.c.config.network.max_redirects
                        )),
                    };
                }
                // Spec 011 FR-009: with "Crawl URLs found only in sitemaps"
                // off, a sitemap URL waits for a link before it is queued.
                if in_sitemap
                    && internal
                    && !self.c.config.scope.sitemaps.crawl_sitemap_only
                    && matches!(
                        placement,
                        Placement::Queue {
                            check_only: false,
                            ..
                        }
                    )
                {
                    placement = Placement::NotCrawled {
                        reason: NotCrawledReason::SitemapOnly,
                        detail: None,
                    };
                }
                (url.to_string(), host, internal, placement)
            }
            Err(problem) => {
                let Some((stored, Verdict::NotCrawled { reason, detail })) = problem_verdict(&problem) else {
                    // Empty, fragment-only or self links: stored on the
                    // page with a reason, never queued (FR-024).
                    if let Some(from) = from {
                        self.put_link(&NewLink {
                            from,
                            to: Some(from),
                            raw,
                            kind,
                            nofollow: nofollow == Some(Nofollow::Link),
                            page_nofollow: nofollow == Some(Nofollow::Page),
                            followed: false,
                            pos: context.map(|(p, _)| p),
                            target: None,
                            skip_reason: Some(self_reason(raw)),
                            context: context.map(|(_, r)| r),
                        })?;
                    }
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
        let skip_reason = match &placement {
            Placement::NotCrawled {
                reason: NotCrawledReason::MalformedUrl,
                ..
            } => Some("malformed"),
            Placement::NotCrawled {
                reason: NotCrawledReason::UnsupportedScheme,
                ..
            } => Some("unsupported_scheme"),
            _ => None,
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
            self.put_link(&NewLink {
                from,
                to: Some(id),
                raw,
                kind,
                nofollow: link_nofollow,
                page_nofollow,
                followed,
                pos: context.map(|(p, _)| p),
                target: None,
                skip_reason,
                context: context.map(|(_, r)| r),
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

/// The facts of one attempt the retry decision needs (spec 004 plan D1).
fn attempt_of(result: &Result<PageOutcome, CheckError>, round_attempts: u32) -> Attempt {
    match result {
        Ok(o) => Attempt {
            status: Some(o.status),
            error: o.body_problem.as_ref().map(|p| (p.class, p.reason)),
            retry_after: o
                .retry_after
                .as_deref()
                .filter(|_| o.status == 429 || o.status == 503)
                .and_then(|v| parse_retry_after(v, jiff::Timestamp::now())),
            round_attempts,
        },
        Err(e) => Attempt {
            status: None,
            error: Some((e.class, e.reason)),
            retry_after: None,
            round_attempts,
        },
    }
}

/// Failures that say more about the network than about the site (plan D7).
fn is_network_failure(result: &Result<PageOutcome, CheckError>) -> bool {
    matches!(
        result,
        Err(CheckError {
            class: ErrorClass::Connection | ErrorClass::DnsTemporary,
            ..
        }) | Err(CheckError {
            class: ErrorClass::Timeout,
            reason: Some(ErrorReason::Connect),
            ..
        })
    )
}

/// "HTTP 503", "timeout (read)": for log messages.
fn describe(a: &Attempt) -> String {
    match (a.error, a.status) {
        (Some((c, Some(r))), _) => format!("{} ({})", c.as_str(), r.as_str()),
        (Some((c, None)), _) => c.as_str().to_string(),
        (None, Some(s)) => format!("HTTP {s}"),
        (None, None) => "no answer".into(),
    }
}

fn human_duration(d: Duration) -> String {
    let secs = d.as_secs_f64();
    if secs < 10.0 {
        format!("{secs:.1} s")
    } else if secs < 120.0 {
        format!("{} s", secs.round() as u64)
    } else {
        format!("{} min", (secs / 60.0).round() as u64)
    }
}

/// The crawl-delay that applies, unless the user chose to ignore it (FR-027,
/// FR-028).
fn honoured_delay(config: &CrawlConfig, delay: Option<Duration>) -> Option<Duration> {
    delay.filter(|d| config.robots.honour_crawl_delay && !d.is_zero())
}
