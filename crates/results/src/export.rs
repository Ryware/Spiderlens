//! CSV export and TSV copy (plan D11, D12; FR-026..FR-031).
//!
//! Export reads inside one read transaction, which in WAL mode is a
//! snapshot: rows the crawl commits after the export starts are not seen
//! (FR-030), and the crawl never waits for the export.

use std::borrow::Cow;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use rusqlite::types::Value;

use crate::Error;
use crate::issues::ResolvedSettings;
use crate::reader::connect;
use crate::tabs::{ColumnDef, TabDef};
use crate::view::{ViewQuery, compile};
use spiderlens_config::Segment;

#[derive(Debug, thiserror::Error)]
pub enum ExportError {
    #[error("export cancelled")]
    Cancelled,
    #[error("could not write the file: {0}")]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Results(#[from] Error),
    #[error("{} already exists", .0.display())]
    Exists(PathBuf),
}

impl From<rusqlite::Error> for ExportError {
    fn from(e: rusqlite::Error) -> Self {
        Self::Results(e.into())
    }
}

/// Values a spreadsheet would run as a formula (OWASP CSV injection).
pub(crate) fn neutralise(s: &str) -> std::borrow::Cow<'_, str> {
    match s.chars().next() {
        Some('=' | '+' | '-' | '@' | '\t' | '\r') => format!("'{s}").into(),
        _ => s.into(),
    }
}

/// One CSV field: formula-like values neutralised, quoted when needed
/// (FR-029). Shared with the comparison exports (spec 012).
pub fn csv_field(out: &mut impl Write, s: &str) -> std::io::Result<()> {
    let s = neutralise(s);
    if s.contains([',', '"', '\r', '\n']) {
        write!(out, "\"{}\"", s.replace('"', "\"\""))
    } else {
        out.write_all(s.as_bytes())
    }
}

fn tsv_field(out: &mut String, s: &str) {
    if s.contains(['\t', '"', '\r', '\n']) {
        out.push('"');
        out.push_str(&s.replace('"', "\"\""));
        out.push('"');
    } else {
        out.push_str(s);
    }
}

/// The text a cell exports as: full value, booleans as Yes and No.
pub(crate) fn cell_text(c: &ColumnDef, v: &Value) -> String {
    match v {
        Value::Null | Value::Blob(_) => String::new(),
        Value::Integer(i) if c.kind == crate::ValueKind::Bool => (if *i != 0 { "Yes" } else { "No" }).into(),
        Value::Integer(i) if c.kind == crate::ValueKind::Issues => issues_text(*i),
        Value::Integer(i) => i.to_string(),
        Value::Real(f) => f.to_string(),
        Value::Text(s) => s.clone(),
    }
}

pub(crate) fn issues_text(v: i64) -> String {
    let (p, n) = crate::tabs::decode_issues(v);
    format!("{} · {n}", p.label())
}

fn json_text(c: &ColumnDef, v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::Null => String::new(),
        serde_json::Value::Number(n) if c.kind == crate::ValueKind::Bool => {
            (if n.as_i64() != Some(0) { "Yes" } else { "No" }).into()
        }
        serde_json::Value::Number(n) if c.kind == crate::ValueKind::Issues => {
            n.as_i64().map(issues_text).unwrap_or_default()
        }
        serde_json::Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

/// Resolves column ids (empty = the tab's default visible columns).
pub(crate) fn pick_columns<'t>(
    tab: &'t TabDef,
    ids: &[String],
) -> Result<Vec<(usize, &'t ColumnDef)>, Error> {
    if ids.is_empty() {
        return Ok(tab
            .columns
            .iter()
            .enumerate()
            .filter(|(_, c)| c.default_visible)
            .collect());
    }
    ids.iter()
        .map(|id| {
            tab.columns
                .iter()
                .enumerate()
                .find(|(_, c)| c.id == id.as_str())
                .ok_or_else(|| Error::invalid(format!("unknown column {id:?} on {}", tab.id)))
        })
        .collect()
}

