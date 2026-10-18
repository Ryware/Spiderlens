//! The `agent_entry_points` Crawl Analysis pass (spec 016 plan D4, FR-008 to
//! FR-010): what each internal host offers to AI agents, into
//! `agent_entry_points`.
//!
//! Two sources, both read from data the crawl already stored, so the pass
//! makes no request:
//!
//! - the three `/.well-known/` files the site file fetch kept in
//!   `site_files` (AI Catalog, API catalog, A2A agent card), parsed here;
//! - the `service-desc`, `service-doc` and `api-catalog` links found on pages
//!   (`agent_links`, raw HTML), listed as `not_fetched`.
//!
//! Parsing is defensive: documents are capped by the fetch at 1 MB, `serde_json`
//! limits nesting, lists are cut at [`MAX_ITEMS`] and nothing indexes or
//! unwraps input.

use std::collections::HashSet;
use std::time::Duration;

use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use spiderlens_engine::extract::body;
use ts_rs::TS;
use url::Url;

use crate::Result;
use crate::analysis::{Pass, PassContext};
use crate::reader::Reader;
use crate::tabs::Need;

/// Entries and skills kept in a summary; the document may list more.
const MAX_ITEMS: usize = 200;
/// Problems kept per document.
const MAX_PROBLEMS: usize = 25;
/// Longest text kept from a document.
const MAX_TEXT: usize = 500;
/// Page-linked entry points listed in one crawl.
const MAX_LINK_ROWS: usize = 10_000;
/// The site file fetch keeps this much of a body (`site_files::MAX_BYTES`).
const MAX_BODY: usize = 1024 * 1024 + 1;

/// The well-known path of each document kind.
const WELL_KNOWN: [(&str, &str); 3] = [
    ("/.well-known/ai-catalog.json", "ai_catalog"),
    ("/.well-known/api-catalog", "api_catalog"),
    ("/.well-known/agent-card.json", "a2a_card"),
];

pub struct AgentEntryPoints;

impl Pass for AgentEntryPoints {
    fn id(&self) -> &'static str {
        "agent_entry_points"
    }
    fn version(&self) -> u32 {
        1
    }
    fn needs(&self) -> &'static [Need] {
        &[Need::Agents]
    }
    /// A few documents per host and one pass over the agent links.
    fn budget(&self) -> Duration {
        Duration::from_secs(1)
    }
    fn run(&self, cx: &mut PassContext<'_>) -> Result<()> {
        let conn = cx.conn;
        let s = cx.schema;
        create(conn, s)?;
        conn.execute(&format!("DELETE FROM {s}.agent_entry_points"), [])?;
        const STEPS: u64 = 2;
        cx.progress(0, STEPS, "Agent entry points")?;
        well_known(conn, s)?;
        cx.progress(1, STEPS, "Agent entry points")?;
        page_links(conn, s)?;
        cx.progress(STEPS, STEPS, "Agent entry points")?;
        Ok(())
    }
}

/// The pass's table in `schema` (also created by the crawl file's spec 016
/// migration step).
fn create(conn: &Connection, schema: &str) -> rusqlite::Result<()> {
    conn.execute_batch(&format!(
        "CREATE TABLE IF NOT EXISTS {schema}.agent_entry_points (
             id INTEGER PRIMARY KEY,
             host_key TEXT NOT NULL,
             kind TEXT NOT NULL,
             url TEXT NOT NULL,
             found_via TEXT NOT NULL,
             found_on INTEGER,
             status INTEGER,
             outcome TEXT NOT NULL,
             content_type TEXT,
             size INTEGER,
             convention_version TEXT,
             problems TEXT,
             summary TEXT,
             mcp_discovery TEXT);
         CREATE INDEX IF NOT EXISTS {schema}.agent_entry_points_host
             ON agent_entry_points(host_key, kind);"
    ))
}

const INSERT: &str = "(host_key, kind, url, found_via, found_on, status, outcome, content_type,
                      size, convention_version, problems, summary)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)";

/// One `site_files` row of a well-known path: host, path, status, content
/// type, size, outcome, error, zstd body.
type FileRow = (
    String,
    String,
    Option<i64>,
    Option<String>,
    Option<i64>,
    String,
    Option<String>,
    Option<Vec<u8>>,
);

