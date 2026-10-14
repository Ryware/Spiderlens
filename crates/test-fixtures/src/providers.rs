//! Fake external providers for spec 024 tests (plan D12). Never contacts
//! the internet. One fake serves Google sign-in (authorize, token, revoke)
//! and Search Console, with a request log, configurable rows and injected
//! failures.

use std::collections::{HashMap, HashSet, VecDeque};
use std::net::{SocketAddr, TcpListener as StdListener};
use std::sync::{Arc, Mutex, mpsc};
use std::thread;

use axum::Router;
use axum::extract::{Form, Path, Query, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Redirect, Response};
use axum::routing::{get, post};
use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

/// One Search Console page row.
#[derive(Debug, Clone)]
pub struct GscRow {
    pub page: String,
    pub clicks: u64,
    pub impressions: u64,
    pub position: f64,
}

#[derive(Debug, Clone, Default)]
pub struct GoogleData {
    pub email: String,
    /// `(siteUrl, permissionLevel)`.
    pub sites: Vec<(String, String)>,
    pub rows: Vec<GscRow>,
    /// page → (query, clicks, impressions)
    pub queries: HashMap<String, Vec<(String, u64, u64)>>,
}

/// One request the fake received.
#[derive(Debug, Clone)]
pub struct Logged {
    pub method: String,
    pub path: String,
    pub body: String,
}

#[derive(Default)]
struct Inner {
    data: GoogleData,
    log: Vec<Logged>,
    /// code → (code_challenge, scope)
    codes: HashMap<String, (String, String)>,
    access: HashSet<String>,
    refresh: HashSet<String>,
    revoked: Vec<String>,
    failures: VecDeque<(u16, String, Option<u64>)>,
    counter: u64,
}

type Shared = Arc<Mutex<Inner>>;

pub struct FakeGoogle {
    addr: SocketAddr,
    inner: Shared,
}

