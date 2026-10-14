//! Lists behind every count (FR-013, FR-021): a group of URL pairs, paged
//! by position from an on-disk view table, with previous and current values
//! side by side, and CSV export.

use std::collections::HashMap;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use rusqlite::types::Value;
use serde::{Deserialize, Serialize};
use spiderlens_results::{ColumnDef, ValueKind, build_derived, csv_field, tab};
use ts_rs::TS;

use crate::elements::{Element, ElementKind};
use crate::registry::Layout;
use crate::summary::{PAIRS, grp};
use crate::{Comparison, Error, Result};

/// Which change of a filter a group lists (story 1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum FilterChange {
    New,
    Added,
    Removed,
    Missing,
    NotReached,
}

/// URL-level groups of the summary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum UrlGroup {
    New,
    Missing,
    NotReached,
    Collisions,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(export)]
pub enum Group {
    Filter {
        tab: String,
        filter: String,
        change: FilterChange,
    },
    Element {
        element: String,
    },
    Urls {
        which: UrlGroup,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct GroupSort {
    pub column: String,
    pub desc: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum Which {
    Previous,
    Current,
    Both,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct GroupColumn {
    pub id: String,
    pub label: String,
    pub side: Which,
    pub kind: ValueKind,
    pub sortable: bool,
    pub width: u16,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct GroupRow {
    pub prev_id: Option<i64>,
    pub cur_id: Option<i64>,
    pub values: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct GroupPage {
    pub view: u32,
    pub columns: Vec<GroupColumn>,
    pub from: u32,
    pub total: u32,
    pub rows: Vec<GroupRow>,
}

/// Where a column's value comes from.
#[derive(Clone)]
enum Source {
    /// An expression over the workspace pair (`p`, `c`).
    Ws(String),
    /// A results column read from one crawl file by URL id.
    Crawl(usize, &'static ColumnDef),
}

pub(crate) struct GroupView {
    table: String,
    total: u32,
    columns: Vec<GroupColumn>,
    sources: Vec<Source>,
}

fn kind_of(e: &Element) -> ValueKind {
    match (e.kind, e.id) {
        (_, "status_code") => ValueKind::Status,
        (_, "response_time") => ValueKind::Ms,
        (_, "redirect_target" | "canonical") => ValueKind::Url,
        (_, "final_state") => ValueKind::State,
        (ElementKind::Percent | ElementKind::Absolute | ElementKind::Factor, _) => ValueKind::Int,
        _ => ValueKind::Text,
    }
}

/// Set values are stored joined by U+001F; lists show them comma-separated.
fn display_sql(e: &Element, alias: &str) -> String {
    let col = format!("{alias}.{}", e.col());
    match (e.kind, e.id) {
        (ElementKind::Set, _) => format!("replace({col}, char(31), ', ')"),
        (_, "content") => format!("CASE WHEN {col} IS NULL THEN NULL ELSE printf('%016x', {col}) END"),
        _ => col,
    }
}

impl Comparison {
    fn element(&self, id: &str) -> Result<&Element> {
        self.elements
            .iter()
            .find(|e| e.id == id)
            .ok_or_else(|| Error::invalid(format!("unknown element {id:?}")))
    }

    fn group_sql(&self, g: &Group) -> Result<String> {
        Ok(match g {
            Group::Filter { tab, filter, change } => {
                let k = self
                    .layout
                    .find(tab, filter)
                    .ok_or_else(|| Error::invalid(format!("unknown filter {tab} › {filter}")))?;
                let (pb, cb) = (Layout::test("p", k), Layout::test("c", k));
                match change {
                    FilterChange::New => format!("m.grp = {} AND c.reach = 0 AND {cb} = 1", grp::NEW),
                    FilterChange::Added => {
                        format!(
                            "m.grp = {} AND c.reach = 0 AND {cb} = 1 AND {pb} = 0",
                            grp::MATCHED
                        )
                    }
                    FilterChange::Removed => {
                        format!(
                            "m.grp = {} AND c.reach = 0 AND {pb} = 1 AND {cb} = 0",
                            grp::MATCHED
                        )
                    }
                    FilterChange::Missing => format!("m.grp = {} AND {pb} = 1", grp::MISSING),
                    FilterChange::NotReached => format!(
                        "m.grp IN ({}, {}) AND {pb} = 1",
                        grp::NOT_REACHED,
                        grp::NOT_REACHED_INCOMPLETE
                    ),
                }
            }
            Group::Element { element } => {
                let e = self.element(element)?;
                format!(
                    "m.grp = {} AND {} AND {}",
                    grp::MATCHED,
                    e.applies_sql(),
                    e.changed_sql()
                )
            }
            Group::Urls { which } => match which {
                UrlGroup::New => format!("m.grp = {} AND c.reach = 0", grp::NEW),
                UrlGroup::Missing => format!("m.grp = {}", grp::MISSING),
                UrlGroup::NotReached => {
                    format!("m.grp IN ({}, {})", grp::NOT_REACHED, grp::NOT_REACHED_INCOMPLETE)
                }
                UrlGroup::Collisions => "m.cur_id IN (SELECT cur_id FROM matches WHERE cur_id IS NOT NULL \
                     GROUP BY cur_id HAVING COUNT(*) > 1)"
                    .into(),
            },
        })
    }

    fn group_columns(&self, g: &Group) -> Result<(Vec<GroupColumn>, Vec<Source>)> {
        let mut cols = vec![
            GroupColumn {
                id: "address_previous".into(),
                label: "Address (previous)".into(),
                side: Which::Previous,
                kind: ValueKind::Url,
                sortable: true,
                width: 380,
            },
            GroupColumn {
                id: "address_current".into(),
                label: "Address (current)".into(),
                side: Which::Current,
                kind: ValueKind::Url,
                sortable: true,
                width: 380,
            },
        ];
        let mut src = vec![Source::Ws("p.url".into()), Source::Ws("c.url".into())];
        let pair = |cols: &mut Vec<GroupColumn>, src: &mut Vec<Source>, e: &Element, label: &str| {
            for (side, alias, word) in [
                (Which::Previous, "p", "previous"),
                (Which::Current, "c", "current"),
            ] {
                cols.push(GroupColumn {
                    id: format!("{}_{word}", e.id),
                    label: format!("{label} ({word})"),
                    side,
                    kind: kind_of(e),
                    sortable: true,
                    width: if kind_of(e) == ValueKind::Int { 110 } else { 260 },
                });
                src.push(Source::Ws(display_sql(e, alias)));
            }
        };
        match g {
            Group::Element { element } => {
                let e = self.element(element)?;
                pair(&mut cols, &mut src, e, e.label);
                if matches!(
                    e.kind,
                    ElementKind::Percent | ElementKind::Absolute | ElementKind::Factor
                ) {
                    let col = e.col();
                    cols.push(GroupColumn {
                        id: "difference".into(),
                        label: "Difference".into(),
                        side: Which::Both,
                        kind: ValueKind::Int,
                        sortable: true,
                        width: 110,
                    });
                    src.push(Source::Ws(format!("(c.{col} - p.{col})")));
                    cols.push(GroupColumn {
                        id: "difference_percent".into(),
                        label: "Difference %".into(),
                        side: Which::Both,
                        kind: ValueKind::Int,
                        sortable: true,
                        width: 120,
                    });
                    src.push(Source::Ws(format!(
                        "CASE WHEN p.{col} > 0 THEN CAST(round((c.{col} - p.{col}) * 100.0 / p.{col}) AS INTEGER) END"
                    )));
                }
            }
            Group::Filter { tab: t, .. } => {
                let t = tab(t).ok_or_else(|| Error::invalid(format!("unknown tab {t:?}")))?;
                for c in t
                    .columns
                    .iter()
                    .filter(|c| c.default_visible && c.id != "address")
                {
                    if let Some(e) = self.elements.iter().find(|e| e.id == c.id && e.expr == c.expr) {
                        pair(&mut cols, &mut src, e, c.label);
                        continue;
                    }
                    for (n, word, side) in [(0, "previous", Which::Previous), (1, "current", Which::Current)]
                    {
                        cols.push(GroupColumn {
                            id: format!("{}_{word}", c.id),
                            label: format!("{} ({word})", c.label),
                            side,
                            kind: c.kind,
                            sortable: false,
                            width: c.width,
                        });
                        src.push(Source::Crawl(n, c));
                    }
                }
            }
            Group::Urls { .. } => {
                for id in ["final_state", "status_code", "title"] {
                    let e = self.element(id)?;
                    pair(&mut cols, &mut src, e, e.label);
                }
            }
        }
        Ok((cols, src))
    }

    /// Opens a group list: builds its order on disk and returns the first
    /// page.
    pub fn open_group(
        &mut self,
        g: &Group,
        search: &str,
        sort: Option<&GroupSort>,
        count: u32,
    ) -> Result<GroupPage> {
        let cond = self.group_sql(g)?;
        let (columns, sources) = self.group_columns(g)?;
        let order = match sort {
            Some(s) => {
                let i = columns
                    .iter()
                    .position(|c| c.id == s.column && c.sortable)
                    .ok_or_else(|| Error::invalid(format!("cannot sort by {:?}", s.column)))?;
                let Source::Ws(expr) = &sources[i] else {
                    return Err(Error::invalid("column not sortable"));
                };
                let dir = if s.desc { "DESC" } else { "ASC" };
                format!("({expr}) IS NULL, ({expr}) {dir}, m.rowid")
            }
            None => "COALESCE(c.url, p.url), m.rowid".into(),
        };
        let search = search.trim();
        let id = self.next_view;
        self.next_view += 1;
        let table = format!("view_{id}");
        self.ws.execute_batch(&format!(
            "CREATE TABLE {table} (pos INTEGER PRIMARY KEY, prev_id INTEGER, cur_id INTEGER)"
        ))?;
        let total = self.ws.execute(
            &format!(
                "INSERT INTO {table} (pos, prev_id, cur_id)
                 SELECT row_number() OVER (ORDER BY {order}) - 1, m.prev_id, m.cur_id FROM {PAIRS}
                 WHERE ({cond}) AND (?1 = '' OR cmp_contains(COALESCE(c.url, '') || ' ' || COALESCE(p.url, ''), ?1))"
            ),
            [search],
        )? as u32;
        self.views.insert(
            id,
            GroupView {
                table,
                total,
                columns,
                sources,
            },
        );
        self.rows(id, 0, count)
    }

    pub fn close_group(&mut self, view: u32) -> Result<()> {
        if let Some(v) = self.views.remove(&view) {
            self.ws
                .execute_batch(&format!("DROP TABLE IF EXISTS {}", v.table))?;
        }
        Ok(())
    }

    /// Rows `from..from + count` of an open group list (at most 1,000).
    pub fn rows(&mut self, view: u32, from: u32, count: u32) -> Result<GroupPage> {
        let v = self
            .views
            .get(&view)
            .ok_or_else(|| Error::invalid(format!("no open list {view}")))?;
        let count = count.min(1_000);
        let ws_exprs: Vec<String> = v
            .sources
            .iter()
            .map(|s| match s {
                Source::Ws(e) => e.clone(),
                Source::Crawl(..) => "NULL".into(),
            })
            .collect();
        let sql = format!(
            "SELECT v.prev_id, v.cur_id, {} FROM {} v
             LEFT JOIN side_rows p ON p.side = 0 AND p.id = v.prev_id
             LEFT JOIN side_rows c ON c.side = 1 AND c.id = v.cur_id
             WHERE v.pos >= ?1 AND v.pos < ?2 ORDER BY v.pos",
            ws_exprs.join(", "),
            v.table
        );
        let mut rows: Vec<GroupRow> = Vec::new();
        {
            let mut stmt = self.ws.prepare(&sql)?;
            let mut q = stmt.query([from, from.saturating_add(count)])?;
            while let Some(r) = q.next()? {
                let mut values = Vec::with_capacity(ws_exprs.len());
                for i in 0..ws_exprs.len() {
                    values.push(json(r.get::<_, Value>(i + 2)?));
                }
                rows.push(GroupRow {
                    prev_id: r.get(0)?,
                    cur_id: r.get(1)?,
                    values,
                });
            }
        }
        let crawl_cols: Vec<(usize, usize, &'static ColumnDef)> = v
            .sources
            .iter()
            .enumerate()
            .filter_map(|(i, s)| match s {
                Source::Crawl(n, c) => Some((i, *n, *c)),
                Source::Ws(_) => None,
            })
            .collect();
        let (columns, total) = (v.columns.clone(), v.total);
        for side in 0..2 {
            let cols: Vec<_> = crawl_cols.iter().filter(|(_, n, _)| *n == side).collect();
            if cols.is_empty() {
                continue;
            }
            let ids: Vec<i64> = rows
                .iter()
                .filter_map(|r| if side == 0 { r.prev_id } else { r.cur_id })
                .collect();
            let values =
                self.crawl_values(side, &cols.iter().map(|(_, _, c)| *c).collect::<Vec<_>>(), &ids)?;
            for r in &mut rows {
                let Some(id) = (if side == 0 { r.prev_id } else { r.cur_id }) else {
                    continue;
                };
                if let Some(vals) = values.get(&id) {
                    for ((i, _, _), val) in cols.iter().zip(vals) {
                        r.values[*i] = val.clone();
                    }
                }
            }
        }
        Ok(GroupPage {
            view,
            columns,
            from,
            total,
            rows,
        })
    }

    /// Values of results columns for some URL ids of one crawl file.
    fn crawl_values(
        &mut self,
        side: usize,
        cols: &[&'static ColumnDef],
        ids: &[i64],
    ) -> Result<HashMap<i64, Vec<serde_json::Value>>> {
        let s = &mut self.sides[side];
        for c in cols {
            if let Some(d) = c.derived
                && !s.derived.contains(&d)
            {
                build_derived(&s.conn, d)?;
                s.derived.push(d);
            }
        }
        let exprs: Vec<&str> = cols
            .iter()
            .map(|c| if s.features.has(c.need) { c.expr } else { "NULL" })
            .collect();
        let mut out = HashMap::new();
        if ids.is_empty() {
            return Ok(out);
        }
        let list = ids.iter().map(i64::to_string).collect::<Vec<_>>().join(",");
        let mut stmt = s.conn.prepare(&format!(
            "SELECT u.id, {} FROM urls u WHERE u.id IN ({list})",
            exprs.join(", ")
        ))?;
        let mut q = stmt.query([])?;
        while let Some(r) = q.next()? {
            let id: i64 = r.get(0)?;
            let vals = (0..cols.len())
                .map(|i| Ok(json(r.get::<_, Value>(i + 1)?)))
                .collect::<rusqlite::Result<Vec<_>>>()?;
            out.insert(id, vals);
        }
        Ok(out)
    }

    /// Writes an open group list as CSV through `<dest>.part` (FR-013,
    /// spec 007 FR-029, FR-030). Returns the number of rows.
    pub fn export_group(
        &mut self,
        view: u32,
        dest: &Path,
        cancel: &AtomicBool,
        progress: &mut dyn FnMut(u64, u64),
    ) -> Result<u64> {
        let part = part_path(dest);
        let result = self.write_group(view, &part, cancel, progress).and_then(|n| {
            std::fs::rename(&part, dest)?;
            Ok(n)
        });
        if result.is_err() {
            let _ = std::fs::remove_file(&part);
        }
        result
    }

    fn write_group(
        &mut self,
        view: u32,
        path: &Path,
        cancel: &AtomicBool,
        progress: &mut dyn FnMut(u64, u64),
    ) -> Result<u64> {
        let first = self.rows(view, 0, 0)?;
        let total = u64::from(first.total);
        let file = std::fs::File::create(path)?;
        let mut out = BufWriter::with_capacity(1 << 16, &file);
        out.write_all(b"\xEF\xBB\xBF")?;
        for (n, c) in first.columns.iter().enumerate() {
            if n > 0 {
                out.write_all(b",")?;
            }
            csv_field(&mut out, &c.label)?;
        }
        out.write_all(b"\r\n")?;
        let mut written = 0u64;
        progress(0, total);
        while written < total {
            if cancel.load(Ordering::Relaxed) {
                return Err(Error::Cancelled);
            }
            let page = self.rows(view, written as u32, 1_000)?;
            if page.rows.is_empty() {
                break;
            }
            for r in &page.rows {
                for (n, (v, c)) in r.values.iter().zip(&page.columns).enumerate() {
                    if n > 0 {
                        out.write_all(b",")?;
                    }
                    csv_field(&mut out, &cell(v, c.kind))?;
                }
                out.write_all(b"\r\n")?;
            }
            written += page.rows.len() as u64;
            progress(written, total);
        }
        out.flush()?;
        drop(out);
        file.sync_all()?;
        Ok(written)
    }
}

fn cell(v: &serde_json::Value, kind: ValueKind) -> String {
    match v {
        serde_json::Value::Null => String::new(),
        serde_json::Value::Number(n) if kind == ValueKind::Bool => {
            (if n.as_i64() != Some(0) { "Yes" } else { "No" }).into()
        }
        serde_json::Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

fn json(v: Value) -> serde_json::Value {
    match v {
        Value::Null | Value::Blob(_) => serde_json::Value::Null,
        Value::Integer(i) => i.into(),
        Value::Real(f) => serde_json::Number::from_f64(f).map_or(serde_json::Value::Null, Into::into),
        Value::Text(s) => s.into(),
    }
}

fn part_path(dest: &Path) -> PathBuf {
    let mut name = dest.file_name().unwrap_or_default().to_os_string();
    name.push(".part");
    dest.with_file_name(name)
}
