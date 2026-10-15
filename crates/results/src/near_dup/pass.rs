//! The `near_duplicates` analysis pass (plan D4): reads signatures, runs
//! [`analyze`], replaces the three output tables in one transaction.
//!
//! It takes a connection and a schema so it can write into the crawl file
//! (`main`, writer side, at crawl end) or into a viewer's `temp` schema for
//! read-only files, the way spec 009's Analysis runner hands passes their
//! output schema (009 plan D12).

use std::collections::HashMap;
use std::sync::atomic::AtomicBool;
use std::time::Instant;

use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use spiderlens_config::ContentSettings;
use spiderlens_engine::extract::shingle::Signature;

use super::analyze::{self, Item, Params};
use crate::{Error, Result};

pub const PASS_ID: &str = "near_duplicates";
/// Bump when signatures, scoring or output tables change meaning.
pub const PASS_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Schema {
    Main,
    Temp,
}

impl Schema {
    fn name(self) -> &'static str {
        match self {
            Self::Main => "main",
            Self::Temp => "temp",
        }
    }
}

/// The settings that change the pass's output, recorded with it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UsedSettings {
    pub threshold: u8,
    pub report_floor: u8,
    pub min_words: u32,
    pub indexable_only: bool,
    pub content_selector: Option<String>,
    pub exclude_selectors: Vec<String>,
}