/// One TSV line from a row page row (`[id, cells…]`), for copy (FR-026).
pub fn write_tsv_row(tab: &TabDef, columns: &[String], row: &[serde_json::Value]) -> Result<String, Error> {
    let cols = pick_columns(tab, columns)?;
    let mut s = String::new();
    for (n, (i, c)) in cols.iter().enumerate() {
        if n > 0 {
            s.push('\t');
        }
        tsv_field(&mut s, &json_text(c, &row[i + 1]));
    }
    Ok(s)
}

pub(crate) fn tsv_header(tab: &TabDef, columns: &[String]) -> Result<String, Error> {
    let cols = pick_columns(tab, columns)?;
    let mut s = String::new();
    for (n, (_, c)) in cols.iter().enumerate() {
        if n > 0 {
            s.push('\t');
        }
        tsv_field(&mut s, &c.label);
    }
    Ok(s)
}

/// Writes the view as CSV to `out`: byte order mark, header row, every
/// matching row in view order (FR-028, FR-029). `ranges` limits it to
/// positions (a selection). Returns the number of rows written.
// The export's inputs are independent choices; a struct would only rename them.
#[allow(clippy::too_many_arguments)]
pub fn write_csv(
    crawl: &Path,
    q: &ViewQuery,
    columns: &[String],
    ranges: Option<&[(u32, u32)]>,
    all_values: Option<&str>,
    out: impl Write,
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(u64, u64),
) -> Result<u64, ExportError> {
    let settings = ResolvedSettings::default();
    write_csv_with(
        crawl, q, columns, ranges, all_values, &settings, out, cancel, progress,
    )
}

/// [`write_csv`] with the issue settings in force (spec 009 FR-025: an
/// issue's export lists the same URLs the app shows).
#[allow(clippy::too_many_arguments)]
pub fn write_csv_with(
    crawl: &Path,
    q: &ViewQuery,
    columns: &[String],
    ranges: Option<&[(u32, u32)]>,
    all_values: Option<&str>,
    settings: &ResolvedSettings,
    out: impl Write,
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(u64, u64),
) -> Result<u64, ExportError> {
    write_csv_in(
        crawl, q, columns, ranges, all_values, settings, None, out, cancel, progress,
    )
}

