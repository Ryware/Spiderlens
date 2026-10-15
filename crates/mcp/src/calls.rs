//! What each tool does. Plain synchronous code over `spiderlens_results`,
//! the same read layer as the app and the CLI, so a tab or filter id means
//! the same thing everywhere (plan D7).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::AtomicBool;

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use spiderlens_engine::crawl::{CrawlStore, WriterLock, file};
use spiderlens_results::{
    ListKind, Reader, Sort, TabDef, UrlScope, ViewQuery, ci_contains, tab as find_tab, tabs as all_tabs,
};

use crate::ToolError;
use crate::access::{Access, Allowed, fnv1a};
use crate::envelope::{self, MAX_ANSWER, wrap};

/// Rows per page by default and at most (FR-023).
const DEFAULT_LIMIT: u32 = 100;
const MAX_LIMIT: u32 = 500;
const MAX_EXPORT_ROWS: u32 = 1_000;
/// Items of each list `page_details` includes.
const DETAIL_LIST: u32 = 50;
/// HTML chunk size.
const HTML_CHUNK: usize = 32 * 1024;
/// Room kept for the fields around the rows.
const ROW_BUDGET: usize = MAX_ANSWER - 4 * 1024;

type Args = Map<String, Value>;

/// One open crawl file and the views opened on it, by query.
struct Open {
    reader: Reader,
    views: HashMap<String, u32>,
}

pub(crate) struct Calls {
    pub access: Access,
    open: Mutex<HashMap<PathBuf, Open>>,
}

impl Calls {
    pub fn new(access: Access) -> Self {
        Self {
            access,
            open: Mutex::new(HashMap::new()),
        }
    }

    /// Runs one tool. `crawl` is set to the crawl id the call was about, for
    /// the activity log.
    pub fn call(&self, tool: &str, args: &Args, crawl: &mut Option<String>) -> Result<Value, ToolError> {
        let settings = self.access.check_read()?;
        if tool == "list_crawls" {
            return Ok(self.list_crawls(&settings));
        }
        let id = str_arg(args, "crawl")?.ok_or_else(|| ToolError::invalid("`crawl` is required."))?;
        let target = self.access.resolve(&settings, id)?;
        *crawl = Some(target.id.clone());
        let mut out = match tool {
            "crawl_summary" => self.crawl_summary(&target)?,
            "list_tabs" => self.with_reader(&target.path, |o| list_tabs(o, args))?,
            "query_pages" => self.with_reader(&target.path, |o| query_pages(o, &target.id, args))?,
            "page_details" => self.with_reader(&target.path, |o| page_details(o, args))?,
            "page_agent_findings" => self.with_reader(&target.path, |o| page_agent_findings(o, args))?,
            "page_html" => page_html(&target.path, args)?,
            "export_rows" => export_rows(&target.path, args)?,
            other => return Err(ToolError::invalid(format!("Unknown tool {other}."))),
        };
        if let Some(o) = out.as_object_mut() {
            o.insert("crawl".into(), Value::String(target.id.clone()));
            if let Some(at) = in_progress(&target.path) {
                o.insert("in_progress".into(), Value::Bool(true));
                o.insert("as_of".into(), Value::String(at));
            }
        }
        Ok(out)
    }

    fn with_reader<T>(
        &self,
        path: &Path,
        f: impl FnOnce(&mut Open) -> Result<T, ToolError>,
    ) -> Result<T, ToolError> {
        let mut open = self.open.lock().unwrap_or_else(|e| e.into_inner());
        if !open.contains_key(path) {
            let reader = Reader::open(path).map_err(ToolError::from_results)?;
            open.insert(
                path.to_path_buf(),
                Open {
                    reader,
                    views: HashMap::new(),
                },
            );
        }
        let o = open.get_mut(path).expect("inserted");
        f(o)
    }