fn well_known(conn: &Connection, s: &str) -> Result<()> {
    let mut read = conn.prepare(
        "SELECT host_key, path, status, content_type, size, outcome, error, body
         FROM site_files WHERE path IN (?1, ?2, ?3) ORDER BY host_key, path",
    )?;
    let files: Vec<FileRow> = read
        .query_map([WELL_KNOWN[0].0, WELL_KNOWN[1].0, WELL_KNOWN[2].0], |r| {
            Ok((
                r.get(0)?,
                r.get(1)?,
                r.get(2)?,
                r.get(3)?,
                r.get(4)?,
                r.get(5)?,
                r.get(6)?,
                r.get(7)?,
            ))
        })?
        .collect::<rusqlite::Result<_>>()?;
    let mut insert = conn.prepare(&format!("INSERT INTO {s}.agent_entry_points {INSERT}"))?;
    for (host_key, path, status, content_type, size, outcome, error, packed) in files {
        let Some(kind) = WELL_KNOWN.iter().find(|(p, _)| *p == path).map(|(_, k)| *k) else {
            continue;
        };
        let bytes = packed
            .as_deref()
            .and_then(|p| body::decompress(p, size.unwrap_or(0).clamp(0, MAX_BODY as i64) as usize).ok());
        let e = evaluate(
            kind,
            &outcome,
            error.as_deref(),
            content_type.as_deref(),
            bytes.as_deref(),
        );
        let raw = format!("{host_key}{path}");
        let url = Url::parse(&raw).map_or(raw, |u| u.to_string());
        // Documents that were read carry a problems list, even an empty one.
        let read_ok = e.summary.is_some() || !e.problems.is_empty();
        let problems = read_ok.then(|| serde_json::to_string(&e.problems).unwrap_or_else(|_| "[]".into()));
        insert.execute(params![
            host_key,
            kind,
            url,
            "well_known",
            Option::<i64>::None,
            status,
            e.outcome,
            content_type,
            size,
            e.convention_version,
            problems,
            e.summary.map(|v| v.to_string()),
        ])?;
    }
    Ok(())
}

/// Service links found on pages, one row per distinct address per host.
fn page_links(conn: &Connection, s: &str) -> Result<()> {
    let mut read = conn.prepare(
        "SELECT l.url_id, u.url, l.kind, l.href, l.via
         FROM agent_links l JOIN urls u ON u.id = l.url_id
         WHERE l.source = 'raw' AND l.kind IN ('service_desc', 'service_doc', 'api_catalog')
         ORDER BY l.url_id, l.pos",
    )?;
    let mut insert = conn.prepare(&format!("INSERT INTO {s}.agent_entry_points {INSERT}"))?;
    let mut seen: HashSet<(String, String, String)> = HashSet::new();
    let mut rows = read.query([])?;
    while let Some(r) = rows.next()? {
        if seen.len() >= MAX_LINK_ROWS {
            break;
        }
        let url_id: i64 = r.get(0)?;
        let page: String = r.get(1)?;
        let kind: String = r.get(2)?;
        let href: String = r.get(3)?;
        let via: String = r.get(4)?;
        let Some((host_key, target)) = resolve(&page, &href) else {
            continue;
        };
        if !seen.insert((host_key.clone(), kind.clone(), target.clone())) {
            continue;
        }
        let found_via = if via == "header" {
            "link_header"
        } else {
            "page_link"
        };
        insert.execute(params![
            host_key,
            kind,
            target,
            found_via,
            url_id,
            Option::<i64>::None,
            "not_fetched",
            Option::<String>::None,
            Option::<i64>::None,
            Option::<String>::None,
            Option::<String>::None,
            Option::<String>::None,
        ])?;
    }
    Ok(())
}

/// The page's host (as `hosts` and `site_files` key it) and the linked
/// address resolved against the page; None for anything but http(s).
fn resolve(page: &str, href: &str) -> Option<(String, String)> {
    let page = Url::parse(page).ok()?;
    let mut target = page.join(href.trim()).ok()?;
    if !matches!(target.scheme(), "http" | "https") {
        return None;
    }
    target.set_fragment(None);
    let host_key = format!(
        "{}://{}:{}",
        page.scheme(),
        page.host_str()?,
        page.port_or_known_default()?
    );
    Some((host_key, target.to_string()))
}

/// One finding about a document (`problems` column).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct EntryProblem {
    pub code: String,
    pub message: String,
}

