//! Reads of the pass's tables for the app and the CLI. Unqualified table
//! names find a viewer's `temp` results first, then the file's own.

use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;
use spiderlens_config::ContentSettings;
use spiderlens_engine::extract::{ContentRules, body, main_text};
use ts_rs::TS;

use super::diff::{self, CompareSide, Comparison};
use super::pass::{last_run, stale_reason};
use crate::Result;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum NearDupState {
    /// No results yet (crawl running, or never analysed).
    NotRun,
    /// The analysis is running now (set by the app, which owns the run).
    Running,
    Done,
    /// Results exist but do not match the data or settings.
    Stale,
    /// The file cannot be analysed (no signatures).
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct NearDupStatus {
    pub state: NearDupState,
    pub reason: Option<String>,
    pub threshold: Option<u8>,
    #[ts(type = "number")]
    pub compared: u64,
    #[ts(type = "number")]
    pub too_short: u64,
    #[ts(type = "number")]
    pub not_indexable: u64,
    #[ts(type = "number")]
    pub no_signature: u64,
    #[ts(type = "number")]
    pub near_duplicates: u64,
    #[ts(type = "number")]
    pub clusters: u64,
    /// The file keeps raw HTML, so the comparison view can show text.
    pub html_stored: bool,
}

fn has(conn: &Connection, table: &str) -> rusqlite::Result<bool> {
    conn.query_row(
        "SELECT EXISTS (SELECT 1 FROM sqlite_temp_master WHERE type = 'table' AND name = ?1)
             OR EXISTS (SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1)",
        [table],
        |r| r.get(0),
    )
}

/// Whether results can be read now.
pub(crate) fn has_results(conn: &Connection) -> rusqlite::Result<bool> {
    if !has(conn, "near_dup_pages")? {
        return Ok(false);
    }
    conn.query_row("SELECT EXISTS (SELECT 1 FROM near_dup_pages)", [], |r| r.get(0))
}

/// The state of the analysis for this file and the active settings.
pub fn status(conn: &Connection, settings: &ContentSettings) -> Result<NearDupStatus> {
    let html_stored: bool =
        has(conn, "bodies")? && conn.query_row("SELECT EXISTS (SELECT 1 FROM bodies)", [], |r| r.get(0))?;
    let mut st = NearDupStatus {
        state: NearDupState::NotRun,
        reason: None,
        threshold: None,
        compared: 0,
        too_short: 0,
        not_indexable: 0,
        no_signature: 0,
        near_duplicates: 0,
        clusters: 0,
        html_stored,
    };
    if !has(conn, "content_sig")? {
        st.state = NearDupState::Unavailable;
        st.reason = Some(
            "This crawl was made before near duplicate analysis; crawl the site again to see them.".into(),
        );
        return Ok(st);
    }
    let running: bool = conn
        .query_row("SELECT value FROM meta WHERE key = 'status'", [], |r| {
            r.get::<_, String>(0)
        })
        .optional()?
        .is_some_and(|s| s == "running");
    if !has_results(conn)? {
        st.reason = Some(if running {
            "Analysis runs when the crawl finishes or is paused.".into()
        } else {
            "Not analysed yet.".into()
        });
        return Ok(st);
    }
    let in_temp: bool = conn.query_row(
        "SELECT EXISTS (SELECT 1 FROM sqlite_temp_master WHERE name = 'near_dup_pages')",
        [],
        |r| r.get(0),
    )?;
    st.state = NearDupState::Done;
    if !in_temp && let Some(why) = stale_reason(conn, settings)? {
        st.state = NearDupState::Stale;
        st.reason = Some(why.into());
    }
    st.threshold = last_run(conn)?
        .map(|r| r.settings.threshold)
        .or(Some(settings.threshold));
    let mut stmt =
        conn.prepare("SELECT status, COUNT(*), SUM(near_count > 0) FROM near_dup_pages GROUP BY status")?;
    let mut rows = stmt.query([])?;
    while let Some(r) = rows.next()? {
        let status: String = r.get(0)?;
        let n: u64 = r.get::<_, i64>(1)? as u64;
        match status.as_str() {
            "compared" => {
                st.compared = n;
                st.near_duplicates = r.get::<_, Option<i64>>(2)?.unwrap_or(0) as u64;
            }
            "too_short" => st.too_short = n,
            "not_indexable" => st.not_indexable = n,
            _ => st.no_signature = n,
        }
    }
    st.clusters = conn.query_row("SELECT COUNT(*) FROM near_dup_clusters", [], |r| {
        r.get::<_, i64>(0)
    })? as u64;
    Ok(st)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct ClusterRow {
    pub id: u32,
    pub size: u32,
    pub min_sim: u8,
    pub max_sim: u8,
    #[ts(type = "number")]
    pub representative_id: i64,
    pub representative: String,
    pub exact: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct ClustersPage {
    pub total: u32,
    pub rows: Vec<ClusterRow>,
}

/// Clusters, largest first (cluster numbers are already in that order).
pub fn clusters(conn: &Connection, offset: u32, limit: u32) -> Result<ClustersPage> {
    if !has_results(conn)? {
        return Ok(ClustersPage {
            total: 0,
            rows: vec![],
        });
    }
    let total: u32 = conn.query_row("SELECT COUNT(*) FROM near_dup_clusters", [], |r| r.get(0))?;
    let mut stmt = conn.prepare(
        "SELECT c.id, c.size, c.min_sim, c.max_sim, c.representative_id, u.url, c.exact
         FROM near_dup_clusters c JOIN urls u ON u.id = c.representative_id
         ORDER BY c.id LIMIT ?1 OFFSET ?2",
    )?;
    let rows = stmt
        .query_map(params![limit, offset], |r| {
            Ok(ClusterRow {
                id: r.get(0)?,
                size: r.get(1)?,
                min_sim: r.get(2)?,
                max_sim: r.get(3)?,
                representative_id: r.get(4)?,
                representative: r.get(5)?,
                exact: r.get(6)?,
            })
        })?
        .collect::<rusqlite::Result<_>>()?;
    Ok(ClustersPage { total, rows })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct PairRow {
    #[ts(type = "number")]
    pub other_id: i64,
    pub address: String,
    pub sim: u8,
    pub exact: bool,
    pub cluster: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct PairsPage {
    pub status: Option<String>,
    /// Pages at or above the threshold (may exceed the rows kept).
    pub near_count: u32,
    pub near_count_is_min: bool,
    pub cluster: Option<u32>,
    pub threshold: Option<u8>,
    /// Kept matches at or above the closest-match floor, highest first.
    pub rows: Vec<PairRow>,
}

/// One page's matches for the inspector (FR-018).
pub fn pairs(conn: &Connection, url_id: i64) -> Result<PairsPage> {
    let empty = PairsPage {
        status: None,
        near_count: 0,
        near_count_is_min: false,
        cluster: None,
        threshold: None,
        rows: vec![],
    };
    if !has_results(conn)? {
        return Ok(empty);
    }
    let head: Option<(String, u32, bool, Option<u32>)> = conn
        .query_row(
            "SELECT status, near_count, near_count_is_min, cluster FROM near_dup_pages WHERE url_id = ?1",
            [url_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .optional()?;
    let Some((status, near_count, is_min, cluster)) = head else {
        return Ok(empty);
    };
    let mut stmt = conn.prepare(
        "SELECT q.other_id, u.url, q.sim, q.exact, n.cluster FROM near_dup_pairs q
         JOIN urls u ON u.id = q.other_id LEFT JOIN near_dup_pages n ON n.url_id = q.other_id
         WHERE q.url_id = ?1 ORDER BY q.sim DESC, q.other_id",
    )?;
    let rows = stmt
        .query_map([url_id], |r| {
            Ok(PairRow {
                other_id: r.get(0)?,
                address: r.get(1)?,
                sim: r.get(2)?,
                exact: r.get(3)?,
                cluster: r.get(4)?,
            })
        })?
        .collect::<rusqlite::Result<_>>()?;
    Ok(PairsPage {
        status: Some(status),
        near_count,
        near_count_is_min: is_min,
        cluster,
        threshold: last_run(conn)?.map(|r| r.settings.threshold),
        rows,
    })
}

/// One row of the CLI's `--pages` view.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PageSummaryRow {
    pub address: String,
    pub status: String,
    pub main_words: Option<u32>,
    pub closest_similarity: Option<u8>,
    pub closest_address: Option<String>,
    pub near_duplicates: u32,
    pub near_duplicates_is_minimum: bool,
    pub cluster: Option<u32>,
    pub exact_duplicate: bool,
}

pub fn page_rows(conn: &Connection) -> Result<Vec<PageSummaryRow>> {
    if !has_results(conn)? {
        return Ok(vec![]);
    }
    let mut stmt = conn.prepare(
        "SELECT u.url, n.status, p.main_words, n.closest_sim, c.url, n.near_count, n.near_count_is_min,
                n.cluster, EXISTS (SELECT 1 FROM near_dup_pairs q WHERE q.url_id = n.url_id AND q.exact = 1)
         FROM near_dup_pages n JOIN urls u ON u.id = n.url_id LEFT JOIN pages p ON p.url_id = n.url_id
         LEFT JOIN urls c ON c.id = n.closest_id ORDER BY u.url",
    )?;
    let rows = stmt
        .query_map([], |r| {
            Ok(PageSummaryRow {
                address: r.get(0)?,
                status: r.get(1)?,
                main_words: r.get(2)?,
                closest_similarity: r.get(3)?,
                closest_address: r.get(4)?,
                near_duplicates: r.get(5)?,
                near_duplicates_is_minimum: r.get(6)?,
                cluster: r.get(7)?,
                exact_duplicate: r.get(8)?,
            })
        })?
        .collect::<rusqlite::Result<_>>()?;
    Ok(rows)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct MemberRow {
    #[ts(type = "number")]
    pub url_id: i64,
    pub address: String,
    /// Similarity to the cluster's representative, when that pair was kept.
    pub sim: Option<u8>,
    pub exact: bool,
    pub representative: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct MembersPage {
    pub total: u32,
    #[ts(type = "number")]
    pub representative_id: i64,
    pub rows: Vec<MemberRow>,
}

/// One cluster's pages: the representative first, then the closest to it.
pub fn members(conn: &Connection, cluster: u32, offset: u32, limit: u32) -> Result<MembersPage> {
    let empty = MembersPage {
        total: 0,
        representative_id: 0,
        rows: vec![],
    };
    if !has_results(conn)? {
        return Ok(empty);
    }
    let Some(rep): Option<i64> = conn
        .query_row(
            "SELECT representative_id FROM near_dup_clusters WHERE id = ?1",
            [cluster],
            |r| r.get(0),
        )
        .optional()?
    else {
        return Ok(empty);
    };
    let total: u32 = conn.query_row(
        "SELECT COUNT(*) FROM near_dup_pages WHERE cluster = ?1",
        [cluster],
        |r| r.get(0),
    )?;
    let mut stmt = conn.prepare(
        "SELECT n.url_id, u.url,
                COALESCE((SELECT q.sim FROM near_dup_pairs q WHERE q.url_id = ?2 AND q.other_id = n.url_id),
                         (SELECT q.sim FROM near_dup_pairs q WHERE q.url_id = n.url_id AND q.other_id = ?2)) AS sim,
                COALESCE((SELECT q.exact FROM near_dup_pairs q WHERE q.url_id = ?2 AND q.other_id = n.url_id), 0)
         FROM near_dup_pages n JOIN urls u ON u.id = n.url_id
         WHERE n.cluster = ?1
         ORDER BY n.url_id <> ?2, sim IS NULL, sim DESC, u.url
         LIMIT ?3 OFFSET ?4",
    )?;
    let rows = stmt
        .query_map(params![cluster, rep, limit, offset], |r| {
            let url_id: i64 = r.get(0)?;
            Ok(MemberRow {
                url_id,
                address: r.get(1)?,
                sim: if url_id == rep { None } else { r.get(2)? },
                exact: r.get(3)?,
                representative: url_id == rep,
            })
        })?
        .collect::<rusqlite::Result<_>>()?;
    Ok(MembersPage {
        total,
        representative_id: rep,
        rows,
    })
}

/// One unordered near duplicate pair (at or above the threshold).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PairExportRow {
    pub address: String,
    pub other_address: String,
    pub similarity: u8,
    pub exact: bool,
    pub cluster: Option<u32>,
}

/// Every stored pair at or above the threshold once, optionally one cluster.
pub fn pair_rows(conn: &Connection, cluster: Option<u32>) -> Result<Vec<PairExportRow>> {
    if !has_results(conn)? {
        return Ok(vec![]);
    }
    let threshold = last_run(conn)?.map_or(0, |r| r.settings.threshold);
    let mut stmt = conn.prepare(
        "SELECT a.url, b.url, MAX(x.sim), MAX(x.exact), n.cluster FROM (
             SELECT MIN(url_id, other_id) AS lo, MAX(url_id, other_id) AS hi, sim, exact FROM near_dup_pairs
         ) x JOIN urls a ON a.id = x.lo JOIN urls b ON b.id = x.hi
         JOIN near_dup_pages n ON n.url_id = x.lo
         WHERE x.sim >= ?1 AND (?2 IS NULL OR n.cluster = ?2)
         GROUP BY x.lo, x.hi ORDER BY n.cluster, a.url, b.url",
    )?;
    let rows = stmt
        .query_map(params![threshold, cluster], |r| {
            Ok(PairExportRow {
                address: r.get(0)?,
                other_address: r.get(1)?,
                similarity: r.get(2)?,
                exact: r.get(3)?,
                cluster: r.get(4)?,
            })
        })?
        .collect::<rusqlite::Result<_>>()?;
    Ok(rows)
}

/// Reads and re-extracts one page's stored HTML.
fn side(
    conn: &Connection,
    id: i64,
    rules: Option<&ContentRules>,
) -> Result<Option<(String, spiderlens_engine::extract::MainText)>> {
    let row: Option<(String, Option<Vec<u8>>, Option<i64>)> = conn
        .query_row(
            "SELECT u.url, b.body, b.size FROM urls u LEFT JOIN bodies b ON b.url_id = u.id WHERE u.id = ?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()?;
    let Some((url, Some(packed), Some(size))) = row else {
        return Ok(None);
    };
    let html = body::decompress(&packed, size as usize).map_err(|e| crate::Error::Invalid(e.to_string()))?;
    let parsed = url::Url::parse(&url).map_err(|e| crate::Error::Invalid(e.to_string()))?;
    Ok(Some((url, main_text(&parsed, None, &html, rules))))
}

/// The comparison of two pages (FR-020). `full` shows the whole body with
/// left-out parts marked.
pub fn compare(
    conn: &Connection,
    a: i64,
    b: i64,
    full: bool,
    settings: &ContentSettings,
) -> Result<Comparison> {
    let rules = ContentRules::new(settings.content_selector.as_deref(), &settings.exclude_selectors)
        .map_err(crate::Error::Invalid)?;
    let recorded: Option<(u8, bool)> = if has_results(conn)? {
        conn.query_row(
            "SELECT sim, exact FROM near_dup_pairs WHERE url_id = ?1 AND other_id = ?2",
            [a, b],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?
    } else {
        None
    };
    let (sa, sb) = (side(conn, a, Some(&rules))?, side(conn, b, Some(&rules))?);
    let (Some((ua, ta)), Some((ub, tb))) = (sa, sb) else {
        return Ok(Comparison {
            sim: recorded.map(|r| r.0),
            exact: recorded.is_some_and(|r| r.1),
            unavailable: Some("html_not_stored".into()),
            a: None,
            b: None,
        });
    };
    let (pa, pb) = if full {
        (diff::full_text(&ta), diff::full_text(&tb))
    } else {
        diff::diff_main(&ta.main_paragraphs(), &tb.main_paragraphs())
    };
    Ok(Comparison {
        sim: recorded.map(|r| r.0),
        exact: recorded.is_some_and(|r| r.1),
        unavailable: None,
        a: Some(CompareSide {
            url_id: a,
            address: ua,
            source: diff::source_label(ta.source),
            paragraphs: pa,
        }),
        b: Some(CompareSide {
            url_id: b,
            address: ub,
            source: diff::source_label(tb.source),
            paragraphs: pb,
        }),
    })
}
