//! Agent checks that need the network, run once per host after the last
//! page, next to the per-host site files (spec 016 FR-011, FR-012, plan D4):
//!
//! - each Markdown alternate a page points to is requested once, and its
//!   status and content type say whether it is real Markdown;
//! - each MCP endpoint a catalog or card lists gets one discovery request
//!   (`server/discover`, or `initialize` when the server does not know it).
//!   Never a tool call.
//!
//! Requests carry no cookies or credentials and the callers keep to
//! robots.txt, crawl-delay and hosts the crawl gave up on.

use std::collections::HashMap;
use std::time::Duration;

use reqwest::Client;
use reqwest::header::{ACCEPT, CONTENT_TYPE, WWW_AUTHENTICATE};
use serde_json::{Value, json};
use url::Url;

use crate::fetch::{BodyPolicy, OnLimit, get_following};

/// Bodies of Markdown alternates are read up to this and dropped.
pub const MD_MAX_BYTES: u64 = 256 * 1024;
/// Largest MCP answer read.
pub const MCP_MAX_BYTES: usize = 256 * 1024;
/// One MCP request, connection to last byte.
pub const MCP_TIMEOUT: Duration = Duration::from_secs(10);
/// MCP endpoints probed per host.
pub const MCP_PER_HOST: usize = 5;
/// The protocol version sent with `server/discover`.
pub const DISCOVER_VERSION: &str = "2026-07-28";
/// The protocol version offered by the `initialize` fallback.
pub const INITIALIZE_VERSION: &str = "2025-11-25";

/// How deep a catalog or card is searched for endpoints.
const MAX_DEPTH: usize = 8;
/// Objects looked at in one document.
const MAX_NODES: usize = 5_000;

// ---------------------------------------------------------------- Markdown

/// `ok` for a 2xx answer served as Markdown or plain text, `wrong_type` for
/// any other 2xx, `error` otherwise.
pub fn md_outcome(status: u16, content_type: Option<&str>) -> &'static str {
    if !(200..300).contains(&status) {
        return "error";
    }
    let essence = content_type
        .and_then(|c| c.split(';').next())
        .map(|c| c.trim().to_ascii_lowercase())
        .unwrap_or_default();
    match essence.as_str() {
        "text/markdown" | "text/plain" | "text/x-markdown" => "ok",
        _ => "wrong_type",
    }
}

/// One Markdown address to request and the pages pointing at it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MdJob {
    pub host_key: String,
    pub url: Url,
    pub url_ids: Vec<i64>,
}

/// Which alternates to request: the first `max` distinct addresses per host
/// in page order. Pages pointing at the others, or at another host, are
/// `unverified`.
#[derive(Debug, Default)]
pub struct MdPlan {
    max: usize,
    pub jobs: Vec<MdJob>,
    pub unverified: Vec<i64>,
    by_url: HashMap<String, usize>,
    per_host: HashMap<String, usize>,
}

impl MdPlan {
    pub fn new(max: usize) -> Self {
        Self {
            max,
            ..Self::default()
        }
    }

    /// Adds the alternate `alt` of the page `page_url` (as stored: it may be
    /// relative to the page). `host_key` is the page's host.
    pub fn add(&mut self, url_id: i64, page_url: &str, host_key: &str, alt: &str) {
        let target = Url::parse(page_url).ok().and_then(|p| {
            let mut t = p.join(alt.trim()).ok()?;
            t.set_fragment(None);
            matches!(t.scheme(), "http" | "https").then_some(t)
        });
        // Another host has its own robots.txt that was not read.
        let Some(target) = target.filter(|t| origin_key(t) == host_key) else {
            self.unverified.push(url_id);
            return;
        };
        let key = target.to_string();
        if let Some(&i) = self.by_url.get(&key) {
            self.jobs[i].url_ids.push(url_id);
            return;
        }
        let n = self.per_host.entry(host_key.to_string()).or_default();
        if *n >= self.max {
            self.unverified.push(url_id);
            return;
        }
        *n += 1;
        self.by_url.insert(key, self.jobs.len());
        self.jobs.push(MdJob {
            host_key: host_key.to_string(),
            url: target,
            url_ids: vec![url_id],
        });
    }
}

