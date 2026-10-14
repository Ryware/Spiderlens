//! Matching (plan D4) and the counts the Overview, the Change detection
//! view and the CLI summary show (FR-011, FR-014..FR-016).

use rusqlite::Connection;
use serde::Serialize;
use ts_rs::TS;

use crate::elements::{Element, ElementKind};
use crate::info::{ConfigDiff, CrawlInfo};
use crate::registry::Layout;
use crate::side::filter_available;
use crate::{Result, Side};

/// `matches.grp` values.
pub(crate) mod grp {
    /// In both crawls.
    pub const MATCHED: i64 = 0;
    /// Only in the current crawl.
    pub const NEW: i64 = 1;
    /// Only in the previous crawl, which had reached it.
    pub const MISSING: i64 = 2;
    /// In both, but the current crawl had not reached it (queued, limit, stop).
    pub const NOT_REACHED: i64 = 3;
    /// Only in the previous crawl, beyond where an incomplete current crawl got.
    pub const NOT_REACHED_INCOMPLETE: i64 = 4;
    /// Only in the previous crawl, which had not reached it either.
    pub const NEITHER: i64 = 5;
}

pub(crate) fn build_matches(ws: &Connection, current_complete: bool) -> Result<()> {
    // The shallowest depth the current crawl left unfinished; previous-only
    // URLs at or below it may simply not have been reached (research R4).
    let open_depth: Option<i64> = if current_complete {
        None
    } else {
        ws.query_row(
            "SELECT MIN(depth) FROM side_rows WHERE side = 1 AND reach > 0",
            [],
            |r| r.get(0),
        )?
    };
    ws.execute_batch(&format!(
        "CREATE INDEX side_key ON side_rows(side, key);
         INSERT INTO matches (prev_id, cur_id, key, grp)
           SELECT p.id, c.id, p.key,
             CASE
               WHEN c.id IS NOT NULL AND c.reach > 0 AND p.reach = 0 THEN {nr}
               WHEN c.id IS NOT NULL THEN {matched}
               WHEN p.reach > 0 THEN {neither}
               WHEN {open} IS NOT NULL AND p.depth >= {open} THEN {nri}
               ELSE {missing}
             END
           FROM side_rows p LEFT JOIN side_rows c ON c.side = 1 AND c.key = p.key
           WHERE p.side = 0;
         INSERT INTO matches (prev_id, cur_id, key, grp)
           SELECT NULL, c.id, c.key, {new} FROM side_rows c
           WHERE c.side = 1 AND NOT EXISTS (SELECT 1 FROM side_rows p WHERE p.side = 0 AND p.key = c.key);
         CREATE INDEX matches_prev ON matches(prev_id);
         CREATE INDEX matches_cur ON matches(cur_id);
         CREATE INDEX matches_grp ON matches(grp);",
        nr = grp::NOT_REACHED,
        matched = grp::MATCHED,
        neither = grp::NEITHER,
        nri = grp::NOT_REACHED_INCOMPLETE,
        missing = grp::MISSING,
        new = grp::NEW,
        open = open_depth.map_or("NULL".to_string(), |d| d.to_string()),
    ))?;
    Ok(())
}