/// What reading one well-known file gave.
#[derive(Debug, Clone, PartialEq)]
pub struct Evaluated {
    /// ok, absent, blocked_robots, error, too_large, wrong_type or invalid.
    pub outcome: &'static str,
    /// What the document states, `unrecognised` when its shape is unknown,
    /// None when it is a known shape that states no version.
    pub convention_version: Option<String>,
    /// name, description and endpoints or entries; None unless parsed.
    pub summary: Option<Value>,
    pub problems: Vec<EntryProblem>,
}

fn problem(list: &mut Vec<EntryProblem>, code: &str, message: impl Into<String>) {
    if list.len() < MAX_PROBLEMS {
        list.push(EntryProblem {
            code: code.into(),
            message: message.into(),
        });
    }
}

/// Reads one stored site file for `kind` (`ai_catalog`, `api_catalog` or
/// `a2a_card`). `site_outcome`, `error`, `content_type` and `body` are the
/// `site_files` columns (body already decompressed). Never panics.
pub fn evaluate(
    kind: &str,
    site_outcome: &str,
    error: Option<&str>,
    content_type: Option<&str>,
    body: Option<&[u8]>,
) -> Evaluated {
    let bare = |outcome| Evaluated {
        outcome,
        convention_version: None,
        summary: None,
        problems: Vec::new(),
    };
    match site_outcome {
        "not_found" => return bare("absent"),
        "skipped" => {
            return bare(if error.is_some_and(|e| e.starts_with("blocked by robots")) {
                "blocked_robots"
            } else {
                "error"
            });
        }
        "too_large" => return bare("too_large"),
        "ok" => {}
        _ => return bare("error"),
    }
    let mut problems = Vec::new();
    let bytes = body.unwrap_or_default();
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    let Ok(doc) = serde_json::from_slice::<Value>(bytes) else {
        problem(
            &mut problems,
            "not_json",
            "The file is not JSON (a missing page often answers with HTML).",
        );
        return Evaluated {
            outcome: "wrong_type",
            convention_version: None,
            summary: None,
            problems,
        };
    };
    let media = content_type.unwrap_or("").to_ascii_lowercase();
    let (expected, ok) = if kind == "api_catalog" {
        ("application/linkset+json", media.contains("linkset+json"))
    } else {
        ("application/json", media.contains("json"))
    };
    if !ok {
        problem(
            &mut problems,
            "content_type",
            format!(
                "Served as {}; expected {expected}.",
                content_type.unwrap_or("no content type")
            ),
        );
    }
    let parsed = match kind {
        "ai_catalog" => parse_ai_catalog(&doc),
        "api_catalog" => parse_api_catalog(&doc),
        _ => parse_a2a_card(&doc),
    };
    problems.extend(parsed.problems);
    problems.truncate(MAX_PROBLEMS);
    if !parsed.valid {
        return Evaluated {
            outcome: "invalid",
            convention_version: Some("unrecognised".into()),
            summary: None,
            problems,
        };
    }
    Evaluated {
        outcome: "ok",
        convention_version: parsed.version,
        summary: Some(parsed.summary),
        problems,
    }
}

struct Parsed {
    valid: bool,
    version: Option<String>,
    summary: Value,
    problems: Vec<EntryProblem>,
}

impl Parsed {
    fn invalid(code: &str, message: &str) -> Self {
        let mut problems = Vec::new();
        problem(&mut problems, code, message);
        Self {
            valid: false,
            version: None,
            summary: Value::Null,
            problems,
        }
    }
}

fn clip(s: &str) -> String {
    s.trim().chars().take(MAX_TEXT).collect()
}

/// The first of `keys` that is a non-blank string.
fn text(v: &Value, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|k| match v.get(*k) {
        Some(Value::String(s)) if !s.trim().is_empty() => Some(clip(s)),
        _ => None,
    })
}

/// `href` of each link object in `v[rel]`, as the Linkset format lists them.
fn hrefs(v: &Value, rel: &str) -> Vec<String> {
    v.get(rel)
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|l| text(l, &["href"]))
                .take(MAX_ITEMS)
                .collect()
        })
        .unwrap_or_default()
}

