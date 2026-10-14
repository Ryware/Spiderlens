//! CSV export and TSV copy (plan D11, D12; FR-026..FR-031).
//!
//! Export reads inside one read transaction, which in WAL mode is a
//! snapshot: rows the crawl commits after the export starts are not seen
//! (FR-030), and the crawl never waits for the export.

use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use rusqlite::types::Value;

use crate::Error;
use crate::reader::connect;
use crate::tabs::{ColumnDef, TabDef};
use crate::view::{ViewQuery, compile};

#[derive(Debug, thiserror::Error)]
pub enum ExportError {
    #[error("export cancelled")]
    Cancelled,
    #[error("could not write the file: {0}")]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Results(#[from] Error),
}

impl From<rusqlite::Error> for ExportError {
    fn from(e: rusqlite::Error) -> Self {
        Self::Results(e.into())
    }
}

/// Values a spreadsheet would run as a formula (OWASP CSV injection).
fn neutralise(s: &str) -> std::borrow::Cow<'_, str> {
    match s.chars().next() {
        Some('=' | '+' | '-' | '@' | '\t' | '\r') => format!("'{s}").into(),
        _ => s.into(),
    }
}

fn csv_field(out: &mut impl Write, s: &str) -> std::io::Result<()> {
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
fn cell_text(c: &ColumnDef, v: &Value) -> String {
    match v {
        Value::Null | Value::Blob(_) => String::new(),
        Value::Integer(i) if c.kind == crate::ValueKind::Bool => (if *i != 0 { "Yes" } else { "No" }).into(),
        Value::Integer(i) => i.to_string(),
        Value::Real(f) => f.to_string(),
        Value::Text(s) => s.clone(),
    }
}

fn json_text(c: &ColumnDef, v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::Null => String::new(),
        serde_json::Value::Number(n) if c.kind == crate::ValueKind::Bool => {
            (if n.as_i64() != Some(0) { "Yes" } else { "No" }).into()
        }
        serde_json::Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

/// Resolves column ids (empty = the tab's default visible columns).
pub(crate) fn pick_columns(
    tab: &'static TabDef,
    ids: &[String],
) -> Result<Vec<(usize, &'static ColumnDef)>, Error> {
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
                .find(|(_, c)| c.id == id)
                .ok_or_else(|| Error::invalid(format!("unknown column {id:?} on {}", tab.id)))
        })
        .collect()
}

/// One TSV line from a row page row (`[id, cells…]`), for copy (FR-026).
pub fn write_tsv_row(
    tab: &'static TabDef,
    columns: &[String],
    row: &[serde_json::Value],
) -> Result<String, Error> {
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

pub(crate) fn tsv_header(tab: &'static TabDef, columns: &[String]) -> Result<String, Error> {
    let cols = pick_columns(tab, columns)?;
    let mut s = String::new();
    for (n, (_, c)) in cols.iter().enumerate() {
        if n > 0 {
            s.push('\t');
        }
        tsv_field(&mut s, c.label);
    }
    Ok(s)
}

/// Writes the view as CSV to `out`: byte order mark, header row, every
/// matching row in view order (FR-028, FR-029). `ranges` limits it to
/// positions (a selection). Returns the number of rows written.
pub fn write_csv(
    crawl: &Path,
    q: &ViewQuery,
    columns: &[String],
    ranges: Option<&[(u32, u32)]>,
    out: impl Write,
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(u64, u64),
) -> Result<u64, ExportError> {
    let (conn, features) = connect(crawl)?;
    let c = compile(q, &features)?;
    let cols = pick_columns(c.tab, columns)?;
    let links = &features.link_settings;
    for d in &c.derived {
        if *d == crate::tabs::Derived::LinkMetrics && links.weighting.is_some() {
            crate::tabs::build_derived(&conn, crate::tabs::Derived::LinkPositions, links)?;
        }
        crate::tabs::build_derived(&conn, *d, links)?;
    }
    let mut out = BufWriter::with_capacity(1 << 16, out);
    out.write_all(b"\xEF\xBB\xBF")?;
    for (n, (_, col)) in cols.iter().enumerate() {
        if n > 0 {
            out.write_all(b",")?;
        }
        csv_field(&mut out, col.label)?;
    }
    out.write_all(b"\r\n")?;

    // The snapshot starts with the first read inside the transaction.
    conn.execute_batch("BEGIN")?;
    let params: Vec<&dyn rusqlite::ToSql> = match &c.search {
        Some(s) => vec![s],
        None => vec![],
    };
    let total: i64 = conn.query_row(
        &format!("SELECT COUNT(*) FROM urls u NOT INDEXED WHERE {}", c.where_sql),
        params.as_slice(),
        |r| r.get(0),
    )?;
    let select: String = cols
        .iter()
        .map(|(_, col)| if features.has(col.need) { col.expr } else { "NULL" })
        .collect::<Vec<_>>()
        .join(", ");
    let full = vec![(0u32, u32::try_from(total).unwrap_or(u32::MAX))];
    let ranges = ranges.map_or(full, <[_]>::to_vec);
    let total: u64 = ranges.iter().map(|(a, b)| u64::from(b.saturating_sub(*a))).sum();
    let mut written = 0u64;
    progress(0, total);
    for (a, b) in ranges {
        let sql = format!(
            "SELECT {select} FROM urls u NOT INDEXED WHERE {} ORDER BY {} LIMIT {} OFFSET {a}",
            c.where_sql,
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
pub fn export_file(
    crawl: &Path,
    q: &ViewQuery,
    columns: &[String],
    ranges: Option<&[(u32, u32)]>,
    dest: &Path,
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(u64, u64),
) -> Result<u64, ExportError> {
    let part = part_path(dest);
    let result = (|| {
        let file = std::fs::File::create(&part)?;
        let n = write_csv(crawl, q, columns, ranges, &file, cancel, progress)?;
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
            .map(|v| v.tab)
            .ok_or_else(|| Error::invalid("view closed"))?;
        let rows = self.rows_at(view, ranges, COPY_LIMIT)?;
        let mut s = tsv_header(tab, columns)?;
        for row in rows {
            s.push('\n');
            s.push_str(&write_tsv_row(tab, columns, &row)?);
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
