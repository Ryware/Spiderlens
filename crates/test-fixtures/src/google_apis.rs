//! Fake Google URL Inspection and Analytics services for spec 024 tests
//! (tasks T028 and T037, plan D12). Never contacts the internet. They check
//! the bearer token only by its shape (`access-…`, as `FakeGoogle` issues
//! them), so tests sign in through `FakeGoogle` and call these for data.

use std::collections::{BTreeMap, HashMap, VecDeque};
use std::net::{SocketAddr, TcpListener as StdListener};
use std::sync::{Arc, Mutex, mpsc};
use std::thread;

use axum::Router;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use serde_json::{Value, json};

use crate::providers::Logged;

fn spawn(router: Router) -> SocketAddr {
    let listener = StdListener::bind("127.0.0.1:0").expect("bind fake port");
    listener.set_nonblocking(true).expect("nonblocking");
    let addr = listener.local_addr().expect("addr");
    let (ready_tx, ready_rx) = mpsc::channel();
    thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .expect("fake runtime");
        rt.block_on(async move {
            let listener = tokio::net::TcpListener::from_std(listener).expect("listener");
            ready_tx.send(()).ok();
            axum::serve(listener, router).await.ok();
        });
    });
    ready_rx.recv().expect("fake started");
    addr
}

fn authorised(headers: &HeaderMap) -> bool {
    headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .is_some_and(|t| t.starts_with("access-"))
}

fn error(status: u16, message: &str, google_status: &str) -> Response {
    (
        StatusCode::from_u16(status).expect("status"),
        axum::Json(json!({"error": {"code": status, "message": message, "status": google_status}})),
    )
        .into_response()
}

fn unauthorised() -> Response {
    error(401, "Request had invalid authentication credentials.", "UNAUTHENTICATED")
}

// ---------------------------------------------------------------- URL Inspection

/// What the fake answers for one inspected URL.
#[derive(Debug, Clone)]
pub struct InspectionAnswer {
    pub verdict: String,
    pub coverage_state: String,
    pub indexing_state: String,
    pub robots_state: String,
    pub page_fetch_state: String,
    pub google_canonical: Option<String>,
    pub user_canonical: Option<String>,
    /// RFC 3339.
    pub last_crawl: String,
    pub crawled_as: String,
}

impl InspectionAnswer {
    /// An indexed page whose canonical is itself.
    pub fn indexed(url: &str) -> Self {
        Self {
            verdict: "PASS".into(),
            coverage_state: "Submitted and indexed".into(),
            indexing_state: "INDEXING_ALLOWED".into(),
            robots_state: "ALLOWED".into(),
            page_fetch_state: "SUCCESSFUL".into(),
            google_canonical: Some(url.into()),
            user_canonical: Some(url.into()),
            last_crawl: "2026-10-12T08:30:00Z".into(),
            crawled_as: "MOBILE".into(),
        }
    }

    /// A page Google chose not to index.
    pub fn not_indexed(url: &str) -> Self {
        Self {
            verdict: "NEUTRAL".into(),
            coverage_state: "Crawled - currently not indexed".into(),
            google_canonical: None,
            ..Self::indexed(url)
        }
    }

    /// A page Google indexed under another canonical.
    pub fn canonical_elsewhere(url: &str, canonical: &str) -> Self {
        Self {
            google_canonical: Some(canonical.into()),
            ..Self::indexed(url)
        }
    }

    /// A page robots.txt keeps Google from fetching.
    pub fn blocked_by_robots(url: &str) -> Self {
        Self {
            verdict: "NEUTRAL".into(),
            coverage_state: "Blocked by robots.txt".into(),
            robots_state: "DISALLOWED".into(),
            page_fetch_state: "BLOCKED_ROBOTS_TXT".into(),
            google_canonical: None,
            ..Self::indexed(url)
        }
    }
}

#[derive(Default)]
struct InspectionInner {
    sites: Vec<String>,
    allowance: u64,
    used: u64,
    day: u32,
    answers: HashMap<String, InspectionAnswer>,
    inspected: Vec<String>,
    log: Vec<Logged>,
    failures: VecDeque<(u16, String)>,
}

type InspectionShared = Arc<Mutex<InspectionInner>>;

/// `urlInspection.index:inspect` with a daily allowance per site and its own
/// day counter: [`FakeInspection::next_day`] moves the clock past the reset.
pub struct FakeInspection {
    addr: SocketAddr,
    inner: InspectionShared,
}