/// The AI Catalog: a host description and a list of entries (agents, MCP
/// servers, API descriptions), each with a location or an inline document.
fn parse_ai_catalog(doc: &Value) -> Parsed {
    let Some(obj) = doc.as_object() else {
        return Parsed::invalid("not_object", "The catalog is not a JSON object.");
    };
    let Some(list) = ["entries", "resources", "catalog"]
        .iter()
        .find_map(|k| obj.get(*k).and_then(Value::as_array))
    else {
        return Parsed::invalid("no_entries", "The catalog has no `entries` list.");
    };
    let host = doc.get("host").unwrap_or(&Value::Null);
    let mut problems = Vec::new();
    let mut entries = Vec::new();
    for (i, e) in list.iter().take(MAX_ITEMS).enumerate() {
        if !e.is_object() {
            problem(
                &mut problems,
                "entry_not_object",
                format!("Entry {} is not an object.", i + 1),
            );
            continue;
        }
        let url = text(e, &["url", "href", "location"]);
        let inline = ["inline", "data", "card"].iter().any(|k| e.get(*k).is_some());
        if url.is_none() && !inline {
            problem(
                &mut problems,
                "entry_without_location",
                format!("Entry {} has neither a `url` nor an inline document.", i + 1),
            );
        }
        entries.push(json!({
            "id": text(e, &["identifier", "id"]),
            "name": text(e, &["displayName", "name", "title"]),
            "type": text(e, &["mediaType", "type", "kind"]),
            "url": url,
            "inline": inline,
        }));
    }
    if list.is_empty() {
        problem(&mut problems, "no_entries", "The catalog lists no entries.");
    }
    Parsed {
        valid: true,
        version: text(doc, &["specVersion", "spec_version", "version"]),
        summary: json!({
            "name": text(doc, &["name", "displayName"]).or_else(|| text(host, &["displayName", "name"])),
            "description": text(doc, &["description"]).or_else(|| text(host, &["description"])),
            "entries": entries,
            "entry_count": list.len(),
        }),
        problems,
    }
}

/// An API catalog (RFC 9727): a Linkset whose items each name an API
/// (`anchor`) and its `service-desc` and `service-doc` links.
fn parse_api_catalog(doc: &Value) -> Parsed {
    let Some(set) = doc.get("linkset").and_then(Value::as_array) else {
        return Parsed::invalid("no_linkset", "The document has no `linkset` list (RFC 9727).");
    };
    let mut problems = Vec::new();
    let mut entries = Vec::new();
    for (i, item) in set.iter().take(MAX_ITEMS).enumerate() {
        if !item.is_object() {
            problem(
                &mut problems,
                "entry_not_object",
                format!("Item {} is not an object.", i + 1),
            );
            continue;
        }
        let anchor = text(item, &["anchor"]);
        let desc = hrefs(item, "service-desc");
        let docs = hrefs(item, "service-doc");
        if anchor.is_none() {
            problem(
                &mut problems,
                "missing_anchor",
                format!("Item {} has no `anchor`.", i + 1),
            );
        }
        if desc.is_empty() && docs.is_empty() {
            problem(
                &mut problems,
                "no_service_links",
                format!("Item {} has no `service-desc` or `service-doc` link.", i + 1),
            );
        }
        entries.push(json!({ "anchor": anchor, "service_desc": desc, "service_doc": docs }));
    }
    if set.is_empty() {
        problem(&mut problems, "no_entries", "The catalog lists no APIs.");
    }
    Parsed {
        valid: true,
        version: None,
        summary: json!({ "entries": entries, "entry_count": set.len() }),
        problems,
    }
}