/// The join every pair query uses: `p` previous row, `c` current row.
pub(crate) const PAIRS: &str = "matches m \
     LEFT JOIN side_rows p ON p.side = 0 AND p.id = m.prev_id \
     LEFT JOIN side_rows c ON c.side = 1 AND c.id = m.cur_id";

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct StateDelta {
    pub state: String,
    pub previous: u64,
    pub current: u64,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct FilterDelta {
    pub tab: String,
    pub tab_title: String,
    pub filter: String,
    pub filter_label: String,
    /// Null when that crawl cannot evaluate the filter.
    pub previous: Option<u64>,
    pub current: Option<u64>,
    pub new: u64,
    pub added: u64,
    pub removed: u64,
    pub missing: u64,
    pub not_reached: u64,
    pub comparable: bool,
    pub reason: Option<String>,
}

impl FilterDelta {
    pub fn change(&self) -> i64 {
        self.current.unwrap_or(0) as i64 - self.previous.unwrap_or(0) as i64
    }
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct ElementDelta {
    pub element: String,
    pub label: String,
    pub kind: ElementKind,
    pub on: bool,
    pub threshold: Option<f64>,
    pub changed: u64,
    pub comparable: bool,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct Summary {
    pub previous: CrawlInfo,
    pub current: CrawlInfo,
    pub config_diffs: Vec<ConfigDiff>,
    pub by_state: Vec<StateDelta>,
    /// URLs with a final state in both crawls.
    pub matched: u64,
    pub new: u64,
    pub missing: u64,
    pub not_reached: u64,
    pub collisions: u64,
    /// Internal HTML pages of the previous crawl missing from the current one.
    pub missing_pages: u64,
    /// Pages indexable in the previous crawl and not in the current one.
    pub lost_indexable: u64,
    /// Fewer than 10% of the smaller crawl's URLs matched (FR-010).
    pub low_overlap: bool,
    pub filters: Vec<FilterDelta>,
    pub elements: Vec<ElementDelta>,
}

fn count(ws: &Connection, sql: &str) -> Result<u64> {
    Ok(ws
        .query_row(sql, [], |r| r.get::<_, Option<i64>>(0))?
        .unwrap_or(0) as u64)
}

pub(crate) fn summary(
    ws: &Connection,
    sides: &[Side; 2],
    layout: &Layout,
    elements: &[Element],
) -> Result<Summary> {
    let mut by_state: Vec<StateDelta> = Vec::new();
    {
        let mut stmt = ws.prepare(
            "SELECT e_final_state, SUM(side = 0), SUM(side = 1) FROM side_rows
             WHERE reach = 0 GROUP BY 1 ORDER BY 1",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(StateDelta {
                state: r.get::<_, Option<String>>(0)?.unwrap_or_default(),
                previous: r.get::<_, i64>(1)? as u64,
                current: r.get::<_, i64>(2)? as u64,
            })
        })?;
        for r in rows {
            by_state.push(r?);
        }
    }
    let matched = count(
        ws,
        &format!(
            "SELECT COUNT(*) FROM {PAIRS} WHERE m.grp = {} AND p.reach = 0",
            grp::MATCHED
        ),
    )?;
    let new = count(
        ws,
        &format!(
            "SELECT COUNT(*) FROM {PAIRS} WHERE m.grp = {} AND c.reach = 0",
            grp::NEW
        ),
    )?;
    let missing = count(
        ws,
        &format!("SELECT COUNT(*) FROM matches WHERE grp = {}", grp::MISSING),
    )?;
    let not_reached = count(
        ws,
        &format!(
            "SELECT COUNT(*) FROM matches WHERE grp IN ({}, {})",
            grp::NOT_REACHED,
            grp::NOT_REACHED_INCOMPLETE
        ),
    )?;
    let collisions = count(
        ws,
        "SELECT COUNT(*) FROM (SELECT cur_id FROM matches WHERE cur_id IS NOT NULL GROUP BY cur_id HAVING COUNT(*) > 1)",
    )?;
    let missing_pages = count(
        ws,
        &format!(
            "SELECT COUNT(*) FROM {PAIRS} WHERE m.grp = {} AND p.html_ok = 1",
            grp::MISSING
        ),
    )?;
    let lost_indexable = count(
        ws,
        &format!(
            "SELECT COUNT(*) FROM {PAIRS} WHERE m.grp = {} AND p.e_indexability = 'Indexable' \
             AND c.reach = 0 AND c.e_indexability IS NOT 'Indexable'",
            grp::MATCHED
        ),
    )?;
    let final_prev = count(ws, "SELECT COUNT(*) FROM side_rows WHERE side = 0 AND reach = 0")?;
    let final_cur = count(ws, "SELECT COUNT(*) FROM side_rows WHERE side = 1 AND reach = 0")?;
    let smaller = final_prev.min(final_cur);
    Ok(Summary {
        config_diffs: crate::info::config_diffs(&sides[0].info, &sides[1].info),
        previous: sides[0].info.clone(),
        current: sides[1].info.clone(),
        by_state,
        matched,
        new,
        missing,
        not_reached,
        collisions,
        missing_pages,
        lost_indexable,
        low_overlap: smaller > 0 && matched * 10 < smaller,
        filters: filter_deltas(ws, sides, layout)?,
        elements: element_deltas(ws, sides, elements)?,
    })
}

