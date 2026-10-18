//! A large synthetic site for crawl benchmarks (spec 008, FR-032).
//!
//! `bench-site --pages 100000 [--delay-ms 100] [--max-open 4] [--flaky-per-mille 20] [--port 8700]`
//!
//! Serves `/p/0` .. `/p/{pages-1}` on 127.0.0.1. Every page links to its ten
//! children (a tree, so a breadth-first crawl reaches every page), its parent,
//! three pseudo-random pages and a shared 20-link navigation block, like a
//! real site's menu. Bodies are about 4 KB. Only ever binds to loopback.
//! With `--max-open N`, a request arriving while N are already open gets
//! 429 Too Many Requests, like a rate-limited server (SC-004).
//! With `--flaky-per-mille N`, N pages in 1000 answer 503 the first time
//! they are requested and normally after that (spec 004 retries).
//!
//! With `--pdfs N`, N pages also link to a PDF (`/pdf/0` .. `/pdf/{N-1}`),
//! each about `--pdf-mb` MB (default 5) with 30 pages of text, for the PDF
//! benchmark of spec 027 (SC-C2).
//!
//! With `--spa`, every page is a client-rendered application shell (spec
//! 014, T031): the same title-less HTML for all pages plus one shared,
//! cacheable script of about 200 KB (`/static/app.js`) that fetches the
//! page's content and links as JSON (`/api/{n}.json`) and writes them into
//! the page. The shell also carries the tree links (children and parent) in
//! its raw HTML, so a crawl with rendering off still walks the whole site;
//! the three pseudo-random links exist only in the JSON, so only rendering
//! finds them. Requests for the script and the JSON are not page requests.
//!
//! `GET /__stats` returns what the server saw: requests, peak simultaneous
//! requests and the most requests started in any one second, so caps can be
//! checked from the server side (SC-003). `GET /__reset` clears them. It
//! also returns `pages` (requests for `/p/N`) with the milliseconds since
//! the server started of the first and the last of them (`first_page_ms`,
//! `last_page_ms`), which give the fetch rate as the site saw it (T038).

use std::fmt::Write as _;
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use axum::Router;
use axum::extract::{Path, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;

struct Stats {
    started: Instant,
    requests: AtomicU64,
    too_many: AtomicU64,
    open: AtomicU64,
    peak_open: AtomicU64,
    /// Requests started in the current second, and that second's number.
    this_second: AtomicU64,
    second: AtomicU64,
    peak_per_second: AtomicU64,
    /// Requests for `/p/N`, and when the first and the last came
    /// (milliseconds since start, plus one so zero means none).
    pages: AtomicU64,
    first_page_ms: AtomicU64,
    last_page_ms: AtomicU64,
}

struct App {
    pages: u64,
    pdfs: u64,
    pdf: Vec<u8>,
    delay: Duration,
    max_open: Option<u64>,
    flaky_per_mille: u64,
    /// Application shells instead of plain pages (`--spa`).
    spa: bool,
    /// The shared script of `--spa` pages.
    script: String,
    /// Flaky pages that already failed once.
    failed_once: std::sync::Mutex<std::collections::HashSet<u64>>,
    stats: Stats,
}

fn arg(name: &str) -> Option<u64> {
    let args: Vec<String> = std::env::args().collect();
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .and_then(|v| v.parse().ok())
}

#[tokio::main]
async fn main() {
    let app = Arc::new(App {
        pages: arg("--pages").unwrap_or(100_000),
        pdfs: arg("--pdfs").unwrap_or(0),
        pdf: {
            let mb = arg("--pdf-mb").unwrap_or(5) as usize;
            spiderlens_test_fixtures::pdf::PdfSpec {
                padding: (mb * 1024 * 1024).saturating_sub(64 * 1024),
                ..spiderlens_test_fixtures::pdf::PdfSpec::titled("Benchmark document", 30)
            }
            .build()
        },
        delay: Duration::from_millis(arg("--delay-ms").unwrap_or(0)),
        max_open: arg("--max-open"),
        flaky_per_mille: arg("--flaky-per-mille").unwrap_or(0),
        spa: std::env::args().any(|a| a == "--spa"),
        script: spa_script(),
        failed_once: Default::default(),
        stats: Stats {
            started: Instant::now(),
            requests: AtomicU64::new(0),
            too_many: AtomicU64::new(0),
            open: AtomicU64::new(0),
            peak_open: AtomicU64::new(0),
            this_second: AtomicU64::new(0),
            second: AtomicU64::new(0),
            peak_per_second: AtomicU64::new(0),
            pages: AtomicU64::new(0),
            first_page_ms: AtomicU64::new(0),
            last_page_ms: AtomicU64::new(0),
        },
    });
    let port = arg("--port").unwrap_or(8700) as u16;
    let router = Router::new()
        .route("/p/{n}", get(page))
        .route("/pdf/{n}", get(pdf))
        .route("/static/app.js", get(spa_js))
        .route("/api/{file}", get(spa_json))
        .route("/__stats", get(stats))
        .route("/__reset", get(reset))
        .fallback(|| async { StatusCode::NOT_FOUND })
        .with_state(app.clone());
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let listener = tokio::net::TcpListener::bind(addr).await.expect("bind");
    eprintln!(
        "bench-site: {} pages, {} ms delay, http://{addr}/p/0",
        app.pages,
        app.delay.as_millis()
    );
    axum::serve(listener, router).await.expect("serve");
}

/// Counts one request in, and its end when dropped.
struct Open<'a>(&'a Stats);