fn origin_key(url: &Url) -> String {
    format!(
        "{}://{}:{}",
        url.scheme(),
        url.host_str().unwrap_or(""),
        url.port_or_known_default().unwrap_or(0)
    )
}

/// Requests one alternate and returns its status (None when nothing
/// answered) and outcome.
pub async fn fetch_markdown(client: &Client, url: &Url) -> (Option<u16>, &'static str) {
    let drop_body = |_: Option<&str>| false;
    let policy = BodyPolicy {
        max_bytes: MD_MAX_BYTES,
        on_limit: OnLimit::Truncate,
        keep: &drop_body,
    };
    match get_following(client, url, 5, &policy, &[]).await.result {
        Err(_) => (None, "error"),
        Ok(f) => {
            let status = f.status.as_u16();
            (Some(status), md_outcome(status, f.content_type.as_deref()))
        }
    }
}

// --------------------------------------------------------------------- MCP

/// MCP endpoint addresses listed by a catalog or card, resolved against the
/// address `base` it was served from. Lenient about shape: an object counts
/// when its `transport`, `type`, `protocol` or `mediaType` mentions MCP or
/// `streamable-http` (its `url` or `endpoint` is the address), and so does
/// the value of a key such as `mcp` or `mcpEndpoint`. Addresses ending in
/// `.json` are cards, not endpoints, and are left out. Same host or any
/// https address; at most [`MCP_PER_HOST`].
pub fn mcp_endpoints(base: &Url, doc: &[u8]) -> Vec<Url> {
    let doc = doc.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(doc);
    let Ok(value) = serde_json::from_slice::<Value>(doc) else {
        return Vec::new();
    };
    let mut found: Vec<Url> = Vec::new();
    let mut nodes = 0;
    walk(&value, base, false, 0, &mut nodes, &mut found);
    found
}

fn walk(v: &Value, base: &Url, tagged: bool, depth: usize, nodes: &mut usize, out: &mut Vec<Url>) {
    *nodes += 1;
    if depth > MAX_DEPTH || *nodes > MAX_NODES || out.len() >= MCP_PER_HOST {
        return;
    }
    match v {
        Value::Array(a) => {
            for x in a {
                walk(x, base, tagged, depth + 1, nodes, out);
            }
        }
        Value::Object(o) => {
            let here = tagged
                || ["transport", "type", "protocol", "protocolBinding", "mediaType"]
                    .iter()
                    .filter_map(|k| o.get(*k).and_then(Value::as_str))
                    .any(mentions_mcp);
            if here {
                for k in ["url", "endpoint", "href", "location", "uri"] {
                    if let Some(s) = o.get(k).and_then(Value::as_str) {
                        push(base, s, out);
                    }
                }
            }
            for (k, x) in o {
                if is_mcp_key(k) {
                    match x {
                        Value::String(s) => push(base, s, out),
                        other => walk(other, base, true, depth + 1, nodes, out),
                    }
                } else if x.is_object() || x.is_array() {
                    walk(x, base, tagged, depth + 1, nodes, out);
                }
            }
        }
        // Only reached from an array under an `mcp` key.
        Value::String(s) if tagged => push(base, s, out),
        _ => {}
    }
}

fn mentions_mcp(s: &str) -> bool {
    let s = s.to_ascii_lowercase();
    s.contains("mcp") || s.contains("streamable-http") || s.contains("streamable_http")
}

fn is_mcp_key(k: &str) -> bool {
    matches!(
        k.to_ascii_lowercase().as_str(),
        "mcp" | "mcpendpoint" | "mcp_endpoint" | "mcpurl" | "mcp_url" | "mcpservers" | "mcp_servers"
    )
}

