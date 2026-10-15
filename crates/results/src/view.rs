//! Views: a tab, filter, search and sort over the crawl, paged by position
//! (plan D4, D5).
//!
//! Opening a view answers the first screen at once with a top-k query, then
//! [`Reader::materialize`] stores the full order in `temp.view_N(pos, id)`.
//! From then on any page anywhere is a range read on `pos`, and "where is
//! row X now" is an index lookup, which keeps live refreshes anchored.

use std::borrow::Cow;
use std::collections::HashSet;
use std::time::Instant;

use rusqlite::types::Value;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::reader::Reader;
use crate::tabs::{self, ColumnDef, Derived, TabDef};
use crate::values::Values;
use crate::{Error, Result};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum UrlScope {
    #[default]
    All,
    Internal,
    External,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Sort {
    pub column: String,
    pub desc: bool,
}

/// What the user is looking at (spec Key Entities: View).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ViewQuery {
    pub tab: String,
    pub filter: String,
    #[serde(default)]
    pub scope: UrlScope,
    #[serde(default)]
    pub search: String,
    #[serde(default)]
    pub sort: Option<Sort>,
    /// Raw or rendered values of a rendered crawl (spec 014 FR-021);
    /// omitted means rendered when the crawl has rendering.
    #[serde(default)]
    #[ts(optional)]
    pub values: Option<Values>,
    /// Spec 027 FR-B04: only URLs in this segment; 0 means in no segment.
    #[serde(default)]
    #[ts(optional)]
    pub segment: Option<u32>,
}

/// A page of rows. Each row is `[id, value of each tab column…]` in the
/// tab's column order; values the file lacks are null.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct RowPage {
    pub view: u32,
    pub version: u32,
    /// Position of the first row.
    pub from: u32,
    pub rows: Vec<Vec<serde_json::Value>>,
    /// Rows in the view; null until the full order is built.
    pub total: Option<u32>,
    pub ready: bool,
}

/// Result of a live refresh.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct Refresh {
    pub version: u32,
    pub total: u32,
    /// New position of each anchor id, or null when it left the view.
    pub positions: Vec<Option<u32>>,
    pub build_ms: u32,
}

pub(crate) struct View {
    pub tab: Cow<'static, TabDef>,
    pub query: ViewQuery,
    pub where_sql: String,
    pub order_sql: String,
    pub search: Option<String>,
    pub derived: Vec<Derived>,
    pub version: u64,
    /// The values the view shows; its rows are read in this mode.
    pub values: Values,
    pub total: Option<u32>,
    /// `temp.view_<id>_<generation>` once materialised.
    pub table: Option<String>,
    pub generation: u32,
}

/// The parts of a view's query that do not depend on paging.
pub(crate) struct Compiled {
    pub tab: Cow<'static, TabDef>,
    pub where_sql: String,
    pub order_sql: String,
    pub search: Option<String>,
    pub derived: Vec<Derived>,
    pub values: Values,
}

impl Compiled {
    /// `FROM … WHERE …` of the view's rows.
    pub fn source_where(&self) -> String {
        format!("FROM {} WHERE {}", self.tab.source_sql(), self.where_sql)
    }
}

