//! T001 spike (spec 014, research R1, R6, R12): drive headless Chromium with
//! `chromiumoxide` 0.9 or `chromey` with `Fetch` interception on every
//! request, every request answered from Rust, against client-rendered pages
//! from `spiderlens-test-fixtures`.
//!
//! ```text
//! cargo run --release -p spiderlens-render --features spike --example spike -- \
//!     --crate oxide|chromey --browser PATH [--shell] --tabs 8 [--pages 2000] \
//!     [--mode fulfil|continue] [--out results.jsonl]
//! ```
//!
//! `--shell` means the browser is `chrome-headless-shell` (old headless);
//! otherwise the full browser runs in new headless mode. `--mode continue`
//! answers subresources with `Fetch.continueRequest` and lets Chrome fetch
//! them itself (mapped to the fixture with `--host-resolver-rules`), to
//! measure what fulfilling every request from Rust costs. In fulfil mode the
//! main document is fulfilled from memory, as the engine will do (plan D3).
//! In continue mode it is continued too: a document fulfilled from memory has
//! no address, Chrome then treats the page as public and fails its requests
//! to 127.0.0.1 with `net::ERR_FAILED` (Local Network Access checks).
//!
//! Prints one JSON line of results and appends it to `--out` if given.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use base64::Engine as _;
use spiderlens_test_fixtures::js_site::{JS_HOST, JsSite, JsSiteBuilder};

mod oxide {
    use cdp_oxide as cdp;
    /// chromiumoxide takes argument keys and adds the leading `--` itself.
    fn browser_arg(a: String) -> String {
        a.trim_start_matches("--").to_string()
    }
    include!("spike/driver.rs");
}

mod chromey {
    use cdp_chromey as cdp;
    /// chromey passes arguments through as written.
    fn browser_arg(a: String) -> String {
        a
    }
    include!("spike/driver.rs");
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    Fulfil,
    Continue,
}

struct Args {
    krate: String,
    browser: PathBuf,
    shell: bool,
    tabs: usize,
    pages: u64,
    mode: Mode,
    out: Option<PathBuf>,
}

fn parse_args() -> Args {
    let args: Vec<String> = std::env::args().collect();
    let get = |name: &str| {
        args.iter()
            .position(|a| a == name)
            .and_then(|i| args.get(i + 1))
            .cloned()
    };
    Args {
        krate: get("--crate").unwrap_or_else(|| "oxide".into()),
        browser: get("--browser").expect("--browser PATH").into(),
        shell: args.iter().any(|a| a == "--shell"),
        tabs: get("--tabs").and_then(|v| v.parse().ok()).unwrap_or(4),
        pages: get("--pages").and_then(|v| v.parse().ok()).unwrap_or(2000),
        mode: match get("--mode").as_deref() {
            Some("continue") => Mode::Continue,
            _ => Mode::Fulfil,
        },
        out: get("--out").map(PathBuf::from),
    }
}

/// The main document, fetched once by "the engine" before the browser asks for it.
pub struct MainDoc {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

/// What to do with one paused subresource request.
pub enum Answer {
    Fulfil {
        status: u16,
        headers: Vec<(String, String)>,
        body_b64: String,
    },
    Continue,
    Fail,
}

/// State shared by the drivers, the interception tasks and the sampler.
pub struct Shared {
    pub mode: Mode,
    pages: u64,
    port: u16,
    client: reqwest::Client,
    next: AtomicU64,
    // Paused requests: open now, peak, and when each one arrived in Rust.
    seq: AtomicU64,
    paused: Mutex<HashMap<u64, Instant>>,
    paused_peak: AtomicU64,
    pub paused_total: AtomicU64,
    over_5s: AtomicU64,
    max_pause_us: AtomicU64,
    pause_us_total: AtomicU64,
    // Fulfil cost.
    b64_bytes: AtomicU64,
    b64_nanos: AtomicU64,
    // Pages.
    done: AtomicU64,
    failed: AtomicU64,
    page_ms: Mutex<Vec<u32>>,
    errors: Mutex<Vec<String>>,
    pub browser_pid: AtomicU32,
    /// Total time per phase: setup, navigate, wait for ready, content, teardown.
    phases: [AtomicU64; 5],
}

/// Ends one paused request's wait when dropped.
pub struct PauseToken<'a> {
    shared: &'a Shared,
    id: u64,
}

