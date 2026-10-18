//! Fake link-data providers for spec 024 tests (plan D12, T040): Ahrefs API
//! v3 Batch Analysis, Majestic `GetIndexItemInfo` and Moz Links API
//! `url_metrics` on one local server. Each one meters its own units (Ahrefs
//! API units, Majestic units, Moz rows), can run out of allowance, logs
//! requests and can inject failures. Never contacts the internet.

use std::collections::{HashMap, VecDeque};
use std::net::{SocketAddr, TcpListener as StdListener};
use std::sync::{Arc, Mutex, mpsc};
use std::thread;

use axum::Router;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use serde_json::{Value, json};

/// The numbers the fakes answer for a URL. Deterministic, so a test can
/// compute what a page should have stored.
#[derive(Debug, Clone, PartialEq)]
pub struct LinkValues {
    pub domain_rating: u64,
    pub url_rating: u64,
    pub backlinks: u64,
    pub refdomains: u64,
    pub trust_flow: u64,
    pub citation_flow: u64,
    pub page_authority: u64,
    pub domain_authority: u64,
    pub spam_score: u64,
}

pub fn expected(url: &str) -> LinkValues {
    // FNV-1a, so the values do not depend on a hasher's version.
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in url.bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    LinkValues {
        domain_rating: 20 + h % 60,
        url_rating: 5 + (h >> 8) % 70,
        backlinks: 10 + (h >> 16) % 5000,
        refdomains: 1 + (h >> 28) % 400,
        trust_flow: (h >> 36) % 80,
        citation_flow: (h >> 44) % 90,
        page_authority: 10 + (h >> 4) % 80,
        domain_authority: 15 + (h >> 12) % 70,
        spam_score: (h >> 20) % 20,
    }
}

/// The keys the fakes accept.
#[derive(Debug, Clone)]
pub struct LinkKeys {
    pub ahrefs: String,
    pub majestic: String,
    /// `ACCESS_ID:SECRET_KEY`.
    pub moz: String,
}

impl Default for LinkKeys {
    fn default() -> Self {
        Self {
            ahrefs: "ahrefs-key".into(),
            majestic: "majestic-key".into(),
            moz: "moz-id:moz-secret".into(),
        }
    }
}

/// One request the fake received. Keys are never logged.
#[derive(Debug, Clone)]
pub struct Logged {
    pub provider: &'static str,
    pub targets: usize,
    pub body: String,
}

struct Inner {
    keys: LinkKeys,
    /// Ahrefs units per row and field; the client's estimate assumes 1.
    ahrefs_rate: u64,
    ahrefs_min: u64,
    allowance: HashMap<&'static str, u64>,
    used: HashMap<&'static str, u64>,
    log: Vec<Logged>,
    failures: VecDeque<(u16, String)>,
}

type Shared = Arc<Mutex<Inner>>;

pub struct FakeLinks {
    addr: SocketAddr,
    inner: Shared,
}

