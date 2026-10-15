//! Filling the workspace from one crawl file (plan D2, D3, D8).

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use rusqlite::Connection;
use spiderlens_results::{Derived, Features, build_derived, tab};

use crate::elements::Element;
use crate::registry::{BITS, Layout};
use crate::{Error, Result};

const BATCH: i64 = 50_000;

/// Reasons a URL was left for later because of a limit or a stop (spec 003).
pub(crate) const LIMIT_REASONS: &str = "('over_depth_limit', 'over_url_limit', 'crawl_stopped')";

pub(crate) fn create_tables(ws: &Connection, layout: &Layout, elements: &[Element]) -> Result<()> {
    let mut cols: Vec<String> = (0..layout.columns())
        .map(|i| format!("fb{i} INTEGER NOT NULL"))
        .collect();
    for e in elements {
        cols.push(e.col());
        if let Some(h) = e.hash_col() {
            cols.push(h);
        }
    }
    ws.execute_batch(&format!(
        "CREATE TABLE side_rows (
           side INTEGER NOT NULL,
           id INTEGER NOT NULL,
           url TEXT NOT NULL,
           key TEXT NOT NULL,
           internal INTEGER NOT NULL,
           html_ok INTEGER NOT NULL,
           reach INTEGER NOT NULL,
           depth INTEGER,
           {},
           PRIMARY KEY (side, id)
         );
         CREATE TABLE matches (
           prev_id INTEGER,
           cur_id INTEGER,
           key TEXT NOT NULL,
           grp INTEGER NOT NULL
         );
         CREATE TABLE info (key TEXT PRIMARY KEY, value TEXT NOT NULL);",
        cols.join(",\n           ")
    ))?;
    Ok(())
}

/// Which filters and elements a side can evaluate.
pub(crate) fn filter_available(features: &Features, layout: &Layout, k: usize) -> bool {
    let f = layout.slots[k].filter;
    // Issue hits depend on the issue settings, which a comparison does not
    // carry yet (issue deltas are spec 012 follow-up work).
    features.has(f.need) && f.derived != Some(Derived::IssueHits)
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn fill(
    conn: &Connection,
    ws_file: &Path,
    side: u8,
    features: &Features,
    layout: &Layout,
    elements: &[Element],
    mapped: bool,
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(u64, u64),
) -> Result<()> {
    conn.execute("ATTACH DATABASE ?1 AS cmp", [ws_file.to_string_lossy()])?;
    let result = fill_attached(conn, side, features, layout, elements, mapped, cancel, progress);
    if result.is_err() {
        let _ = conn.execute_batch("ROLLBACK");
    }
    conn.execute_batch("DETACH DATABASE cmp")?;
    result
}

#[allow(clippy::too_many_arguments)]
fn fill_attached(
    conn: &Connection,
    side: u8,
    features: &Features,
    layout: &Layout,
    elements: &[Element],
    mapped: bool,
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(u64, u64),
) -> Result<()> {
    // One transaction: a snapshot of a crawl that is still running, and one
    // commit of the workspace rows.
    conn.execute_batch("BEGIN")?;
    let mut derived: Vec<Derived> = Vec::new();
    for (k, s) in layout.slots.iter().enumerate() {
        if !filter_available(features, layout, k) {
            continue;
        }
        for d in s.filter.derived.into_iter().chain(s.tab.rows) {
            if !derived.contains(&d) {
                derived.push(d);
            }
        }
    }
    // A tab that lists the rows of a derived table reads it as `x`; only
    // one such table exists (page titles).
    let rows: Vec<Derived> = layout
        .slots
        .iter()
        .filter_map(|s| s.tab.rows)
        .fold(Vec::new(), |mut v, d| {
            if !v.contains(&d) {
                v.push(d);
            }
            v
        });
    debug_assert!(rows.len() <= 1, "two tabs read different tables as x");
    let join = rows
        .first()
        .filter(|d| derived.contains(d))
        .map_or(String::new(), |d| {
            format!(" LEFT JOIN temp.{} x ON x.id = u.id", d.table())
        });
    for &d in &derived {
        build_derived(conn, d, features)?;
    }

    let html_ok = tab("page-titles").expect("page titles tab").eligible_on_urls();
    let internal = tab("internal").expect("internal tab").eligible_on_urls();
    let mut names = vec![
        "side", "id", "url", "key", "internal", "html_ok", "reach", "depth",
    ]
    .into_iter()
    .map(String::from)
    .collect::<Vec<_>>();
    let mut exprs = vec![
        side.to_string(),
        "u.id".into(),
        "u.url".into(),
        if mapped {
            "map_url(u.url)".into()
        } else {
            "u.url".into()
        },
        format!("CASE WHEN {internal} THEN 1 ELSE 0 END"),
        format!("CASE WHEN {html_ok} THEN 1 ELSE 0 END"),
        format!(
            "CASE WHEN u.state IN ('queued', 'in_flight') THEN 1 WHEN u.reason IN {LIMIT_REASONS} THEN 2 ELSE 0 END"
        ),
        "u.depth".into(),
    ];
    for col in 0..layout.columns() {
        let parts: Vec<String> = (col * BITS..((col + 1) * BITS).min(layout.slots.len()))
            .filter(|&k| filter_available(features, layout, k))
            .map(|k| {
                let s = &layout.slots[k];
                format!(
                    "(CASE WHEN ({}) AND ({}) THEN {} ELSE 0 END)",
                    s.tab.eligible_on_urls(),
                    s.filter.cond,
                    1i64 << (k % BITS)
                )
            })
            .collect();
        names.push(format!("fb{col}"));
        exprs.push(if parts.is_empty() {
            "0".into()
        } else {
            parts.join(" | ")
        });
    }
    for e in elements {
        let ok = features.has(e.need);
        names.push(e.col());
        exprs.push(if ok { e.expr.into() } else { "NULL".into() });
        if let (Some(h), Some(hx)) = (e.hash_col(), e.hash_expr) {
            names.push(h);
            exprs.push(if ok { hx.into() } else { "NULL".into() });
        }
    }
    let sql = format!(
        "INSERT INTO cmp.side_rows ({}) SELECT {} FROM main.urls u{join} WHERE u.id BETWEEN ?1 AND ?2",
        names.join(", "),
        exprs.join(",\n  ")
    );
    let (lo, hi, total): (Option<i64>, Option<i64>, i64) =
        conn.query_row("SELECT MIN(id), MAX(id), COUNT(*) FROM main.urls", [], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?))
        })?;
    let mut stmt = conn.prepare(&sql)?;
    let mut done = 0u64;
    progress(0, total as u64);
    if let (Some(lo), Some(hi)) = (lo, hi) {
        let mut a = lo;
        while a <= hi {
            if cancel.load(Ordering::Relaxed) {
                return Err(Error::Cancelled);
            }
            let b = a.saturating_add(BATCH - 1);
            done += stmt.execute([a, b])? as u64;
            progress(done, total as u64);
            a = b.saturating_add(1);
        }
    }
    drop(stmt);
    conn.execute_batch("COMMIT")?;
    Ok(())
}