impl Drop for PauseToken<'_> {
    fn drop(&mut self) {
        let since = self.shared.paused.lock().expect("lock").remove(&self.id);
        if let Some(since) = since {
            let us = since.elapsed().as_micros() as u64;
            self.shared.pause_us_total.fetch_add(us, Ordering::Relaxed);
            self.shared.max_pause_us.fetch_max(us, Ordering::Relaxed);
            if us > 5_000_000 {
                self.shared.over_5s.fetch_add(1, Ordering::Relaxed);
            }
        }
    }
}

impl Shared {
    pub fn next_page(&self) -> Option<u64> {
        let n = self.next.fetch_add(1, Ordering::Relaxed);
        (n < self.pages).then_some(n)
    }

    pub fn page_url(&self, n: u64) -> String {
        format!("http://{JS_HOST}:{}/app/{n}", self.port)
    }

    pub async fn main_doc(&self, n: u64) -> Result<MainDoc, String> {
        let r = self
            .client
            .get(self.page_url(n))
            .send()
            .await
            .map_err(|e| e.to_string())?;
        let status = r.status().as_u16();
        let headers = headers_of(&r);
        let body = r.bytes().await.map_err(|e| e.to_string())?.to_vec();
        Ok(MainDoc {
            status,
            headers,
            body,
        })
    }

    pub fn pause_begin(&self) -> PauseToken<'_> {
        let id = self.seq.fetch_add(1, Ordering::Relaxed);
        let mut p = self.paused.lock().expect("lock");
        p.insert(id, Instant::now());
        self.paused_peak.fetch_max(p.len() as u64, Ordering::Relaxed);
        self.paused_total.fetch_add(1, Ordering::Relaxed);
        PauseToken { shared: self, id }
    }

    /// Paused right now, and the age of the oldest in milliseconds.
    fn paused_now(&self) -> (u64, u64) {
        let p = self.paused.lock().expect("lock");
        let oldest = p
            .values()
            .map(|t| t.elapsed().as_millis() as u64)
            .max()
            .unwrap_or(0);
        (p.len() as u64, oldest)
    }

    pub fn encode(&self, body: &[u8]) -> String {
        let t = Instant::now();
        let s = base64::engine::general_purpose::STANDARD.encode(body);
        self.b64_nanos
            .fetch_add(t.elapsed().as_nanos() as u64, Ordering::Relaxed);
        self.b64_bytes.fetch_add(s.len() as u64, Ordering::Relaxed);
        s
    }

    /// Answers a subresource: fetched by Rust and fulfilled, or continued.
    pub async fn answer(&self, url: &str) -> Answer {
        if self.mode == Mode::Continue {
            return Answer::Continue;
        }
        match self.client.get(url).send().await {
            Ok(r) => {
                let status = r.status().as_u16();
                let headers = headers_of(&r);
                match r.bytes().await {
                    Ok(body) => Answer::Fulfil {
                        status,
                        headers,
                        body_b64: self.encode(&body),
                    },
                    Err(_) => Answer::Fail,
                }
            }
            Err(_) => Answer::Fail,
        }
    }

    pub fn page_done(&self, n: u64, html: &str, started: Instant) {
        let ok = html.contains(&format!("<title>App page {n}</title>")) && html.contains("data-ready=\"1\"");
        if ok {
            self.done.fetch_add(1, Ordering::Relaxed);
            self.page_ms
                .lock()
                .expect("lock")
                .push(started.elapsed().as_millis() as u32);
        } else {
            self.page_failed(n, "rendered HTML is missing the client-rendered title".into());
        }
    }

    pub fn page_failed(&self, n: u64, e: String) {
        self.failed.fetch_add(1, Ordering::Relaxed);
        let mut errors = self.errors.lock().expect("lock");
        if errors.len() < 10 {
            errors.push(format!("page {n}: {e}"));
        }
    }

    pub fn add_phase(&self, phase: usize, since: Instant) {
        self.phases[phase].fetch_add(since.elapsed().as_micros() as u64, Ordering::Relaxed);
    }

    pub fn note_error(&self, e: String) {
        let mut errors = self.errors.lock().expect("lock");
        if errors.len() < 10 {
            errors.push(e);
        }
    }
}

