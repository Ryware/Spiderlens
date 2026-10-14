//! Raw or rendered values in the existing tabs (spec 014 FR-021, plan D7,
//! research R11).
//!
//! In rendered mode the reader's connection gets two temporary views that
//! shadow `urls` and `pages` (temporary objects come first in SQLite's name
//! lookup): a rendered page's title, directives flags and canonical, and its
//! `pages` row, come from `rendered_pages`. Both are plain LEFT JOINs that
//! SQLite flattens, so every existing primary-key lookup stays one. The
//! list tables (links, page text, directives) cannot be shadowed the same
//! way: a UNION view is scanned in full from a correlated subquery. The few
//! tab expressions that read them have a rendered variant instead
//! ([`crate::ColumnDef`] `rendered`).
//!
//! The `urls` view reads `main.urls` NOT INDEXED, as the tab queries do
//! (`FROM urls u NOT INDEXED`): the hint does not pass through a view, and
//! without it SQLite picks the state index and sorts, so a first screen in
//! id order costs a full sort. Lookups by id still use the rowid.
//!
//! The view adds `rendered_row`, 1 when the row shows rendered values.
//!
//! A page has rendered values when it has a `rendered_pages` row: rendered
//! or timed out with HTML. Every other row keeps its raw values.

use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::features::Features;
use crate::tabs::Need;

/// Which version of a rendered crawl's pages the tabs show.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum Values {
    #[default]
    Raw,
    Rendered,
}

impl Values {
    /// What a view shows: the request, or rendered when the crawl has
    /// rendering and nothing was asked; always raw without rendering.
    pub fn resolve(requested: Option<Self>, features: &Features) -> Self {
        if features.has(Need::Rendering) {
            requested.unwrap_or(Self::Rendered)
        } else {
            Self::Raw
        }
    }
}

/// `urls` columns rendering can change (data-model.md `rendered_pages`).
const URL_FIELDS: [&str; 6] = [
    "title",
    "noindex",
    "nofollow",
    "canonical",
    "canonical_count",
    "canonical_conflict",
];

/// Whether SQL over `urls u` reads a field the rendered `urls` view
/// changes. Used where SQLite cannot drop the view's join by itself.
pub(crate) fn reads_overlay(sql: &str) -> bool {
    URL_FIELDS
        .iter()
        .chain(&["rendered_row"])
        .any(|f| sql.contains(&format!("u.{f}")))
}

fn columns(conn: &Connection, table: &str) -> rusqlite::Result<Vec<String>> {
    let mut stmt = conn.prepare(&format!("PRAGMA main.table_info({table})"))?;
    let names = stmt.query_map([], |r| r.get::<_, String>(1))?;
    names.collect()
}

/// Creates (rendered) or drops (raw) the temporary views on `conn`.
pub(crate) fn apply(conn: &Connection, values: Values) -> rusqlite::Result<()> {
    conn.execute_batch("DROP VIEW IF EXISTS temp.urls; DROP VIEW IF EXISTS temp.pages;")?;
    if values == Values::Raw {
        return Ok(());
    }
    let rendered = columns(conn, "rendered_pages")?;
    let pick = |alias: &str, c: &str, from_rendered: bool| {
        if from_rendered {
            format!("CASE WHEN rp.url_id IS NULL THEN {alias}.{c} ELSE rp.{c} END AS {c}")
        } else {
            format!("{alias}.{c}")
        }
    };
    let url_cols: Vec<String> = columns(conn, "urls")?
        .iter()
        .map(|c| pick("u", c, URL_FIELDS.contains(&c.as_str())))
        .collect();
    let page_cols: Vec<String> = columns(conn, "pages")?
        .iter()
        .map(|c| pick("p", c, c != "url_id" && rendered.contains(c)))
        .collect();
    conn.execute_batch(&format!(
        "CREATE TEMP VIEW urls AS SELECT {}, rp.url_id IS NOT NULL AS rendered_row FROM main.urls u NOT INDEXED
           LEFT JOIN main.rendered_pages rp ON rp.url_id = u.id;
         CREATE TEMP VIEW pages AS SELECT {} FROM main.pages p
           LEFT JOIN main.rendered_pages rp ON rp.url_id = p.url_id;",
        url_cols.join(", "),
        page_cols.join(", ")
    ))
}
