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
//! `GET /__stats` returns what the server saw: requests, peak simultaneous
//! requests and the most requests started in any one second, so caps can be
//! checked from the server side (SC-003). `GET /__reset` clears them.

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
}

struct App {
    pages: u64,
    pdfs: u64,
    pdf: Vec<u8>,
    delay: Duration,
    max_open: Option<u64>,
    flaky_per_mille: u64,
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
        },
    });
    let port = arg("--port").unwrap_or(8700) as u16;
    let router = Router::new()
        .route("/p/{n}", get(page))
        .route("/pdf/{n}", get(pdf))
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
        "{{\"requests\":{},\"too_many\":{},\"peak_open\":{},\"peak_per_second\":{}}}\n",
        s.requests.load(Ordering::Relaxed),
        s.too_many.load(Ordering::Relaxed),
        s.peak_open.load(Ordering::Relaxed),
        s.peak_per_second.load(Ordering::Relaxed),
    );
    ([(header::CONTENT_TYPE, "application/json")], body)
}

async fn reset(State(app): State<Arc<App>>) -> impl IntoResponse {
    let s = &app.stats;
    s.requests.store(0, Ordering::Relaxed);
    s.too_many.store(0, Ordering::Relaxed);
    s.peak_open.store(0, Ordering::Relaxed);
    s.peak_per_second.store(0, Ordering::Relaxed);
    "ok\n"
}
