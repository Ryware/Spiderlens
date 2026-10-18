//! Public types of `spiderlens-render` (contracts/render-crate.md).

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use futures_util::future::BoxFuture;
use url::Url;

/// Which browser family was found.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BrowserName {
    ChromeHeadlessShell,
    GoogleChrome,
    MicrosoftEdge,
    Chromium,
}

impl BrowserName {
    /// The name shown to users and stored in `meta.renderer`.
    pub fn label(self) -> &'static str {
        match self {
            Self::ChromeHeadlessShell => "Chrome Headless Shell",
            Self::GoogleChrome => "Google Chrome",
            Self::MicrosoftEdge => "Microsoft Edge",
            Self::Chromium => "Chromium",
        }
    }
}

/// Where the browser came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BrowserSource {
    /// Downloaded by SpiderLens into the shared data folder.
    Downloaded,
    /// Installed on the machine in a standard location.
    Installed,
    /// A path the user gave (`--browser` or the app preference).
    UserPath,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BrowserInfo {
    pub name: BrowserName,
    /// For example `155.0.8059.39`.
    pub version: String,
    pub major: u32,
    pub source: BrowserSource,
    pub path: PathBuf,
}

#[derive(Clone, Debug)]
pub struct FindOptions {
    /// `--browser` or the app preference.
    pub user_path: Option<PathBuf>,
    /// The shared SpiderLens data folder, see [`crate::data_dir`].
    pub data_dir: PathBuf,
}

/// Why no usable browser is available.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NotUsable {
    NotFound,
    TooOld { found: BrowserInfo, min_major: u32 },
    Broken { path: PathBuf, message: String },
}

/// The pinned `chrome-headless-shell` download for one platform (research R14).
#[derive(Clone, Debug)]
pub struct DownloadPlan {
    pub version: &'static str,
    pub platform: &'static str,
    pub url: String,
    pub bytes: u64,
    pub sha256: &'static str,
}

/// What a download is doing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DownloadStage {
    Download,
    Verify,
    Unpack,
}

#[derive(Clone, Copy, Debug)]
pub struct DownloadProgress {
    pub stage: DownloadStage,
    pub done_bytes: u64,
    pub total_bytes: u64,
    /// The connection was lost: the download waits and resumes from
    /// `done_bytes` when it returns (nothing is deleted).
    pub waiting: bool,
}

/// Set by the caller to cancel a download.
#[derive(Clone, Debug, Default)]
pub struct CancellationFlag(Arc<AtomicBool>);

impl CancellationFlag {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.0.store(true, Ordering::Relaxed);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
}

#[derive(Debug)]
pub enum DownloadError {
    /// No pinned download for this platform.
    NoPlan,
    Cancelled,
    /// The archive's SHA-256 differs from the pinned one.
    Checksum,
    /// No connection to the download host.
    Network(String),
    /// The connection stayed lost past the caller's limit. The bytes
    /// already written are kept for a later resume.
    ConnectionLost,
    /// The archive was unpacked but holds no usable browser.
    Unusable(String),
    Io(std::io::Error),
}

impl std::fmt::Display for DownloadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoPlan => f.write_str("No browser download is available for this platform."),
            Self::Cancelled => f.write_str("The download was cancelled."),
            Self::Checksum => f.write_str(
                "The downloaded browser does not match its expected checksum and was removed.",
            ),
            Self::Network(m) => write!(f, "No internet connection: the browser can't be downloaded now ({m})."),
            Self::ConnectionLost => f.write_str(
                "Connection lost. The part already downloaded is kept; run the command again to resume.",
            ),
            Self::Unusable(m) => write!(f, "The downloaded browser cannot be used: {m}"),
            Self::Io(e) => write!(f, "Could not write the browser: {e}"),
        }
    }
}

/// Options for one browser process.
#[derive(Clone, Debug)]
pub struct LaunchOptions {
    /// Per-page JavaScript heap limit (`Failed { reason: PageTooLarge }` beyond it).
    pub heap_limit_mb: u32,
    /// Restart the browser after this many pages.
    pub restart_every: u32,
    /// Start the browser through a guard process that closes it if this
    /// process dies (research R12). `None` starts it directly, which leaves
    /// it running if this process is killed; only tests do that.
    pub guard: Option<GuardCommand>,
}

impl Default for LaunchOptions {
    fn default() -> Self {
        Self {
            heap_limit_mb: 512,
            restart_every: 1_000,
            guard: None,
        }
    }
}