/// [`write_csv_with`] grouping URLs by `segments` (None: the segments stored
/// with the crawl). With the default columns and any enabled segment, the
/// Segment column is added (spec 027 FR-B09).
#[allow(clippy::too_many_arguments)]
pub fn write_csv_in(
    crawl: &Path,
    q: &ViewQuery,
    columns: &[String],
    ranges: Option<&[(u32, u32)]>,
    all_values: Option<&str>,
    settings: &ResolvedSettings,
    segments: Option<&[Segment]>,
    out: impl Write,
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(u64, u64),
) -> Result<u64, ExportError> {
    let (conn, features, _) = connect(crawl, settings)?;
    let custom = crate::custom::tabs(&conn)?;
    let c = compile(q, &features, &custom)?;
    crate::values::apply(&conn, c.values)?;
    let grouped = crate::segments::prepare(&conn, segments, &features)?;
    let mut cols = pick_columns(&c.tab, columns)?;
    if columns.is_empty() && grouped {
        cols.extend(
            c.tab
                .columns
                .iter()
                .enumerate()
                .filter(|(_, col)| col.id == "segment"),
        );
    }
    let links = &features.link_settings;
    if c.derived.contains(&crate::tabs::Derived::LinkMetrics) && links.weighting.is_some() {
        crate::tabs::build_derived(&conn, crate::tabs::Derived::LinkPositions, &features)?;
    }
    crate::issues::build_derived_with(&conn, &features, settings, &c.derived)?;
    let mut out = BufWriter::with_capacity(1 << 16, out);
    out.write_all(b"\xEF\xBB\xBF")?;
    for (n, (_, col)) in cols.iter().enumerate() {
        if n > 0 {
            out.write_all(b",")?;
        }
        csv_field(&mut out, &col.label)?;
    }
    out.write_all(b"\r\n")?;

    // The snapshot starts with the first read inside the transaction.
    conn.execute_batch("BEGIN")?;
    let params: Vec<&dyn rusqlite::ToSql> = match &c.search {
        Some(s) => vec![s],
        None => vec![],
    };
    let total: i64 = conn.query_row(
        &format!("SELECT COUNT(*) {}", c.source_where()),
        params.as_slice(),
        |r| r.get(0),
    )?;
    let select: String = cols
        .iter()
        .map(|(_, col)| {
            if !features.has(col.need) {
                Cow::Borrowed("NULL")
            } else if let Some(e) = all_values.and_then(|sep| crate::custom::all_values_expr(col, sep)) {
                Cow::Owned(e)
            } else {
                Cow::Borrowed(col.sql(c.values))
            }
        })
        .collect::<Vec<_>>()
        .join(", ");
    let full = vec![(0u32, u32::try_from(total).unwrap_or(u32::MAX))];
    let ranges = ranges.map_or(full, <[_]>::to_vec);
    let total: u64 = ranges.iter().map(|(a, b)| u64::from(b.saturating_sub(*a))).sum();
    let mut written = 0u64;
    progress(0, total);
    for (a, b) in ranges {
        let sql = format!(
            "SELECT {select} {} ORDER BY {} LIMIT {} OFFSET {a}",
            c.source_where(),
            c.order_sql,
            b.saturating_sub(a)
        );
        let mut stmt = conn.prepare(&sql)?;
        let mut rows = stmt.query(params.as_slice())?;
        while let Some(r) = rows.next()? {
            for (n, (_, col)) in cols.iter().enumerate() {
                if n > 0 {
                    out.write_all(b",")?;
                }
                let v: Value = r.get(n)?;
                if let Value::Text(s) = &v {
                    csv_field(&mut out, s)?;
                } else {
                    out.write_all(cell_text(col, &v).as_bytes())?;
                }
            }
            out.write_all(b"\r\n")?;
            written += 1;
            if written.is_multiple_of(10_000) {
                if cancel.load(Ordering::Relaxed) {
                    return Err(ExportError::Cancelled);
                }
                progress(written, total);
            }
        }
    }
    conn.execute_batch("COMMIT")?;
    out.flush()?;
    progress(written, total);
    Ok(written)
}

/// Exports to `dest` through `<dest>.part`, renamed when complete; on
/// cancel or failure the part file is removed and `dest` is untouched
/// (FR-030, Edge case: export failures).
#[allow(clippy::too_many_arguments)]
pub fn export_file(
    crawl: &Path,
    q: &ViewQuery,
    columns: &[String],
    ranges: Option<&[(u32, u32)]>,
    all_values: Option<&str>,
    dest: &Path,
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(u64, u64),
) -> Result<u64, ExportError> {
    let settings = ResolvedSettings::default();
    export_file_with(
        crawl, q, columns, ranges, all_values, &settings, dest, cancel, progress,
    )
}

/// [`export_file`] with the issue settings in force.
#[allow(clippy::too_many_arguments)]
pub fn export_file_with(
    crawl: &Path,
    q: &ViewQuery,
    columns: &[String],
    ranges: Option<&[(u32, u32)]>,
    all_values: Option<&str>,
    settings: &ResolvedSettings,
    dest: &Path,
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(u64, u64),
) -> Result<u64, ExportError> {
    export_file_in(
        crawl, q, columns, ranges, all_values, settings, None, dest, cancel, progress,
    )
}

