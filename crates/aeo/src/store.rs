//! The AI checks' tables in the crawl file (spec 015 data-model.md, plan D14).
//!
//! The tables arrive with one additive step in `crawl::migrate::STEPS`; an
//! older file gets them when the checks first run on it.

use rusqlite::{Connection, OptionalExtension};

/// Results written by an analysis run; cleared before the next one.
const RESULT_TABLES: &[&str] = &[
    "aeo_hosts",
    "aeo_bot_access",
    "aeo_pages",
    "aeo_findings",
    "aeo_site",
];

/// Brings an older crawl file up to this build's format. Every step since
/// the base only adds tables or nullable columns, so no backup is needed.
pub fn ensure_schema(conn: &Connection) -> rusqlite::Result<()> {
    spiderlens_engine::crawl::migrate::upgrade(conn).map(|_| ())
}

pub fn clear_results(conn: &Connection) -> rusqlite::Result<()> {
    for t in RESULT_TABLES {
        conn.execute(&format!("DELETE FROM {t}"), [])?;
    }
    Ok(())
}

/// The findings indexes, as the migration step creates them. A full run
/// drops them before writing and builds them once at the end, which is
/// faster than keeping them up to date row by row.
const FINDING_INDEXES: &str = "CREATE INDEX IF NOT EXISTS aeo_findings_rule ON aeo_findings(rule_id, url_id);
     CREATE INDEX IF NOT EXISTS aeo_findings_url ON aeo_findings(url_id);";

pub fn drop_finding_indexes(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch("DROP INDEX IF EXISTS aeo_findings_rule; DROP INDEX IF EXISTS aeo_findings_url;")
}

pub fn create_finding_indexes(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(FINDING_INDEXES)
}

pub fn has_table(conn: &Connection, name: &str) -> rusqlite::Result<bool> {
    Ok(conn
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1",
            [name],
            |_| Ok(()),
        )
        .optional()?
        .is_some())
}

pub fn meta(conn: &Connection, key: &str) -> rusqlite::Result<Option<String>> {
    conn.query_row("SELECT value FROM meta WHERE key = ?1", [key], |r| r.get(0))
        .optional()
}

pub fn set_meta(conn: &Connection, key: &str, value: &str) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO meta(key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = ?2",
        [key, value],
    )?;
    Ok(())
}
