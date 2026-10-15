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
    pub load_types: Option<Vec<ResourceKind>>, // None: `block` decides (default
                                    // blocks image, media, font); Some: exactly
                                    // these types load, the rest fail with
                                    // BlockedByClient (third_party still applies)
    pub cookies: Vec<SeedCookie>,   // seeds the page's fresh context; empty by default
    pub proxy: Option<String>,      // proxy of the page's fresh context; None by default
    pub probes: Vec<PageProbe>,     // plugin hook, empty by default (see "Probes")
    pub probes_max_total: Duration, // all probes of one page together; default 5 s
}

impl RenderJob {
    /// A job with the defaults above: no probes, `load_types` None, no
    /// cookies, no proxy, `block` = images, media and fonts.
    pub fn new(url: Url, main: MainResponse, user_agent: String, window: Window) -> Self;
}

pub struct SeedCookie { pub name: String, pub value: String, pub domain: String, pub path: String, pub secure: bool, pub http_only: bool, pub expires: Option<f64> }

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
    pub probes: Vec<(String, ProbeOutcome)>, // one per RenderJob.probes entry, same order
}
```

### Probes (plugin hook for specs 025 and 026)

```rust
pub struct PageProbe {
    pub name: String,               // unique within the job; keys the outcome
    pub script: String,             // a JavaScript function expression, called as
                                    // (script)(args) with args parsed from args_json
    pub args_json: String,          // JSON, "null" when the probe takes none
    pub budget: Duration,           // this probe's own timeout
}

pub enum ProbeOutcome {
    Ok(String),                     // the JSON the function returned (awaited if a promise)
    TimedOut { spent: Duration },
    TooLarge,                       // the JSON result passed 4 MB
    Failed(String),                 // the script threw, or the result is not JSON
}
```

- Probes run after the page settled, after the HTML was serialised and after the screenshot, so they can never change `html` or `screenshot`.
- Each probe runs in its own isolated world (`Page.createIsolatedWorld` on the main frame): it sees the DOM but not the page's JavaScript globals, and the page cannot see it.
- Each probe has its own timeout (`budget`), and all probes of one page together stop at `probes_max_total` (default 5 s). A probe that has not started when the total is spent is reported `TimedOut { spent: 0 }`.
- A result larger than 4 MB (`PROBE_RESULT_MAX_BYTES`) is dropped and reported `TooLarge`.
- A probe never changes `RenderOutcome.state`: a probe that times out, fails or is too large leaves a `Rendered` page `Rendered`. Probes do not run on a `Failed` page.
- `RenderJob.probes` is empty by default; with no probes no isolated world is created and the render costs nothing extra.

Phase 1 defines these types only. Running probes, `load_types`, cookies and the context proxy arrive with the render path (T016 onwards).

`ResourceRecord` is filled from what `Network::fetch` returned plus the request type the browser reported; the engine adds the outcome detail it alone knows (cache hit, robots rule) by wrapping the answer.

## Guarantees

- No network access of its own: every request is answered by `RenderJob.main` or `Network::fetch` (research R6). WebSockets are refused.
- No state shared between `render` calls: each gets a new browser context, closed afterwards (FR-011). The context starts empty except for `RenderJob.cookies` and `RenderJob.proxy`, which the caller sets the same way for every page of a crawl (from the crawl's cookie jar and proxy setting), so nothing one page does reaches the next. Because every request is answered by the caller (research R6), the context proxy only matters for requests the browser would make itself; the caller applies the same proxy to its own fetches.
- `render` returns within `max_time` plus 2 s (SC-007), even if the browser hangs: a timer closes the context and, if that does not answer, restarts the browser.
- No `unsafe` code (workspace lint).
