//! The `orphans` Crawl Analysis pass (spec 011 plan D6, research R5) and
//! what the views read from it.
//!
//! An internal URL known from a non-link source (a sitemap, or a Search
//! Console or analytics row from spec 024's `orphan_candidates`) is an
//! orphan when no crawled internal HTML page other than itself links to it
//! with a followed anchor (area links are anchors, spec 005), and it is not
//! reached through a redirect or meta refresh from a linked URL. One linked
//! only through nofollow links is `nofollow_only`. Canonical and hreflang
//! annotations are not links here (FR-011).

use std::time::Duration;

use rusqlite::{Connection, OptionalExtension};
use serde::Serialize;
use spiderlens_config::{Mode, StoredCrawlConfig};
use ts_rs::TS;

use crate::analysis::{Pass, PassContext};
use crate::tabs::Need;
use crate::{Reader, Result};

pub struct Orphans;

impl Pass for Orphans {
    fn id(&self) -> &'static str {
        "orphans"
    }
    fn version(&self) -> u32 {
        1
    }
    fn needs(&self) -> &'static [Need] {
        &[Need::SitemapEntries]
    }
    /// Spec 011 plan D6: 3 s per 100,000 URLs.
    fn budget(&self) -> Duration {
        Duration::from_secs(3)
    }
    fn run(&self, cx: &mut PassContext<'_>) -> Result<()> {
        let conn = cx.conn;
        let s = cx.schema;
        conn.execute_batch(&format!(
            "CREATE TABLE IF NOT EXISTS {s}.orphans (
                 url_id INTEGER PRIMARY KEY, class TEXT NOT NULL, sources TEXT NOT NULL);
             DELETE FROM {s}.orphans;"
        ))?;
        // A list crawl follows no links, so every listed URL would look
        // orphaned (FR-015): the table stays empty and the view says why.
        if list_mode(conn)? {
            return Ok(());
        }
        cx.progress(0, 3, "Orphan pages")?;
        conn.execute_batch(
            "DROP TABLE IF EXISTS temp.orphan_linked;
             DROP TABLE IF EXISTS temp.orphan_reached;
             CREATE TEMP TABLE orphan_linked (id INTEGER PRIMARY KEY);
             CREATE TEMP TABLE orphan_reached (id INTEGER PRIMARY KEY);",
        )?;
        let from_pages = "JOIN urls p ON p.id = l.from_id
             WHERE l.kind = 'anchor' AND l.to_id IS NOT NULL AND l.to_id <> l.from_id
               AND p.internal = 1 AND p.state = 'crawled' AND p.error_class IS NULL
               AND p.status BETWEEN 200 AND 299 AND content_kind(p.content_type) = 'html'";
        conn.execute(
            &format!("INSERT OR IGNORE INTO temp.orphan_linked SELECT l.to_id FROM links l {from_pages} AND l.followed = 1"),
            [],
        )?;
        conn.execute(
            &format!("INSERT OR IGNORE INTO temp.orphan_reached SELECT l.to_id FROM links l {from_pages}"),
            [],
        )?;
        // The start URL is where the crawl came in: what it redirects to is
        // reached too.
        for table in ["orphan_linked", "orphan_reached"] {
            conn.execute(
                &format!("INSERT OR IGNORE INTO temp.{table} SELECT id FROM urls WHERE source = 'start'"),
                [],
            )?;
        }
        cx.progress(1, 3, "Orphan pages")?;
        // Redirect and meta refresh targets of reached URLs are reached too,
        // hop by hop, at most the crawl's redirect limit deep.
        let hops = crate::features::Features::detect(conn)?.redirect_limit.max(1) + 1;
        for table in ["orphan_linked", "orphan_reached"] {
            for _ in 0..hops {
                let added = conn.execute(
                    &format!(
                        "INSERT OR IGNORE INTO temp.{table}
                         SELECT l.to_id FROM links l JOIN temp.{table} r ON r.id = l.from_id
                         WHERE l.kind IN ('redirect', 'meta_refresh', 'js_redirect') AND l.to_id IS NOT NULL"
                    ),
                    [],
                )?;
                cx.check()?;
                if added == 0 {
                    break;
                }
            }
        }
        cx.progress(2, 3, "Orphan pages")?;
        let others = if has_table(conn, "orphan_candidates")? {
            "UNION SELECT u.id, o.source FROM orphan_candidates o JOIN urls u ON u.url = o.url WHERE u.internal = 1"
        } else {
            ""
        };
        // The start page is where the crawl came in, not an orphan.
        conn.execute(
            &format!(
                "INSERT INTO {s}.orphans (url_id, class, sources)
                 SELECT c.id,
                        CASE WHEN c.id IN (SELECT id FROM temp.orphan_reached) THEN 'nofollow_only' ELSE 'orphan' END,
                        group_concat(c.src, ',')
                 FROM (SELECT u.id, 'sitemap' AS src FROM urls u WHERE u.internal = 1 AND u.in_sitemap = 1
                       {others}) c
                 JOIN urls u ON u.id = c.id
                 WHERE c.id NOT IN (SELECT id FROM temp.orphan_linked) AND u.source <> 'start'
                 GROUP BY c.id"
            ),
            [],
        )?;
        conn.execute_batch("DROP TABLE temp.orphan_linked; DROP TABLE temp.orphan_reached;")?;
        cx.progress(3, 3, "Orphan pages")?;
        Ok(())
    }
}