fn headers_of(r: &reqwest::Response) -> Vec<(String, String)> {
    r.headers()
        .iter()
        .filter(|(n, _)| {
            // The body is already decoded and its length may change.
            !matches!(
                n.as_str(),
                "content-encoding" | "content-length" | "transfer-encoding"
            )
        })
        .filter_map(|(n, v)| Some((n.as_str().to_string(), v.to_str().ok()?.to_string())))
        .collect()
}

/// Resident memory of a process and all its descendants, in bytes.
fn tree_memory(sys: &sysinfo::System, root: u32) -> u64 {
    let root = sysinfo::Pid::from_u32(root);
    let procs = sys.processes();
    let mut total = 0;
    for (pid, p) in procs {
        // On Linux sysinfo also lists threads; their memory is the process's.
        if p.thread_kind().is_some() {
            continue;
        }
        let mut cur = Some(*pid);
        let mut hops = 0;
        while let Some(c) = cur {
            if c == root {
                total += p.memory();
                break;
            }
            cur = procs.get(&c).and_then(|p| p.parent());
            hops += 1;
            if hops > 16 {
                break;
            }
        }
    }
    total
}

#[derive(Default)]
struct Samples {
    /// (seconds since start, paused now, oldest paused ms, tree memory bytes)
    rows: Vec<(f64, u64, u64, u64)>,
}

fn percentile(sorted: &[u32], p: f64) -> u32 {
    if sorted.is_empty() {
        return 0;
    }
    sorted[((sorted.len() - 1) as f64 * p).round() as usize]
}

