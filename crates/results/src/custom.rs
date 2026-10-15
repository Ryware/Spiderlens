//! The Custom extraction and Custom search tabs (spec 013 plan D9).
//!
//! Unlike the other tabs these depend on the file: their columns and
//! filters come from the rules stored with the crawl. Values are read from
//! the packed `custom_results` row through primary-key lookups, like the
//! content columns.

use std::borrow::Cow;

use rusqlite::Connection;
use spiderlens_engine::crawl::store::custom::{RuleKind, read_rules};
use spiderlens_engine::custom::flags;

use crate::tabs::{ColumnDef, FilterDef, Need, TabDef, ValueKind};
use crate::{Error, Result};

pub const EXTRACTION: &str = "custom_extraction";
pub const SEARCH: &str = "custom_search";

/// Pages the rules ran on (or that had no stored HTML for a rerun).
const ELIGIBLE: &str = "EXISTS (SELECT 1 FROM custom_results cr WHERE cr.url_id = u.id)";

fn slot_expr(f: &str, slot: u32) -> String {
    format!("(SELECT {f}(cr.slots, {slot}) FROM custom_results cr WHERE cr.url_id = u.id)")
}

fn first_expr(slot: u32) -> String {
    format!("(SELECT json_extract(cr.firsts, '$[{slot}]') FROM custom_results cr WHERE cr.url_id = u.id)")
}

fn column(id: String, label: String, expr: String, kind: ValueKind, width: u16) -> ColumnDef {
    ColumnDef {
        id: Cow::Owned(id),
        label: Cow::Owned(label),
        expr: Cow::Owned(expr),
        kind,
        width,
        default_visible: true,
        need: Need::Base,
        derived: None,
        rendered: None,
        rendered_derived: None,
        sort_key: None,
        key: None,
    }
}

fn filter(id: String, label: String, cond: String) -> FilterDef {
    FilterDef {
        id: Cow::Owned(id),
        label: Cow::Owned(label),
        cond: Cow::Owned(cond),
        need: Need::Base,
        derived: None,
        rendered: None,
        threshold: None,
        hide_when_empty: None,
    }
}

fn address() -> ColumnDef {
    column(
        "address".into(),
        "Address".into(),
        "u.url".into(),
        ValueKind::Url,
        420,
    )
}

fn all_filter() -> FilterDef {
    filter("all".into(), "All".into(), "1".into())
}

/// The two tabs for the rules stored with the crawl; none when it has no
/// rules (FR-022, FR-023).
pub(crate) fn tabs(conn: &Connection) -> Result<Vec<TabDef>> {
    let rules = read_rules(conn).map_err(|e| Error::invalid(e.to_string()))?;
    let mut out = Vec::new();
    let extractors: Vec<_> = rules.iter().filter(|r| r.kind == RuleKind::Extractor).collect();
    let searches: Vec<_> = rules.iter().filter(|r| r.kind == RuleKind::Search).collect();
    if !extractors.is_empty() {
        let mut columns = vec![address()];
        let mut filters = vec![all_filter()];
        for r in &extractors {
            let s = r.slot;
            let first = first_expr(s);
            let mut value = column(
                format!("x{s}"),
                r.name.clone(),
                first.clone(),
                ValueKind::Text,
                220,
            );
            // Numbers sort numerically, then text naturally (spec
            // Assumptions: numeric sorting when every value is a number).
            value.sort_key = Some(Cow::Owned(format!("custom_num({first})")));
            columns.push(value);
            let mut count = column(
                format!("x{s}_count"),
                format!("{}: matches", r.name),
                slot_expr("custom_count", s),
                ValueKind::Int,
                110,
            );
            count.default_visible = false;
            columns.push(count);
            let flags = slot_expr("custom_flags", s);
            filters.push(filter(
                format!("x{s}_has"),
                format!("{}: has a value", r.name),
                format!("{flags} & {} = {}", flags::RUN | flags::NO_MATCH, flags::RUN),
            ));
            filters.push(filter(
                format!("x{s}_none"),
                format!("{}: no match", r.name),
                format!("{flags} & {} != 0", flags::NO_MATCH),
            ));
            filters.push(filter(
                format!("x{s}_more"),
                format!("{}: more than stored", r.name),
                format!("{flags} & {} != 0", flags::MORE_THAN_STORED),
            ));
        }
        out.push(TabDef {
            id: Cow::Borrowed(EXTRACTION),
            title: Cow::Borrowed("Custom extraction"),
            eligible: Cow::Borrowed(ELIGIBLE),
            rows: None,
            columns,
            filters,
            search_column: None,
            scoped: false,
        });
    }
    if !searches.is_empty() {
        let mut columns = vec![address()];
        let mut filters = vec![all_filter()];
        for r in &searches {
            let s = r.slot;
            columns.push(column(
                format!("s{s}"),
                r.name.clone(),
                slot_expr("custom_count", s),
                ValueKind::Int,
                140,
            ));
            let not = serde_json::from_str::<spiderlens_config::Search>(&r.def_json)
                .is_ok_and(|d| d.condition == spiderlens_config::SearchCondition::NotContains);
            let flags = slot_expr("custom_flags", s);
            filters.push(filter(
                format!("s{s}_meets"),
                format!(
                    "{}: {}",
                    r.name,
                    if not { "does not contain" } else { "contains" }
                ),
                format!("{flags} & {} != 0", flags::MEETS),
            ));
        }
        out.push(TabDef {
            id: Cow::Borrowed(SEARCH),
            title: Cow::Borrowed("Custom search"),
            eligible: Cow::Borrowed(ELIGIBLE),
            rows: None,
            columns,
            filters,
            search_column: None,
            scoped: false,
        });
    }
    Ok(out)
}