fn push(base: &Url, raw: &str, out: &mut Vec<Url>) {
    if out.len() >= MCP_PER_HOST {
        return;
    }
    let Ok(mut url) = base.join(raw.trim()) else {
        return;
    };
    url.set_fragment(None);
    let same_host =
        url.host_str() == base.host_str() && url.port_or_known_default() == base.port_or_known_default();
    let allowed = match url.scheme() {
        "https" => true,
        "http" => same_host,
        _ => false,
    };
    if !allowed || url.path().to_ascii_lowercase().ends_with(".json") || out.contains(&url) {
        return;
    }
    out.push(url);
}

/// What the discovery request to one endpoint found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpRow {
    pub host_key: String,
    pub url: String,
    pub status: Option<u16>,
    /// The endpoint answered with a JSON-RPC response.
    pub answered: bool,
    pub protocol_versions: Vec<String>,
    /// 401 with a `WWW-Authenticate` challenge: not broken, needs a login.
    pub requires_sign_in: bool,
    pub error: Option<String>,
    pub checked_at: i64,
}

struct Reply {
    status: u16,
    challenge: bool,
    rpc: Option<Value>,
}

/// Sends the discovery request to `url`, then `initialize` when the server
/// answers that the method does not exist. Nothing else is ever sent.
pub async fn probe_mcp(client: &Client, host_key: &str, url: &Url) -> McpRow {
    let mut row = McpRow {
        host_key: host_key.to_string(),
        url: url.to_string(),
        status: None,
        answered: false,
        protocol_versions: Vec::new(),
        requires_sign_in: false,
        error: None,
        checked_at: jiff::Timestamp::now().as_millisecond(),
    };
    let discover = json!({"jsonrpc": "2.0", "id": 1, "method": "server/discover", "params": {}});
    let mut reply = match post(client, url, &discover, Some(DISCOVER_VERSION)).await {
        Ok(r) => r,
        Err(e) => {
            row.error = Some(e);
            return row;
        }
    };
    if rpc_error_code(&reply.rpc) == Some(-32601) {
        let initialize = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": INITIALIZE_VERSION,
                "capabilities": {},
                "clientInfo": {"name": "SpiderLens", "version": env!("CARGO_PKG_VERSION")}
            }
        });
        reply = match post(client, url, &initialize, None).await {
            Ok(r) => r,
            Err(e) => {
                row.error = Some(e);
                return row;
            }
        };
    }
    row.status = Some(reply.status);
    if reply.status == 401 && reply.challenge {
        row.requires_sign_in = true;
        return row;
    }
    match &reply.rpc {
        Some(rpc) => {
            row.answered = true;
            row.protocol_versions = protocol_versions(rpc);
            if let Some(msg) = rpc
                .get("error")
                .and_then(|e| e.get("message"))
                .and_then(Value::as_str)
            {
                row.error = Some(msg.chars().take(200).collect());
            }
        }
        None => row.error = Some(format!("HTTP {} without a JSON-RPC answer", reply.status)),
    }
    row
}

async fn post(client: &Client, url: &Url, body: &Value, version: Option<&str>) -> Result<Reply, String> {
    let mut request = client
        .post(url.clone())
        .timeout(MCP_TIMEOUT)
        .header(CONTENT_TYPE, "application/json")
        .header(ACCEPT, "application/json, text/event-stream")
        .body(body.to_string());
    if let Some(v) = version {
        request = request.header("MCP-Protocol-Version", v);
    }
    let work = async {
        let mut response = request.send().await.map_err(|e| describe(&e))?;
        let status = response.status().as_u16();
        let challenge = response.headers().contains_key(WWW_AUTHENTICATE);
        let mut bytes: Vec<u8> = Vec::new();
        let mut rpc = None;
        while let Some(chunk) = response.chunk().await.map_err(|e| describe(&e))? {
            let room = MCP_MAX_BYTES - bytes.len();
            bytes.extend_from_slice(&chunk[..chunk.len().min(room)]);
            // An event stream may stay open after its answer.
            rpc = parse_rpc(&bytes);
            if rpc.is_some() || bytes.len() >= MCP_MAX_BYTES {
                break;
            }
        }
        Ok::<Reply, String>(Reply {
            status,
            challenge,
            rpc: rpc.or_else(|| parse_rpc(&bytes)),
        })
    };
    match tokio::time::timeout(MCP_TIMEOUT, work).await {
        Ok(r) => r,
        Err(_) => Err("timed out after 10 seconds".into()),
    }
}