/// [`export_file_with`] grouping URLs by `segments` (see [`write_csv_in`]).
#[allow(clippy::too_many_arguments)]
pub fn export_file_in(
    crawl: &Path,
    q: &ViewQuery,
    columns: &[String],
    ranges: Option<&[(u32, u32)]>,
    all_values: Option<&str>,
    settings: &ResolvedSettings,
    segments: Option<&[Segment]>,
    dest: &Path,
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(u64, u64),
) -> Result<u64, ExportError> {
    let part = part_path(dest);
    let result = (|| {
        let file = std::fs::File::create(&part)?;
        let n = write_csv_in(
            crawl, q, columns, ranges, all_values, settings, segments, &file, cancel, progress,
        )?;
        file.sync_all()?;
        drop(file);
        std::fs::rename(&part, dest)?;
        Ok(n)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&part);
    }
    result
}

fn part_path(dest: &Path) -> PathBuf {
    let mut name = dest.file_name().unwrap_or_default().to_os_string();
    name.push(".part");
    dest.with_file_name(name)
}

// ---------------------------------------------------------------------------
// Accessibility exports (spec 026 FR-026, T064)

/// The accessibility exports, as `(id, title)`. Each starts with the
/// disclaimer of FR-025 (CSV: a first line `# <text>`; JSON: a `disclaimer`
/// field).
pub const A11Y_EXPORTS: [(&str, &str); 4] = [
    ("a11y_violations", "Accessibility violations"),
    (
        "a11y_violations_level",
        "Accessibility violations in the WCAG target",
    ),
    ("a11y_needs_review", "Accessibility results that need review"),
    ("a11y_summary", "Accessibility summary by check"),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum A11yFormat {
    Csv,
    Json,
}

/// One export's columns and query. `check` and `title` are the positions of
/// the check id and of the column the readable title goes into.
struct A11yQuery {
    headers: &'static [&'static str],
    /// `{impact}` stands for the impact ordering expression.
    sql: &'static str,
    check: usize,
    title: usize,
}

/// Critical first, then serious, moderate, minor, none.
const A11Y_IMPACT_ORDER: &str = "CASE r.impact WHEN 'critical' THEN 0 WHEN 'serious' THEN 1 \
     WHEN 'moderate' THEN 2 WHEN 'minor' THEN 3 ELSE 4 END";

const A11Y_ROW_HEADERS: &[&str] = &[
    "address", "check_id", "title", "kind", "source", "impact", "wcag", "elements",
];

fn a11y_query(id: &str) -> Option<A11yQuery> {
    Some(match id {
        "a11y_violations" => A11yQuery {
            headers: A11Y_ROW_HEADERS,
            sql: "SELECT u.url, r.check_id, '', r.kind, r.source, COALESCE(r.impact, ''), COALESCE(r.wcag, ''), \
                  COALESCE(r.nodes_total, 0) FROM a11y_results r JOIN urls u ON u.id = r.url_id \
                  WHERE r.kind = 'violation' ORDER BY u.url, {impact}, r.check_id",
            check: 1,
            title: 2,
        },
        "a11y_violations_level" => A11yQuery {
            headers: A11Y_ROW_HEADERS,
            sql: "SELECT u.url, r.check_id, '', r.kind, r.source, COALESCE(r.impact, ''), COALESCE(r.wcag, ''), \
                  COALESCE(r.nodes_total, 0) FROM a11y_results r JOIN urls u ON u.id = r.url_id \
                  WHERE r.kind = 'violation' AND wcag_in_target(r.wcag) ORDER BY u.url, {impact}, r.check_id",
            check: 1,
            title: 2,
        },
        "a11y_needs_review" => A11yQuery {
            headers: A11Y_ROW_HEADERS,
            sql: "SELECT u.url, r.check_id, '', r.kind, r.source, COALESCE(r.impact, ''), COALESCE(r.wcag, ''), \
                  COALESCE(r.nodes_total, 0) FROM a11y_results r JOIN urls u ON u.id = r.url_id \
                  WHERE r.kind = 'needs_review' ORDER BY u.url, {impact}, r.check_id",
            check: 1,
            title: 2,
        },
        "a11y_summary" => A11yQuery {
            headers: &["check_id", "title", "impact", "pages", "elements"],
            sql: "SELECT r.check_id, '', COALESCE(r.impact, ''), COUNT(DISTINCT r.url_id), \
                  COALESCE(SUM(r.nodes_total), 0) FROM a11y_results r \
                  WHERE r.kind = 'violation' AND wcag_in_target(r.wcag) \
                  GROUP BY r.check_id, r.impact ORDER BY {impact}, 4 DESC, r.check_id",
            check: 0,
            title: 1,
        },
        _ => return None,
    })
}