    fn list_crawls(&self, settings: &spiderlens_config::AssistantSettings) -> Value {
        let crawls: Vec<Value> = self
            .access
            .list(settings)
            .into_iter()
            .map(|a| match file::info(&a.path) {
                Ok(i) => json!({
                    "crawl": a.id,
                    "name": i.name,
                    "start_url": i.start_url,
                    "started_at": i.started_at,
                    "finished_at": i.ended_at,
                    "state": i.status,
                    "urls": i.counts.discovered,
                    "pages": i.counts.crawled,
                    "saved": !a.unsaved,
                    "file_size": i.size_bytes,
                    "started_by": Value::Null,
                }),
                Err(e) => json!({"crawl": a.id, "readable": false, "error": e.to_string()}),
            })
            .collect();
        let n = crawls.len();
        let mut v = json!({ "crawls": crawls, "count": n });
        if n == 0 {
            v["note"] = "No crawls yet. Crawls the user opens in SpiderLens, and crawl files in folders they allow in Settings › AI assistants, appear here.".into();
        }
        v
    }

    fn crawl_summary(&self, target: &Allowed) -> Result<Value, ToolError> {
        let info = file::info(&target.path).map_err(|e| ToolError::invalid(e.to_string()))?;
        let features: Vec<Value> = self.with_reader(&target.path, |o| {
            Ok(o.reader
                .features()
                .available
                .iter()
                .map(|n| Value::String(n.feature_id().into()))
                .collect())
        })?;
        let mut config = info
            .config
            .as_ref()
            .map(|c| {
                let mut v = serde_json::to_value(&c.config).unwrap_or(Value::Null);
                redact_headers(&mut v);
                v
            })
            .unwrap_or(Value::Null);
        if let Some(o) = config.as_object_mut() {
            // A List-mode input can be 10,000 URLs; the counts say enough.
            o.remove("input");
        }
        let v = json!({
            "name": info.name,
            "file_name": target.path.file_name().map(|n| n.to_string_lossy().to_string()),
            "saved": !target.unsaved,
            "status": info.status,
            "pause_reason": info.pause_reason,
            "mode": info.mode,
            "start_url": info.start_url,
            "started_at": info.started_at,
            "finished_at": info.ended_at,
            "counts": info.counts,
            "size_bytes": info.size_bytes,
            "format_version": info.format_version,
            "data_features": features,
            "missing_secrets": info.missing_secrets.len(),
            "settings": config,
            "resolved_hosts": info.config.as_ref().map(|c| c.resolved_hosts.clone()),
            "complete": info.status == "completed",
        });
        Ok(wrap(
            v,
            &[
                "name",
                "file_name",
                "status",
                "pause_reason",
                "mode",
                "start_url",
                "started_at",
                "finished_at",
                "counts",
                "data_features",
                "settings",
            ],
        ))
    }
}

/// Header values become `set`, whatever they are (secrets are never
/// stored, and plain values can be cookies).
fn redact_headers(config: &mut Value) {
    if let Some(headers) = config
        .pointer_mut("/identity/headers")
        .and_then(Value::as_array_mut)
    {
        for h in headers {
            if let Some(o) = h.as_object_mut() {
                o.insert("value".into(), Value::String("set".into()));
            }
        }
    }
}

/// Set while another process is writing the crawl: the time of its last
/// commit, from the file times.
fn in_progress(path: &Path) -> Option<String> {
    WriterLock::held_by_other(path)?;
    let mut wal = path.file_name()?.to_os_string();
    wal.push("-wal");
    let modified = [path.to_path_buf(), path.with_file_name(wal)]
        .iter()
        .filter_map(|p| std::fs::metadata(p).and_then(|m| m.modified()).ok())
        .max()?;
    let ts = jiff::Timestamp::try_from(modified).ok()?;
    Some(ts.round(jiff::Unit::Second).ok()?.to_string())
}

// ---------------------------------------------------------------------------
// Arguments

fn str_arg<'a>(args: &'a Args, key: &str) -> Result<Option<&'a str>, ToolError> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) => Ok(Some(s)),
        Some(_) => Err(ToolError::invalid(format!("`{key}` must be a string."))),
    }
}

