//! Fake Google Sheets API v4 for spec 024 tests (plan D12, T043): creates
//! spreadsheets with a grid, takes `values:batchUpdate`, and refuses what a
//! real spreadsheet refuses: more cells than its capacity and writes outside
//! the grid. Keeps everything written so a test can read it back. Accepts
//! any bearer token (sign-in is the fake Google's job). Never contacts the
//! internet.

use std::collections::VecDeque;
use std::net::{SocketAddr, TcpListener as StdListener};
use std::sync::{Arc, Mutex, mpsc};
use std::thread;

use axum::Router;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use serde_json::{Value, json};

/// One worksheet and what was written into it.
#[derive(Debug, Clone)]
pub struct FakeTab {
    pub id: i64,
    pub title: String,
    pub grid_rows: u64,
    pub grid_cols: u64,
    /// Row by row, from row 1.
    pub values: Vec<Vec<Value>>,
}

#[derive(Debug, Clone)]
pub struct FakeSpreadsheet {
    pub id: String,
    pub title: String,
    pub tabs: Vec<FakeTab>,
}

impl FakeSpreadsheet {
    /// The cells a client sees as text, row by row (numbers as written).
    pub fn text_rows(&self, tab: usize) -> Vec<Vec<String>> {
        self.tabs[tab]
            .values
            .iter()
            .map(|r| {
                r.iter()
                    .map(|v| match v {
                        Value::String(s) => s.clone(),
                        other => other.to_string(),
                    })
                    .collect()
            })
            .collect()
    }
}

/// One request the fake received.
#[derive(Debug, Clone)]
pub struct Logged {
    pub path: String,
    pub body: String,
}

struct Inner {
    capacity: u64,
    sheets: Vec<FakeSpreadsheet>,
    log: Vec<Logged>,
    tokens: Vec<String>,
    failures: VecDeque<(u16, String)>,
}

type Shared = Arc<Mutex<Inner>>;

pub struct FakeSheets {
    addr: SocketAddr,
    inner: Shared,
}