/// Writes the accessibility export `id` (one of [`A11Y_EXPORTS`]) of a crawl
/// file. `target` counts against another WCAG target than the crawl's own
/// (the `_level` and summary exports). Returns the number of rows.
pub fn write_a11y(
    crawl: &Path,
    id: &str,
    format: A11yFormat,
    target: Option<spiderlens_config::WcagTarget>,
    out: impl Write,
) -> Result<u64, ExportError> {
    let q = a11y_query(id).ok_or_else(|| {
        Error::invalid(format!(
            "unknown export {id:?}; use {}",
            A11Y_EXPORTS.iter().map(|e| e.0).collect::<Vec<_>>().join(", ")
        ))
    })?;
    let (conn, mut features, _) = connect(crawl, &ResolvedSettings::default())?;
    if !features.has(crate::tabs::Need::AccessibilityStatic)
        && !features.has(crate::tabs::Need::AccessibilityFull)
    {
        return Err(Error::invalid(crate::tabs::Need::AccessibilityStatic.unavailable_reason()).into());
    }
    if let Some(t) = target {
        features.a11y.wcag = t;
        crate::sql::register_a11y(&conn, &features.a11y)?;
    }
    let sql = q.sql.replace("{impact}", A11Y_IMPACT_ORDER);
    let mut stmt = conn.prepare(&sql)?;
    let n = q.headers.len();
    let mut rows: Vec<Vec<Value>> = Vec::new();
    let mut cursor = stmt.query([])?;
    while let Some(r) = cursor.next()? {
        let mut row: Vec<Value> = (0..n).map(|i| r.get(i)).collect::<rusqlite::Result<_>>()?;
        let title = match &row[q.check] {
            Value::Text(check) => Some(crate::detail::a11y_title(check)),
            _ => None,
        };
        if let Some(t) = title {
            row[q.title] = Value::Text(t);
        }
        rows.push(row);
    }
    let mut out = BufWriter::with_capacity(1 << 16, out);
    match format {
        A11yFormat::Csv => {
            write!(out, "# {}\r\n", crate::detail::A11Y_DISCLAIMER)?;
            for (i, h) in q.headers.iter().enumerate() {
                if i > 0 {
                    out.write_all(b",")?;
                }
                csv_field(&mut out, h)?;
            }
            out.write_all(b"\r\n")?;
            for row in &rows {
                for (i, v) in row.iter().enumerate() {
                    if i > 0 {
                        out.write_all(b",")?;
                    }
                    match v {
                        Value::Text(t) => csv_field(&mut out, t)?,
                        Value::Integer(n) => write!(out, "{n}")?,
                        Value::Real(f) => write!(out, "{f}")?,
                        Value::Null | Value::Blob(_) => {}
                    }
                }
                out.write_all(b"\r\n")?;
            }
        }
        A11yFormat::Json => {
            let json_rows: Vec<serde_json::Value> = rows
                .iter()
                .map(|row| {
                    serde_json::Value::Object(
                        q.headers
                            .iter()
                            .zip(row)
                            .map(|(h, v)| {
                                let j = match v {
                                    Value::Text(t) => serde_json::Value::String(t.clone()),
                                    Value::Integer(n) => serde_json::json!(n),
                                    Value::Real(f) => serde_json::json!(f),
                                    Value::Null | Value::Blob(_) => serde_json::Value::Null,
                                };
                                ((*h).to_string(), j)
                            })
                            .collect(),
                    )
                })
                .collect();
            let doc = serde_json::json!({
                "disclaimer": crate::detail::A11Y_DISCLAIMER,
                "rows": json_rows,
            });
            serde_json::to_writer_pretty(&mut out, &doc).map_err(std::io::Error::from)?;
            out.write_all(b"\n")?;
        }
    }
    out.flush()?;
    Ok(rows.len() as u64)
}