impl FakeGoogle {
    pub fn start(data: GoogleData) -> Self {
        let inner: Shared = Arc::new(Mutex::new(Inner {
            data,
            ..Inner::default()
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

    pub fn auth_url(&self) -> String {
        format!("{}/o/oauth2/v2/auth", self.base())
    }

    pub fn token_url(&self) -> String {
        format!("{}/token", self.base())
    }

    pub fn revoke_url(&self) -> String {
        format!("{}/revoke", self.base())
    }

    pub fn webmasters_url(&self) -> String {
        format!("{}/webmasters/v3", self.base())
    }

    pub fn log(&self) -> Vec<Logged> {
        self.inner.lock().unwrap().log.clone()
    }

    /// Requests to the data APIs (not sign-in).
    pub fn api_calls(&self) -> usize {
        self.log()
            .iter()
            .filter(|l| l.path.starts_with("/webmasters"))
            .count()
    }

    pub fn revoked(&self) -> Vec<String> {
        self.inner.lock().unwrap().revoked.clone()
    }

    /// The next data-API request fails with this status and body.
    pub fn fail_next(&self, status: u16, body: &str, retry_after: Option<u64>) {
        self.inner
            .lock()
            .unwrap()
            .failures
            .push_back((status, body.to_string(), retry_after));
    }

    /// Makes every issued access token invalid, as if they had expired.
    pub fn expire_access_tokens(&self) {
        self.inner.lock().unwrap().access.clear();
    }

    /// A refresh token the fake accepts, for tests that skip sign-in.
    pub fn issue_refresh_token(&self) -> String {
        let mut g = self.inner.lock().unwrap();
        g.counter += 1;
        let t = format!("refresh-{}", g.counter);
        g.refresh.insert(t.clone());
        t
    }
}

fn routes(state: Shared) -> Router {
    Router::new()
        .route("/o/oauth2/v2/auth", get(authorize))
        .route("/token", post(token))
        .route("/revoke", post(revoke))
        .route("/webmasters/v3/sites", get(sites))
        .route("/webmasters/v3/sites/{site}/searchAnalytics/query", post(query))
        .with_state(state)
}

fn log(state: &Shared, method: &str, path: &str, body: &str) {
    state.lock().unwrap().log.push(Logged {
        method: method.into(),
        path: path.into(),
        body: body.into(),
    });
}

/// Consent is given at once: redirects back with a code bound to the PKCE
/// challenge.
async fn authorize(State(s): State<Shared>, Query(q): Query<HashMap<String, String>>) -> Response {
    log(&s, "GET", "/o/oauth2/v2/auth", "");
    let redirect = q.get("redirect_uri").cloned().unwrap_or_default();
    let state = q.get("state").cloned().unwrap_or_default();
    if q.get("code_challenge_method").map(String::as_str) != Some("S256") {
        return Redirect::to(&format!("{redirect}/?error=invalid_request&state={state}")).into_response();
    }
    let mut g = s.lock().unwrap();
    g.counter += 1;
    let code = format!("code-{}", g.counter);
    g.codes.insert(
        code.clone(),
        (
            q.get("code_challenge").cloned().unwrap_or_default(),
            q.get("scope").cloned().unwrap_or_default(),
        ),
    );
    Redirect::to(&format!("{redirect}/?code={code}&state={state}")).into_response()
}

async fn token(State(s): State<Shared>, Form(f): Form<HashMap<String, String>>) -> Response {
    log(
        &s,
        "POST",
        "/token",
        f.get("grant_type").map_or("", String::as_str),
    );
    let mut g = s.lock().unwrap();
    g.counter += 1;
    let n = g.counter;
    match f.get("grant_type").map(String::as_str) {
        Some("authorization_code") => {
            let code = f.get("code").cloned().unwrap_or_default();
            let Some((challenge, scope)) = g.codes.remove(&code) else {
                return (
                    StatusCode::BAD_REQUEST,
                    axum::Json(json!({"error": "invalid_grant"})),
                )
                    .into_response();
            };
            let verifier = f.get("code_verifier").cloned().unwrap_or_default();
            if URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes())) != challenge {
                return (
                    StatusCode::BAD_REQUEST,
                    axum::Json(json!({"error": "invalid_grant", "error_description": "PKCE"})),
                )
                    .into_response();
            }
            let access = format!("access-{n}");
            let refresh = format!("refresh-{n}");
            g.access.insert(access.clone());
            g.refresh.insert(refresh.clone());
            let id_payload = URL_SAFE_NO_PAD.encode(json!({"email": g.data.email}).to_string());
            axum::Json(json!({
                "access_token": access,
                "refresh_token": refresh,
                "expires_in": 3599,
                "scope": scope,
                "id_token": format!("h.{id_payload}.s"),
            }))
            .into_response()
        }
        Some("refresh_token") => {
            let r = f.get("refresh_token").cloned().unwrap_or_default();
            if !g.refresh.contains(&r) {
                return (
                    StatusCode::BAD_REQUEST,
                    axum::Json(json!({"error": "invalid_grant"})),
                )
                    .into_response();
            }
            let access = format!("access-{n}");
            g.access.insert(access.clone());
            axum::Json(json!({"access_token": access, "expires_in": 3599})).into_response()
        }
        _ => (
            StatusCode::BAD_REQUEST,
            axum::Json(json!({"error": "unsupported_grant_type"})),
        )
            .into_response(),
    }
}

async fn revoke(State(s): State<Shared>, Form(f): Form<HashMap<String, String>>) -> Response {
    let t = f.get("token").cloned().unwrap_or_default();
    log(&s, "POST", "/revoke", "");
    let mut g = s.lock().unwrap();
    g.refresh.remove(&t);
    g.revoked.push(t);
    StatusCode::OK.into_response()
}

/// Checks the bearer token and pops an injected failure, if any.
fn gate(s: &Shared, headers: &HeaderMap) -> Option<Response> {
    let mut g = s.lock().unwrap();
    if let Some((status, body, retry)) = g.failures.pop_front() {
        let mut r = (StatusCode::from_u16(status).unwrap(), body).into_response();
        if let Some(sec) = retry {
            r.headers_mut()
                .insert(header::RETRY_AFTER, sec.to_string().parse().unwrap());
        }
        return Some(r);
    }
    let token = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .unwrap_or_default();
    if !g.access.contains(token) {
        return Some(
            (
                StatusCode::UNAUTHORIZED,
                axum::Json(json!({"error": {"code": 401, "message": "Request had invalid authentication credentials."}})),
            )
                .into_response(),
        );
    }
    None
}

async fn sites(State(s): State<Shared>, headers: HeaderMap) -> Response {
    log(&s, "GET", "/webmasters/v3/sites", "");
    if let Some(r) = gate(&s, &headers) {
        return r;
    }
    let g = s.lock().unwrap();
    let entries: Vec<Value> = g
        .data
        .sites
        .iter()
        .map(|(u, p)| json!({"siteUrl": u, "permissionLevel": p}))
        .collect();
    axum::Json(json!({"siteEntry": entries})).into_response()
}

async fn query(
    State(s): State<Shared>,
    Path(site): Path<String>,
    headers: HeaderMap,
    body: String,
) -> Response {
    log(
        &s,
        "POST",
        &format!("/webmasters/v3/sites/{site}/searchAnalytics/query"),
        &body,
    );
    if let Some(r) = gate(&s, &headers) {
        return r;
    }
    let req: Value = serde_json::from_str(&body).unwrap_or_default();
    let g = s.lock().unwrap();
    if !g.data.sites.iter().any(|(u, _)| *u == site) {
        return (
            StatusCode::FORBIDDEN,
            axum::Json(json!({"error": {"code": 403, "message": "User does not have sufficient permission for site."}})),
        )
            .into_response();
    }
    let dims: Vec<&str> = req["dimensions"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect();
    if dims == ["query"] {
        let page = req
            .pointer("/dimensionFilterGroups/0/filters/0/expression")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let rows: Vec<Value> = g
            .data
            .queries
            .get(page)
            .into_iter()
            .flatten()
            .map(|(q, c, i)| {
                json!({"keys": [q], "clicks": c, "impressions": i,
                       "ctr": if *i > 0 { *c as f64 / *i as f64 } else { 0.0 }, "position": 3.0})
            })
            .collect();
        return axum::Json(json!({"rows": rows})).into_response();
    }
    let start = req["startRow"].as_u64().unwrap_or(0) as usize;
    let limit = req["rowLimit"].as_u64().unwrap_or(1000) as usize;
    let rows: Vec<Value> = g
        .data
        .rows
        .iter()
        .skip(start)
        .take(limit)
        .map(|r| {
            json!({"keys": [r.page], "clicks": r.clicks, "impressions": r.impressions,
                   "ctr": if r.impressions > 0 { r.clicks as f64 / r.impressions as f64 } else { 0.0 },
                   "position": r.position})
        })
        .collect();
    if rows.is_empty() {
        return axum::Json(json!({"responseAggregationType": "byPage"})).into_response();
    }
    axum::Json(json!({"rows": rows, "responseAggregationType": "byPage"})).into_response()
}
