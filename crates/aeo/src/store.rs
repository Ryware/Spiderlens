//! The AI checks' tables in the crawl file (spec 015 data-model.md, plan D14).
//!
//! `SCHEMA` becomes one additive step in `crawl::migrate::STEPS` when the
//! storage code (spec 006) is on main; until then [`ensure_schema`] creates
//! the same tables, and the step's SQL is this constant unchanged.

use rusqlite::{Connection, OptionalExtension};

pub const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS site_files (
    host_key TEXT NOT NULL,
    path TEXT NOT NULL,
    status INTEGER,
    final_url TEXT,
    content_type TEXT,
    size INTEGER,
    body BLOB,
    outcome TEXT NOT NULL,
    error TEXT,
    fetched_at INTEGER,
    PRIMARY KEY (host_key, path)
);
CREATE TABLE IF NOT EXISTS aeo_hosts (
    host_key TEXT PRIMARY KEY,
    robots TEXT NOT NULL,
    signals TEXT,
    llms_txt TEXT,
    llms_full_txt TEXT,
    tdmrep TEXT
);
CREATE TABLE IF NOT EXISTS aeo_bot_access (
    host_key TEXT NOT NULL,
    bot TEXT NOT NULL,
    home_verdict TEXT NOT NULL,
    matched_group TEXT,
    urls_total INTEGER NOT NULL,
    urls_blocked INTEGER NOT NULL,
    rules TEXT,
    PRIMARY KEY (host_key, bot)
);
CREATE TABLE IF NOT EXISTS aeo_pages (
    url_id INTEGER PRIMARY KEY,
    page_type TEXT NOT NULL,
    page_type_reason TEXT,
    score INTEGER,
    access INTEGER,
    readable INTEGER,
    answer INTEGER,
    trust INTEGER,
    structured INTEGER,
    checks_used INTEGER NOT NULL,
    checks_total INTEGER NOT NULL,
    provisional INTEGER NOT NULL,
    blocked_bots INTEGER NOT NULL,
    flags INTEGER NOT NULL,
    version_used TEXT NOT NULL,
    js_dependence REAL,
    published TEXT,
    modified TEXT,
    dates_agree INTEGER,
    author TEXT,
    author_url_id INTEGER,
    question_headings INTEGER,
    direct_answers INTEGER,
    figures INTEGER,
    quotations INTEGER,
    citations INTEGER,
    lists INTEGER,
    tables INTEGER,
    longest_paragraph INTEGER,
    snippet_limits TEXT
);
CREATE TABLE IF NOT EXISTS aeo_findings (
    url_id INTEGER,
    rule_id TEXT NOT NULL,
    outcome TEXT NOT NULL,
    points_lost INTEGER NOT NULL,
    evidence TEXT
);
CREATE INDEX IF NOT EXISTS aeo_findings_rule ON aeo_findings(rule_id, url_id);
CREATE INDEX IF NOT EXISTS aeo_findings_url ON aeo_findings(url_id);
CREATE TABLE IF NOT EXISTS aeo_site (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    score INTEGER,
    access INTEGER,
    readable INTEGER,
    answer INTEGER,
    trust INTEGER,
    structured INTEGER,
    site_checks INTEGER,
    distribution TEXT,
    entity TEXT,
    pages_found TEXT,
    coverage TEXT,
    top_fixes TEXT,
    site_outcomes TEXT,
    profile TEXT,
    versions TEXT,
    ran_at INTEGER,
    pages_analyzed INTEGER,
    duration_ms INTEGER
);
CREATE TABLE IF NOT EXISTS aeo_probe (
    host_key TEXT NOT NULL,
    bot TEXT NOT NULL,
    url_id INTEGER NOT NULL,
    status INTEGER,
    size INTEGER,
    content_type TEXT,
    outcome TEXT NOT NULL,
    detail TEXT,
    probed_at INTEGER,
    PRIMARY KEY (host_key, bot, url_id)
);
"#;

/// Results written by an analysis run; cleared before the next one.
const RESULT_TABLES: &[&str] = &[
    "aeo_hosts",
    "aeo_bot_access",
    "aeo_pages",
    "aeo_findings",
    "aeo_site",
];

pub fn ensure_schema(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(SCHEMA)
}

pub fn clear_results(conn: &Connection) -> rusqlite::Result<()> {
    for t in RESULT_TABLES {
        conn.execute(&format!("DELETE FROM {t}"), [])?;
    }
    Ok(())
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