/// [`write_a11y`] to `dest` through `<dest>.part`, as [`export_file`] does.
pub fn export_a11y_file(
    crawl: &Path,
    id: &str,
    format: A11yFormat,
    target: Option<spiderlens_config::WcagTarget>,
    dest: &Path,
) -> Result<u64, ExportError> {
    let part = part_path(dest);
    let result = (|| {
        let file = std::fs::File::create(&part)?;
        let n = write_a11y(crawl, id, format, target, &file)?;
        file.sync_all()?;
        drop(file);
        std::fs::rename(&part, dest)?;
        Ok(n)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&part);
    }
    result
}

impl crate::Reader {
    /// The rows at `ranges` as tab-separated text with a header row of
    /// `columns`, full values (FR-026). Over `COPY_LIMIT` rows is refused
    /// with an explanation (FR-027).
    pub fn copy_tsv(
        &mut self,
        view: u32,
        ranges: &[(u32, u32)],
        columns: &[String],
    ) -> Result<String, Error> {
        let tab = self
            .views
            .get(&view)
            .map(|v| v.tab.clone())
            .ok_or_else(|| Error::invalid("view closed"))?;
        let rows = self.rows_at(view, ranges, COPY_LIMIT)?;
        let mut s = tsv_header(&tab, columns)?;
        for row in rows {
            s.push('\n');
            s.push_str(&write_tsv_row(&tab, columns, &row)?);
        }
        Ok(s)
    }
}

/// Largest selection copied through the clipboard (Assumptions: copy limit).
pub const COPY_LIMIT: u32 = 10_000;

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn field(s: &str) -> String {
        let mut v = Vec::new();
        csv_field(&mut v, s).unwrap();
        String::from_utf8(v).unwrap()
    }

    #[test]
    fn quoting_and_formulas() {
        assert_eq!(field("plain"), "plain");
        assert_eq!(field("a,b"), "\"a,b\"");
        assert_eq!(field("say \"hi\""), "\"say \"\"hi\"\"\"");
        assert_eq!(field("two\nlines"), "\"two\nlines\"");
        assert_eq!(field("=HYPERLINK(\"x\")"), "\"'=HYPERLINK(\"\"x\"\")\"");
        assert_eq!(field("+1"), "'+1");
        assert_eq!(field("-2"), "'-2");
        assert_eq!(field("@SUM(A1)"), "'@SUM(A1)");
        assert_eq!(field("דף הבית"), "דף הבית");
    }

    /// Parses one RFC 4180 field back, for the round-trip property.
    fn unquote(s: &str) -> String {
        if let Some(inner) = s.strip_prefix('"').and_then(|s| s.strip_suffix('"')) {
            inner.replace("\"\"", "\"")
        } else {
            s.to_string()
        }
    }

    proptest! {
        #[test]
        fn fields_round_trip(s in "\\PC*") {
            let f = field(&s);
            let back = unquote(&f);
            let expected = neutralise(&s).into_owned();
            prop_assert_eq!(back, expected);
            // A quoted field is the only place a separator may appear.
            if !f.starts_with('"') {
                prop_assert!(!f.contains([',', '\n', '\r', '"']));
            }
        }
    }
}