/// How to start the guard: `exe args... --render-guard <pid> <browser> <browser args...>`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GuardCommand {
    pub exe: PathBuf,
    /// Arguments placed before `--render-guard`.
    pub args: Vec<std::ffi::OsString>,
}

impl GuardCommand {
    /// The running executable, which must handle `--render-guard` by calling
    /// [`crate::run_guard`] (the `spiderlens` and desktop binaries do).
    pub fn current_exe() -> std::io::Result<Self> {
        Ok(Self {
            exe: std::env::current_exe()?,
            args: Vec::new(),
        })
    }
}

#[derive(Debug)]
pub enum LaunchError {
    /// The browser is not usable.
    NotUsable(NotUsable),
    /// The process did not start or did not answer.
    Start(String),
}

/// The main document as the crawl received it. The body is decoded, so the
/// headers carry no `Content-Encoding`.
#[derive(Clone, Debug)]
pub struct MainResponse {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

/// The rendering window in CSS pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Window {
    pub width: u32,
    pub height: u32,
    pub mobile: bool,
}

/// Resource types answered with `BlockedByClient` without a network call.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BlockSet {
    pub images: bool,
    pub media: bool,
    pub fonts: bool,
    pub stylesheets: bool,
    pub third_party: bool,
}

impl Default for BlockSet {
    /// The spec's default (FR-014): images, media and fonts are not loaded.
    fn default() -> Self {
        Self {
            images: true,
            media: true,
            fonts: true,
            stylesheets: false,
            third_party: false,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ScreenshotOptions {
    /// At most 8,192 CSS pixels.
    pub max_height: u32,
    /// WebP quality, 60 by default.
    pub quality: u8,
}

impl Default for ScreenshotOptions {
    fn default() -> Self {
        Self {
            max_height: 8_192,
            quality: 60,
        }
    }
}

/// Tells whether a URL belongs to the crawled site.
pub type InternalHosts = Arc<dyn Fn(&Url) -> bool + Send + Sync>;

/// A cookie placed in a page's fresh browser context before it loads.
#[derive(Clone, Debug, PartialEq)]
pub struct SeedCookie {
    pub name: String,
    pub value: String,
    pub domain: String,
    pub path: String,
    pub secure: bool,
    pub http_only: bool,
    /// Seconds since the Unix epoch; `None` for a session cookie.
    pub expires: Option<f64>,
}

/// The largest probe result kept, in bytes of JSON.
pub const PROBE_RESULT_MAX_BYTES: usize = 4 * 1024 * 1024;

/// Default for [`RenderJob::probes_max_total`].
pub const PROBES_MAX_TOTAL_DEFAULT: Duration = Duration::from_secs(5);

/// A script run on the settled page in an isolated world (plugin hook for
/// specs 025 and 026). See `contracts/render-crate.md`, "Probes".
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PageProbe {
    /// Unique within the job; keys the outcome.
    pub name: String,
    /// A JavaScript function expression, called with the parsed `args_json`.
    pub script: String,
    /// JSON; `"null"` when the probe takes no arguments.
    pub args_json: String,
    /// This probe's own timeout.
    pub budget: Duration,
}

/// What one probe produced. A probe never changes the page's [`RenderState`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProbeOutcome {
    /// The JSON the function returned.
    Ok(String),
    /// The probe's own budget or the page's total probe time ran out.
    TimedOut { spent: Duration },
    /// The result passed [`PROBE_RESULT_MAX_BYTES`].
    TooLarge,
    /// The script threw, or its result was not JSON.
    Failed(String),
}

/// One page to render.
#[derive(Clone)]
pub struct RenderJob {
    pub url: Url,
    pub main: MainResponse,
    pub user_agent: String,
    pub window: Window,
    /// 1 to 60 s.
    pub max_time: Duration,
    pub min_wait: Duration,
    pub block: BlockSet,
    pub internal_hosts: InternalHosts,
    pub screenshot: Option<ScreenshotOptions>,
    /// `None`: `block` decides. `Some`: exactly these types load and every
    /// other type fails with `BlockedByClient`; `block.third_party` still
    /// applies.
    pub load_types: Option<Vec<ResourceKind>>,
    /// Cookies seeded into the page's fresh context (the crawl's cookie jar).
    pub cookies: Vec<SeedCookie>,
    /// Proxy of the page's fresh context (the crawl's proxy setting).
    pub proxy: Option<String>,
    /// Probes run after settle, serialisation and screenshot. Empty by default.
    pub probes: Vec<PageProbe>,
    /// All probes of one page together; [`PROBES_MAX_TOTAL_DEFAULT`] by default.
    pub probes_max_total: Duration,
}

impl RenderJob {
    /// A job with the defaults: 10 s cap, no minimum wait, images, media and
    /// fonts blocked, only the page's own host internal, no screenshot, no
    /// probes, no cookies, no proxy.
    pub fn new(url: Url, main: MainResponse, user_agent: String, window: Window) -> Self {
        let host = url.host_str().map(str::to_owned);
        Self {
            url,
            main,
            user_agent,
            window,
            max_time: Duration::from_secs(10),
            min_wait: Duration::ZERO,
            block: BlockSet::default(),
            internal_hosts: Arc::new(move |u: &Url| u.host_str() == host.as_deref()),
            screenshot: None,
            load_types: None,
            cookies: Vec::new(),
            proxy: None,
            probes: Vec::new(),
            probes_max_total: PROBES_MAX_TOTAL_DEFAULT,
        }
    }
}

/// Answers every request the page makes except the main document.
/// Implemented by the engine (permits, robots.txt, cache, recording).
pub trait Network: Send + Sync {
    fn fetch(&self, req: ResourceRequest) -> BoxFuture<'_, ResourceAnswer>;
}

/// The request type the browser reported.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResourceKind {
    Document,
    Script,
    Stylesheet,
    Image,
    Media,
    Font,
    Xhr,
    Fetch,
    Ping,
    CspViolationReport,
    Other,
}

#[derive(Clone, Debug)]
pub struct ResourceRequest {
    pub url: Url,
    pub method: String,
    pub headers: Vec<(String, String)>,
    pub body: Option<Vec<u8>>,
    pub kind: ResourceKind,
    pub third_party: bool,
}

/// Why a request was failed instead of answered.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FailReason {
    BlockedByClient,
    Failed,
    TimedOut,
    ConnectionRefused,
    NameNotResolved,
    Aborted,
}