impl<'a> Open<'a> {
    /// Also returns how many are open, this one included.
    fn new(s: &'a Stats) -> (Self, u64) {
        s.requests.fetch_add(1, Ordering::Relaxed);
        let open = s.open.fetch_add(1, Ordering::SeqCst) + 1;
        s.peak_open.fetch_max(open, Ordering::Relaxed);
        let sec = s.started.elapsed().as_secs();
        if s.second.swap(sec, Ordering::SeqCst) != sec {
            s.this_second.store(0, Ordering::SeqCst);
        }
        let n = s.this_second.fetch_add(1, Ordering::SeqCst) + 1;
        s.peak_per_second.fetch_max(n, Ordering::Relaxed);
        (Self(s), open)
    }
}

impl Drop for Open<'_> {
    fn drop(&mut self) {
        self.0.open.fetch_sub(1, Ordering::SeqCst);
    }
}

/// A cheap deterministic mix (splitmix64).
fn mix(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9e37_79b9_7f4a_7c15);
    x = (x ^ (x >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    x ^ (x >> 31)
}

async fn page(State(app): State<Arc<App>>, Path(n): Path<u64>) -> Response {
    let (_open, open) = Open::new(&app.stats);
    let at = app.stats.started.elapsed().as_millis() as u64 + 1;
    app.stats.pages.fetch_add(1, Ordering::Relaxed);
    let _ = app
        .stats
        .first_page_ms
        .compare_exchange(0, at, Ordering::Relaxed, Ordering::Relaxed);
    app.stats.last_page_ms.store(at, Ordering::Relaxed);
    if app.max_open.is_some_and(|max| open > max) {
        app.stats.too_many.fetch_add(1, Ordering::Relaxed);
        return (StatusCode::TOO_MANY_REQUESTS, "slow down\n").into_response();
    }
    if !app.delay.is_zero() {
        tokio::time::sleep(app.delay).await;
    }
    if n >= app.pages {
        return StatusCode::NOT_FOUND.into_response();
    }
    if n != 0 && mix(n ^ 0xf1a4) % 1000 < app.flaky_per_mille && app.failed_once.lock().unwrap().insert(n) {
        return (StatusCode::SERVICE_UNAVAILABLE, "try again\n").into_response();
    }
    if app.spa {
        return spa_shell(&app, n);
    }
    let mut html = String::with_capacity(6000);
    let _ = write!(
        html,
        "<!doctype html><html><head><meta charset=\"utf-8\"><title>Page {n}</title>\
         <meta name=\"description\" content=\"Synthetic benchmark page {n}\"></head><body><nav>"
    );
    for i in 0..20.min(app.pages) {
        let _ = write!(html, "<a href=\"/p/{i}\">Section {i}</a> ");
    }
    let _ = write!(html, "</nav><main><h1>Page {n}</h1>");
    for _ in 0..12 {
        html.push_str(
            "<p>Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod tempor \
             incididunt ut labore et dolore magna aliqua. Ut enim ad minim veniam.</p>",
        );
    }
    html.push_str("<ul>");
    if n > 0 {
        let _ = write!(html, "<li><a href=\"/p/{}\">Up</a></li>", (n - 1) / 10);
    }
    for c in (n * 10 + 1)..=(n * 10 + 10) {
        if c < app.pages {
            let _ = write!(html, "<li><a href=\"/p/{c}\">Child {c}</a></li>");
        }
    }
    for k in 0..3 {
        let r = mix(n * 3 + k) % app.pages;
        let _ = write!(html, "<li><a href=\"/p/{r}\">Related {r}</a></li>");
    }
    if app.pdfs > 0 && n % (app.pages / app.pdfs).max(1) == 0 {
        let k = n / (app.pages / app.pdfs).max(1);
        if k < app.pdfs {
            let _ = write!(html, "<li><a href=\"/pdf/{k}\">Report {k}</a></li>");
        }
    }
    html.push_str("</ul></main></body></html>");
    ([(header::CONTENT_TYPE, "text/html; charset=utf-8")], html).into_response()
}

/// The links of page `n` that every crawl finds: its parent and children.
fn tree_links(n: u64, pages: u64) -> Vec<u64> {
    let mut links = Vec::new();
    if n > 0 {
        links.push((n - 1) / 10);
    }
    links.extend(((n * 10 + 1)..=(n * 10 + 10)).filter(|c| *c < pages));
    links
}

/// An application shell: the title is a placeholder, the content comes from
/// the script, and only the tree links are in the HTML.
fn spa_shell(app: &App, n: u64) -> Response {
    let mut html = String::with_capacity(1500);
    let _ = write!(
        html,
        "<!doctype html><html><head><meta charset=\"utf-8\"><title>Loading</title>\
         <script src=\"/static/app.js\" defer></script></head>\
         <body data-page=\"{n}\"><div id=\"app\"><nav>"
    );
    for l in tree_links(n, app.pages) {
        let _ = write!(html, "<a href=\"/p/{l}\">Page {l}</a> ");
    }
    html.push_str("</nav></div></body></html>");
    ([(header::CONTENT_TYPE, "text/html; charset=utf-8")], html).into_response()
}

/// About 200 KB of script, the same for every page.
fn spa_script() -> String {
    let mut js = String::with_capacity(210_000);
    js.push_str(
        "(function(){var n=document.body.getAttribute('data-page');\
         fetch('/api/'+n+'.json').then(function(r){return r.json()}).then(function(d){\
         document.title=d.title;var a=document.getElementById('app');\
         var h='<h1>'+d.heading+'</h1>';\
         for(var i=0;i<d.paragraphs;i++){h+='<p>'+d.text+'</p>'}\
         h+='<ul>';d.links.forEach(function(l){h+='<li><a href=\"/p/'+l+'\">Page '+l+'</a></li>'});\
         a.innerHTML=h+'</ul>';document.body.setAttribute('data-ready','1')})})();\n",
    );
    let mut i = 0u64;
    while js.len() < 200_000 {
        let _ = writeln!(js, "var _v{i}=\"{:016x}{:016x}\";", mix(i), mix(i ^ 0xabcd));
        i += 1;
    }
    js
}

async fn spa_js(State(app): State<Arc<App>>) -> Response {
    let _open = Open::new(&app.stats);
    (
        [
            (header::CONTENT_TYPE, "text/javascript"),
            (header::CACHE_CONTROL, "public, max-age=3600"),
        ],
        app.script.clone(),
    )
        .into_response()
}

/// `/api/{n}.json`: the page's content and all of its links.
async fn spa_json(State(app): State<Arc<App>>, Path(file): Path<String>) -> Response {
    let _open = Open::new(&app.stats);
    let Some(n) = file.strip_suffix(".json").and_then(|n| n.parse::<u64>().ok()) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    if !app.spa || n >= app.pages {
        return StatusCode::NOT_FOUND.into_response();
    }
    let mut links = tree_links(n, app.pages);
    links.extend((0..3).map(|k| mix(n * 3 + k) % app.pages));
    let links: Vec<String> = links.iter().map(u64::to_string).collect();
    let body = format!(
        "{{\"title\":\"App page {n}\",\"heading\":\"Page {n}\",\"paragraphs\":6,\
         \"text\":\"Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod tempor.\",\
         \"links\":[{}]}}",
        links.join(",")
    );
    (
        [
            (header::CONTENT_TYPE, "application/json"),
            (header::CACHE_CONTROL, "no-store"),
        ],
        body,
    )
        .into_response()
}

async fn pdf(State(app): State<Arc<App>>, Path(n): Path<u64>) -> Response {
    let _open = Open::new(&app.stats);
    if n >= app.pdfs {
        return StatusCode::NOT_FOUND.into_response();
    }
    ([(header::CONTENT_TYPE, "application/pdf")], app.pdf.clone()).into_response()
}

async fn stats(State(app): State<Arc<App>>) -> impl IntoResponse {
    let s = &app.stats;
    let body = format!(
        "{{\"requests\":{},\"too_many\":{},\"peak_open\":{},\"peak_per_second\":{},\"pages\":{},\"first_page_ms\":{},\"last_page_ms\":{}}}\n",
        s.requests.load(Ordering::Relaxed),
        s.too_many.load(Ordering::Relaxed),
        s.peak_open.load(Ordering::Relaxed),
        s.peak_per_second.load(Ordering::Relaxed),
        s.pages.load(Ordering::Relaxed),
        s.first_page_ms.load(Ordering::Relaxed),
        s.last_page_ms.load(Ordering::Relaxed),
    );
    ([(header::CONTENT_TYPE, "application/json")], body)
}

async fn reset(State(app): State<Arc<App>>) -> impl IntoResponse {
    let s = &app.stats;
    s.requests.store(0, Ordering::Relaxed);
    s.too_many.store(0, Ordering::Relaxed);
    s.peak_open.store(0, Ordering::Relaxed);
    s.peak_per_second.store(0, Ordering::Relaxed);
    s.pages.store(0, Ordering::Relaxed);
    s.first_page_ms.store(0, Ordering::Relaxed);
    s.last_page_ms.store(0, Ordering::Relaxed);
    "ok\n"
}