fn has_table(conn: &Connection, name: &str) -> rusqlite::Result<bool> {
    conn.query_row(
        "SELECT EXISTS (SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1)",
        [name],
        |r| r.get(0),
    )
}

fn list_mode(conn: &Connection) -> rusqlite::Result<bool> {
    let config: Option<String> = conn
        .query_row("SELECT value FROM meta WHERE key = 'config'", [], |r| r.get(0))
        .optional()?;
    Ok(config
        .and_then(|c| serde_json::from_str::<StoredCrawlConfig>(&c).ok())
        .is_some_and(|s| s.config.mode == Mode::List))
}

/// The orphan view's header (FR-012, FR-013, FR-015).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct OrphanSummary {
    /// False until the pass has run on this crawl (the reason says when).
    pub available: bool,
    pub reason: Option<String>,
    /// A List mode crawl: orphans need a Spider mode crawl.
    pub list_mode: bool,
    #[ts(type = "number")]
    pub orphans: u64,
    #[ts(type = "number")]
    pub nofollow_only: u64,
    /// The crawl is running: counts can still change.
    pub provisional: bool,
    /// The crawl did not reach every internal URL (stopped, paused, or a
    /// URL or depth limit), so some orphans may be linked from pages that
    /// were not crawled.
    pub incomplete: bool,
    /// Internal URLs found but not crawled because of that.
    #[ts(type = "number")]
    pub not_crawled: u64,
}

/// One orphan, for the CLI and other features (FR-014).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct OrphanRow {
    #[ts(type = "number")]
    pub url_id: i64,
    pub url: String,
    /// `orphan` or `nofollow_only`.
    pub class: String,
    /// `sitemap`, `search_console`, `analytics`.
    pub sources: Vec<String>,
    pub status: Option<u16>,
    /// As the results table shows it: "Indexable", "Not crawled: found only
    /// in sitemap".
    pub state: String,
    /// As the results table's Indexability column shows it.
    pub indexability: Option<String>,
    /// The first sitemap file that lists it.
    pub sitemap: Option<String>,
}

/// One entry listing a URL, for the inspector's Sitemaps section (FR-026).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct UrlSitemapEntry {
    #[ts(type = "number")]
    pub file_id: i64,
    pub location: String,
    pub lastmod_raw: Option<String>,
    /// UTC seconds when the value is a valid date.
    #[ts(type = "number | null")]
    pub lastmod: Option<i64>,
}

/// Internal URLs left behind by a stop, a pause or a limit, not by a rule.
const NOT_REACHED: &str = "internal = 1 AND (state = 'queued' OR (state = 'not_crawled' \
     AND reason IN ('over_url_limit', 'over_depth_limit', 'crawl_stopped')))";