impl FakeLinks {
    pub fn start(keys: LinkKeys) -> Self {
        let inner: Shared = Arc::new(Mutex::new(Inner {
            keys,
            ahrefs_rate: 1,
            ahrefs_min: 50,
            allowance: HashMap::new(),
            used: HashMap::new(),
            log: Vec::new(),
            failures: VecDeque::new(),
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

    pub fn ahrefs_url(&self) -> String {
        format!("{}/ahrefs/v3", self.base())
    }

    pub fn majestic_url(&self) -> String {
        format!("{}/majestic/api/json", self.base())
    }

    pub fn moz_url(&self) -> String {
        format!("{}/moz/v2", self.base())
    }

    /// Ahrefs units per row and field (default 1). The client assumes 1, so a
    /// higher number makes its estimate too low.
    pub fn set_ahrefs_rate(&self, units_per_row_field: u64) {
        self.inner.lock().unwrap().ahrefs_rate = units_per_row_field;
    }

    /// What the plan has left; a request that costs more is refused as out of units.
    pub fn set_allowance(&self, provider: &'static str, units: u64) {
        self.inner.lock().unwrap().allowance.insert(provider, units);
    }

    /// Units or rows a provider (`ahrefs`, `majestic`, `moz`) has charged.
    pub fn used(&self, provider: &str) -> u64 {
        self.inner
            .lock()
            .unwrap()
            .used
            .get(provider)
            .copied()
            .unwrap_or(0)
    }

    pub fn log(&self) -> Vec<Logged> {
        self.inner.lock().unwrap().log.clone()
    }

    /// Requests a provider received.
    pub fn requests(&self, provider: &str) -> usize {
        self.log().iter().filter(|l| l.provider == provider).count()
    }

    /// The next request fails with this status and body.
    pub fn fail_next(&self, status: u16, body: &str) {
        self.inner
            .lock()
            .unwrap()
            .failures
            .push_back((status, body.to_string()));
    }
}

fn routes(state: Shared) -> Router {
    Router::new()
        .route("/ahrefs/v3/batch-analysis", post(ahrefs))
        .route("/majestic/api/json", get(majestic))
        .route("/moz/v2/url_metrics", post(moz))
        .with_state(state)
}

fn record(s: &Shared, provider: &'static str, targets: usize, body: &str) {
    s.lock().unwrap().log.push(Logged {
        provider,
        targets,
        body: body.into(),
    });
}

fn injected(s: &Shared) -> Option<Response> {
    let (status, body) = s.lock().unwrap().failures.pop_front()?;
    Some((StatusCode::from_u16(status).unwrap(), body).into_response())
}

/// Takes `cost` from the plan's allowance and counts it as used; false when
/// the plan cannot pay for it.
fn charge(s: &Shared, provider: &'static str, cost: u64) -> bool {
    let mut g = s.lock().unwrap();
    if let Some(left) = g.allowance.get_mut(provider) {
        if cost > *left {
            return false;
        }
        *left -= cost;
    }
    *g.used.entry(provider).or_insert(0) += cost;
    true
}

fn unauthorized() -> Response {
    (
        StatusCode::UNAUTHORIZED,
        axum::Json(json!({"error": "Invalid API key"})),
    )
        .into_response()
}

async fn ahrefs(State(s): State<Shared>, headers: HeaderMap, body: String) -> Response {
    let req: Value = serde_json::from_str(&body).unwrap_or_default();
    let targets = req["targets"].as_array().cloned().unwrap_or_default();
    record(&s, "ahrefs", targets.len(), &body);
    if let Some(r) = injected(&s) {
        return r;
    }
    let bearer = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .unwrap_or_default()
        .to_string();
    let (key, rate, min) = {
        let g = s.lock().unwrap();
        (g.keys.ahrefs.clone(), g.ahrefs_rate, g.ahrefs_min)
    };
    if bearer != key {
        return unauthorized();
    }
    if targets.len() > 100 {
        return (
            StatusCode::BAD_REQUEST,
            axum::Json(json!({"error": "At most 100 targets per request"})),
        )
            .into_response();
    }
    let select: Vec<String> = req["select"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|v| v.as_str().map(str::to_string))
        .collect();
    let cost = (targets.len() as u64 * select.len() as u64 * rate).max(min);
    if !charge(&s, "ahrefs", cost) {
        return (
            StatusCode::FORBIDDEN,
            axum::Json(json!({"error": "API units quota exceeded for this plan"})),
        )
            .into_response();
    }
    let rows: Vec<Value> = targets
        .iter()
        .enumerate()
        .map(|(i, t)| {
            let v = expected(t["url"].as_str().unwrap_or_default());
            let mut row = serde_json::Map::new();
            row.insert("index".into(), json!(i));
            for f in &select {
                let n = match f.as_str() {
                    "domain_rating" => v.domain_rating,
                    "url_rating" => v.url_rating,
                    "backlinks" => v.backlinks,
                    "refdomains" => v.refdomains,
                    _ => continue,
                };
                row.insert(f.clone(), json!(n));
            }
            Value::Object(row)
        })
        .collect();
    let mut resp = axum::Json(json!({"targets": rows})).into_response();
    resp.headers_mut().insert(
        "x-api-units-cost-total-actual",
        HeaderValue::from_str(&cost.to_string()).unwrap(),
    );
    resp
}

async fn majestic(State(s): State<Shared>, Query(q): Query<HashMap<String, String>>) -> Response {
    let items: usize = q.get("items").and_then(|v| v.parse().ok()).unwrap_or(0);
    let urls: Vec<String> = (0..items)
        .map(|i| q.get(&format!("item{i}")).cloned().unwrap_or_default())
        .collect();
    record(&s, "majestic", items, &urls.join("\n"));
    if let Some(r) = injected(&s) {
        return r;
    }
    let reply = |code: &str, message: &str| {
        axum::Json(json!({"Code": code, "ErrorMessage": message, "FullError": message})).into_response()
    };
    if q.get("app_api_key").map(String::as_str) != Some(s.lock().unwrap().keys.majestic.as_str()) {
        return reply("InvalidAPIKey", "The API key is not valid");
    }
    if q.get("cmd").map(String::as_str) != Some("GetIndexItemInfo") {
        return reply("InvalidCommand", "Unknown command");
    }
    if items > 100 {
        return reply("TooManyItems", "At most 100 items per request");
    }
    if !charge(&s, "majestic", items as u64) {
        return reply(
            "InsufficientIndexItemInfoUnits",
            "Not enough units left on the plan",
        );
    }
    let data: Vec<Value> = urls
        .iter()
        .enumerate()
        .map(|(i, u)| {
            let v = expected(u);
            json!({
                "ItemNum": i, "Item": u, "ResultCode": "OK", "Status": "Found",
                "TrustFlow": v.trust_flow, "CitationFlow": v.citation_flow,
                "ExtBackLinks": v.backlinks, "RefDomains": v.refdomains,
            })
        })
        .collect();
    axum::Json(json!({"Code": "OK", "ErrorMessage": "", "DataTables": {"Results": {"Data": data}}}))
        .into_response()
}

async fn moz(State(s): State<Shared>, headers: HeaderMap, body: String) -> Response {
    let req: Value = serde_json::from_str(&body).unwrap_or_default();
    let targets: Vec<String> = req["targets"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|v| v.as_str().map(str::to_string))
        .collect();
    record(&s, "moz", targets.len(), &body);
    if let Some(r) = injected(&s) {
        return r;
    }
    let sent = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Basic "))
        .and_then(|v| STANDARD.decode(v).ok())
        .and_then(|v| String::from_utf8(v).ok())
        .unwrap_or_default();
    if sent != s.lock().unwrap().keys.moz {
        return unauthorized();
    }
    if targets.len() > 50 {
        return (
            StatusCode::BAD_REQUEST,
            axum::Json(json!({"message": "At most 50 targets per request"})),
        )
            .into_response();
    }
    if !charge(&s, "moz", targets.len() as u64) {
        return (
            StatusCode::FORBIDDEN,
            axum::Json(json!({"message": "Monthly row quota exceeded"})),
        )
            .into_response();
    }
    let results: Vec<Value> = targets
        .iter()
        .map(|u| {
            let v = expected(u);
            json!({
                "page": u, "page_authority": v.page_authority, "domain_authority": v.domain_authority,
                "spam_score": v.spam_score, "root_domains_to_page": v.refdomains,
                "pages_to_page": v.backlinks,
            })
        })
        .collect();
    axum::Json(json!({"results": results})).into_response()
}
