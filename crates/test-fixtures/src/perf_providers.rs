//! Fake page experience providers for spec 024 US3 tests (plan D12). Never
//! contacts the internet. One local server answers the Chrome UX Report API
//! (404 for pages with too little traffic, 75th percentiles as strings for
//! CLS) and PageSpeed Insights v5 (slow answers, categories, audits), checks
//! the API key header, logs every request with its arrival time and the
//! number of PageSpeed calls in flight, and can fail the next request.

use std::collections::{HashMap, VecDeque};
use std::net::{SocketAddr, TcpListener as StdListener};
use std::sync::{Arc, Mutex, mpsc};
use std::thread;
use std::time::{Duration, Instant};

use axum::Router;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use serde_json::{Value, json};

/// The 75th percentiles one target (a page or an origin) has on one device.
#[derive(Debug, Clone, Copy)]
pub struct CruxVitals {
    pub lcp_ms: u64,
    pub inp_ms: u64,
    pub cls: f64,
    pub fcp_ms: u64,
    pub ttfb_ms: u64,
}

impl CruxVitals {
    pub const GOOD: Self = Self {
        lcp_ms: 2100,
        inp_ms: 150,
        cls: 0.05,
        fcp_ms: 1500,
        ttfb_ms: 600,
    };
    pub const POOR: Self = Self {
        lcp_ms: 4800,
        inp_ms: 620,
        cls: 0.31,
        fcp_ms: 3400,
        ttfb_ms: 2000,
    };
}

/// What one PageSpeed test of a page returns.
#[derive(Debug, Clone, Copy)]
pub struct PsiPage {
    pub performance: f64,
    pub accessibility: f64,
    pub best_practices: f64,
    pub seo: f64,
    pub lcp_ms: f64,
    pub cls: f64,
    pub tbt_ms: f64,
    pub speed_index_ms: f64,
}