impl From<&ContentSettings> for UsedSettings {
    fn from(c: &ContentSettings) -> Self {
        Self {
            threshold: c.threshold,
            report_floor: c.report_floor.min(c.threshold),
            min_words: c.min_words,
            indexable_only: c.indexable_only,
            content_selector: c.content_selector.clone().filter(|s| !s.trim().is_empty()),
            exclude_selectors: c
                .exclude_selectors
                .iter()
                .filter(|s| !s.trim().is_empty())
                .cloned()
                .collect(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PassRecord {
    pub version: u32,
    pub data_version: i64,
    pub ms: u64,
    pub settings: UsedSettings,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PassSummary {
    pub compared: u64,
    pub too_short: u64,
    pub not_indexable: u64,
    pub no_signature: u64,
    pub clusters: u64,
    pub ms: u64,
}

/// Output tables, created in the chosen schema when missing.
const TABLES: &str = "
CREATE TABLE IF NOT EXISTS {s}.near_dup_pages (
    url_id INTEGER PRIMARY KEY,
    status TEXT NOT NULL,
    closest_id INTEGER,
    closest_sim INTEGER,
    near_count INTEGER NOT NULL DEFAULT 0,
    near_count_is_min INTEGER NOT NULL DEFAULT 0,
    cluster INTEGER
);
CREATE INDEX IF NOT EXISTS {s}.near_dup_pages_cluster ON near_dup_pages(cluster) WHERE cluster IS NOT NULL;
CREATE TABLE IF NOT EXISTS {s}.near_dup_pairs (
    url_id INTEGER NOT NULL,
    other_id INTEGER NOT NULL,
    sim INTEGER NOT NULL,
    exact INTEGER NOT NULL,
    PRIMARY KEY (url_id, other_id)
) WITHOUT ROWID;
CREATE TABLE IF NOT EXISTS {s}.near_dup_clusters (
    id INTEGER PRIMARY KEY,
    size INTEGER NOT NULL,
    min_sim INTEGER NOT NULL,
    max_sim INTEGER NOT NULL,
    representative_id INTEGER NOT NULL,
    exact INTEGER NOT NULL
);
";

/// Internal pages fetched successfully as HTML (spec 007 FR-010).
pub(crate) const PAGES: &str = "u.internal = 1 AND u.state = 'crawled' AND u.error_class IS NULL \
     AND u.status BETWEEN 200 AND 299 AND content_kind(u.content_type) = 'html'";

fn has_table(conn: &Connection, schema: &str, table: &str) -> rusqlite::Result<bool> {
    conn.query_row(
        &format!("SELECT EXISTS (SELECT 1 FROM {schema}.sqlite_master WHERE type = 'table' AND name = ?1)"),
        [table],
        |r| r.get(0),
    )
}

/// The record of the last run, if any.
pub(crate) fn last_run(conn: &Connection) -> rusqlite::Result<Option<PassRecord>> {
    let analysis: Option<String> = conn
        .query_row("SELECT value FROM meta WHERE key = 'analysis'", [], |r| r.get(0))
        .optional()?;
    Ok(analysis
        .and_then(|a| serde_json::from_str::<serde_json::Value>(&a).ok())
        .and_then(|v| v.get(PASS_ID).cloned())
        .and_then(|v| serde_json::from_value(v).ok()))
}

/// Why the stored results do not match `settings`, or `None` when current.
pub fn stale_reason(conn: &Connection, settings: &ContentSettings) -> rusqlite::Result<Option<&'static str>> {
    let Some(run) = last_run(conn)? else {
        return Ok(Some("not analysed yet"));
    };
    if run.version != PASS_VERSION {
        return Ok(Some("made by an earlier version of the analysis"));
    }
    if run.settings != UsedSettings::from(settings) {
        return Ok(Some("content settings changed"));
    }
    if run.data_version != crawled_count(conn)? {
        return Ok(Some("more pages were crawled"));
    }
    Ok(None)
}

/// The data version a run is tied to: crawled URLs, which change on resume.
fn crawled_count(conn: &Connection) -> rusqlite::Result<i64> {
    conn.query_row("SELECT COUNT(*) FROM urls WHERE state = 'crawled'", [], |r| {
        r.get(0)
    })
}

enum Status {
    Compared,
    TooShort,
    NotIndexable,
    NoSignature,
}

impl Status {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Compared => "compared",
            Self::TooShort => "too_short",
            Self::NotIndexable => "not_indexable",
            Self::NoSignature => "no_signature",
        }
    }
}

/// Runs the pass. `progress(done, total, label)` reports steps.
pub fn run_pass(
    conn: &Connection,
    schema: Schema,
    settings: &ContentSettings,
    cancel: &AtomicBool,
    mut progress: impl FnMut(usize, usize, &str),
) -> Result<PassSummary> {
    let started = Instant::now();
    // The writer's connection has none of the reader's SQL functions.
    crate::sql::register(conn)?;
    if !has_table(conn, "main", "content_sig")? {
        return Err(Error::Unavailable(
            "This crawl file has no content signatures; near duplicates can be analysed after a new crawl."
                .into(),
        ));
    }
    let used = UsedSettings::from(settings);
    let s = schema.name();
    progress(0, 1, "Reading pages");

    let inlinks: HashMap<i64, i64> = {
        let mut stmt =
            conn.prepare("SELECT to_id, COUNT(*) FROM links WHERE to_id IS NOT NULL GROUP BY to_id")?;
        stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<rusqlite::Result<_>>()?
    };
    let mut statuses: Vec<(i64, Status)> = Vec::new();
    let mut items: Vec<Item> = Vec::new();
    {
        let mut stmt = conn.prepare(&format!(
            "SELECT u.id, u.url, u.noindex, u.canonical, p.main_words, p.main_hash, c.sig
             FROM urls u LEFT JOIN pages p ON p.url_id = u.id LEFT JOIN content_sig c ON c.url_id = u.id
             WHERE {PAGES} ORDER BY u.id"
        ))?;
        let mut rows = stmt.query([])?;
        while let Some(r) = rows.next()? {
            let id: i64 = r.get(0)?;
            let url: String = r.get(1)?;
            let noindex: Option<bool> = r.get(2)?;
            let canonical: Option<String> = r.get(3)?;
            let words: Option<i64> = r.get(4)?;
            let hash: Option<i64> = r.get(5)?;
            let sig: Option<Vec<u8>> = r.get(6)?;
            let indexable = !noindex.unwrap_or(false) && canonical.as_deref().is_none_or(|c| c == url);
            let status = if used.indexable_only && !indexable {
                Status::NotIndexable
            } else if words.is_none() {
                Status::NoSignature
            } else if words.unwrap_or(0) < i64::from(used.min_words) {
                Status::TooShort
            } else {
                match sig.as_deref().and_then(Signature::from_bytes) {
                    Some(sig) => {
                        items.push(Item {
                            id,
                            sig,
                            hash: hash.unwrap_or(0),
                            inlinks: inlinks.get(&id).copied().unwrap_or(0),
                            url_len: url.len(),
                        });
                        Status::Compared
                    }
                    None => Status::NoSignature,
                }
            };
            statuses.push((id, status));
        }
    }

    let params = Params::new(used.threshold, used.report_floor);
    let analysis = analyze::analyze(&items, &params, cancel, |done, total| {
        progress(done, total + 1, "Comparing pages");
    })
    .map_err(|_| Error::Invalid("Analysis cancelled".into()))?;

    // Write: one transaction replaces everything.
    let tx = conn.unchecked_transaction()?;
    tx.execute_batch(&TABLES.replace("{s}", s))?;
    tx.execute_batch(&format!(
        "DELETE FROM {s}.near_dup_pages; DELETE FROM {s}.near_dup_pairs; DELETE FROM {s}.near_dup_clusters;"
    ))?;
    let index_of: HashMap<i64, usize> = items.iter().enumerate().map(|(i, it)| (it.id, i)).collect();
    {
        let mut page_row = tx.prepare(&format!(
            "INSERT INTO {s}.near_dup_pages (url_id, status, closest_id, closest_sim, near_count, near_count_is_min, cluster)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)"
        ))?;
        let mut pair_row = tx.prepare(&format!(
            "INSERT INTO {s}.near_dup_pairs (url_id, other_id, sim, exact) VALUES (?1, ?2, ?3, ?4)"
        ))?;
        for (id, status) in &statuses {
            let result = index_of.get(id).map(|&i| (i, &analysis.pages[i]));
            page_row.execute(params![
                id,
                status.as_str(),
                result.and_then(|(_, r)| r.closest.map(|(o, _)| items[o].id)),
                result.and_then(|(_, r)| r.closest.map(|(_, sim)| sim)),
                result.map_or(0, |(_, r)| r.near_count),
                result.is_some_and(|(_, r)| r.count_is_min),
                result.and_then(|(_, r)| r.cluster.map(|c| c as i64 + 1)),
            ])?;
            if let Some((i, r)) = result {
                for &(o, sim) in &r.top {
                    pair_row.execute(params![id, items[o].id, sim, items[i].hash == items[o].hash])?;
                }
            }
        }
        let mut cluster_row = tx.prepare(&format!(
            "INSERT INTO {s}.near_dup_clusters (id, size, min_sim, max_sim, representative_id, exact)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)"
        ))?;
        for (n, c) in analysis.clusters.iter().enumerate() {
            cluster_row.execute(params![
                n as i64 + 1,
                c.size as i64,
                c.min_sim,
                c.max_sim,
                items[c.representative].id,
                c.exact
            ])?;
        }
    }
    let ms = started.elapsed().as_millis() as u64;
    if schema == Schema::Main {
        let mut all: serde_json::Map<String, serde_json::Value> = tx
            .query_row("SELECT value FROM meta WHERE key = 'analysis'", [], |r| {
                r.get::<_, String>(0)
            })
            .optional()?
            .and_then(|a| serde_json::from_str(&a).ok())
            .unwrap_or_default();
        let record = PassRecord {
            version: PASS_VERSION,
            data_version: crawled_count(&tx)?,
            ms,
            settings: used,
        };
        all.insert(PASS_ID.into(), serde_json::to_value(record).expect("serializes"));
        tx.execute(
            "INSERT INTO meta(key, value) VALUES ('analysis', ?1) ON CONFLICT(key) DO UPDATE SET value = ?1",
            [serde_json::Value::Object(all).to_string()],
        )?;
    }
    tx.commit()?;
    let count = |want: &str| statuses.iter().filter(|(_, st)| st.as_str() == want).count() as u64;
    Ok(PassSummary {
        compared: count("compared"),
        too_short: count("too_short"),
        not_indexable: count("not_indexable"),
        no_signature: count("no_signature"),
        clusters: analysis.clusters.len() as u64,
        ms,
    })
}

/// The content settings stored with the crawl (defaults when absent).
pub fn stored_settings(conn: &Connection) -> rusqlite::Result<ContentSettings> {
    let config: Option<String> = conn
        .query_row("SELECT value FROM meta WHERE key = 'config'", [], |r| r.get(0))
        .optional()?;
    Ok(config
        .and_then(|c| serde_json::from_str::<spiderlens_config::StoredCrawlConfig>(&c).ok())
        .map(|s| s.config.content)
        .unwrap_or_default())
}

/// The settings of the last run, else those stored with the crawl: what a
/// viewer compares and re-runs with when the user names none.
pub fn effective_settings(conn: &Connection) -> rusqlite::Result<ContentSettings> {
    let stored = stored_settings(conn)?;
    Ok(match last_run(conn)? {
        Some(run) => ContentSettings {
            threshold: run.settings.threshold,
            report_floor: run.settings.report_floor,
            min_words: run.settings.min_words,
            indexable_only: run.settings.indexable_only,
            content_selector: run.settings.content_selector,
            exclude_selectors: run.settings.exclude_selectors,
        },
        None => stored,
    })
}

/// Runs the pass on a crawl file. Writes into the file when it can be
/// opened for writing and `write` is set; otherwise into a temporary
/// schema of the returned connection, which keeps the results readable
/// for as long as it is open.
pub fn analyse_file(
    path: &std::path::Path,
    settings: &ContentSettings,
    write: bool,
    cancel: &AtomicBool,
    progress: impl FnMut(usize, usize, &str),
) -> Result<(Connection, PassSummary)> {
    use rusqlite::OpenFlags;
    let writable = if write {
        Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .ok()
    } else {
        None
    };
    let (conn, schema) = match writable {
        Some(c) => (c, Schema::Main),
        None => (
            Connection::open_with_flags(
                path,
                OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
            )?,
            Schema::Temp,
        ),
    };
    conn.busy_timeout(std::time::Duration::from_secs(5))?;
    conn.pragma_update(None, "temp_store", "FILE")?;
    let summary = run_pass(&conn, schema, settings, cancel, progress)?;
    Ok((conn, summary))
}