impl FakeSheets {
    /// `capacity` is the most cells one spreadsheet holds (Google: 10,000,000).
    pub fn start(capacity: u64) -> Self {
        let inner: Shared = Arc::new(Mutex::new(Inner {
            capacity,
            sheets: Vec::new(),
            log: Vec::new(),
            tokens: Vec::new(),
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

    pub fn base_url(&self) -> String {
        format!("http://{}/v4", self.addr)
    }

    pub fn spreadsheets(&self) -> Vec<FakeSpreadsheet> {
        self.inner.lock().unwrap().sheets.clone()
    }

    pub fn log(&self) -> Vec<Logged> {
        self.inner.lock().unwrap().log.clone()
    }

    /// Requests whose path ends with `suffix`.
    pub fn requests(&self, suffix: &str) -> usize {
        self.log().iter().filter(|l| l.path.ends_with(suffix)).count()
    }

    /// Bearer tokens the fake saw, in order.
    pub fn tokens(&self) -> Vec<String> {
        self.inner.lock().unwrap().tokens.clone()
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
        .route("/v4/spreadsheets", post(create))
        .route("/v4/spreadsheets/{id}/values:batchUpdate", post(batch_update))
        .with_state(state)
}

/// Checks the token and pops an injected failure.
fn gate(s: &Shared, path: &str, headers: &HeaderMap, body: &str) -> Option<Response> {
    let mut g = s.lock().unwrap();
    g.log.push(Logged {
        path: path.into(),
        body: body.into(),
    });
    if let Some((status, b)) = g.failures.pop_front() {
        return Some((StatusCode::from_u16(status).unwrap(), b).into_response());
    }
    let token = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .unwrap_or_default()
        .to_string();
    if token.is_empty() {
        return Some(error(401, "Request had invalid authentication credentials."));
    }
    g.tokens.push(token);
    None
}

fn error(status: u16, message: &str) -> Response {
    (
        StatusCode::from_u16(status).unwrap(),
        axum::Json(json!({"error": {"code": status, "message": message}})),
    )
        .into_response()
}

async fn create(State(s): State<Shared>, headers: HeaderMap, body: String) -> Response {
    if let Some(r) = gate(&s, "/v4/spreadsheets", &headers, &body) {
        return r;
    }
    let req: Value = serde_json::from_str(&body).unwrap_or_default();
    let mut g = s.lock().unwrap();
    let requested = req["sheets"].as_array().cloned().unwrap_or_default();
    let mut tabs: Vec<FakeTab> = Vec::new();
    for (i, t) in requested.iter().enumerate() {
        let p = &t["properties"];
        tabs.push(FakeTab {
            id: p["sheetId"].as_i64().unwrap_or(i as i64),
            title: p["title"].as_str().unwrap_or("Sheet1").to_string(),
            grid_rows: p
                .pointer("/gridProperties/rowCount")
                .and_then(Value::as_u64)
                .unwrap_or(1000),
            grid_cols: p
                .pointer("/gridProperties/columnCount")
                .and_then(Value::as_u64)
                .unwrap_or(26),
            values: Vec::new(),
        });
    }
    if tabs.is_empty() {
        tabs.push(FakeTab {
            id: 0,
            title: "Sheet1".into(),
            grid_rows: 1000,
            grid_cols: 26,
            values: Vec::new(),
        });
    }
    let cells: u64 = tabs.iter().map(|t| t.grid_rows * t.grid_cols).sum();
    if cells > g.capacity {
        let message = format!(
            "This action would increase the number of cells in the workbook above the limit of {} cells.",
            g.capacity
        );
        return error(400, &message);
    }
    let id = format!("fake-sheet-{}", g.sheets.len() + 1);
    let sheets: Vec<Value> = tabs
        .iter()
        .map(|t| {
            json!({"properties": {
                "sheetId": t.id, "title": t.title,
                "gridProperties": {"rowCount": t.grid_rows, "columnCount": t.grid_cols},
            }})
        })
        .collect();
    let title = req
        .pointer("/properties/title")
        .and_then(Value::as_str)
        .unwrap_or("Untitled")
        .to_string();
    g.sheets.push(FakeSpreadsheet {
        id: id.clone(),
        title,
        tabs,
    });
    axum::Json(json!({
        "spreadsheetId": id,
        "spreadsheetUrl": format!("https://docs.google.com/spreadsheets/d/{id}/edit"),
        "sheets": sheets,
    }))
    .into_response()
}

/// The sheet title and first row of an A1 range such as `'Page titles'!A2`.
fn parse_range(range: &str) -> Option<(String, usize)> {
    let (sheet, cell) = range.rsplit_once('!')?;
    let sheet = sheet
        .strip_prefix('\'')
        .and_then(|x| x.strip_suffix('\''))
        .map_or_else(|| sheet.to_string(), |x| x.replace("''", "'"));
    let digits: String = cell.chars().skip_while(|c| c.is_ascii_alphabetic()).collect();
    Some((sheet, digits.parse().ok()?))
}

async fn batch_update(
    State(s): State<Shared>,
    Path(id): Path<String>,
    headers: HeaderMap,
    body: String,
) -> Response {
    if let Some(r) = gate(
        &s,
        &format!("/v4/spreadsheets/{id}/values:batchUpdate"),
        &headers,
        &body,
    ) {
        return r;
    }
    let req: Value = serde_json::from_str(&body).unwrap_or_default();
    if !matches!(req["valueInputOption"].as_str(), Some("RAW" | "USER_ENTERED")) {
        return error(400, "Invalid value at 'value_input_option'");
    }
    let mut g = s.lock().unwrap();
    let Some(sheet) = g.sheets.iter_mut().find(|x| x.id == id) else {
        return error(404, "Requested entity was not found.");
    };
    let mut updated = 0u64;
    for d in req["data"].as_array().cloned().unwrap_or_default() {
        let Some((title, first)) = d["range"].as_str().and_then(parse_range) else {
            return error(400, "Unable to parse range");
        };
        let Some(tab) = sheet.tabs.iter_mut().find(|t| t.title == title) else {
            return error(400, &format!("Unable to parse range: {title}"));
        };
        let rows = d["values"].as_array().cloned().unwrap_or_default();
        let widest = rows
            .iter()
            .map(|r| r.as_array().map_or(0, Vec::len))
            .max()
            .unwrap_or(0) as u64;
        let last = (first + rows.len()).saturating_sub(1) as u64;
        if first == 0 || last > tab.grid_rows || widest > tab.grid_cols {
            return error(400, "Range exceeds grid limits.");
        }
        for (i, r) in rows.iter().enumerate() {
            let at = first - 1 + i;
            if tab.values.len() <= at {
                tab.values.resize(at + 1, Vec::new());
            }
            tab.values[at] = r.as_array().cloned().unwrap_or_default();
            updated += 1;
        }
    }
    axum::Json(json!({"spreadsheetId": id, "totalUpdatedRows": updated})).into_response()
}