fn describe(e: &reqwest::Error) -> String {
    let kind = if e.is_timeout() {
        "timed out"
    } else if e.is_connect() {
        "could not connect"
    } else {
        "request failed"
    };
    kind.into()
}

/// The JSON-RPC response in a body served as JSON or as server-sent events.
pub fn parse_rpc(bytes: &[u8]) -> Option<Value> {
    let is_rpc = |v: &Value| {
        v.is_object() && v.get("jsonrpc").is_some() && (v.get("result").is_some() || v.get("error").is_some())
    };
    if let Ok(v) = serde_json::from_slice::<Value>(bytes)
        && is_rpc(&v)
    {
        return Some(v);
    }
    let text = String::from_utf8_lossy(bytes);
    text.lines()
        .filter_map(|l| l.strip_prefix("data:"))
        .filter_map(|d| serde_json::from_str::<Value>(d.trim()).ok())
        .find(is_rpc)
}

fn rpc_error_code(rpc: &Option<Value>) -> Option<i64> {
    rpc.as_ref()?.get("error")?.get("code")?.as_i64()
}

/// Protocol versions a discovery or initialize answer (or its error data)
/// states.
fn protocol_versions(rpc: &Value) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut take = |v: Option<&Value>| match v {
        Some(Value::String(s)) if !s.is_empty() => out.push(s.chars().take(40).collect()),
        Some(Value::Array(a)) => out.extend(
            a.iter()
                .filter_map(Value::as_str)
                .take(20)
                .map(|s| s.chars().take(40).collect()),
        ),
        _ => {}
    };
    for holder in [rpc.get("result"), rpc.get("error").and_then(|e| e.get("data"))]
        .into_iter()
        .flatten()
    {
        for k in [
            "supportedVersions",
            "supported_versions",
            "protocolVersions",
            "supported",
            "protocolVersion",
        ] {
            take(holder.get(k));
        }
    }
    out.dedup();
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> Url {
        Url::parse("https://example.test/.well-known/ai-catalog.json").unwrap()
    }

    fn found(doc: &str) -> Vec<String> {
        mcp_endpoints(&base(), doc.as_bytes())
            .iter()
            .map(Url::to_string)
            .collect()
    }

    #[test]
    fn md_outcome_by_status_and_type() {
        assert_eq!(md_outcome(200, Some("text/markdown; charset=utf-8")), "ok");
        assert_eq!(md_outcome(200, Some("Text/Plain")), "ok");
        assert_eq!(md_outcome(204, Some("text/x-markdown")), "ok");
        assert_eq!(md_outcome(200, Some("text/html")), "wrong_type");
        assert_eq!(md_outcome(200, None), "wrong_type");
        assert_eq!(md_outcome(404, Some("text/markdown")), "error");
        assert_eq!(md_outcome(500, None), "error");
        assert_eq!(md_outcome(301, Some("text/markdown")), "error");
    }

    #[test]
    fn md_plan_caps_distinct_urls_per_host_and_groups_pages() {
        let mut plan = MdPlan::new(2);
        let host = "https://a.test:443";
        plan.add(1, "https://a.test/one", host, "/one.md");
        plan.add(2, "https://a.test/dir/two", host, "two.md");
        plan.add(3, "https://a.test/other", host, "/one.md#top");
        plan.add(4, "https://a.test/three", host, "/three.md");
        plan.add(5, "https://a.test/four", host, "https://b.test/four.md");
        assert_eq!(plan.jobs.len(), 2);
        assert_eq!(plan.jobs[0].url.as_str(), "https://a.test/one.md");
        assert_eq!(plan.jobs[0].url_ids, vec![1, 3]);
        assert_eq!(plan.jobs[1].url.as_str(), "https://a.test/dir/two.md");
        assert_eq!(plan.unverified, vec![4, 5]);
    }

    #[test]
    fn extracts_endpoints_from_tagged_entries_and_mcp_keys() {
        let doc = r#"{
            "entries": [
                {"mediaType": "application/mcp+json", "url": "/mcp"},
                {"name": "docs", "transport": "streamable-http", "endpoint": "https://api.example.test/v1/mcp"},
                {"type": "api", "url": "/openapi"},
                {"mediaType": "application/mcp-server+json", "url": "/.well-known/mcp/server-card.json"}
            ],
            "mcp": "/tools/mcp",
            "services": {"mcp": {"url": "https://example.test/other"}},
            "remotes": [{"type": "streamable-http", "url": "http://elsewhere.test/mcp"}]
        }"#;
        let urls = found(doc);
        assert!(urls.contains(&"https://example.test/mcp".to_string()), "{urls:?}");
        assert!(urls.contains(&"https://api.example.test/v1/mcp".to_string()));
        assert!(urls.contains(&"https://example.test/tools/mcp".to_string()));
        assert!(urls.contains(&"https://example.test/other".to_string()));
        // An API entry, a card (.json) and a plain http address on another
        // host are not endpoints.
        assert!(
            !urls
                .iter()
                .any(|u| u.ends_with("/openapi") || u.ends_with(".json"))
        );
        assert!(!urls.iter().any(|u| u.starts_with("http://elsewhere")));
    }

    #[test]
    fn endpoint_extraction_is_capped_deduplicated_and_defensive() {
        let many: Vec<String> = (0..20)
            .map(|i| format!(r#"{{"transport": "mcp", "url": "/mcp/{}"}}"#, i % 8))
            .collect();
        let urls = found(&format!(r#"{{"entries": [{}]}}"#, many.join(",")));
        assert_eq!(urls.len(), MCP_PER_HOST);
        let unique: std::collections::HashSet<_> = urls.iter().collect();
        assert_eq!(unique.len(), urls.len());
        assert!(found("not json").is_empty());
        assert!(found("[[[[[[[[[[[[[[[{\"mcp\": \"/x\"}]]]]]]]]]]]]]]]").is_empty());
        assert!(found(r#"{"entries": [{"name": "a"}]}"#).is_empty());
    }

    #[test]
    fn reads_json_and_event_stream_answers() {
        let json = br#"{"jsonrpc":"2.0","id":1,"result":{"supportedVersions":["2026-07-28","2025-11-25"]}}"#;
        let rpc = parse_rpc(json).unwrap();
        assert_eq!(protocol_versions(&rpc), vec!["2026-07-28", "2025-11-25"]);
        let sse = b"event: message\ndata: {\"jsonrpc\":\"2.0\",\"id\":1,\"error\":{\"code\":-32601,\"message\":\"nope\"}}\n\n";
        let rpc = parse_rpc(sse);
        assert_eq!(rpc_error_code(&rpc), Some(-32601));
        let init = br#"{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":"2025-11-25","capabilities":{}}}"#;
        assert_eq!(protocol_versions(&parse_rpc(init).unwrap()), vec!["2025-11-25"]);
        assert!(parse_rpc(b"<html>").is_none());
        assert!(parse_rpc(br#"{"hello": 1}"#).is_none());
    }
}
