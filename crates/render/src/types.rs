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

#[derive(Clone, Copy, Debug)]
pub struct DownloadProgress {
    pub done_bytes: u64,
    pub total_bytes: u64,
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
    Network(String),
    Io(std::io::Error),
}

/// Options for one browser process.
#[derive(Clone, Copy, Debug)]
pub struct LaunchOptions {
    /// Per-page JavaScript heap limit (`Failed { reason: PageTooLarge }` beyond it).
    pub heap_limit_mb: u32,
    /// Restart the browser after this many pages.
    pub restart_every: u32,
}

impl Default for LaunchOptions {
    fn default() -> Self {
        Self {
            heap_limit_mb: 512,
            restart_every: 1_000,
        }
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
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BlockSet {
    pub images: bool,
    pub media: bool,
    pub fonts: bool,
    pub stylesheets: bool,
    pub third_party: bool,
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
        }
    }
}