impl Reader {
    pub fn orphan_summary(&self) -> Result<OrphanSummary> {
        let list_mode = list_mode(&self.conn)?;
        let available = self.features.has(Need::Orphans) && !list_mode;
        let reason = if list_mode {
            Some("Orphan pages need a Spider mode crawl: a list crawl does not follow links.".to_string())
        } else if !available {
            let missing = if self.features.has(Need::SitemapEntries) {
                Need::Orphans
            } else {
                Need::SitemapEntries
            };
            Some(missing.unavailable_reason().to_string())
        } else {
            None
        };
        let (orphans, nofollow_only) = if available {
            self.conn.query_row(
                "SELECT COUNT(*) FILTER (WHERE class = 'orphan'), COUNT(*) FILTER (WHERE class = 'nofollow_only') FROM orphans",
                [],
                |r| Ok((r.get::<_, i64>(0)? as u64, r.get::<_, i64>(1)? as u64)),
            )?
        } else {
            (0, 0)
        };
        let status: Option<String> = self
            .conn
            .query_row("SELECT value FROM meta WHERE key = 'status'", [], |r| r.get(0))
            .optional()?;
        let not_crawled = self.conn.query_row(
            &format!("SELECT COUNT(*) FROM urls WHERE {NOT_REACHED}"),
            [],
            |r| r.get::<_, i64>(0),
        )? as u64;
        Ok(OrphanSummary {
            available,
            reason,
            list_mode,
            orphans,
            nofollow_only,
            provisional: status.as_deref() == Some("running"),
            incomplete: not_crawled > 0 || matches!(status.as_deref(), Some("stopped" | "paused")),
            not_crawled,
        })
    }

    /// Every orphan in URL order; `nofollow_only` lists that class instead.
    pub fn orphan_rows(&self, nofollow_only: bool) -> Result<Vec<OrphanRow>> {
        if !self.features.has(Need::Orphans) {
            return Ok(Vec::new());
        }
        let mut stmt = self.conn.prepare(
            "SELECT u.id, u.url, o.class, o.sources, u.status,
                    final_state(u.state, u.reason, u.reason_detail, u.check_only, u.fetched_at, u.error_class, u.status),
                    indexability(u.state, u.error_class, u.status, u.content_type, u.noindex, u.canonical, u.url),
                    (SELECT f.location FROM sitemap_entries e JOIN sitemap_files f ON f.id = e.file_id
                     WHERE e.url_id = u.id ORDER BY e.id LIMIT 1)
             FROM orphans o JOIN urls u ON u.id = o.url_id
             WHERE o.class = ?1 ORDER BY u.url",
        )?;
        let class = if nofollow_only { "nofollow_only" } else { "orphan" };
        let rows = stmt.query_map([class], |r| {
            let sources: String = r.get(3)?;
            Ok(OrphanRow {
                url_id: r.get(0)?,
                url: r.get(1)?,
                class: r.get(2)?,
                sources: sources.split(',').map(str::to_string).collect(),
                status: r.get(4)?,
                state: r.get::<_, Option<String>>(5)?.unwrap_or_default(),
                indexability: r.get(6)?,
                sitemap: r.get(7)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// The sitemap entries that list a URL, in reading order.
    pub fn sitemap_entries_for_url(&self, url_id: i64) -> Result<Vec<UrlSitemapEntry>> {
        if !self.features.has(Need::SitemapEntries) {
            return Ok(Vec::new());
        }
        let mut stmt = self.conn.prepare(
            "SELECT e.file_id, f.location, e.lastmod_raw, e.lastmod
             FROM sitemap_entries e JOIN sitemap_files f ON f.id = e.file_id
             WHERE e.url_id = ?1 AND e.kind = 'url' ORDER BY e.id",
        )?;
        let rows = stmt.query_map([url_id], |r| {
            Ok(UrlSitemapEntry {
                file_id: r.get(0)?,
                location: r.get(1)?,
                lastmod_raw: r.get(2)?,
                lastmod: r.get(3)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }
}
