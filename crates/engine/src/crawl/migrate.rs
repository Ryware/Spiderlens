//! Crawl file format versions as a list of migration steps (spec 006 plan
//! D4, contracts/crawl-file.md).
//!
//! The format-2 schema (`store::SCHEMA`, spec 005) is the base. A new file is
//! the base plus every step; an older file gets the missing steps when it is
//! resumed. `meta.format_version` is the last step applied. `meta.read_compat`
//! is the oldest format whose reader can still read the file: additive steps
//! keep it, breaking steps raise it, so an older SpiderLens can still view a
//! newer file read-only when only columns or tables were added.

use rusqlite::{Connection, OptionalExtension};

/// The schema every step builds on.
pub const BASE: i64 = 2;

pub struct Step {
    pub version: i64,
    /// Only adds tables, indexes or nullable columns: older readers can still
    /// read the file.
    pub additive: bool,
    pub sql: &'static str,
}

/// Every change to the crawl file after the base, in order. Append only.
pub const STEPS: &[Step] = &[Step {
    version: 3,
    additive: true,
    sql: "CREATE TABLE events (
        id INTEGER PRIMARY KEY,
        at INTEGER NOT NULL,
        kind TEXT NOT NULL,
        app_version TEXT NOT NULL,
        os TEXT NOT NULL,
        detail TEXT
    );",
}, Step {
    // External integrations (spec 024, data-model.md). Tables only.
    version: 4,
    additive: true,
    sql: INTEGRATIONS,
}];

const INTEGRATIONS: &str = "
CREATE TABLE integration_runs (
  id INTEGER PRIMARY KEY,
  provider TEXT NOT NULL,
  account_label TEXT,
  property TEXT,
  params TEXT NOT NULL,
  state TEXT NOT NULL,
  created_at INTEGER NOT NULL,
  started_at INTEGER,
  finished_at INTEGER,
  resume_at INTEGER,
  cursor TEXT,
  requested INTEGER NOT NULL DEFAULT 0,
  done INTEGER NOT NULL DEFAULT 0,
  failed INTEGER NOT NULL DEFAULT 0,
  matched INTEGER,
  unmatched INTEGER,
  usage INTEGER,
  usage_cap INTEGER,
  message TEXT,
  app_version TEXT NOT NULL
);
CREATE TABLE integration_queue (
  run_id INTEGER NOT NULL,
  url_id INTEGER NOT NULL,
  pos INTEGER NOT NULL,
  state TEXT NOT NULL,
  PRIMARY KEY (run_id, url_id)
);
CREATE INDEX integration_queue_pending ON integration_queue(run_id, state, pos);
CREATE TABLE integration_status (
  url_id INTEGER NOT NULL,
  provider TEXT NOT NULL,
  run_id INTEGER NOT NULL,
  reason TEXT NOT NULL,
  error_class TEXT,
  attempts INTEGER,
  detail TEXT,
  PRIMARY KEY (url_id, provider)
);
CREATE TABLE gsc_pages (
  url_id INTEGER PRIMARY KEY,
  run_id INTEGER NOT NULL,
  clicks INTEGER NOT NULL,
  impressions INTEGER NOT NULL,
  ctr REAL NOT NULL,
  position REAL NOT NULL
);
CREATE TABLE gsc_queries (
  url_id INTEGER NOT NULL,
  run_id INTEGER NOT NULL,
  query TEXT NOT NULL,
  clicks INTEGER NOT NULL,
  impressions INTEGER NOT NULL,
  ctr REAL NOT NULL,
  position REAL NOT NULL,
  PRIMARY KEY (url_id, query)
);
CREATE TABLE gsc_inspection (
  url_id INTEGER PRIMARY KEY,
  run_id INTEGER NOT NULL,
  verdict TEXT,
  coverage_state TEXT,
  indexing_state TEXT,
  robots_state TEXT,
  page_fetch_state TEXT,
  google_canonical TEXT,
  user_canonical TEXT,
  canonical_mismatch INTEGER,
  last_crawl INTEGER,
  crawled_as TEXT,
  inspected_at INTEGER NOT NULL,
  raw BLOB
);
CREATE TABLE ga4_pages (
  url_id INTEGER PRIMARY KEY,
  run_id INTEGER NOT NULL,
  sessions INTEGER NOT NULL,
  engaged_sessions INTEGER NOT NULL,
  engagement_rate REAL,
  key_events REAL,
  revenue REAL
);
CREATE TABLE cwv_field (
  url_id INTEGER NOT NULL,
  form_factor TEXT NOT NULL,
  run_id INTEGER NOT NULL,
  level TEXT NOT NULL,
  lcp_ms INTEGER, inp_ms INTEGER, cls REAL, fcp_ms INTEGER, ttfb_ms INTEGER,
  lcp_rating TEXT, inp_rating TEXT, cls_rating TEXT, fcp_rating TEXT, ttfb_rating TEXT,
  period_start TEXT, period_end TEXT,
  PRIMARY KEY (url_id, form_factor)
);
CREATE TABLE psi_lab (
  url_id INTEGER NOT NULL,
  strategy TEXT NOT NULL,
  run_id INTEGER NOT NULL,
  performance INTEGER, accessibility INTEGER, best_practices INTEGER, seo INTEGER,
  lcp_ms INTEGER, cls REAL, tbt_ms INTEGER, speed_index_ms INTEGER,
  opportunities TEXT,
  audits BLOB,
  tested_at INTEGER NOT NULL,
  PRIMARY KEY (url_id, strategy)
);
CREATE TABLE link_metrics (
  url_id INTEGER NOT NULL,
  provider TEXT NOT NULL,
  run_id INTEGER NOT NULL,
  metrics TEXT NOT NULL,
  fetched_at INTEGER NOT NULL,
  PRIMARY KEY (url_id, provider)
);
CREATE TABLE orphan_candidates (
  id INTEGER PRIMARY KEY,
  url TEXT NOT NULL,
  source TEXT NOT NULL,
  run_id INTEGER NOT NULL,
  clicks INTEGER, impressions INTEGER, sessions INTEGER,
  UNIQUE (url, source)
);
";