/// All filters' deltas: per-side totals in one pass over `side_rows`, the
/// groups in one pass over the pairs.
pub(crate) fn filter_deltas(ws: &Connection, sides: &[Side; 2], layout: &Layout) -> Result<Vec<FilterDelta>> {
    let n = layout.slots.len();
    let totals_sql = format!(
        "SELECT side, {} FROM side_rows GROUP BY side",
        (0..n)
            .map(|k| format!("SUM({})", Layout::test("side_rows", k)))
            .collect::<Vec<_>>()
            .join(", ")
    );
    let mut totals = [vec![0u64; n], vec![0u64; n]];
    {
        let mut stmt = ws.prepare(&totals_sql)?;
        let mut rows = stmt.query([])?;
        while let Some(r) = rows.next()? {
            let side: i64 = r.get(0)?;
            for (k, t) in totals[side as usize].iter_mut().enumerate() {
                *t = r.get::<_, Option<i64>>(k + 1)?.unwrap_or(0) as u64;
            }
        }
    }
    let mut parts = Vec::with_capacity(n * 5);
    for k in 0..n {
        let (pb, cb) = (Layout::test("p", k), Layout::test("c", k));
        parts.push(format!(
            "SUM(CASE WHEN m.grp = {} AND c.reach = 0 THEN {cb} ELSE 0 END)",
            grp::NEW
        ));
        parts.push(format!(
            "SUM(CASE WHEN m.grp = {} AND c.reach = 0 THEN {cb} * (1 - {pb}) ELSE 0 END)",
            grp::MATCHED
        ));
        parts.push(format!(
            "SUM(CASE WHEN m.grp = {} AND c.reach = 0 THEN {pb} * (1 - {cb}) ELSE 0 END)",
            grp::MATCHED
        ));
        parts.push(format!(
            "SUM(CASE WHEN m.grp = {} THEN {pb} ELSE 0 END)",
            grp::MISSING
        ));
        parts.push(format!(
            "SUM(CASE WHEN m.grp IN ({}, {}) THEN {pb} ELSE 0 END)",
            grp::NOT_REACHED,
            grp::NOT_REACHED_INCOMPLETE
        ));
    }
    let groups: Vec<u64> = ws.query_row(&format!("SELECT {} FROM {PAIRS}", parts.join(", ")), [], |r| {
        (0..n * 5)
            .map(|i| Ok(r.get::<_, Option<i64>>(i)?.unwrap_or(0) as u64))
            .collect::<rusqlite::Result<Vec<_>>>()
    })?;
    Ok(layout
        .slots
        .iter()
        .enumerate()
        .map(|(k, s)| {
            let avail = [
                filter_available(&sides[0].features, layout, k),
                filter_available(&sides[1].features, layout, k),
            ];
            let comparable = avail[0] && avail[1];
            let g = &groups[k * 5..k * 5 + 5];
            FilterDelta {
                tab: s.tab.id.into(),
                tab_title: s.tab.title.into(),
                filter: s.filter.id.into(),
                filter_label: s.filter.label.into(),
                previous: avail[0].then_some(totals[0][k]),
                current: avail[1].then_some(totals[1][k]),
                new: g[0],
                added: if comparable { g[1] } else { 0 },
                removed: if comparable { g[2] } else { 0 },
                missing: g[3],
                not_reached: g[4],
                comparable,
                reason: (!comparable).then(|| {
                    let side = if avail[0] { "current" } else { "previous" };
                    format!("The {side} crawl: {}", s.filter.need.unavailable_reason())
                }),
            }
        })
        .collect())
}

pub(crate) fn element_deltas(
    ws: &Connection,
    sides: &[Side; 2],
    elements: &[Element],
) -> Result<Vec<ElementDelta>> {
    let parts: Vec<String> = elements
        .iter()
        .map(|e| {
            format!(
                "SUM(CASE WHEN {} AND {} THEN 1 ELSE 0 END)",
                e.applies_sql(),
                e.changed_sql()
            )
        })
        .collect();
    let counts: Vec<u64> = ws.query_row(
        &format!(
            "SELECT {} FROM {PAIRS} WHERE m.grp = {}",
            parts.join(", "),
            grp::MATCHED
        ),
        [],
        |r| {
            (0..elements.len())
                .map(|i| Ok(r.get::<_, Option<i64>>(i)?.unwrap_or(0) as u64))
                .collect::<rusqlite::Result<Vec<_>>>()
        },
    )?;
    Ok(elements
        .iter()
        .zip(counts)
        .map(|(e, changed)| {
            let avail = [sides[0].features.has(e.need), sides[1].features.has(e.need)];
            let comparable = avail[0] && avail[1];
            ElementDelta {
                element: e.id.into(),
                label: e.label.into(),
                kind: e.kind,
                on: e.on,
                threshold: e.threshold,
                changed: if comparable && e.on { changed } else { 0 },
                comparable,
                reason: (!comparable).then(|| {
                    let side = if avail[0] { "current" } else { "previous" };
                    format!("The {side} crawl: {}", e.need.unavailable_reason())
                }),
            }
        })
        .collect())
}
