//! Bulk exports (spec 010 FR-018 to FR-025, plan D8).
//!
//! A bulk table is data: an id, what one row is, and a function that
//! streams its rows from the crawl file into a [`Sink`]. Every table is
//! read inside one read transaction, which in WAL mode is a snapshot: an
//! export started during a crawl holds the data as it was then, and the
//! crawl never waits for it (FR-024).

pub mod sink;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use rusqlite::Connection;
use rusqlite::types::Value;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

pub use sink::{Col, CsvFolderSink, Finished, OverviewRow, Sink, XlsxSink};

use crate::export::{ExportError, pick_columns};
use crate::features::Features;
use crate::issues::{ResolvedSettings, catalogue, display_name};
use crate::reader::connect;
use crate::tabs::{self, Derived, Need};
use crate::view::{UrlScope, ViewQuery, compile};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum BulkFormat {
    /// A folder with one CSV file per table.
    Csv,
    /// One Excel workbook with an Overview sheet.
    Xlsx,
}

/// The tables a bulk export can hold, in export order.
pub const TABLES: [(&str, &str); 7] = [
    ("summary", "Crawl summary"),
    ("tabs", "All tabs"),
    ("issues", "All issues"),
    ("links", "All links"),
    ("images", "All images"),
    ("redirect_chains", "Redirect chains"),
    ("headers", "Response headers"),
];

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct BulkOptions {
    /// Ids from [`TABLES`]; empty means all.
    pub tables: Vec<String>,
    pub format: BulkFormat,
    #[ts(type = "string")]
    pub out: PathBuf,
    pub overwrite: bool,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct BulkProgress {
    pub table: String,
    pub tables_done: u32,
    pub tables_total: u32,
    #[ts(type = "number")]
    pub rows: u64,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct BulkResult {
    #[ts(type = "string[]")]
    pub files: Vec<PathBuf>,
    pub tables: Vec<TableCount>,
    #[ts(type = "number")]
    pub truncated_cells: u64,
    pub split_sheets: u32,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct TableCount {
    pub table: String,
    #[ts(type = "number")]
    pub rows: u64,
}

/// One output table: a tab, an issue, links, ...
struct Out<'a> {
    conn: &'a Connection,
    features: &'a Features,
    sink: &'a mut dyn Sink,
    cancel: &'a AtomicBool,
    progress: &'a mut dyn FnMut(BulkProgress),
    done: u32,
    total: u32,
    counts: Vec<TableCount>,
}

impl Out<'_> {
    /// Streams `sql`'s rows into a table named `name`.
    fn table(
        &mut self,
        name: &str,
        title: &str,
        cols: &[Col],
        sql: &str,
        extra: OverviewExtra,
    ) -> Result<u64, ExportError> {
        self.sink.begin(name, cols)?;
        let mut stmt = self.conn.prepare(sql)?;
        let n = cols.len();
        let mut rows = stmt.query([])?;
        let mut count = 0u64;
        let mut buf: Vec<Value> = Vec::with_capacity(n);
        while let Some(r) = rows.next()? {
            buf.clear();
            for i in 0..n {
                buf.push(r.get(i)?);
            }
            self.sink.row(&buf)?;
            count += 1;
            if count.is_multiple_of(10_000) {
                if self.cancel.load(Ordering::Relaxed) {
                    return Err(ExportError::Cancelled);
                }
                (self.progress)(BulkProgress {
                    table: title.into(),
                    tables_done: self.done,
                    tables_total: self.total,
                    rows: count,
                });
            }
        }
        self.sink.end(OverviewRow {
            table: title.into(),
            sheet: name.into(),
            rows: count,
            priority: extra.priority,
            why: extra.why,
            fix: extra.fix,
        })?;
        self.done += 1;
        self.counts.push(TableCount {
            table: title.into(),
            rows: count,
        });
        if self.cancel.load(Ordering::Relaxed) {
            return Err(ExportError::Cancelled);
        }
        (self.progress)(BulkProgress {
            table: title.into(),
            tables_done: self.done,
            tables_total: self.total,
            rows: count,
        });
        Ok(count)
    }

    /// A 007 tab or a 009 issue tab with filter All and every column: the
    /// same rows and values as the app's export of it (FR-020).
    fn view(
        &mut self,
        tab_id: &str,
        name: &str,
        title: &str,
        extra: OverviewExtra,
    ) -> Result<u64, ExportError> {
        let t = tabs::tab(tab_id).ok_or_else(|| crate::Error::invalid(format!("unknown tab {tab_id}")))?;
        let q = ViewQuery {
            tab: tab_id.into(),
            filter: "all".into(),
            scope: UrlScope::All,
            search: String::new(),
            sort: None,
        };
        let c = compile(&q, self.features)?;
        let ids: Vec<String> = t.columns.iter().map(|c| c.id.to_string()).collect();
        let cols = pick_columns(c.tab, &ids)?;
        let select: String = cols
            .iter()
            .map(|(_, col)| {
                if self.features.has(col.need) {
                    col.expr
                } else {
                    "NULL"
                }
            })
            .collect::<Vec<_>>()
            .join(", ");
        let sql = format!(
            "SELECT {select} FROM urls u NOT INDEXED WHERE {} ORDER BY {}",
            c.where_sql, c.order_sql
        );
        let sink_cols: Vec<Col> = cols
            .iter()
            .map(|(_, col)| Col {
                label: col.label.to_string(),
                kind: col.kind,
            })
            .collect();
        self.table(name, title, &sink_cols, &sql, extra)
    }
}

#[derive(Default)]
struct OverviewExtra {
    priority: String,
    why: String,
    fix: String,
}

/// Runs a bulk export of the crawl file at `crawl` (FR-018 to FR-025).
pub fn run_bulk(
    crawl: &Path,
    settings: &ResolvedSettings,
    opts: &BulkOptions,
    progress: &mut dyn FnMut(BulkProgress),
    cancel: &AtomicBool,
) -> Result<BulkResult, ExportError> {
    run_bulk_with(crawl, settings, opts, progress, cancel, None)
}

/// [`run_bulk`] with a smaller sheet row limit, for tests of continuation
/// sheets.
#[doc(hidden)]
pub fn run_bulk_with(
    crawl: &Path,
    settings: &ResolvedSettings,
    opts: &BulkOptions,
    progress: &mut dyn FnMut(BulkProgress),
    cancel: &AtomicBool,
    max_rows: Option<u32>,
) -> Result<BulkResult, ExportError> {
    let wanted: Vec<&str> = if opts.tables.is_empty() || opts.tables.iter().any(|t| t == "all") {
        TABLES.iter().map(|t| t.0).collect()
    } else {
        for t in &opts.tables {
            if !TABLES.iter().any(|(id, _)| id == t) {
                return Err(crate::Error::invalid(format!(
                    "unknown table {t:?}; use {}",
                    TABLES.iter().map(|t| t.0).collect::<Vec<_>>().join(", ")
                ))
                .into());
            }
        }
        TABLES
            .iter()
            .map(|t| t.0)
            .filter(|id| opts.tables.iter().any(|t| t == id))
            .collect()
    };
    let (conn, features, _) = connect(crawl, settings)?;
    let mut sink: Box<dyn Sink> = match opts.format {
        BulkFormat::Csv => Box::new(CsvFolderSink::create(&opts.out, opts.overwrite)?),
        BulkFormat::Xlsx => match max_rows {
            Some(m) => Box::new(XlsxSink::with_max_rows(&opts.out, opts.overwrite, m)?),
            None => Box::new(XlsxSink::create(&opts.out, opts.overwrite)?),
        },
    };
    let result = (|| -> Result<Vec<TableCount>, ExportError> {
        // Derived tables first: they are temp tables, written outside the
        // snapshot, built from the same committed state the snapshot sees
        // unless the crawl commits in between (counts then differ by the
        // rows added, never by rows lost).
        let mut derived = vec![Derived::IssueHits, Derived::Redirects];
        for t in tabs::tabs() {
            for c in &t.columns {
                if let Some(d) = c.derived
                    && features.has(c.need)
                    && !derived.contains(&d)
                {
                    derived.push(d);
                }
            }
            for f in &t.filters {
                if let Some(d) = f.derived
                    && features.has(f.need)
                    && !derived.contains(&d)
                {
                    derived.push(d);
                }
            }
        }
        crate::issues::build_derived_with(&conn, &features, settings, &derived)?;
        conn.execute_batch("BEGIN")?;
        let issues = issue_list(&conn, &features, settings)?;
        let total = wanted
            .iter()
            .map(|t| match *t {
                "tabs" => tabs::tabs().len(),
                "issues" => issues.len(),
                _ => 1,
            })
            .sum::<usize>() as u32;
        let mut out = Out {
            conn: &conn,
            features: &features,
            sink: sink.as_mut(),
            cancel,
            progress,
            done: 0,
            total,
            counts: Vec::new(),
        };
        for t in &wanted {
            match *t {
                "summary" => summary(&mut out)?,
                "tabs" => {
                    for tab in tabs::tabs() {
                        out.view(
                            tab.id,
                            &format!("tab-{}", tab.id),
                            tab.title,
                            OverviewExtra::default(),
                        )?;
                    }
                }
                "issues" => {
                    for (i, rule) in &issues {
                        let r = &catalogue()[*i];
                        let s = settings.rules[*i];
                        out.view(
                            &format!("issue:{}", r.id),
                            &format!("issue-{}", r.id),
                            &display_name(r, s.threshold),
                            OverviewExtra {
                                priority: s.priority.label().into(),
                                why: rule.why.into(),
                                fix: rule.fix.into(),
                            },
                        )?;
                    }
                }
                "links" => links(&mut out)?,
                "images" => images(&mut out)?,
                "redirect_chains" => redirect_chains(&mut out)?,
                "headers" => headers(&mut out)?,
                _ => unreachable!("checked above"),
            }
        }
        conn.execute_batch("COMMIT")?;
        Ok(out.counts)
    })();
    match result {
        Ok(tables) => {
            let f = sink.finish()?;
            Ok(BulkResult {
                files: f.files,
                tables,
                truncated_cells: f.truncated_cells,
                split_sheets: f.split_sheets,
            })
        }
        Err(e) => {
            sink.abort();
            Err(e)
        }
    }
}

/// Enabled rules with affected URLs, in priority then count order.
fn issue_list(
    conn: &Connection,
    features: &Features,
    settings: &ResolvedSettings,
) -> Result<Vec<(usize, &'static crate::issues::Rule)>, ExportError> {
    let mut counts: Vec<(usize, i64)> = Vec::new();
    let mut stmt = conn.prepare("SELECT rule, COUNT(*) FROM temp.issue_hits GROUP BY rule")?;
    let mut rows = stmt.query([])?;
    while let Some(r) = rows.next()? {
        counts.push((r.get::<_, i64>(0)? as usize, r.get(1)?));
    }
    let mut list: Vec<(usize, i64)> = counts
        .into_iter()
        .filter(|(i, n)| *n > 0 && settings.rules[*i].enabled && features.has(catalogue()[*i].need))
        .collect();
    list.sort_by(|a, b| {
        settings.rules[a.0]
            .priority
            .cmp(&settings.rules[b.0].priority)
            .then(b.1.cmp(&a.1))
            .then(a.0.cmp(&b.0))
    });
    Ok(list.into_iter().map(|(i, _)| (i, &catalogue()[i])).collect())
}

fn summary(out: &mut Out) -> Result<(), ExportError> {
    let cols = [Col::text("Figure"), Col::int("Value")];
    let sql = "SELECT 'URLs found', COUNT(*) FROM urls \
               UNION ALL SELECT 'URLs crawled', COUNT(*) FROM urls WHERE state = 'crawled' \
               UNION ALL SELECT 'URLs not crawled', COUNT(*) FROM urls WHERE state = 'not_crawled' \
               UNION ALL SELECT 'Failed (no response)', COUNT(*) FROM urls WHERE state = 'crawled' AND error_class IS NOT NULL \
               UNION ALL SELECT 'Links', COUNT(*) FROM links";
    out.table("summary", "Crawl summary", &cols, sql, OverviewExtra::default())?;
    Ok(())
}

fn links(out: &mut Out) -> Result<(), ExportError> {
    let ext = out.features.has(Need::Extraction);
    let cols = [
        Col::text("Source"),
        Col::text("Target"),
        Col::text("Type"),
        Col::text("Anchor text"),
        Col::text("Alt text"),
        Col::text("Rel"),
        Col::bool("Nofollow"),
        Col::bool("Followed"),
        Col::bool("Internal target"),
        Col::int("Target status"),
        Col::text("Target state"),
    ];
    let sql = format!(
        "SELECT s.url, COALESCE(t.url, l.target, l.raw), l.kind, {}, {}, {}, l.nofollow, l.followed, t.internal, \
         t.status, CASE WHEN t.error_class IS NOT NULL THEN t.error_class ELSE t.state END \
         FROM links l JOIN urls s ON s.id = l.from_id LEFT JOIN urls t ON t.id = l.to_id \
         ORDER BY l.from_id, l.rowid",
        if ext { "l.text" } else { "NULL" },
        if ext { "l.alt" } else { "NULL" },
        if ext { "l.rel" } else { "NULL" },
    );
    out.table("links", "All links", &cols, &sql, OverviewExtra::default())?;
    Ok(())
}

fn images(out: &mut Out) -> Result<(), ExportError> {
    let ext = out.features.has(Need::Extraction);
    let timing = out.features.has(Need::Timing);
    let cols = [
        Col::text("Page"),
        Col::text("Image"),
        Col::text("Alt text"),
        Col::bool("Alt missing"),
        Col::bool("Alt empty"),
        Col::int("Status"),
        Col::int("Size"),
    ];
    let sql = format!(
        "SELECT s.url, COALESCE(t.url, l.target, l.raw), {alt}, {missing}, {empty}, t.status, {size} \
         FROM links l JOIN urls s ON s.id = l.from_id LEFT JOIN urls t ON t.id = l.to_id \
         WHERE l.kind = 'image' ORDER BY l.from_id, l.rowid",
        alt = if ext { "l.alt" } else { "NULL" },
        missing = if ext { "l.alt IS NULL" } else { "NULL" },
        empty = if ext { "l.alt = ''" } else { "NULL" },
        size = if timing { "t.size_bytes" } else { "NULL" },
    );
    out.table("images", "All images", &cols, &sql, OverviewExtra::default())?;
    Ok(())
}

fn redirect_chains(out: &mut Out) -> Result<(), ExportError> {
    let cols = [
        Col::text("Start"),
        Col::text("Hops"),
        Col::int("Hop count"),
        Col::text("Final URL"),
        Col::int("Final status"),
        Col::text("Final state"),
        Col::bool("Loop"),
    ];
    // Every hop in order, from a recursive walk of `redirect_to`; the start
    // URLs are those no other redirect points at, plus loop members.
    let sql = "WITH RECURSIVE walk(start, cur, n, path, hops) AS ( \
                 SELECT r.id, u.redirect_to, 1, ',' || r.id || ',', u.url || ' (' || COALESCE(u.status, '?') || ')' \
                 FROM temp.redirects r JOIN urls u ON u.id = r.id \
                 UNION ALL \
                 SELECT w.start, x.redirect_to, w.n + 1, w.path || w.cur || ',', \
                        w.hops || ' → ' || x.url || ' (' || COALESCE(x.status, '?') || ')' \
                 FROM walk w JOIN urls x ON x.id = w.cur \
                 WHERE x.redirect_to IS NOT NULL AND instr(w.path, ',' || w.cur || ',') = 0 AND w.n < 100 \
               ), last AS (SELECT start, hops, MAX(n) AS n FROM walk GROUP BY start) \
               SELECT s.url, last.hops, r.hops, f.url, f.status, \
                      CASE WHEN f.error_class IS NOT NULL THEN f.error_class ELSE f.state END, r.is_loop \
               FROM temp.redirects r JOIN urls s ON s.id = r.id JOIN last ON last.start = r.id \
               LEFT JOIN urls f ON f.id = r.final_id \
               WHERE r.id NOT IN (SELECT u.redirect_to FROM urls u WHERE u.redirect_to IS NOT NULL) OR r.is_loop \
               ORDER BY r.id";
    out.table(
        "redirect-chains",
        "Redirect chains",
        &cols,
        sql,
        OverviewExtra::default(),
    )?;
    Ok(())
}

fn headers(out: &mut Out) -> Result<(), ExportError> {
    let cols = [Col::text("Address"), Col::text("Header"), Col::text("Value")];
    let sql = if out.features.has(Need::Extraction) {
        "SELECT u.url, json_extract(j.value, '$[0]'), json_extract(j.value, '$[1]') \
         FROM headers h JOIN urls u ON u.id = h.url_id, json_each(h.headers) j ORDER BY h.url_id, j.key"
    } else {
        "SELECT NULL, NULL, NULL WHERE 0"
    };
    out.table(
        "headers",
        "Response headers",
        &cols,
        sql,
        OverviewExtra::default(),
    )?;
    Ok(())
}