/// Turns a view query into SQL fragments, checking every id against the
/// tab definitions (ids never reach SQL as text).
pub(crate) fn compile(q: &ViewQuery, features: &crate::Features, custom: &[TabDef]) -> Result<Compiled> {
    let values = Values::resolve(q.values, features);
    let tab = tabs::find(&q.tab, custom).ok_or_else(|| Error::invalid(format!("unknown tab {:?}", q.tab)))?;
    let filter = tab
        .filter(&q.filter)
        .ok_or_else(|| Error::invalid(format!("unknown filter {:?} on {}", q.filter, tab.id)))?;
    if !features.has(filter.need) {
        return Err(Error::Unavailable(format!(
            "{} > {}: {}",
            tab.title,
            filter.label,
            filter.need.unavailable_reason()
        )));
    }
    let mut derived: Vec<Derived> = Vec::new();
    let mut add = |d: Option<Derived>| {
        if let Some(d) = d
            && !derived.contains(&d)
        {
            derived.push(d);
        }
    };
    add(tab.rows);
    add(filter.derived);
    let mut where_sql = format!("({}) AND ({})", tab.eligible, filter.sql(values));
    match (tab.scoped, q.scope) {
        (true, UrlScope::Internal) => where_sql.push_str(" AND u.internal = 1"),
        (true, UrlScope::External) => where_sql.push_str(" AND u.internal = 0"),
        _ => {}
    }
    where_sql.push_str(&segment_sql(q.segment));
    let search = q.search.trim();
    let search = (!search.is_empty()).then(|| search.to_lowercase());
    if search.is_some() {
        let main = tab
            .search_column
            .as_deref()
            .and_then(|c| tab.column(c))
            .filter(|c| features.has(c.need))
            .map(|c| format!(" OR ci_contains({}, ?1)", c.sql(values)))
            .unwrap_or_default();
        where_sql.push_str(&format!(" AND (ci_contains(u.url, ?1){main})"));
    }
    let order_sql = match &q.sort {
        None => "u.id".to_string(),
        Some(s) => {
            let c = tab
                .column(&s.column)
                .ok_or_else(|| Error::invalid(format!("unknown column {:?} on {}", s.column, tab.id)))?;
            add(c.derived_in(values));
            sort_sql(c, s.desc, features, values)
        }
    };
    // Columns read derived tables too.
    for c in &tab.columns {
        if features.has(c.need) {
            add(c.derived_in(values));
        }
    }
    Ok(Compiled {
        tab,
        where_sql,
        order_sql,
        search,
        derived,
        values,
    })
}

/// The segment filter as an `AND …` clause (the id is a number, never text).
pub(crate) fn segment_sql(segment: Option<u32>) -> String {
    match segment {
        None => String::new(),
        Some(0) => " AND u.id NOT IN (SELECT url_id FROM temp.url_segment)".into(),
        Some(id) => format!(" AND u.id IN (SELECT url_id FROM temp.url_segment WHERE seg = {id})"),
    }
}

/// FR-015: numbers numerically, text naturally and case-insensitively,
/// empty values last in both directions, ties by discovery order.
fn sort_sql(c: &ColumnDef, desc: bool, features: &crate::Features, values: Values) -> String {
    if !features.has(c.need) {
        return "u.id".into();
    }
    let dir = if desc { "DESC" } else { "ASC" };
    let e = c.sql(values);
    if let Some(k) = c.key {
        format!("({k} IS NULL OR {k} = '') ASC, {k} {dir}, u.id ASC")
    } else if let Some(k) = &c.sort_key {
        format!("(({e}) IS NULL OR ({e}) = '') ASC, ({k}) {dir}, natkey({e}) {dir}, u.id ASC")
    } else if c.kind.is_text() {
        format!("(({e}) IS NULL OR ({e}) = '') ASC, natkey({e}) {dir}, u.id ASC")
    } else {
        format!("(({e}) IS NULL) ASC, ({e}) {dir}, u.id ASC")
    }
}

/// `u.id, <col>, <col>…`: unavailable columns read as NULL.
pub(crate) fn select_list(tab: &TabDef, features: &crate::Features, values: Values) -> String {
    let mut s = String::from("u.id");
    for c in &tab.columns {
        s.push_str(", ");
        if features.has(c.need) {
            s.push_str(c.sql(values));
        } else {
            s.push_str("NULL");
        }
    }
    s
}

pub(crate) fn to_json(v: Value) -> serde_json::Value {
    match v {
        Value::Null => serde_json::Value::Null,
        Value::Integer(i) => i.into(),
        Value::Real(f) => serde_json::Number::from_f64(f).map_or(serde_json::Value::Null, Into::into),
        Value::Text(s) => s.into(),
        Value::Blob(_) => serde_json::Value::Null,
    }
}

pub(crate) fn read_rows(
    stmt: &mut rusqlite::Statement<'_>,
    params: &[&dyn rusqlite::ToSql],
    width: usize,
) -> Result<Vec<Vec<serde_json::Value>>> {
    let mut rows = stmt.query(params)?;
    let mut out = Vec::new();
    while let Some(r) = rows.next()? {
        let mut row = Vec::with_capacity(width);
        for i in 0..width {
            row.push(to_json(r.get::<_, Value>(i)?));
        }
        out.push(row);
    }
    Ok(out)
}

fn params_of(search: &Option<String>) -> Vec<&dyn rusqlite::ToSql> {
    match search {
        Some(s) => vec![s as &dyn rusqlite::ToSql],
        None => vec![],
    }
}