#[derive(Clone, Debug)]
pub enum ResourceAnswer {
    Response {
        status: u16,
        headers: Vec<(String, String)>,
        body: Vec<u8>,
    },
    Fail {
        reason: FailReason,
    },
}

/// Why a render failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FailedReason {
    Crash,
    RendererError,
    PageTooLarge,
    NavigationError,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RenderState {
    Rendered,
    TimedOut,
    Failed { reason: FailedReason, retryable: bool },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConsoleLevel {
    Error,
    Warning,
    /// An uncaught exception.
    Exception,
}

#[derive(Clone, Debug)]
pub struct ConsoleEntry {
    pub level: ConsoleLevel,
    pub message: String,
    /// Script URL, line and column, when the browser reported them.
    pub source: Option<(String, u32, u32)>,
}

/// What the page asked for and what it got, in request order.
#[derive(Clone, Debug)]
pub struct ResourceRecord {
    pub url: Url,
    pub kind: ResourceKind,
    pub third_party: bool,
    /// The status of a response, `None` for a failed request.
    pub status: Option<u16>,
    pub failed: Option<FailReason>,
    pub bytes: u64,
}

#[derive(Clone, Debug)]
pub struct Screenshot {
    /// WebP bytes.
    pub webp: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

/// The result of one render. Every problem is a state, never a panic.
#[derive(Clone, Debug)]
pub struct RenderOutcome {
    pub state: RenderState,
    /// The serialised DOM including the doctype (`None` only if failed).
    pub html: Option<String>,
    pub js_redirect: Option<Url>,
    /// At most 50.
    pub console: Vec<ConsoleEntry>,
    /// Errors and warnings in total.
    pub console_totals: (u32, u32),
    pub resources: Vec<ResourceRecord>,
    pub render_ms: u32,
    pub window: Window,
    pub screenshot: Option<Screenshot>,
    /// One entry per [`RenderJob::probes`] entry, in the same order.
    pub probes: Vec<(String, ProbeOutcome)>,
}

impl RenderOutcome {
    /// A failure with nothing rendered.
    pub fn failed(reason: FailedReason, retryable: bool, window: Window) -> Self {
        Self {
            state: RenderState::Failed { reason, retryable },
            html: None,
            js_redirect: None,
            console: Vec::new(),
            console_totals: (0, 0),
            resources: Vec::new(),
            render_ms: 0,
            window,
            screenshot: None,
            probes: Vec::new(),
        }
    }
}