#[tokio::main]
async fn main() {
    let args = parse_args();
    let site: JsSite = JsSiteBuilder::new().app_pages(args.pages).start();
    let mut client = reqwest::Client::builder()
        .no_proxy()
        .pool_max_idle_per_host(64)
        .timeout(Duration::from_secs(30));
    for (host, addr) in site.resolve() {
        client = client.resolve(&host, addr);
    }
    let shared = Arc::new(Shared {
        mode: args.mode,
        pages: args.pages,
        port: site.port(),
        client: client.build().expect("client"),
        next: AtomicU64::new(0),
        seq: AtomicU64::new(0),
        paused: Mutex::new(HashMap::new()),
        paused_peak: AtomicU64::new(0),
        paused_total: AtomicU64::new(0),
        over_5s: AtomicU64::new(0),
        max_pause_us: AtomicU64::new(0),
        pause_us_total: AtomicU64::new(0),
        b64_bytes: AtomicU64::new(0),
        b64_nanos: AtomicU64::new(0),
        done: AtomicU64::new(0),
        failed: AtomicU64::new(0),
        page_ms: Mutex::new(Vec::new()),
        errors: Mutex::new(Vec::new()),
        browser_pid: AtomicU32::new(0),
        phases: Default::default(),
    });

    let mut browser_args = vec![
        "--no-sandbox".to_string(),
        "--disable-gpu".into(),
        "--disable-dev-shm-usage".into(),
        "--no-proxy-server".into(),
        "--mute-audio".into(),
    ];
    if args.mode == Mode::Continue {
        browser_args.push("--host-resolver-rules=MAP js.test 127.0.0.1, MAP cdn.test 127.0.0.1".into());
    }

    // Sampler: paused requests and browser memory every 250 ms.
    let stop = Arc::new(AtomicBool::new(false));
    let samples = Arc::new(Mutex::new(Samples::default()));
    let start = Instant::now();
    let sampler = {
        let (shared, stop, samples) = (shared.clone(), stop.clone(), samples.clone());
        tokio::task::spawn_blocking(move || {
            let mut sys = sysinfo::System::new();
            while !stop.load(Ordering::Relaxed) {
                let pid = shared.browser_pid.load(Ordering::Relaxed);
                let mem = if pid == 0 {
                    0
                } else {
                    sys.refresh_processes(sysinfo::ProcessesToUpdate::All, true);
                    tree_memory(&sys, pid)
                };
                let (paused, oldest) = shared.paused_now();
                samples
                    .lock()
                    .expect("lock")
                    .rows
                    .push((start.elapsed().as_secs_f64(), paused, oldest, mem));
                std::thread::sleep(Duration::from_millis(250));
            }
        })
    };

    let result = match args.krate.as_str() {
        "oxide" => oxide::run(shared.clone(), &args.browser, args.shell, args.tabs, browser_args).await,
        "chromey" => chromey::run(shared.clone(), &args.browser, args.shell, args.tabs, browser_args).await,
        other => Err(format!("unknown crate {other}")),
    };
    let elapsed = start.elapsed().as_secs_f64();
    stop.store(true, Ordering::Relaxed);
    sampler.await.ok();
    if let Err(e) = &result {
        shared.note_error(format!("run: {e}"));
    }

    let rows = samples.lock().expect("lock").rows.clone();
    let mems: Vec<u64> = rows.iter().map(|r| r.3).filter(|m| *m > 0).collect();
    let peak_mem = mems.iter().copied().max().unwrap_or(0);
    let mut steady: Vec<u64> = mems[mems.len() / 2..].to_vec();
    steady.sort_unstable();
    let steady_mem = steady.get(steady.len() / 2).copied().unwrap_or(0);
    let paused_mean = if rows.is_empty() {
        0.0
    } else {
        rows.iter().map(|r| r.1 as f64).sum::<f64>() / rows.len() as f64
    };
    let oldest_seen = rows.iter().map(|r| r.2).max().unwrap_or(0);
    // Paused requests over time: the most seen in each 10 s window.
    let mut paused_series: Vec<u64> = Vec::new();
    for r in &rows {
        let bucket = (r.0 / 10.0) as usize;
        if paused_series.len() <= bucket {
            paused_series.resize(bucket + 1, 0);
        }
        paused_series[bucket] = paused_series[bucket].max(r.1);
    }
    let mut ms = shared.page_ms.lock().expect("lock").clone();
    ms.sort_unstable();
    let done = shared.done.load(Ordering::Relaxed);
    let paused_total = shared.paused_total.load(Ordering::Relaxed);
    let b64_bytes = shared.b64_bytes.load(Ordering::Relaxed);
    let line = format!(
        "{{\"crate\":\"{}\",\"browser\":\"{}\",\"mode\":\"{:?}\",\"tabs\":{},\"pages\":{},\"done\":{},\"failed\":{},\
         \"secs\":{:.1},\"pages_per_sec\":{:.1},\"page_ms_p50\":{},\"page_ms_p95\":{},\"page_ms_max\":{},\
         \"mem_peak_mb\":{},\"mem_steady_mb\":{},\"paused_total\":{},\"paused_peak\":{},\"paused_mean\":{:.1},\
         \"pause_ms_mean\":{:.2},\"pause_ms_max\":{},\"oldest_paused_sampled_ms\":{},\"paused_over_5s\":{},\"paused_max_per_10s\":{:?},\
         \"b64_mb_per_page\":{:.3},\"b64_encode_ms_total\":{},\"fixture_requests\":{},\"phase_ms_per_page\":{:?},\"errors\":{:?}}}",
        args.krate,
        if args.shell {
            "headless_shell"
        } else {
            "chrome --headless=new"
        },
        args.mode,
        args.tabs,
        args.pages,
        done,
        shared.failed.load(Ordering::Relaxed),
        elapsed,
        done as f64 / elapsed,
        percentile(&ms, 0.5),
        percentile(&ms, 0.95),
        ms.last().copied().unwrap_or(0),
        peak_mem / 1_000_000,
        steady_mem / 1_000_000,
        paused_total,
        shared.paused_peak.load(Ordering::Relaxed),
        paused_mean,
        if paused_total == 0 {
            0.0
        } else {
            shared.pause_us_total.load(Ordering::Relaxed) as f64 / paused_total as f64 / 1000.0
        },
        shared.max_pause_us.load(Ordering::Relaxed) / 1000,
        oldest_seen,
        shared.over_5s.load(Ordering::Relaxed),
        paused_series,
        b64_bytes as f64 / 1e6 / done.max(1) as f64,
        shared.b64_nanos.load(Ordering::Relaxed) / 1_000_000,
        site.requests(),
        shared
            .phases
            .iter()
            .map(|p| p.load(Ordering::Relaxed) / 1000 / done.max(1))
            .collect::<Vec<_>>(),
        shared.errors.lock().expect("lock").clone(),
    );
    println!("{line}");
    if let Some(out) = args.out {
        use std::io::Write as _;
        let mut f = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(out)
            .expect("open --out");
        writeln!(f, "{line}").expect("write --out");
    }
}