/// The format this build writes.
pub const LATEST: i64 = latest();

const fn latest() -> i64 {
    let mut v = BASE;
    let mut i = 0;
    while i < STEPS.len() {
        assert!(
            STEPS[i].version == v + 1,
            "migration steps are numbered without gaps"
        );
        v = STEPS[i].version;
        i += 1;
    }
    v
}

/// The oldest reader format that can read a file at `version`.
pub fn read_compat_of(version: i64) -> i64 {
    STEPS
        .iter()
        .filter(|s| s.version <= version && !s.additive)
        .map(|s| s.version)
        .max()
        .unwrap_or(BASE)
}

fn meta_i64(conn: &Connection, key: &str) -> rusqlite::Result<Option<i64>> {
    Ok(conn
        .query_row("SELECT value FROM meta WHERE key = ?1", [key], |r| {
            r.get::<_, String>(0)
        })
        .optional()?
        .and_then(|v| v.parse().ok()))
}

/// The file's format version (0 when it has none).
pub fn version(conn: &Connection) -> rusqlite::Result<i64> {
    Ok(meta_i64(conn, "format_version")?.unwrap_or(0))
}

/// The oldest reader that can read the file. Files from before this rule
/// (format 2 and older) are readable by their own format.
pub fn read_compat(conn: &Connection) -> rusqlite::Result<i64> {
    match meta_i64(conn, "read_compat")? {
        Some(v) => Ok(v),
        None => Ok(version(conn)?.min(BASE)),
    }
}

/// Applies every step after the file's version, inside one transaction, and
/// records the new version. The caller has the writer lock and has made any
/// backup copy first. Returns the version the file had.
pub fn upgrade(conn: &Connection) -> rusqlite::Result<i64> {
    let from = version(conn)?;
    if from >= LATEST {
        return Ok(from);
    }
    conn.execute_batch("BEGIN IMMEDIATE")?;
    let applied = (|| {
        for step in STEPS.iter().filter(|s| s.version > from) {
            conn.execute_batch(step.sql)?;
        }
        set_version(conn, LATEST)
    })();
    match applied {
        Ok(()) => conn.execute_batch("COMMIT")?,
        Err(e) => {
            let _ = conn.execute_batch("ROLLBACK");
            return Err(e);
        }
    }
    Ok(from)
}

/// Applies every step to a file just created from the base schema.
pub(crate) fn apply_all(conn: &Connection) -> rusqlite::Result<()> {
    for step in STEPS {
        conn.execute_batch(step.sql)?;
    }
    set_version(conn, LATEST)
}

fn set_version(conn: &Connection, v: i64) -> rusqlite::Result<()> {
    let mut stmt = conn.prepare_cached(
        "INSERT INTO meta(key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = ?2",
    )?;
    stmt.execute(["format_version", &v.to_string()])?;
    stmt.execute(["read_compat", &read_compat_of(v).to_string()])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steps_are_numbered_from_the_base() {
        assert_eq!(STEPS[0].version, BASE + 1);
        assert_eq!(LATEST, STEPS.last().unwrap().version);
    }

    #[test]
    fn additive_steps_keep_read_compat() {
        assert_eq!(read_compat_of(LATEST), BASE);
    }
}