/// Positions and ids travel as JSON numbers; u32 holds any crawl the spec
/// targets (and 4 billion more).
fn u32_of(n: i64) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

impl Reader {
    /// Opens a view and returns its first `count` rows (plan D4 step 1).
    pub fn open_view(&mut self, q: &ViewQuery, count: u32) -> Result<RowPage> {
        let custom = self.custom_tabs()?.to_vec();
        let c = compile(q, &self.features, &custom)?;
        self.use_values(c.values)?;
        self.ensure_derived(c.derived.iter().copied())?;
        let version = self.version()?;
        let id = self.next_view;
        self.next_view += 1;
        let view = View {
            tab: c.tab,
            query: q.clone(),
            where_sql: c.where_sql,
            order_sql: c.order_sql,
            search: c.search,
            derived: c.derived,
            version,
            values: c.values,
            total: None,
            table: None,
            generation: 0,
        };
        self.views.insert(id, view);
        self.rows(id, 0, count)
    }

    /// Stores the view's full order (plan D4 step 2). Idempotent.
    pub fn materialize(&mut self, view: u32) -> Result<u32> {
        let v = self
            .views
            .get(&view)
            .ok_or_else(|| Error::invalid("view closed"))?;
        if let (Some(_), Some(total)) = (&v.table, v.total) {
            return Ok(total);
        }
        self.rebuild(view)
    }

    fn rebuild(&mut self, view: u32) -> Result<u32> {
        self.use_values(self.views[&view].values)?;
        let derived = self.views[&view].derived.clone();
        self.ensure_derived(derived)?;
        let version = self.version()?;
        let v = self
            .views
            .get_mut(&view)
            .ok_or_else(|| Error::invalid("view closed"))?;
        v.generation += 1;
        let table = format!("view_{view}_{}", v.generation);
        let sql = format!(
            "CREATE TEMP TABLE {table}(pos INTEGER PRIMARY KEY, id INTEGER NOT NULL);
             "
        );
        self.conn.execute_batch(&sql)?;
        let insert = format!(
            "INSERT INTO temp.{table}(id) SELECT u.id FROM {} WHERE {} ORDER BY {}",
            v.tab.source_sql(),
            v.where_sql,
            v.order_sql
        );
        let params = params_of(&v.search);
        let total = self.conn.execute(&insert, params.as_slice())?;
        self.conn
            .execute_batch(&format!("CREATE INDEX temp.{table}_id ON {table}(id);"))?;
        let old = v.table.replace(table);
        v.total = Some(u32::try_from(total).unwrap_or(u32::MAX));
        v.version = version;
        if let Some(old) = old {
            self.conn
                .execute_batch(&format!("DROP TABLE IF EXISTS temp.{old};"))?;
        }
        Ok(v.total.unwrap_or(0))
    }

