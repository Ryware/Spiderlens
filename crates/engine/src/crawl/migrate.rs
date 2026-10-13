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
pub const STEPS: &[Step] = &[
    Step {
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
    },
    Step {
        // Spec 016: forms, actions, agent entry points and agent readiness scores.
        version: 4,
        additive: true,
        sql: AGENTS,
    },
];

/// Spec 016 data-model.md. `source` is `raw` or `rendered` (spec 014).
const AGENTS: &str = "
CREATE TABLE page_agent (
    url_id INTEGER NOT NULL,
    source TEXT NOT NULL,
    forms INTEGER NOT NULL,
    search_forms INTEGER NOT NULL,
    fields INTEGER NOT NULL,
    unlabelled_fields INTEGER NOT NULL,
    placeholder_only_fields INTEGER NOT NULL,
    nameless_buttons INTEGER NOT NULL,
    actions_without_link INTEGER NOT NULL,
    challenges INTEGER NOT NULL,
    forms_without_submit INTEGER NOT NULL,
    declared_tools INTEGER NOT NULL,
    tools_missing_description INTEGER NOT NULL,
    md_alternate TEXT,
    md_status INTEGER,
    md_outcome TEXT,
    PRIMARY KEY (url_id, source)
) WITHOUT ROWID;
CREATE TABLE page_forms (
    url_id INTEGER NOT NULL,
    source TEXT NOT NULL,
    pos INTEGER NOT NULL,
    method TEXT NOT NULL,
    action TEXT,
    is_search INTEGER NOT NULL,
    has_submit INTEGER NOT NULL,
    field_count INTEGER NOT NULL,
    challenge TEXT,
    tool_name TEXT,
    tool_description TEXT,
    tool_autosubmit INTEGER,
    PRIMARY KEY (url_id, source, pos)
) WITHOUT ROWID;
CREATE TABLE form_fields (
    url_id INTEGER NOT NULL,
    source TEXT NOT NULL,
    pos INTEGER NOT NULL,
    form_pos INTEGER,
    name TEXT,
    kind TEXT NOT NULL,
    required INTEGER NOT NULL,
    accessible_name TEXT,
    name_source TEXT NOT NULL,
    description TEXT,
    tool_param_description TEXT,
    PRIMARY KEY (url_id, source, pos)
) WITHOUT ROWID;
CREATE TABLE page_actions (
    url_id INTEGER NOT NULL,
    source TEXT NOT NULL,
    pos INTEGER NOT NULL,
    kind TEXT NOT NULL,
    text TEXT NOT NULL,
    PRIMARY KEY (url_id, source, pos)
) WITHOUT ROWID;
CREATE TABLE agent_links (
    url_id INTEGER NOT NULL,
    source TEXT NOT NULL,
    pos INTEGER NOT NULL,
    kind TEXT NOT NULL,
    href TEXT NOT NULL,
    via TEXT NOT NULL,
    PRIMARY KEY (url_id, source, pos)
) WITHOUT ROWID;
CREATE TABLE agent_entry_points (
    id INTEGER PRIMARY KEY,
    host_key TEXT NOT NULL,
    kind TEXT NOT NULL,
    url TEXT NOT NULL,
    found_via TEXT NOT NULL,
    found_on INTEGER,
    status INTEGER,
    outcome TEXT NOT NULL,
    content_type TEXT,
    size INTEGER,
    convention_version TEXT,
    problems TEXT,
    summary TEXT,
    mcp_discovery TEXT
);
CREATE INDEX agent_entry_points_host ON agent_entry_points(host_key, kind);
CREATE TABLE agent_scores (
    url_id INTEGER,
    total REAL,
    access REAL,
    discovery REAL,
    reading REAL,
    acting REAL,
    not_checked INTEGER NOT NULL,
    failed_rules TEXT NOT NULL,
    weights_version INTEGER NOT NULL
);
CREATE UNIQUE INDEX agent_scores_url ON agent_scores(url_id);
CREATE UNIQUE INDEX agent_scores_crawl ON agent_scores((url_id IS NULL)) WHERE url_id IS NULL;
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
