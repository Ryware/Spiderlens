//! Everything known about one URL, for the detail pane (plan D10, FR-019..25).

use std::collections::BTreeMap;

use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::reader::Reader;
use crate::tabs::{Derived, Need};
use crate::{Error, Result, sql};

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct RedirectHop {
    #[ts(type = "number")]
    pub id: i64,
    pub url: String,
    pub status: Option<u16>,
    pub final_state: String,
}

/// One URL's spec 023 values (FR-008).
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct LinkMetrics {
    /// 0-100, unrounded.
    pub score: f64,
    pub rank: u32,
    /// Number of scored URLs, for "rank 12 of 4,812".
    pub of: u32,
    pub unique_inlinks: u32,
    pub followed_inlinks: u32,
    pub all_inlinks: u32,
    pub unique_outlinks: u32,
    pub external_outlinks: u32,
    pub pct_linking: f64,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct UrlDetail {
    #[ts(type = "number")]
    pub id: i64,
    pub url: String,
    /// Decoded host and path, for display (Edge case: internationalised URLs).
    pub url_readable: String,
    pub internal: bool,
    pub final_state: String,
    pub status: Option<u16>,
    pub status_text: Option<String>,
    pub content_type: Option<String>,
    pub depth: u32,
    pub error_class: Option<String>,
    pub error_label: Option<String>,
    pub error_message: Option<String>,
    pub not_crawled_reason: Option<String>,
    pub reason_detail: Option<String>,
    pub robots_rule: Option<String>,
    pub found_on: Option<String>,
    pub discovered_via: String,
    pub in_sitemap: bool,
    /// Milliseconds since the Unix epoch.
    #[ts(type = "number | null")]
    pub fetched_at: Option<i64>,
    /// This URL and each hop after it, in order (FR-021). Empty when it does
    /// not redirect.
    pub redirects: Vec<RedirectHop>,
    pub redirect_loop: bool,
    pub redirect_over_limit: bool,
    #[ts(type = "number")]
    pub inlinks: u64,
    #[ts(type = "number")]
    pub outlinks: u64,
    #[ts(type = "number")]
    pub images: u64,
    /// Body size after transfer decoding, in bytes.
    #[ts(type = "number | null")]
    pub size: Option<i64>,
    /// Request sent to last body byte read.
    #[ts(type = "number | null")]
    pub response_ms: Option<i64>,
    /// "Indexable" or "Non-indexable: <reason>".
    pub indexability: Option<String>,
    /// Response headers in received order, names lowercase (FR-020).
    pub headers: Vec<(String, String)>,
    /// Number of headings, for the Headings list.
    #[ts(type = "number")]
    pub headings: u64,
    /// The page's HTML is stored in the crawl file.
    pub html_stored: bool,
    /// Link Score and link counts (spec 023); None for external URLs and
    /// while the crawl runs.
    pub link_metrics: Option<LinkMetrics>,
    /// Requests made for this URL across the crawl, retries included
    /// (spec 004 FR-020).
    pub attempts: Option<u32>,
    /// The error's sub-reason, such as "refused" or "expired" (FR-013).
    pub error_reason: Option<String>,
    /// The result came from "Retry failed URLs" (US5.3).
    pub manual_retry: bool,
    /// Fields and sections this file has no data for, with the reason.
    #[ts(type = "Record<string, string>")]
    pub unavailable: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ListKind {
    Inlinks,
    Outlinks,
    Images,
    Headings,
}

/// One link in the Inlinks, Outlinks or Images list.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct LinkRow {
    /// The other URL: the source for inlinks, the target otherwise.
    #[ts(type = "number")]
    pub id: i64,
    pub url: String,
    /// The href as written in the page.
    pub raw: String,
    pub kind: String,
    pub nofollow: bool,
    pub followed: bool,
    pub internal: bool,
    pub status: Option<u16>,
    pub final_state: String,
    /// Anchor text, or alt text for images. For images, null means the alt
    /// attribute is missing.
    pub text: Option<String>,
    pub sponsored: bool,
    pub ugc: bool,
    /// Where the anchor sits on its page (spec 023 US4): a `Position` or
    /// "unknown" (HTML not stored). Null when not worked out yet, or not
    /// an anchor.
    pub position: Option<String>,
}

/// One heading of a page, in document order (FR-023).
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct HeadingRow {
    pub level: u8,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct ListPage {
    #[ts(type = "number")]
    pub total: u64,
    pub from: u32,
    pub rows: Vec<LinkRow>,
    /// The rows of the Headings list (empty for the link lists).
    pub headings: Vec<HeadingRow>,
    /// Set when this file cannot list this section.
    pub unavailable: Option<String>,
}

const FINAL_STATE: &str =
    "final_state(x.state, x.reason, x.reason_detail, x.check_only, x.fetched_at, x.error_class, x.status)";

impl Reader {
    pub fn detail(&mut self, id: i64) -> Result<UrlDetail> {
        let sql = format!(
            "SELECT x.url, x.internal, {FINAL_STATE}, x.status, x.content_type, x.depth, x.error_class,
                    x.error_message, x.reason, x.reason_detail, x.robots_rule,
                    (SELECT s.url FROM urls s WHERE s.id = x.source_id), x.source, x.in_sitemap, x.fetched_at
             FROM urls x WHERE x.id = ?1"
        );
        let row = self
            .conn
            .query_row(&sql, [id], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, bool>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, Option<u16>>(3)?,
                    r.get::<_, Option<String>>(4)?,
                    r.get::<_, u32>(5)?,
                    r.get::<_, Option<String>>(6)?,
                    r.get::<_, Option<String>>(7)?,
                    r.get::<_, Option<String>>(8)?,
                    r.get::<_, Option<String>>(9)?,
                    r.get::<_, Option<String>>(10)?,
                    r.get::<_, Option<String>>(11)?,
                    r.get::<_, String>(12)?,
                    r.get::<_, bool>(13)?,
                    r.get::<_, Option<i64>>(14)?,
                ))
            })
            .optional()?
            .ok_or_else(|| Error::invalid(format!("URL {id} is not in this crawl")))?;
        let (
            url,
            internal,
            final_state,
            status,
            content_type,
            depth,
            error_class,
            error_message,
            reason,
            reason_detail,
            robots_rule,
            found_on,
            source,
            in_sitemap,
            fetched_at,
        ) = row;
        let (redirects, redirect_loop) = self.redirect_chain(id)?;
        let limit = self.features.redirect_limit as usize;
        let count = |sql: &str| -> rusqlite::Result<u64> {
            self.conn
                .query_row(sql, [id], |r| r.get::<_, i64>(0))
                .map(|n| n as u64)
        };
        let inlinks = count("SELECT COUNT(*) FROM links WHERE to_id = ?1")?;
        let outlinks = count("SELECT COUNT(*) FROM links WHERE from_id = ?1")?;
        let images = count("SELECT COUNT(*) FROM links WHERE from_id = ?1 AND kind = 'image'")?;
        let content = self.features.has(Need::Extraction);
        let headings = if content {
            count("SELECT COUNT(*) FROM page_text WHERE url_id = ?1 AND kind = 'heading'")?
        } else {
            0
        };
        let (indexability, headers) = if content {
            let ix: Option<String> = self.conn.query_row(
                "SELECT indexability(u.state, u.error_class, u.status, u.content_type, u.noindex, u.canonical, u.url)
                 FROM urls u WHERE u.id = ?1",
                [id],
                |r| r.get(0),
            )?;
            let raw: Option<String> = self
                .conn
                .query_row("SELECT headers FROM headers WHERE url_id = ?1", [id], |r| {
                    r.get(0)
                })
                .optional()?;
            let mut headers = raw
                .and_then(|h| serde_json::from_str::<Vec<(String, String)>>(&h).ok())
                .unwrap_or_default();
            // Older files that are only viewed still hold cookie values
            // (spec 025 FR-026).
            spiderlens_engine::crawl::redact::scrub_pairs(&mut headers);
            (ix, headers)
        } else {
            (None, vec![])
        };
        let (size, response_ms) = if self.features.has(Need::Timing) {
            self.conn.query_row(
                "SELECT size_bytes, response_ms FROM urls WHERE id = ?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )?
        } else {
            (None, None)
        };
        let (attempts, error_reason, manual_retry) = if self.features.has(Need::Attempts) {
            self.conn.query_row(
                "SELECT attempts, error_reason, manual_retry FROM urls WHERE id = ?1",
                [id],
                |r| {
                    Ok((
                        r.get(0)?,
                        r.get(1)?,
                        r.get::<_, Option<bool>>(2)?.unwrap_or(false),
                    ))
                },
            )?
        } else {
            (None, None, false)
        };
        let html_stored = self.features.has(Need::RawHtml)
            && self.conn.query_row(
                "SELECT EXISTS (SELECT 1 FROM bodies WHERE url_id = ?1)",
                [id],
                |r| r.get(0),
            )?;
        let link_metrics = if internal && self.features.has(Need::Analysis) {
            self.ensure_derived([crate::tabs::Derived::LinkMetrics])?;
            self.conn
                .query_row(
                    "SELECT score, rank, (SELECT COUNT(*) FROM link_metrics), unique_inlinks, followed_inlinks,
                            all_inlinks, unique_outlinks, external_outlinks, pct_linking
                     FROM link_metrics WHERE url_id = ?1",
                    [id],
                    |r| {
                        Ok(LinkMetrics {
                            score: r.get(0)?,
                            rank: r.get(1)?,
                            of: r.get(2)?,
                            unique_inlinks: r.get(3)?,
                            followed_inlinks: r.get(4)?,
                            all_inlinks: r.get(5)?,
                            unique_outlinks: r.get(6)?,
                            external_outlinks: r.get(7)?,
                            pct_linking: r.get(8)?,
                        })
                    },
                )
                .optional()?
        } else {
            None
        };
        let mut unavailable = BTreeMap::new();
        for (field, need) in [
            ("indexability", Need::Extraction),
            ("headers", Need::Extraction),
            ("headings", Need::Extraction),
            ("anchor_text", Need::Extraction),
            ("size", Need::Timing),
            ("response_time", Need::Timing),
            ("attempts", Need::Attempts),
            ("raw_html", Need::RawHtml),
            ("link_metrics", Need::Analysis),
        ] {
            if !self.features.has(need) {
                unavailable.insert(field.to_string(), need.unavailable_reason().to_string());
            }
        }
        let reason_label = reason.as_deref().and_then(|r| {
            spiderlens_engine::crawl::NotCrawledReason::parse(r).map(|r| r.label().to_string())
        });
        Ok(UrlDetail {
            id,
            url_readable: sql::readable_url(&url),
            url,
            internal,
            final_state,
            status,
            status_text: status
                .and_then(|s| sql::status_text(i64::from(s)))
                .map(str::to_string),
            content_type,
            depth,
            error_label: error_class
                .as_deref()
                .map(|c| sql::error_class_label(c).to_string()),
            error_class,
            error_message,
            not_crawled_reason: reason_label,
            reason_detail,
            robots_rule,
            found_on,
            discovered_via: source,
            in_sitemap,
            fetched_at,
            redirect_over_limit: redirects.len().saturating_sub(1) > limit,
            redirects,
            redirect_loop,
            inlinks,
            outlinks,
            images,
            size,
            response_ms,
            indexability,
            headers,
            headings,
            html_stored,
            link_metrics,
            attempts,
            error_reason,
            manual_retry,
            unavailable,
        })
    }

    /// Follows stored redirect targets from `id`, stopping at the first URL
    /// that does not redirect, at a loop, or after 100 hops.
    fn redirect_chain(&self, id: i64) -> Result<(Vec<RedirectHop>, bool)> {
        let sql = format!("SELECT x.url, x.status, {FINAL_STATE}, x.redirect_to FROM urls x WHERE x.id = ?1");
        let mut stmt = self.conn.prepare(&sql)?;
        let mut hops: Vec<RedirectHop> = Vec::new();
        let mut cur = Some(id);
        let mut is_loop = false;
        while let Some(c) = cur {
            if hops.iter().any(|h| h.id == c) {
                is_loop = true;
                break;
            }
            if hops.len() > 100 {
                break;
            }
            let Some((url, status, final_state, next)) = stmt
                .query_row([c], |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, Option<u16>>(1)?,
                        r.get::<_, String>(2)?,
                        r.get::<_, Option<i64>>(3)?,
                    ))
                })
                .optional()?
            else {
                break;
            };
            hops.push(RedirectHop {
                id: c,
                url,
                status,
                final_state,
            });
            cur = next;
        }
        if hops.len() == 1 {
            hops.clear();
        }
        Ok((hops, is_loop))
    }

    /// A page of one of the URL's lists (FR-022, FR-023, FR-025).
    pub fn list(&mut self, id: i64, kind: ListKind, from: u32, count: u32) -> Result<ListPage> {
        let count = count.min(1000);
        let (cond, join) = match kind {
            ListKind::Inlinks => ("l.to_id = ?1", "l.from_id"),
            ListKind::Outlinks => ("l.from_id = ?1", "l.to_id"),
            ListKind::Images => ("l.from_id = ?1 AND l.kind = 'image'", "l.to_id"),
            ListKind::Headings => return self.headings(id, from, count),
        };
        let total: i64 =
            self.conn
                .query_row(&format!("SELECT COUNT(*) FROM links l WHERE {cond}"), [id], |r| {
                    r.get(0)
                })?;
        let text = if !self.features.has(Need::Extraction) {
            "NULL, 0, 0"
        } else if kind == ListKind::Images {
            "l.alt, l.sponsored, l.ugc"
        } else {
            "COALESCE(NULLIF(l.text, ''), l.alt), l.sponsored, l.ugc"
        };
        let position = if kind != ListKind::Images && self.has_derived(Derived::LinkPositions)? {
            "CASE WHEN l.kind = 'anchor' THEN COALESCE((SELECT CASE p.position WHEN 1 THEN 'navigation' \
             WHEN 2 THEN 'header' WHEN 3 THEN 'content' WHEN 4 THEN 'sidebar' WHEN 5 THEN 'footer' END \
             FROM link_positions p WHERE p.from_id = l.from_id AND p.pos = l.pos), 'unknown') END"
        } else {
            "NULL"
        };
        let sql = format!(
            "SELECT x.id, x.url, l.raw, l.kind, l.nofollow, l.followed, x.internal, x.status, {FINAL_STATE}, {text},
                    {position}
             FROM links l JOIN urls x ON x.id = {join}
             WHERE {cond} ORDER BY l.rowid LIMIT ?2 OFFSET ?3"
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt
            .query_map(rusqlite::params![id, count, from], |r| {
                Ok(LinkRow {
                    id: r.get(0)?,
                    url: r.get(1)?,
                    raw: r.get(2)?,
                    kind: r.get(3)?,
                    nofollow: r.get(4)?,
                    followed: r.get(5)?,
                    internal: r.get(6)?,
                    status: r.get(7)?,
                    final_state: r.get(8)?,
                    text: r.get(9)?,
                    sponsored: r.get(10)?,
                    ugc: r.get(11)?,
                    position: r.get(12)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(ListPage {
            total: total as u64,
            from,
            rows,
            headings: vec![],
            unavailable: None,
        })
    }

    fn headings(&self, id: i64, from: u32, count: u32) -> Result<ListPage> {
        if !self.features.has(Need::Extraction) {
            return Ok(ListPage {
                total: 0,
                from,
                rows: vec![],
                headings: vec![],
                unavailable: Some(Need::Extraction.unavailable_reason().to_string()),
            });
        }
        let total: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM page_text WHERE url_id = ?1 AND kind = 'heading'",
            [id],
            |r| r.get(0),
        )?;
        let mut stmt = self.conn.prepare(
            "SELECT level, text FROM page_text WHERE url_id = ?1 AND kind = 'heading'
             ORDER BY pos LIMIT ?2 OFFSET ?3",
        )?;
        let headings = stmt
            .query_map(rusqlite::params![id, count, from], |r| {
                Ok(HeadingRow {
                    level: r.get(0)?,
                    text: r.get(1)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(ListPage {
            total: total as u64,
            from,
            rows: vec![],
            headings,
            unavailable: None,
        })
    }
}