    /// `count` rows from position `from` (plan D4).
    pub fn rows(&mut self, view: u32, from: u32, count: u32) -> Result<RowPage> {
        let count = count.min(1000);
        let values = self
            .views
            .get(&view)
            .ok_or_else(|| Error::invalid("view closed"))?
            .values;
        self.use_values(values)?;
        let v = &self.views[&view];
        let select = select_list(&v.tab, &self.features, values);
        let width = v.tab.columns.len() + 1;
        let rows = if let Some(table) = &v.table {
            let sql = format!(
                "SELECT {select} FROM temp.{table} t JOIN urls u ON u.id = t.id{}
                 WHERE t.pos > ?1 AND t.pos <= ?2 ORDER BY t.pos",
                v.tab.join_sql()
            );
            let mut stmt = self.conn.prepare(&sql)?;
            let (a, b) = (i64::from(from), i64::from(from) + i64::from(count));
            read_rows(&mut stmt, &[&a, &b], width)?
        } else {
            // Before the order is stored: a top-k query, fine for the first
            // screens, which is all the UI asks for at this point.
            let sql = if values == Values::Rendered {
                // SQLite computes every result column of every row before a
                // top-k sort keeps `count` of them. Rendered columns each
                // cost lookups, so the rows are picked by id first and their
                // columns read for those rows only (spec 014 T030).
                format!(
                    "SELECT {select} FROM (SELECT u.id AS k FROM urls u NOT INDEXED WHERE {}
                       ORDER BY {} LIMIT {count} OFFSET {from}) t
                     CROSS JOIN urls u ON u.id = t.k ORDER BY {}",
                    v.where_sql, v.order_sql, v.order_sql
                )
            } else {
                format!(
                    "SELECT {select} FROM {} WHERE {} ORDER BY {} LIMIT {count} OFFSET {from}",
                    v.tab.source_sql(),
                    v.where_sql,
                    v.order_sql
                )
            };
            let mut stmt = self.conn.prepare(&sql)?;
            read_rows(&mut stmt, &params_of(&v.search), width)?
        };
        Ok(RowPage {
            view,
            version: v.version as u32,
            from,
            rows,
            total: v.total,
            ready: v.table.is_some(),
        })
    }

    /// Current positions of `ids` in the view (None when not in it).
    pub fn locate(&mut self, view: u32, ids: &[i64]) -> Result<Vec<Option<u32>>> {
        self.materialize(view)?;
        let v = &self.views[&view];
        let table = v.table.as_ref().expect("materialised");
        let mut stmt = self
            .conn
            .prepare(&format!("SELECT pos - 1 FROM temp.{table} WHERE id = ?1"))?;
        ids.iter()
            .map(|id| Ok(stmt.query_row([id], |r| r.get::<_, i64>(0)).map(u32_of).ok()))
            .collect()
    }

    /// The tabs that list URL `id` with no filter, scope or search, in
    /// display order: where "show in table" can take it. Empty when no tab
    /// lists it (a URL still queued, say) or it is not in the crawl.
    pub fn tabs_of(&self, id: i64) -> Result<Vec<&'static str>> {
        let mut out = Vec::new();
        for t in tabs::tabs() {
            let sql = format!(
                "SELECT EXISTS (SELECT 1 FROM urls u WHERE u.id = ?1 AND ({}))",
                t.eligible_on_urls()
            );
            if self.conn.query_row(&sql, [id], |r| r.get::<_, bool>(0))? {
                out.push(t.id.as_ref());
            }
        }
        Ok(out)
    }

    /// Rebuilds the view if the crawl wrote since it was built; returns the
    /// new positions of `anchors` (plan D5). None when nothing changed.
    pub fn refresh(&mut self, view: u32, anchors: &[i64]) -> Result<Option<Refresh>> {
        let current = self.version()?;
        let v = self
            .views
            .get(&view)
            .ok_or_else(|| Error::invalid("view closed"))?;
        if v.table.is_some() && v.version == current {
            return Ok(None);
        }
        let start = Instant::now();
        let total = self.rebuild(view)?;
        let build_ms = start.elapsed().as_millis().min(u128::from(u32::MAX)) as u32;
        let positions = self.locate(view, anchors)?;
        Ok(Some(Refresh {
            version: self.views[&view].version as u32,
            total,
            positions,
            build_ms,
        }))
    }

    pub fn close_view(&mut self, view: u32) -> Result<()> {
        if let Some(v) = self.views.remove(&view)
            && let Some(t) = v.table
        {
            self.conn
                .execute_batch(&format!("DROP TABLE IF EXISTS temp.{t};"))?;
        }
        Ok(())
    }

    /// The view's query, for export and copy.
    pub fn view_query(&self, view: u32) -> Option<&ViewQuery> {
        self.views.get(&view).map(|v| &v.query)
    }

    /// Rows at the given position ranges (inclusive start, exclusive end),
    /// for copy (FR-026). At most `limit` rows.
    pub fn rows_at(
        &mut self,
        view: u32,
        ranges: &[(u32, u32)],
        limit: u32,
    ) -> Result<Vec<Vec<serde_json::Value>>> {
        let n: u64 = ranges.iter().map(|(a, b)| u64::from(b.saturating_sub(*a))).sum();
        if n > u64::from(limit) {
            return Err(Error::invalid(format!(
                "{n} rows are selected; copy is limited to {limit} rows. Export the selection to CSV instead."
            )));
        }
        self.materialize(view)?;
        let mut seen = HashSet::new();
        let mut out = Vec::new();
        let mut merged: Vec<(u32, u32)> = ranges.to_vec();
        merged.sort_unstable();
        for (a, b) in merged {
            for start in (a..b).step_by(1000) {
                let end = (start + 1000).min(b);
                let page = self.rows(view, start, end - start)?;
                for (i, row) in page.rows.into_iter().enumerate() {
                    if seen.insert(start + i as u32) {
                        out.push(row);
                    }
                }
            }
        }
        Ok(out)
    }
}