/// An A2A agent card: identity, endpoints and skills.
fn parse_a2a_card(doc: &Value) -> Parsed {
    if !doc.is_object() {
        return Parsed::invalid("not_object", "The agent card is not a JSON object.");
    }
    let Some(name) = text(doc, &["name"]) else {
        return Parsed::invalid("missing_name", "The agent card has no `name`.");
    };
    let mut problems = Vec::new();
    let description = text(doc, &["description"]);
    if description.is_none() {
        problem(
            &mut problems,
            "missing_description",
            "The agent card has no `description`.",
        );
    }
    let mut endpoints: Vec<String> = text(doc, &["url"]).into_iter().collect();
    for key in ["supportedInterfaces", "additionalInterfaces"] {
        if let Some(a) = doc.get(key).and_then(Value::as_array) {
            endpoints.extend(a.iter().filter_map(|i| text(i, &["url"])).take(MAX_ITEMS));
        }
    }
    endpoints.dedup();
    endpoints.truncate(MAX_ITEMS);
    if endpoints.is_empty() {
        problem(
            &mut problems,
            "missing_url",
            "The agent card names no endpoint `url`.",
        );
    }
    let skills = doc.get("skills").and_then(Value::as_array);
    if skills.is_none_or(|s| s.is_empty()) {
        problem(&mut problems, "no_skills", "The agent card lists no `skills`.");
    }
    let listed: Vec<Value> = skills
        .map(|s| {
            s.iter()
                .take(MAX_ITEMS)
                .map(|k| {
                    json!({
                        "id": text(k, &["id"]),
                        "name": text(k, &["name"]),
                        "description": text(k, &["description"]),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    Parsed {
        valid: true,
        version: text(doc, &["protocolVersion"]),
        summary: json!({
            "name": name,
            "description": description,
            "endpoints": endpoints,
            "skills": listed,
            "skill_count": skills.map_or(0, Vec::len),
            "agent_version": text(doc, &["version"]),
        }),
        problems,
    }
}

/// One row of `agent_entry_points`.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct EntryPointRow {
    #[ts(type = "number")]
    pub id: i64,
    pub host_key: String,
    /// ai_catalog, api_catalog, a2a_card, service_desc or service_doc.
    pub kind: String,
    pub url: String,
    /// well_known, page_link or link_header.
    pub found_via: String,
    /// The page that linked it; None for well-known files.
    #[ts(type = "number | null")]
    pub found_on: Option<i64>,
    pub status: Option<u16>,
    /// ok, absent, blocked_robots, error, too_large, wrong_type, invalid or
    /// not_fetched.
    pub outcome: String,
    pub content_type: Option<String>,
    #[ts(type = "number | null")]
    pub size: Option<i64>,
    pub convention_version: Option<String>,
    pub problems: Vec<EntryProblem>,
    /// name, description and endpoints or entries.
    pub summary: Option<Value>,
    /// Reserved for the MCP discovery request (spec 016 FR-011).
    pub mcp_discovery: Option<Value>,
}

impl Reader {
    /// The agent entry points of every host, once the `agent_entry_points`
    /// pass has run (stored, or in this session's temp schema); empty before.
    pub fn agent_entry_points(&self) -> Result<Vec<EntryPointRow>> {
        let mut stmt = match self.conn.prepare(
            "SELECT id, host_key, kind, url, found_via, found_on, status, outcome, content_type,
                    size, convention_version, problems, summary, mcp_discovery
             FROM agent_entry_points ORDER BY host_key, kind, id",
        ) {
            Ok(s) => s,
            Err(rusqlite::Error::SqliteFailure(_, Some(m))) if m.contains("no such table") => {
                return Ok(Vec::new());
            }
            Err(e) => return Err(e.into()),
        };
        let json = |s: Option<String>| s.and_then(|s| serde_json::from_str::<Value>(&s).ok());
        let rows = stmt
            .query_map([], |r| {
                Ok(EntryPointRow {
                    id: r.get(0)?,
                    host_key: r.get(1)?,
                    kind: r.get(2)?,
                    url: r.get(3)?,
                    found_via: r.get(4)?,
                    found_on: r.get(5)?,
                    status: r.get::<_, Option<i64>>(6)?.and_then(|s| u16::try_from(s).ok()),
                    outcome: r.get(7)?,
                    content_type: r.get(8)?,
                    size: r.get(9)?,
                    convention_version: r.get(10)?,
                    problems: r
                        .get::<_, Option<String>>(11)?
                        .and_then(|p| serde_json::from_str(&p).ok())
                        .unwrap_or_default(),
                    summary: json(r.get(12)?),
                    mcp_discovery: json(r.get(13)?),
                })
            })?
            .collect::<rusqlite::Result<_>>()?;
        Ok(rows)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn eval(kind: &str, ct: &str, body: &str) -> Evaluated {
        evaluate(kind, "ok", None, Some(ct), Some(body.as_bytes()))
    }

    fn codes(e: &Evaluated) -> Vec<&str> {
        e.problems.iter().map(|p| p.code.as_str()).collect()
    }

    #[test]
    fn site_outcomes_map_without_parsing() {
        assert_eq!(
            evaluate("ai_catalog", "not_found", None, None, None).outcome,
            "absent"
        );
        assert_eq!(
            evaluate("ai_catalog", "too_large", None, None, None).outcome,
            "too_large"
        );
        assert_eq!(
            evaluate("ai_catalog", "error", Some("x"), None, None).outcome,
            "error"
        );
        let robots = evaluate(
            "a2a_card",
            "skipped",
            Some("blocked by robots.txt: /.well-known/"),
            None,
            None,
        );
        assert_eq!(robots.outcome, "blocked_robots");
        let gone = evaluate(
            "a2a_card",
            "skipped",
            Some("host unavailable: the crawl gave up on it"),
            None,
            None,
        );
        assert_eq!(gone.outcome, "error");
    }

    #[test]
    fn valid_ai_catalog() {
        let e = eval(
            "ai_catalog",
            "application/json",
            r#"{"specVersion":"1.0","host":{"displayName":"Example"},
                "entries":[{"identifier":"a","displayName":"Agent","mediaType":"application/a2a-agent-card+json","url":"/a.json"},
                           {"identifier":"b","inline":{}}]}"#,
        );
        assert_eq!(e.outcome, "ok");
        assert_eq!(e.convention_version.as_deref(), Some("1.0"));
        assert!(e.problems.is_empty());
        let s = e.summary.unwrap();
        assert_eq!(s["name"], "Example");
        assert_eq!(s["entry_count"], 2);
        assert_eq!(s["entries"][0]["url"], "/a.json");
    }

    #[test]
    fn ai_catalog_entry_without_location_is_a_problem() {
        let e = eval(
            "ai_catalog",
            "application/json",
            r#"{"entries":[{"identifier":"a"},3]}"#,
        );
        assert_eq!(e.outcome, "ok");
        assert_eq!(e.convention_version, None);
        assert_eq!(codes(&e), ["entry_without_location", "entry_not_object"]);
    }

    #[test]
    fn invalid_ai_catalog_is_unrecognised() {
        let e = eval("ai_catalog", "application/json", r#"{"hello":"world"}"#);
        assert_eq!(e.outcome, "invalid");
        assert_eq!(e.convention_version.as_deref(), Some("unrecognised"));
        assert!(e.summary.is_none());
        assert_eq!(codes(&e), ["no_entries"]);
    }

    #[test]
    fn valid_api_catalog() {
        let e = eval(
            "api_catalog",
            "application/linkset+json; profile=\"https://www.rfc-editor.org/info/rfc9727\"",
            r#"{"linkset":[{"anchor":"https://example.com/api",
                "service-desc":[{"href":"https://example.com/openapi.json","type":"application/json"}],
                "service-doc":[{"href":"https://example.com/docs"}]}]}"#,
        );
        assert_eq!(e.outcome, "ok");
        assert!(e.problems.is_empty());
        let s = e.summary.unwrap();
        assert_eq!(s["entries"][0]["anchor"], "https://example.com/api");
        assert_eq!(
            s["entries"][0]["service_desc"][0],
            "https://example.com/openapi.json"
        );
        assert_eq!(s["entries"][0]["service_doc"][0], "https://example.com/docs");
    }

    #[test]
    fn api_catalog_problems_and_media_type() {
        let e = eval(
            "api_catalog",
            "application/json",
            r#"{"linkset":[{"status":[{"href":"/s"}]},"x"]}"#,
        );
        assert_eq!(e.outcome, "ok");
        assert_eq!(
            codes(&e),
            [
                "content_type",
                "missing_anchor",
                "no_service_links",
                "entry_not_object"
            ]
        );
    }

    #[test]
    fn api_catalog_without_linkset_is_invalid() {
        let e = eval("api_catalog", "application/linkset+json", "[1,2,3]");
        assert_eq!(e.outcome, "invalid");
        assert_eq!(codes(&e), ["no_linkset"]);
    }

    #[test]
    fn valid_a2a_card() {
        let e = eval(
            "a2a_card",
            "application/json",
            r#"{"name":"Helper","description":"Helps","url":"https://example.com/a2a","protocolVersion":"0.3.0",
                "skills":[{"id":"s1","name":"Search","description":"Finds things"}]}"#,
        );
        assert_eq!(e.outcome, "ok");
        assert_eq!(e.convention_version.as_deref(), Some("0.3.0"));
        assert!(e.problems.is_empty());
        let s = e.summary.unwrap();
        assert_eq!(s["name"], "Helper");
        assert_eq!(s["endpoints"][0], "https://example.com/a2a");
        assert_eq!(s["skill_count"], 1);
    }

    #[test]
    fn a2a_card_without_name_is_invalid_and_thin_card_has_problems() {
        let e = eval("a2a_card", "application/json", r#"{"url":"https://example.com"}"#);
        assert_eq!(e.outcome, "invalid");
        let thin = eval("a2a_card", "application/json", r#"{"name":"X"}"#);
        assert_eq!(thin.outcome, "ok");
        assert_eq!(codes(&thin), ["missing_description", "missing_url", "no_skills"]);
    }

    #[test]
    fn html_and_empty_bodies_are_the_wrong_type() {
        let html = eval("a2a_card", "text/html", "<!doctype html><html></html>");
        assert_eq!(html.outcome, "wrong_type");
        assert_eq!(codes(&html), ["not_json"]);
        assert_eq!(eval("ai_catalog", "application/json", "").outcome, "wrong_type");
        assert_eq!(
            evaluate("ai_catalog", "ok", None, None, None).outcome,
            "wrong_type"
        );
    }

    #[test]
    fn byte_order_mark_is_skipped() {
        let mut body = b"\xEF\xBB\xBF".to_vec();
        body.extend_from_slice(br#"{"name":"X","description":"d","url":"u","skills":[{}]}"#);
        let e = evaluate("a2a_card", "ok", None, Some("application/json"), Some(&body));
        assert_eq!(e.outcome, "ok");
    }

    #[test]
    fn garbage_and_huge_input_do_not_panic() {
        let garbage: Vec<u8> = (0..4096u32).map(|i| (i * 31 % 251) as u8).collect();
        // Nesting past serde_json's limit is an error, not a stack overflow.
        let deep = format!("{}{}", "[".repeat(100_000), "]".repeat(100_000));
        for kind in ["ai_catalog", "api_catalog", "a2a_card"] {
            let e = evaluate(kind, "ok", None, Some("application/json"), Some(&garbage));
            assert_eq!(e.outcome, "wrong_type");
            assert_eq!(eval(kind, "application/json", &deep).outcome, "wrong_type");
            // JSON that is not an object of the expected shape.
            for body in [
                "null",
                "true",
                "42",
                "\"text\"",
                "[]",
                "{}",
                "{\"entries\":3,\"linkset\":{}}",
            ] {
                let e = eval(kind, "application/json", body);
                assert!(e.outcome == "invalid" || e.outcome == "ok");
            }
        }
    }

    #[test]
    fn long_lists_and_texts_are_cut() {
        let entries: Vec<String> = (0..1000)
            .map(|i| format!(r#"{{"id":"{i}","url":"/{i}"}}"#))
            .collect();
        let body = format!(r#"{{"entries":[{}]}}"#, entries.join(","));
        let s = eval("ai_catalog", "application/json", &body).summary.unwrap();
        assert_eq!(s["entries"].as_array().unwrap().len(), MAX_ITEMS);
        assert_eq!(s["entry_count"], 1000);
        let long = "x".repeat(10_000);
        let card = format!(r#"{{"name":"{long}"}}"#);
        let s = eval("a2a_card", "application/json", &card).summary.unwrap();
        assert_eq!(s["name"].as_str().unwrap().chars().count(), MAX_TEXT);
        // Many bad entries report at most MAX_PROBLEMS problems.
        let bad = format!(r#"{{"entries":[{}]}}"#, vec!["1"; 500].join(","));
        assert_eq!(
            eval("ai_catalog", "application/json", &bad).problems.len(),
            MAX_PROBLEMS
        );
    }

    #[test]
    fn page_links_resolve_against_the_page() {
        let (host, url) = resolve("https://example.com/docs/page", "../openapi.json#x").unwrap();
        assert_eq!(host, "https://example.com:443");
        assert_eq!(url, "https://example.com/openapi.json");
        let (_, other) = resolve("http://example.com/", "https://api.example.com/spec").unwrap();
        assert_eq!(other, "https://api.example.com/spec");
        assert_eq!(resolve("https://example.com/", "mailto:a@b.c"), None);
        assert_eq!(resolve("not a url", "/x"), None);
    }
}