impl FakeInspection {
    /// `sites` are the properties the account may inspect; `allowance` is
    /// the number of inspections one day allows.
    pub fn start(sites: Vec<String>, allowance: u64) -> Self {
        let inner: InspectionShared = Arc::new(Mutex::new(InspectionInner {
            sites,
            allowance,
            ..InspectionInner::default()
        }));
        let router = Router::new()
            .route("/v1/urlInspection/index:inspect", post(inspect))
            .with_state(inner.clone());
        Self {
            addr: spawn(router),
            inner,
        }
    }

    /// The base the client calls (`…/v1`).
    pub fn base(&self) -> String {
        format!("http://{}/v1", self.addr)
    }

    /// What the fake says for `url`; others read as indexed.
    pub fn set_answer(&self, url: &str, answer: InspectionAnswer) {
        self.inner.lock().unwrap().answers.insert(url.into(), answer);
    }

    /// The next day: the allowance is back.
    pub fn next_day(&self) {
        let mut g = self.inner.lock().unwrap();
        g.day += 1;
        g.used = 0;
    }

    pub fn day(&self) -> u32 {
        self.inner.lock().unwrap().day
    }

    /// Inspections counted against today's allowance.
    pub fn used_today(&self) -> u64 {
        self.inner.lock().unwrap().used
    }

    /// Every URL answered with a result, in order, across all days.
    pub fn inspected(&self) -> Vec<String> {
        self.inner.lock().unwrap().inspected.clone()
    }

    /// Requests received, answered or refused.
    pub fn log(&self) -> Vec<Logged> {
        self.inner.lock().unwrap().log.clone()
    }

    /// The next request fails with this status and message.
    pub fn fail_next(&self, status: u16, message: &str) {
        self.inner
            .lock()
            .unwrap()
            .failures
            .push_back((status, message.to_string()));
    }
}

async fn inspect(State(s): State<InspectionShared>, headers: HeaderMap, body: String) -> Response {
    let mut g = s.lock().unwrap();
    g.log.push(Logged {
        method: "POST".into(),
        path: "/v1/urlInspection/index:inspect".into(),
        body: body.clone(),
    });
    if !authorised(&headers) {
        return unauthorised();
    }
    if let Some((status, message)) = g.failures.pop_front() {
        return error(status, &message, "UNAVAILABLE");
    }
    let req: Value = serde_json::from_str(&body).unwrap_or_default();
    let site = req["siteUrl"].as_str().unwrap_or_default();
    let url = req["inspectionUrl"].as_str().unwrap_or_default().to_string();
    if !g.sites.iter().any(|s| s == site) {
        return error(
            403,
            "User does not have sufficient permission for site.",
            "PERMISSION_DENIED",
        );
    }
    if url.is_empty() {
        return error(400, "Invalid inspection URL.", "INVALID_ARGUMENT");
    }
    if g.used >= g.allowance {
        return error(
            429,
            "Quota exceeded for quota metric 'Inspection requests' and limit 'Inspection requests per day per site'.",
            "RESOURCE_EXHAUSTED",
        );
    }
    g.used += 1;
    g.inspected.push(url.clone());
    let a = g
        .answers
        .get(&url)
        .cloned()
        .unwrap_or_else(|| InspectionAnswer::indexed(&url));
    let mut index = json!({
        "verdict": a.verdict,
        "coverageState": a.coverage_state,
        "indexingState": a.indexing_state,
        "robotsTxtState": a.robots_state,
        "pageFetchState": a.page_fetch_state,
        "lastCrawlTime": a.last_crawl,
        "crawledAs": a.crawled_as,
    });
    if let Some(c) = a.google_canonical {
        index["googleCanonical"] = json!(c);
    }
    if let Some(c) = a.user_canonical {
        index["userCanonical"] = json!(c);
    }
    axum::Json(json!({"inspectionResult": {
        "inspectionResultLink": format!("https://search.google.com/search-console/inspect?resource_id={site}"),
        "indexStatusResult": index,
    }}))
    .into_response()
}

// -------------------------------------------------------------------- Analytics

#[derive(Debug, Clone)]
pub struct GaStream {
    pub id: String,
    pub name: String,
    pub default_uri: String,
    /// `WEB_DATA_STREAM`, or an app stream type the client must skip.
    pub kind: String,
}