fn u32_arg(args: &Args, key: &str) -> Result<Option<u32>, ToolError> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => v
            .as_u64()
            .and_then(|n| u32::try_from(n).ok())
            .map(Some)
            .ok_or_else(|| ToolError::invalid(format!("`{key}` must be a whole number."))),
    }
}

fn strings_arg(args: &Args, key: &str) -> Result<Vec<String>, ToolError> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(Vec::new()),
        Some(Value::Array(a)) => a
            .iter()
            .map(|v| {
                v.as_str()
                    .map(String::from)
                    .ok_or_else(|| ToolError::invalid(format!("`{key}` must be a list of strings.")))
            })
            .collect(),
        Some(_) => Err(ToolError::invalid(format!("`{key}` must be a list of strings."))),
    }
}

fn tab_arg(args: &Args) -> Result<&'static TabDef, ToolError> {
    let id = str_arg(args, "tab")?.ok_or_else(|| ToolError::invalid("`tab` is required."))?;
    find_tab(id).ok_or_else(|| {
        let ids: Vec<&str> = all_tabs().iter().map(|t| t.id).collect();
        ToolError::invalid(format!("Unknown tab. Tabs: {}.", ids.join(", ")))
    })
}

fn filter_arg(args: &Args, tab: &TabDef) -> Result<&'static str, ToolError> {
    let id = str_arg(args, "filter")?.unwrap_or("all");
    tab.filters
        .iter()
        .find(|f| f.id == id)
        .map(|f| f.id)
        .ok_or_else(|| {
            ToolError::invalid(format!(
                "Unknown filter on {}. Call list_tabs for its filters.",
                tab.id
            ))
        })
}

fn columns_arg(args: &Args, tab: &'static TabDef) -> Result<Vec<usize>, ToolError> {
    let ids = strings_arg(args, "columns")?;
    if ids.is_empty() {
        return Ok(tab
            .columns
            .iter()
            .enumerate()
            .filter(|(_, c)| c.default_visible)
            .map(|(i, _)| i)
            .collect());
    }
    ids.iter().map(|id| column_index(tab, id)).collect()
}

fn column_index(tab: &TabDef, id: &str) -> Result<usize, ToolError> {
    tab.columns.iter().position(|c| c.id == id).ok_or_else(|| {
        ToolError::invalid(format!(
            "Unknown column {id:?} on {}. Call list_tabs for its columns.",
            tab.id
        ))
    })
}

fn page_id(o: &mut Open, args: &Args) -> Result<i64, ToolError> {
    if let Some(v) = args.get("url_id").filter(|v| !v.is_null()) {
        return v
            .as_i64()
            .ok_or_else(|| ToolError::invalid("`url_id` must be a whole number."));
    }
    let url = str_arg(args, "url")?.ok_or_else(|| ToolError::invalid("Pass `url_id` or `url`."))?;
    let store = CrawlStore::open_read_only(o.reader.path()).map_err(|e| ToolError::invalid(e.to_string()))?;
    store
        .id_of(url)
        .map_err(|e| ToolError::invalid(e.to_string()))?
        .ok_or_else(|| ToolError::invalid("This URL is not in the crawl."))
}

// ---------------------------------------------------------------------------
// Tabs and rows