/// With `--all-values SEP`: every stored value of an extractor column
/// joined by `sep`, instead of the first (FR-025).
pub(crate) fn all_values_expr(column: &ColumnDef, sep: &str) -> Option<String> {
    let slot: u32 = column.id.strip_prefix('x')?.parse().ok()?;
    let sep = sep.replace('\'', "''");
    Some(format!(
        "(SELECT custom_all(cr.firsts, (SELECT cv.data FROM custom_values cv WHERE cv.url_id = u.id), {slot}, '{sep}') \
         FROM custom_results cr WHERE cr.url_id = u.id)"
    ))
}

/// One rule's result on one page, for the detail pane (FR-024).
#[derive(Debug, Clone, serde::Serialize, ts_rs::TS)]
#[ts(export)]
pub struct CustomResult {
    pub name: String,
    /// `extractor` or `search`.
    pub kind: String,
    /// Null when the rule has not run on this page.
    pub count: Option<u32>,
    /// `meets`, `no_match`, `more_than_stored`, `truncated`, `timed_out`,
    /// `error`, `element_not_found`, `not_available`, `bad_json`.
    pub flags: Vec<String>,
    /// Extractor values in document order, or search matches in context.
    pub values: Vec<String>,
    pub error: Option<String>,
    /// `stale` when the rule changed after it ran on this page.
    pub run_state: Option<String>,
}

impl crate::Reader {
    /// Every rule's result on one page, extractors first. Empty when the
    /// crawl has no rules.
    pub fn custom_results(&mut self, id: i64) -> Result<Vec<CustomResult>> {
        use spiderlens_engine::crawl::store::custom::read_page_custom;
        let err = |e: spiderlens_engine::crawl::store::StoreError| Error::invalid(e.to_string());
        let rules = read_rules(&self.conn).map_err(err)?;
        if rules.is_empty() {
            return Ok(Vec::new());
        }
        let page = read_page_custom(&self.conn, id).map_err(err)?;
        Ok(rules
            .into_iter()
            .map(|r| {
                let slot = page.as_ref().and_then(|p| p.slots.get(r.slot as usize));
                let ran = slot.filter(|s| s.1 & flags::RUN != 0);
                CustomResult {
                    kind: r.kind.as_str().to_string(),
                    count: ran.map(|s| s.0),
                    flags: ran
                        .map(|s| flags::names(s.1).into_iter().map(str::to_string).collect())
                        .unwrap_or_default(),
                    values: page.as_ref().map(|p| p.all_values(r.slot)).unwrap_or_default(),
                    error: page.as_ref().and_then(|p| p.error(r.slot)).map(str::to_string),
                    run_state: r.run_state,
                    name: r.name,
                }
            })
            .collect())
    }
}
