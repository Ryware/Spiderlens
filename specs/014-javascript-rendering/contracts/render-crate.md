# Contract: `spiderlens-render` (crates/render)

A library with no knowledge of crawls, crawl files or the UI. It depends on nothing in this workspace. `spiderlens-engine` is its only workspace user. Names below are the public API; private modules may change freely.

## Finding and getting a browser

```rust
pub struct BrowserInfo {
    pub name: BrowserName,          // ChromeHeadlessShell | GoogleChrome | MicrosoftEdge | Chromium
    pub version: String,            // "155.0.8059.39"
    pub major: u32,
    pub source: BrowserSource,      // Downloaded | Installed | UserPath
    pub path: PathBuf,
}

pub struct FindOptions {
    pub user_path: Option<PathBuf>, // --browser or the app preference
    pub data_dir: PathBuf,          // shared SpiderLens data folder, see data_dir()
}

/// User path, then a previous download, then installed Chrome, Chromium, Edge.
/// Runs `<browser> --version` (or reads the version resource on Windows).
pub fn find_browser(opts: &FindOptions) -> Result<BrowserInfo, NotUsable>;

pub enum NotUsable {
    NotFound,
    TooOld { found: BrowserInfo, min_major: u32 },
    Broken { path: PathBuf, message: String },
}

/// The SpiderLens data folder shared by the CLI and the desktop app:
/// Windows %LOCALAPPDATA%\com.ryware.spiderlens, macOS
/// ~/Library/Application Support/com.ryware.spiderlens, Linux
/// $XDG_DATA_HOME/com.ryware.spiderlens (Tauri's app_data_dir for the
/// app's identifier, so both binaries agree).
pub fn data_dir() -> Option<PathBuf>;

pub struct DownloadPlan { pub version: &'static str, pub platform: &'static str, pub url: String, pub bytes: u64, pub sha256: &'static str }

/// The pinned download for this platform, or None where Chrome for Testing has none.
pub fn download_plan() -> Option<DownloadPlan>;

/// Downloads, verifies and unpacks into `<data_dir>/browsers/`. The caller
/// must have the user's consent (FR-045). Partial files are removed on error
/// or cancel.
pub async fn download(
    plan: &DownloadPlan,
    data_dir: &Path,
    progress: &(dyn Fn(DownloadProgress) + Send + Sync),
    cancel: &CancellationFlag,
) -> Result<BrowserInfo, DownloadError>;

pub fn remove_download(data_dir: &Path) -> std::io::Result<()>;
```

## Running it

```rust
pub struct LaunchOptions {
    pub heap_limit_mb: u32,         // per-page JavaScript heap (page too large)
    pub restart_every: u32,         // pages; default 1,000
}

pub struct Browser { /* one process */ }

impl Browser {
    pub async fn launch(info: &BrowserInfo, opts: LaunchOptions) -> Result<Self, LaunchError>;
    /// Renders one page in a fresh browser context. Never panics; every
    /// problem is a RenderOutcome state.
    pub async fn render(&self, job: RenderJob, net: Arc<dyn Network>) -> RenderOutcome;
    /// Resident memory of the browser process tree, in bytes.
    pub fn memory_bytes(&self) -> u64;
    pub async fn restart(&mut self) -> Result<(), LaunchError>;
    /// Closes the process and deletes its temporary profile. Also on Drop.
    pub async fn close(self);
}

/// The guard process entry point (research R12). Called by `spiderlens`
/// and the desktop binary for the hidden `--render-guard <pid>` argument.
pub fn run_guard(parent_pid: u32, browser_args: Vec<OsString>) -> !;
```

## One page

```rust
pub struct RenderJob {
    pub url: Url,
    pub main: MainResponse,         // status, headers (decoded body, so no
                                    // Content-Encoding), body bytes
    pub user_agent: String,
    pub window: Window,             // width, height, mobile
    pub max_time: Duration,         // 1 to 60 s
    pub min_wait: Duration,
    pub block: BlockSet,            // images, media, fonts, stylesheets, third_party
    pub internal_hosts: Arc<dyn Fn(&Url) -> bool + Send + Sync>,
    pub screenshot: Option<ScreenshotOptions>, // max_height 8,192, WebP q60
}

/// Answers every request the page makes except the main document.
/// Implemented by the engine (permits, robots.txt, cache, recording).
pub trait Network: Send + Sync {
    fn fetch(&self, req: ResourceRequest) -> BoxFuture<'_, ResourceAnswer>;
}

pub struct ResourceRequest { pub url: Url, pub method: String, pub headers: Vec<(String, String)>, pub body: Option<Vec<u8>>, pub kind: ResourceKind, pub third_party: bool }

pub enum ResourceAnswer {
    Response { status: u16, headers: Vec<(String, String)>, body: Vec<u8> },
    Fail { reason: FailReason },    // BlockedByClient, Failed, TimedOut, ...
}

pub struct RenderOutcome {
    pub state: RenderState,         // Rendered | TimedOut | Failed { reason, retryable }
    pub html: Option<String>,       // serialised DOM incl. doctype (None only if Failed)
    pub js_redirect: Option<Url>,
    pub console: Vec<ConsoleEntry>, // at most 50
    pub console_totals: (u32, u32), // errors, warnings
    pub resources: Vec<ResourceRecord>, // what the page asked for, in order
    pub render_ms: u32,
    pub window: Window,
    pub screenshot: Option<Screenshot>,
}
```

`ResourceRecord` is filled from what `Network::fetch` returned plus the request type the browser reported; the engine adds the outcome detail it alone knows (cache hit, robots rule) by wrapping the answer.

## Guarantees

- No network access of its own: every request is answered by `RenderJob.main` or `Network::fetch` (research R6). WebSockets are refused.
- No state shared between `render` calls: each gets a new browser context, closed afterwards (FR-011).
- `render` returns within `max_time` plus 2 s (SC-007), even if the browser hangs: a timer closes the context and, if that does not answer, restarts the browser.
- No `unsafe` code (workspace lint).