fn list_tabs(o: &mut Open, args: &Args) -> Result<Value, ToolError> {
    let features = o.reader.features().clone();
    let only = str_arg(args, "tab")?;
    if let Some(id) = only
        && find_tab(id).is_none()
    {
        return Err(ToolError::invalid(
            "Unknown tab. Call list_tabs without `tab` for the list.",
        ));
    }
    let mut out = Vec::new();
    for t in all_tabs() {
        let counts = if only == Some(t.id) {
            Some(
                o.reader
                    .counts(t.id, UrlScope::All)
                    .map_err(ToolError::from_results)?,
            )
        } else {
            None
        };
        let filters: Vec<Value> = t
            .filters
            .iter()
            .map(|f| {
                let available = features.has(f.need);
                let mut v = json!({"id": f.id, "label": f.label, "available": available});
                if !available {
                    v["unavailable_reason"] = f.need.unavailable_reason().into();
                }
                if let Some(c) = &counts {
                    v["count"] = c.counts.get(f.id).cloned().flatten().into();
                }
                v
            })
            .collect();
        let columns: Vec<Value> = t
            .columns
            .iter()
            .map(|c| json!({"id": c.id, "label": c.label, "kind": c.kind, "visible_by_default": c.default_visible, "available": features.has(c.need)}))
            .collect();
        out.push(json!({"tab": t.id, "label": t.title, "columns": columns, "filters": filters}));
    }
    Ok(json!({ "tabs": out }))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Condition {
    column: String,
    op: Op,
    #[serde(default)]
    value: Value,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum Op {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    Contains,
    NotContains,
    Empty,
    NotEmpty,
}

fn matches(cell: &Value, op: Op, want: &Value) -> bool {
    let empty = match cell {
        Value::Null => true,
        Value::String(s) => s.is_empty(),
        _ => false,
    };
    let text = |v: &Value| match v {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    };
    let order = || match (cell.as_f64(), want.as_f64()) {
        (Some(a), Some(b)) => a.partial_cmp(&b),
        _ if empty => None,
        _ => Some(spiderlens_results::natural_cmp(&text(cell), &text(want))),
    };
    use std::cmp::Ordering::*;
    match op {
        Op::Empty => empty,
        Op::NotEmpty => !empty,
        Op::Eq => order() == Some(Equal),
        Op::Ne => order() != Some(Equal),
        Op::Lt => order() == Some(Less),
        Op::Le => matches!(order(), Some(Less | Equal)),
        Op::Gt => order() == Some(Greater),
        Op::Ge => matches!(order(), Some(Greater | Equal)),
        Op::Contains => !empty && ci_contains(&text(cell), &text(want)),
        Op::NotContains => empty || !ci_contains(&text(cell), &text(want)),
    }
}

/// What a `next` token carries: the crawl, a hash of the query and the
/// position to continue from. Stateless: it works after a restart.
#[derive(Serialize, Deserialize)]
struct Cursor {
    c: String,
    q: String,
    o: u32,
}

fn encode_cursor(c: &Cursor) -> String {
    URL_SAFE_NO_PAD.encode(serde_json::to_vec(c).expect("cursor serializes"))
}

fn decode_cursor(s: &str) -> Option<Cursor> {
    serde_json::from_slice(&URL_SAFE_NO_PAD.decode(s).ok()?).ok()
}

fn query_pages(o: &mut Open, crawl: &str, args: &Args) -> Result<Value, ToolError> {
    let tab = tab_arg(args)?;
    let filter = filter_arg(args, tab)?;
    let cols = columns_arg(args, tab)?;
    let search = str_arg(args, "search")?.unwrap_or("").to_string();
    let sort = match args.get("sort").filter(|v| !v.is_null()) {
        Some(s) => {
            let column = s
                .get("column")
                .and_then(Value::as_str)
                .ok_or_else(|| ToolError::invalid("`sort.column` is required."))?;
            column_index(tab, column)?;
            let desc = match s.get("dir").and_then(Value::as_str) {
                None | Some("asc") => false,
                Some("desc") => true,
                Some(_) => return Err(ToolError::invalid("`sort.dir` is asc or desc.")),
            };
            Some(Sort {
                column: column.into(),
                desc,
            })
        }
        None => None,
    };
    let conditions: Vec<(usize, Condition)> = match args.get("where").filter(|v| !v.is_null()) {
        Some(w) => serde_json::from_value::<Vec<Condition>>(w.clone())
            .map_err(|e| ToolError::invalid(format!("`where` is not valid: {e}.")))?
            .into_iter()
            .map(|c| Ok((column_index(tab, &c.column)?, c)))
            .collect::<Result<_, ToolError>>()?,
        None => Vec::new(),
    };
    let limit = u32_arg(args, "limit")?
        .unwrap_or(DEFAULT_LIMIT)
        .clamp(1, MAX_LIMIT);
    let q = ViewQuery {
        tab: tab.id.into(),
        filter: filter.into(),
        scope: UrlScope::All,
        search,
        sort,
    };
    let view_key = serde_json::to_string(&q).expect("query serializes");
    let conds: Vec<&Condition> = conditions.iter().map(|(_, c)| c).collect();
    let query_hash = format!(
        "{:016x}",
        fnv1a(
            serde_json::to_string(&(&view_key, &conds, &cols))
                .expect("serializes")
                .as_bytes()
        )
    );
    let mut from = 0;
    if let Some(c) = str_arg(args, "cursor")? {
        let cur = decode_cursor(c).ok_or_else(|| ToolError::invalid("This cursor is not valid."))?;
        if cur.c != crawl || cur.q != query_hash {
            return Err(ToolError::invalid(
                "This cursor belongs to a different query. Pass the same arguments as the call that returned it.",
            ));
        }
        from = cur.o;
    }

    let view = match o.views.get(&view_key) {
        Some(v) => *v,
        None => {
            let page = o.reader.open_view(&q, 0).map_err(ToolError::from_results)?;
            o.views.insert(view_key.clone(), page.view);
            page.view
        }
    };
    o.reader.refresh(view, &[]).map_err(ToolError::from_results)?;
    let total = o.reader.materialize(view).map_err(ToolError::from_results)?;

    let mut rows = Vec::new();
    let mut used = 0;
    let mut pos = from;
    'scan: while pos < total && (rows.len() as u32) < limit {
        let page = o.reader.rows(view, pos, 500).map_err(ToolError::from_results)?;
        if page.rows.is_empty() {
            break;
        }
        for row in page.rows {
            let keep = conditions
                .iter()
                .all(|(i, c)| matches(&row[i + 1], c.op, &c.value));
            if keep {
                let mut obj = Map::new();
                obj.insert("url_id".into(), row[0].clone());
                for &i in &cols {
                    obj.insert(tab.columns[i].id.into(), row[i + 1].clone());
                }
                let r = wrap(Value::Object(obj), &["url_id"]);
                let n = envelope::size(&r) + 1;
                if used + n > ROW_BUDGET && !rows.is_empty() {
                    break 'scan;
                }
                used += n;
                rows.push(r);
            }
            pos += 1;
            if (rows.len() as u32) >= limit {
                break 'scan;
            }
        }
    }
    let mut out = json!({
        "tab": tab.id,
        "filter": filter,
        "returned": rows.len(),
        "rows": rows,
    });
    if conditions.is_empty() {
        out["total"] = total.into();
    } else {
        out["scanned_to"] = pos.into();
        out["total_before_where"] = total.into();
    }
    if pos < total {
        out["next"] = encode_cursor(&Cursor {
            c: crawl.into(),
            q: query_hash,
            o: pos,
        })
        .into();
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// One page

fn page_details(o: &mut Open, args: &Args) -> Result<Value, ToolError> {
    let id = page_id(o, args)?;
    let detail = o.reader.detail(id).map_err(ToolError::from_results)?;
    let mut v = wrap(serde_json::to_value(&detail).expect("detail serializes"), &[]);
    for (kind, key) in [
        (ListKind::Inlinks, "inlinks"),
        (ListKind::Outlinks, "outlinks"),
        (ListKind::Images, "images"),
        (ListKind::Headings, "headings"),
    ] {
        let page = o
            .reader
            .list(id, kind, 0, DETAIL_LIST)
            .map_err(ToolError::from_results)?;
        let items = if kind == ListKind::Headings {
            serde_json::to_value(&page.headings)
        } else {
            serde_json::to_value(&page.rows)
        }
        .expect("list serializes");
        let total = if kind == ListKind::Headings {
            page.headings.len() as u64
        } else {
            page.total
        };
        v[format!("{key}_total")] = total.into();
        let crawled = v
            .as_object_mut()
            .expect("object")
            .entry("crawled")
            .or_insert_with(|| Value::Object(Map::new()));
        crawled[key] = envelope::cut_strings(items);
    }
    let findings = o.reader.agent_findings(id).map_err(ToolError::from_results)?;
    v["agents"] = wrap(
        serde_json::to_value(&findings).expect("findings serialize"),
        &["unavailable_reason"],
    );
    Ok(envelope::fit(v))
}

fn page_agent_findings(o: &mut Open, args: &Args) -> Result<Value, ToolError> {
    let id = page_id(o, args)?;
    let findings = o.reader.agent_findings(id).map_err(ToolError::from_results)?;
    Ok(envelope::fit(wrap(
        serde_json::to_value(&findings).expect("findings serialize"),
        &["unavailable_reason"],
    )))
}

fn page_html(path: &Path, args: &Args) -> Result<Value, ToolError> {
    match str_arg(args, "version")? {
        None | Some("raw") => {}
        Some("rendered") => {
            return Err(ToolError::invalid("This crawl did not store rendered HTML."));
        }
        Some(_) => return Err(ToolError::invalid("`version` is raw or rendered.")),
    }
    let store = CrawlStore::open_read_only(path).map_err(|e| ToolError::invalid(e.to_string()))?;
    let id = if let Some(v) = args.get("url_id").filter(|v| !v.is_null()) {
        v.as_i64()
            .ok_or_else(|| ToolError::invalid("`url_id` must be a whole number."))?
    } else {
        let url = str_arg(args, "url")?.ok_or_else(|| ToolError::invalid("Pass `url_id` or `url`."))?;
        store
            .id_of(url)
            .map_err(|e| ToolError::invalid(e.to_string()))?
            .ok_or_else(|| ToolError::invalid("This URL is not in the crawl."))?
    };
    let bytes = store
        .page_html(id)
        .map_err(|e| ToolError::invalid(e.to_string()))?
        .ok_or_else(|| ToolError::invalid("This crawl did not store the HTML of this page."))?;
    let html = String::from_utf8_lossy(&bytes);
    let offset = u32_arg(args, "offset")?.unwrap_or(0) as usize;
    if offset > html.len() {
        return Err(ToolError::invalid("`offset` is past the end of the HTML."));
    }
    let start = floor_char(&html, offset);
    let end = floor_char(&html, (start + HTML_CHUNK).min(html.len()));
    let mut v = json!({
        "url_id": id,
        "size": html.len(),
        "offset": start,
        "crawled": {"html": &html[start..end]},
    });
    if end < html.len() {
        v["next_offset"] = end.into();
    }
    Ok(v)
}

fn floor_char(s: &str, mut i: usize) -> usize {
    while i > 0 && !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}

fn export_rows(path: &Path, args: &Args) -> Result<Value, ToolError> {
    let tab = tab_arg(args)?;
    let filter = filter_arg(args, tab)?;
    let cols = strings_arg(args, "columns")?;
    for c in &cols {
        column_index(tab, c)?;
    }
    let limit = u32_arg(args, "limit")?
        .unwrap_or(MAX_EXPORT_ROWS)
        .clamp(1, MAX_EXPORT_ROWS);
    let q = ViewQuery {
        tab: tab.id.into(),
        filter: filter.into(),
        scope: UrlScope::All,
        search: String::new(),
        sort: None,
    };
    let mut buf = Vec::new();
    let rows = spiderlens_results::write_csv(
        path,
        &q,
        &cols,
        Some(&[(0, limit)]),
        &mut buf,
        &AtomicBool::new(false),
        &mut |_, _| {},
    )
    .map_err(|e| ToolError::invalid(e.to_string()))?;
    let mut csv = String::from_utf8_lossy(buf.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(&buf)).into_owned();
    let mut shortened = false;
    let budget = ROW_BUDGET;
    if csv.len() > budget {
        let cut = csv[..floor_char(&csv, budget)].rfind("\r\n").map_or(0, |i| i + 2);
        csv.truncate(cut);
        shortened = true;
    }
    let returned = csv.matches("\r\n").count().saturating_sub(1);
    Ok(json!({
        "tab": tab.id,
        "filter": filter,
        "rows": returned,
        "rows_matching_limit": rows,
        "shortened_to_fit": shortened,
        "crawled": {"csv": csv},
    }))
}