impl Default for PsiPage {
    fn default() -> Self {
        Self {
            performance: 0.87,
            accessibility: 0.92,
            best_practices: 0.96,
            seo: 0.8,
            lcp_ms: 2412.0,
            cls: 0.043,
            tbt_ms: 120.0,
            speed_index_ms: 3100.0,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct PerfData {
    /// The key the fake accepts in `x-goog-api-key`.
    pub api_key: String,
    /// `(url or origin, "PHONE" or "DESKTOP")` → vitals. Anything not listed answers 404.
    pub crux: HashMap<(String, String), CruxVitals>,
    /// Pages with their own numbers; others get the default.
    pub psi_pages: HashMap<String, PsiPage>,
    /// How long each PageSpeed answer takes.
    pub psi_delay: Duration,
}

/// One request the fake received.
#[derive(Debug, Clone)]
pub struct PerfLogged {
    /// `crux` or `psi`.
    pub api: &'static str,
    /// The page or origin asked about.
    pub target: String,
    /// `PHONE`/`DESKTOP` for CrUX, `mobile`/`desktop` for PageSpeed.
    pub device: String,
    /// `url` or `origin` for CrUX; the requested categories for PageSpeed.
    pub detail: String,
    /// Seconds since the fake started.
    pub at: f64,
}

struct Inner {
    data: PerfData,
    started: Instant,
    log: Vec<PerfLogged>,
    failures: VecDeque<(&'static str, u16, String)>,
    in_flight: usize,
    max_in_flight: usize,
}

type Shared = Arc<Mutex<Inner>>;

pub struct FakePerf {
    addr: SocketAddr,
    inner: Shared,
}

impl FakePerf {
    pub fn start(data: PerfData) -> Self {
        let inner: Shared = Arc::new(Mutex::new(Inner {
            data,
            started: Instant::now(),
            log: Vec::new(),
            failures: VecDeque::new(),
            in_flight: 0,
            max_in_flight: 0,
        }));
        let listener = StdListener::bind("127.0.0.1:0").expect("bind fake port");
        listener.set_nonblocking(true).expect("nonblocking");
        let addr = listener.local_addr().expect("addr");
        let (ready_tx, ready_rx) = mpsc::channel();
        let state = inner.clone();
        thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .enable_all()
                .build()
                .expect("fake runtime");
            rt.block_on(async move {
                let listener = tokio::net::TcpListener::from_std(listener).expect("listener");
                ready_tx.send(()).ok();
                axum::serve(listener, routes(state)).await.ok();
            });
        });
        ready_rx.recv().expect("fake started");
        Self { addr, inner }
    }

    pub fn base(&self) -> String {
        format!("http://{}", self.addr)
    }

    pub fn crux_url(&self) -> String {
        format!("{}/crux/v1", self.base())
    }

    pub fn psi_url(&self) -> String {
        format!("{}/psi/v5", self.base())
    }

    pub fn log(&self) -> Vec<PerfLogged> {
        self.inner.lock().unwrap().log.clone()
    }

    pub fn crux_calls(&self) -> Vec<PerfLogged> {
        self.log().into_iter().filter(|l| l.api == "crux").collect()
    }

    pub fn psi_calls(&self) -> Vec<PerfLogged> {
        self.log().into_iter().filter(|l| l.api == "psi").collect()
    }

    /// The most PageSpeed calls that were being answered at the same time.
    pub fn max_psi_in_flight(&self) -> usize {
        self.inner.lock().unwrap().max_in_flight
    }

    /// The next request to this API (`crux` or `psi`) fails with this status and body.
    pub fn fail_next(&self, api: &'static str, status: u16, body: &str) {
        self.inner
            .lock()
            .unwrap()
            .failures
            .push_back((api, status, body.to_string()));
    }

    pub fn set_psi_delay(&self, delay: Duration) {
        self.inner.lock().unwrap().data.psi_delay = delay;
    }
}

fn routes(state: Shared) -> Router {
    Router::new()
        .route("/crux/v1/records:queryRecord", post(crux))
        .route("/psi/v5/runPagespeed", get(psi))
        .with_state(state)
}

/// Pops an injected failure for `api`, then checks the key.
fn gate(s: &Shared, api: &'static str, headers: &HeaderMap) -> Option<Response> {
    let mut g = s.lock().unwrap();
    if let Some((_, status, body)) = g.failures.pop_front_if(|f| f.0 == api) {
        return Some((StatusCode::from_u16(status).unwrap(), body).into_response());
    }
    let key = headers
        .get("x-goog-api-key")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default();
    if key != g.data.api_key {
        return Some(
            (
                StatusCode::FORBIDDEN,
                axum::Json(json!({"error": {"code": 403, "message": "API key not valid. Please pass a valid API key.", "status": "PERMISSION_DENIED"}})),
            )
                .into_response(),
        );
    }
    None
}

fn push(s: &Shared, api: &'static str, target: &str, device: &str, detail: &str) {
    let mut g = s.lock().unwrap();
    let at = g.started.elapsed().as_secs_f64();
    g.log.push(PerfLogged {
        api,
        target: target.into(),
        device: device.into(),
        detail: detail.into(),
        at,
    });
}

async fn crux(State(s): State<Shared>, headers: HeaderMap, body: String) -> Response {
    let req: Value = serde_json::from_str(&body).unwrap_or_default();
    let (kind, target) = match (req["url"].as_str(), req["origin"].as_str()) {
        (Some(u), _) => ("url", u.to_string()),
        (_, Some(o)) => ("origin", o.to_string()),
        _ => {
            return (
                StatusCode::BAD_REQUEST,
                axum::Json(json!({"error": {"code": 400, "message": "url or origin is required"}})),
            )
                .into_response();
        }
    };
    let device = req["formFactor"].as_str().unwrap_or_default().to_string();
    push(&s, "crux", &target, &device, kind);
    if let Some(r) = gate(&s, "crux", &headers) {
        return r;
    }
    let found = s
        .lock()
        .unwrap()
        .data
        .crux
        .get(&(target.clone(), device.clone()))
        .copied();
    let Some(v) = found else {
        return (
            StatusCode::NOT_FOUND,
            axum::Json(json!({"error": {"code": 404, "message": "chrome ux report data not found", "status": "NOT_FOUND"}})),
        )
            .into_response();
    };
    let metric = |p75: Value| json!({"histogram": [], "percentiles": {"p75": p75}});
    let mut key = serde_json::Map::new();
    key.insert(kind.to_string(), json!(target));
    key.insert("formFactor".to_string(), json!(device));
    axum::Json(json!({"record": {
        "key": key,
        "metrics": {
            "largest_contentful_paint": metric(json!(v.lcp_ms)),
            "interaction_to_next_paint": metric(json!(v.inp_ms)),
            // CLS comes as a string, as in the real API.
            "cumulative_layout_shift": metric(json!(format!("{}", v.cls))),
            "first_contentful_paint": metric(json!(v.fcp_ms)),
            "experimental_time_to_first_byte": metric(json!(v.ttfb_ms)),
        },
        "collectionPeriod": {
            "firstDate": {"year": 2026, "month": 9, "day": 14},
            "lastDate": {"year": 2026, "month": 10, "day": 11}
        }
    }}))
    .into_response()
}

async fn psi(
    State(s): State<Shared>,
    headers: HeaderMap,
    Query(q): Query<Vec<(String, String)>>,
) -> Response {
    let param = |k: &str| {
        q.iter()
            .find(|(n, _)| n == k)
            .map(|(_, v)| v.clone())
            .unwrap_or_default()
    };
    let (url, strategy) = (param("url"), param("strategy"));
    let categories: Vec<String> = q
        .iter()
        .filter(|(n, _)| n == "category")
        .map(|(_, v)| v.clone())
        .collect();
    push(&s, "psi", &url, &strategy, &categories.join(","));
    let delay = {
        let mut g = s.lock().unwrap();
        g.in_flight += 1;
        g.max_in_flight = g.max_in_flight.max(g.in_flight);
        g.data.psi_delay
    };
    if !delay.is_zero() {
        tokio::time::sleep(delay).await;
    }
    let page = {
        let mut g = s.lock().unwrap();
        g.in_flight -= 1;
        g.data.psi_pages.get(&url).copied().unwrap_or_default()
    };
    if let Some(r) = gate(&s, "psi", &headers) {
        return r;
    }
    let wants = |c: &str| categories.iter().any(|x| x == c);
    let mut cats = serde_json::Map::new();
    if wants("PERFORMANCE") {
        cats.insert("performance".into(), json!({"score": page.performance}));
    }
    if wants("ACCESSIBILITY") {
        cats.insert(
            "accessibility".into(),
            json!({"score": page.accessibility, "auditRefs": [
                {"id": "image-alt", "weight": 10}, {"id": "color-contrast", "weight": 7}]}),
        );
    }
    if wants("BEST_PRACTICES") {
        cats.insert(
            "best-practices".into(),
            json!({"score": page.best_practices, "auditRefs": [{"id": "uses-https", "weight": 5}]}),
        );
    }
    if wants("SEO") {
        cats.insert(
            "seo".into(),
            json!({"score": page.seo, "auditRefs": [{"id": "meta-description", "weight": 3}]}),
        );
    }
    let opportunity = |title: &str, ms: f64| json!({"title": title, "score": 0.4, "details": {"type": "opportunity", "overallSavingsMs": ms}});
    axum::Json(json!({
        "id": url,
        "lighthouseResult": {
            "requestedUrl": url,
            "categories": cats,
            "audits": {
                "largest-contentful-paint": {"numericValue": page.lcp_ms},
                "cumulative-layout-shift": {"numericValue": page.cls},
                "total-blocking-time": {"numericValue": page.tbt_ms},
                "speed-index": {"numericValue": page.speed_index_ms},
                "render-blocking-resources": opportunity("Eliminate render-blocking resources", 850.0),
                "unused-javascript": opportunity("Reduce unused JavaScript", 600.0),
                "uses-optimized-images": opportunity("Efficiently encode images", 450.0),
                "uses-text-compression": opportunity("Enable text compression", 300.0),
                "unused-css-rules": opportunity("Reduce unused CSS", 250.0),
                "uses-responsive-images": opportunity("Properly size images", 120.0),
                "image-alt": {"title": "Image elements have `[alt]` attributes", "score": 0,
                    "scoreDisplayMode": "binary",
                    "details": {"type": "table", "headings": [],
                        "items": [{"node": {"snippet": "<img src=\"a.png\">", "selector": "img"}}],
                        "debugData": {"type": "debugdata", "big": true}}},
                "color-contrast": {"title": "Contrast is sufficient", "score": 1, "scoreDisplayMode": "binary"},
                "uses-https": {"title": "Uses HTTPS", "score": 1, "scoreDisplayMode": "binary"},
                "meta-description": {"title": "Document has a meta description", "score": 0,
                    "scoreDisplayMode": "binary"},
                "final-screenshot": {"title": "Final Screenshot", "details": {"type": "screenshot", "data": "AAAA"}}
            }
        }
    }))
    .into_response()
}