impl GaStream {
    pub fn web(id: &str, name: &str, default_uri: &str) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            default_uri: default_uri.into(),
            kind: "WEB_DATA_STREAM".into(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct GaProperty {
    /// The number in `properties/<id>`.
    pub id: String,
    pub name: String,
    pub streams: Vec<GaStream>,
}

/// One landing page row of one stream.
#[derive(Debug, Clone)]
pub struct GaRow {
    pub stream: String,
    pub host: String,
    /// The path, with a query string for variants that have one.
    pub page: String,
    pub sessions: u64,
    pub engaged_sessions: u64,
    pub key_events: f64,
    pub revenue: f64,
}

impl GaRow {
    pub fn new(stream: &str, host: &str, page: &str, sessions: u64, engaged_sessions: u64) -> Self {
        Self {
            stream: stream.into(),
            host: host.into(),
            page: page.into(),
            sessions,
            engaged_sessions,
            key_events: 0.0,
            revenue: 0.0,
        }
    }
}

/// Analytics Data API token quota of a standard property.
#[derive(Debug, Clone, Copy)]
pub struct GaQuota {
    pub per_hour: i64,
    pub per_day: i64,
    /// Tokens one `runReport` costs.
    pub cost: i64,
}

impl Default for GaQuota {
    fn default() -> Self {
        Self {
            per_hour: 40_000,
            per_day: 200_000,
            cost: 10,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct AnalyticsData {
    pub properties: Vec<GaProperty>,
    pub rows: Vec<GaRow>,
    pub quota: GaQuota,
}

struct AnalyticsInner {
    data: AnalyticsData,
    hour_left: i64,
    day_left: i64,
    log: Vec<Logged>,
    failures: VecDeque<(u16, String)>,
}

type AnalyticsShared = Arc<Mutex<AnalyticsInner>>;

/// Analytics Admin (`accountSummaries`, `dataStreams`) and Data
/// (`runReport`) with `propertyQuota`.
pub struct FakeAnalytics {
    addr: SocketAddr,
    inner: AnalyticsShared,
}

impl FakeAnalytics {
    pub fn start(data: AnalyticsData) -> Self {
        let inner: AnalyticsShared = Arc::new(Mutex::new(AnalyticsInner {
            hour_left: data.quota.per_hour,
            day_left: data.quota.per_day,
            data,
            log: Vec::new(),
            failures: VecDeque::new(),
        }));
        let router = Router::new()
            .route("/v1beta/accountSummaries", get(account_summaries))
            .route("/v1beta/properties/{p}", post(run_report))
            .route("/v1beta/properties/{p}/dataStreams", get(data_streams))
            .with_state(inner.clone());
        Self {
            addr: spawn(router),
            inner,
        }
    }

    /// The base of the Admin API.
    pub fn admin_base(&self) -> String {
        format!("http://{}/v1beta", self.addr)
    }

    /// The base of the Data API.
    pub fn data_base(&self) -> String {
        format!("http://{}/v1beta", self.addr)
    }

    pub fn log(&self) -> Vec<Logged> {
        self.inner.lock().unwrap().log.clone()
    }

    /// `runReport` requests received, answered or refused.
    pub fn reports(&self) -> Vec<Value> {
        self.log()
            .into_iter()
            .filter(|l| l.path.ends_with(":runReport"))
            .map(|l| serde_json::from_str(&l.body).unwrap_or_default())
            .collect()
    }

    /// Sets the tokens left this hour; a request costing more is refused.
    pub fn set_hour_left(&self, tokens: i64) {
        self.inner.lock().unwrap().hour_left = tokens;
    }

    /// A new hour: the hourly allowance is back.
    pub fn next_hour(&self) {
        let mut g = self.inner.lock().unwrap();
        g.hour_left = g.data.quota.per_hour;
    }

    pub fn fail_next(&self, status: u16, message: &str) {
        self.inner
            .lock()
            .unwrap()
            .failures
            .push_back((status, message.to_string()));
    }
}

fn log_analytics(s: &AnalyticsShared, method: &str, path: &str, body: &str) {
    s.lock().unwrap().log.push(Logged {
        method: method.into(),
        path: path.into(),
        body: body.into(),
    });
}

async fn account_summaries(State(s): State<AnalyticsShared>, headers: HeaderMap) -> Response {
    log_analytics(&s, "GET", "/v1beta/accountSummaries", "");
    if !authorised(&headers) {
        return unauthorised();
    }
    let g = s.lock().unwrap();
    let props: Vec<Value> = g
        .data
        .properties
        .iter()
        .map(|p| json!({"property": format!("properties/{}", p.id), "displayName": p.name}))
        .collect();
    axum::Json(json!({"accountSummaries": [{
        "account": "accounts/1",
        "displayName": "Test account",
        "propertySummaries": props,
    }]}))
    .into_response()
}

async fn data_streams(State(s): State<AnalyticsShared>, Path(p): Path<String>, headers: HeaderMap) -> Response {
    log_analytics(&s, "GET", &format!("/v1beta/properties/{p}/dataStreams"), "");
    if !authorised(&headers) {
        return unauthorised();
    }
    let g = s.lock().unwrap();
    let Some(prop) = g.data.properties.iter().find(|x| x.id == p) else {
        return error(403, "User does not have sufficient permissions for this property.", "PERMISSION_DENIED");
    };
    let streams: Vec<Value> = prop
        .streams
        .iter()
        .map(|st| {
            json!({
                "name": format!("properties/{p}/dataStreams/{}", st.id),
                "type": st.kind,
                "displayName": st.name,
                "webStreamData": {"measurementId": format!("G-{}", st.id), "defaultUri": st.default_uri},
            })
        })
        .collect();
    axum::Json(json!({"dataStreams": streams})).into_response()
}

#[derive(Default, Clone)]
struct Sums {
    sessions: u64,
    engaged: u64,
    key_events: f64,
    revenue: f64,
}

async fn run_report(
    State(s): State<AnalyticsShared>,
    Path(p): Path<String>,
    headers: HeaderMap,
    body: String,
) -> Response {
    log_analytics(&s, "POST", &format!("/v1beta/properties/{p}"), &body);
    if !authorised(&headers) {
        return unauthorised();
    }
    let Some(property) = p.strip_suffix(":runReport") else {
        return error(404, "Not found.", "NOT_FOUND");
    };
    let mut g = s.lock().unwrap();
    if let Some((status, message)) = g.failures.pop_front() {
        return error(status, &message, "UNAVAILABLE");
    }
    if !g.data.properties.iter().any(|x| x.id == property) {
        return error(403, "User does not have sufficient permissions for this property.", "PERMISSION_DENIED");
    }
    let cost = g.data.quota.cost;
    if g.hour_left < cost {
        return error(
            429,
            "Exhausted property tokens per hour quota for this property.",
            "RESOURCE_EXHAUSTED",
        );
    }
    if g.day_left < cost {
        return error(
            429,
            "Exhausted property tokens per day quota for this property.",
            "RESOURCE_EXHAUSTED",
        );
    }
    g.hour_left -= cost;
    g.day_left -= cost;
    let req: Value = serde_json::from_str(&body).unwrap_or_default();
    let dimension = req
        .pointer("/dimensions/1/name")
        .and_then(Value::as_str)
        .unwrap_or("landingPage")
        .to_string();
    let stream = req
        .pointer("/dimensionFilter/filter/stringFilter/value")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let combine = dimension == "landingPage";
    let mut grouped: BTreeMap<(String, String), Sums> = BTreeMap::new();
    for r in g.data.rows.iter().filter(|r| r.stream == stream) {
        let page = if combine {
            r.page.split('?').next().unwrap_or(&r.page).to_string()
        } else {
            r.page.clone()
        };
        let e = grouped.entry((r.host.clone(), page)).or_default();
        e.sessions += r.sessions;
        e.engaged += r.engaged_sessions;
        e.key_events += r.key_events;
        e.revenue += r.revenue;
    }
    let mut all: Vec<((String, String), Sums)> = grouped.into_iter().collect();
    all.sort_by(|a, b| b.1.sessions.cmp(&a.1.sessions).then(a.0.cmp(&b.0)));
    let total = all.len();
    let offset = req["offset"].as_u64().unwrap_or(0) as usize;
    let limit = req["limit"].as_u64().unwrap_or(10_000) as usize;
    let names: Vec<String> = req["metrics"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|m| m["name"].as_str().map(str::to_string))
        .collect();
    let rows: Vec<Value> = all
        .into_iter()
        .skip(offset)
        .take(limit)
        .map(|((host, page), sums)| {
            let metrics: Vec<Value> = names
                .iter()
                .map(|n| {
                    let v = match n.as_str() {
                        "sessions" => sums.sessions.to_string(),
                        "engagedSessions" => sums.engaged.to_string(),
                        "engagementRate" => {
                            if sums.sessions > 0 {
                                (sums.engaged as f64 / sums.sessions as f64).to_string()
                            } else {
                                "0".into()
                            }
                        }
                        "keyEvents" => sums.key_events.to_string(),
                        "totalRevenue" => sums.revenue.to_string(),
                        _ => "0".into(),
                    };
                    json!({"value": v})
                })
                .collect();
            json!({"dimensionValues": [{"value": host}, {"value": page}], "metricValues": metrics})
        })
        .collect();
    let mut out = json!({"rows": rows, "rowCount": total});
    if req["returnPropertyQuota"].as_bool() == Some(true) {
        out["propertyQuota"] = json!({
            "tokensPerHour": {"consumed": cost, "remaining": g.hour_left},
            "tokensPerDay": {"consumed": cost, "remaining": g.day_left},
            "concurrentRequests": {"consumed": 0, "remaining": 10},
        });
    }
    axum::Json(out).into_response()
}
