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
    // Spec 004: attempt counts, the error's sub-reason, the retry timer and
    // the crawl log (data-model.md).
    Step {
        version: 4,
        additive: true,
        sql: "ALTER TABLE urls ADD COLUMN attempts INTEGER;
        ALTER TABLE urls ADD COLUMN round_attempts INTEGER NOT NULL DEFAULT 0;
        ALTER TABLE urls ADD COLUMN error_reason TEXT;
        ALTER TABLE urls ADD COLUMN retry_at INTEGER;
        ALTER TABLE urls ADD COLUMN manual_retry INTEGER NOT NULL DEFAULT 0;
        CREATE TABLE crawl_log (
            id INTEGER PRIMARY KEY,
            at INTEGER NOT NULL,
            kind TEXT NOT NULL,
            host_key TEXT,
            url_id INTEGER,
            message TEXT NOT NULL
        );",
    },
    // Removes cookie values from stored `Set-Cookie` headers (spec 025
    // FR-026), the same rule as `redact::set_cookie`. Data only, so older
    // readers still read the file.
    Step {
        version: 5,
        additive: true,
        sql: "UPDATE headers SET headers = (
        SELECT json_group_array(CASE
            WHEN lower(name) IN ('set-cookie', 'set-cookie2') THEN json_array(
                name,
                CASE WHEN instr(pair, '=') > 0 THEN substr(pair, 1, instr(pair, '=') - 1) || '=' ELSE '' END
                    || '[removed]' || rest)
            ELSE json(pj)
        END)
        FROM (
            SELECT k, pj, name,
                CASE WHEN instr(v, ';') > 0 THEN substr(v, 1, instr(v, ';') - 1) ELSE v END AS pair,
                CASE WHEN instr(v, ';') > 0 THEN substr(v, instr(v, ';')) ELSE '' END AS rest
            FROM (
                SELECT p.key AS k, p.value AS pj, p.value ->> 0 AS name, p.value ->> 1 AS v
                FROM json_each(headers.headers) AS p
            )
            ORDER BY k
        )
    )
    WHERE headers.headers LIKE '%set-cookie%';",
    },
    Step {
        version: 6,
        additive: true,
        sql: RENDERING,
    },
];

/// JavaScript rendering (spec 014 data-model.md): new tables only, empty
/// unless a crawl renders.
const RENDERING: &str = "
CREATE TABLE renders (
    url_id INTEGER PRIMARY KEY,
    state TEXT NOT NULL,
    reason TEXT,
    reason_detail TEXT,
    queue_seq INTEGER,
    attempts INTEGER NOT NULL DEFAULT 0,
    render_ms INTEGER,
    window_w INTEGER,
    window_h INTEGER,
    js_redirect TEXT,
    js_redirect_id INTEGER,
    console_errors INTEGER,
    console_warnings INTEGER,
    resources_failed INTEGER,
    resources_blocked INTEGER,
    bot_challenge INTEGER,
    rendered_at INTEGER
);
CREATE INDEX renders_queue ON renders(queue_seq) WHERE state = 'queued';
CREATE INDEX renders_state ON renders(state, reason);
CREATE TABLE render_inputs (
    url_id INTEGER PRIMARY KEY,
    codec TEXT NOT NULL,
    size INTEGER NOT NULL,
    body BLOB NOT NULL
);
CREATE TABLE rendered_pages (
    url_id INTEGER PRIMARY KEY,
    title_count INTEGER NOT NULL,
    meta_description TEXT,
    meta_description_count INTEGER NOT NULL,
    h1_1 TEXT,
    h1_2 TEXT,
    h1_count INTEGER NOT NULL,
    h2_1 TEXT,
    h2_2 TEXT,
    h2_count INTEGER NOT NULL,
    heading_counts TEXT NOT NULL,
    word_count INTEGER NOT NULL,
    lang TEXT,
    text_hash INTEGER,
    title_hash INTEGER,
    meta_description_hash INTEGER,
    meta_refresh_delay REAL,
    meta_refresh_url TEXT,
    extract_error TEXT,
    no_content INTEGER NOT NULL,
    title TEXT,
    noindex INTEGER,
    nofollow INTEGER,
    canonical TEXT,
    canonical_count INTEGER,
    canonical_conflict INTEGER,
    rendered_html_stored INTEGER NOT NULL
);
CREATE TABLE rendered_text (
    url_id INTEGER NOT NULL,
    kind TEXT NOT NULL,
    level INTEGER,
    pos INTEGER NOT NULL,
    text TEXT NOT NULL,
    in_head INTEGER,
    origin TEXT NOT NULL
);
CREATE INDEX rendered_text_url ON rendered_text(url_id, kind, pos);
CREATE TABLE rendered_directives (
    url_id INTEGER NOT NULL,
    pos INTEGER NOT NULL,
    directive TEXT NOT NULL,
    value TEXT,
    known INTEGER NOT NULL,
    source TEXT NOT NULL,
    crawler TEXT NOT NULL,
    in_head INTEGER,
    origin TEXT NOT NULL
);
CREATE INDEX rendered_directives_url ON rendered_directives(url_id);
CREATE TABLE rendered_structured_data (
    url_id INTEGER NOT NULL,
    pos INTEGER NOT NULL,
    kind TEXT NOT NULL,
    raw TEXT NOT NULL,
    parseable INTEGER NOT NULL,
    origin TEXT NOT NULL
);
CREATE INDEX rendered_structured_data_url ON rendered_structured_data(url_id);
CREATE TABLE rendered_links (
    from_id INTEGER NOT NULL,
    to_id INTEGER,
    raw TEXT NOT NULL,
    kind TEXT NOT NULL,
    nofollow INTEGER NOT NULL,
    page_nofollow INTEGER NOT NULL,
    followed INTEGER NOT NULL,
    pos INTEGER,
    target TEXT,
    skip_reason TEXT,
    text TEXT,
    alt TEXT,
    rel TEXT,
    sponsored INTEGER NOT NULL DEFAULT 0,
    ugc INTEGER NOT NULL DEFAULT 0,
    source TEXT NOT NULL DEFAULT 'html',
    in_head INTEGER,
    noscript INTEGER NOT NULL DEFAULT 0,
    lang TEXT,
    width INTEGER,
    height INTEGER,
    srcset INTEGER NOT NULL DEFAULT 0,
    in_link INTEGER NOT NULL DEFAULT 0,
    rpos INTEGER
);
CREATE INDEX rendered_links_from ON rendered_links(from_id);
CREATE INDEX rendered_links_to ON rendered_links(to_id);
CREATE TABLE raw_only_links (
    from_id INTEGER NOT NULL,
    pos INTEGER NOT NULL,
    PRIMARY KEY (from_id, pos)
) WITHOUT ROWID;
CREATE TABLE render_comparisons (
    url_id INTEGER PRIMARY KEY,
    words_raw INTEGER,
    words_rendered INTEGER,
    words_change INTEGER,
    links_raw INTEGER,
    links_rendered INTEGER,
    links_rendered_only INTEGER,
    links_raw_only INTEGER,
    title_diff TEXT,
    description_diff TEXT,
    h1_diff TEXT,
    canonical_diff TEXT,
    noindex_diff TEXT,
    nofollow_diff TEXT,
    hreflang_diff TEXT,
    structured_rendered_only INTEGER,
    dep_text REAL,
    dep_headings REAL,
    dep_internal_links REAL,
    dep_structured REAL,
    dep_images REAL,
    raw_has_title INTEGER,
    raw_has_description INTEGER,
    raw_has_canonical INTEGER,
    raw_has_directives INTEGER,
    dependence REAL
);
CREATE TABLE render_resources (
    url_id INTEGER NOT NULL,
    pos INTEGER NOT NULL,
    url TEXT NOT NULL,
    type TEXT NOT NULL,
    host TEXT NOT NULL,
    third_party INTEGER NOT NULL,
    outcome TEXT NOT NULL,
    status INTEGER,
    error TEXT,
    robots_rule TEXT,
    bytes INTEGER
);
CREATE INDEX render_resources_url ON render_resources(url_id);
CREATE TABLE console_entries (
    url_id INTEGER NOT NULL,
    pos INTEGER NOT NULL,
    level TEXT NOT NULL,
    message TEXT NOT NULL,
    source_url TEXT,
    line INTEGER,
    col INTEGER
);
CREATE INDEX console_entries_url ON console_entries(url_id);
CREATE TABLE rendered_bodies (
    url_id INTEGER PRIMARY KEY,
    codec TEXT NOT NULL,
    size INTEGER NOT NULL,
    body BLOB NOT NULL
);
CREATE TABLE screenshots (
    url_id INTEGER PRIMARY KEY,
    format TEXT NOT NULL,
    width INTEGER NOT NULL,
    height INTEGER NOT NULL,
    image BLOB NOT NULL
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

    /// Step 5's SQL and `redact::set_cookie` give the same result.
    #[test]
    fn step_5_removes_cookie_values_like_the_writer() {
        let cases = [
            "sid=abc123; Path=/; Secure; HttpOnly; SameSite=Lax",
            "sid=abc",
            "sid=",
            "token=a=b=c; Path=/",
            "abc123; Path=/",
            "abc123",
            "üñí=çødé; Path=/ä",
        ];
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("CREATE TABLE headers (url_id INTEGER PRIMARY KEY, headers TEXT NOT NULL);")
            .unwrap();
        let mut pairs = vec![("Content-Type".to_string(), "text/html; charset=utf-8".to_string())];
        for c in cases {
            pairs.push(("Set-Cookie".to_string(), c.to_string()));
        }
        pairs.push(("set-cookie2".to_string(), "x=1; Version=1".to_string()));
        pairs.push((
            "X-Note".to_string(),
            "Set-Cookie: not a cookie header".to_string(),
        ));
        let raw = serde_json::to_string(&pairs).unwrap();
        conn.execute("INSERT INTO headers VALUES (1, ?1)", [&raw])
            .unwrap();
        conn.execute("INSERT INTO headers VALUES (2, ?1)", [r#"[["Server","x"]]"#])
            .unwrap();
        conn.execute_batch(STEPS.iter().find(|s| s.version == 5).unwrap().sql)
            .unwrap();

        let read = |id: i64| -> String {
            conn.query_row("SELECT headers FROM headers WHERE url_id = ?1", [id], |r| {
                r.get(0)
            })
            .unwrap()
        };
        let got: Vec<(String, String)> = serde_json::from_str(&read(1)).unwrap();
        let want: Vec<(String, String)> = pairs
            .iter()
            .map(|(n, v)| {
                let v = if super::super::redact::is_cookie_header(n) {
                    super::super::redact::set_cookie(v)
                } else {
                    v.clone()
                };
                (n.clone(), v)
            })
            .collect();
        assert_eq!(got, want);
        assert_eq!(read(2), r#"[["Server","x"]]"#);
    }

    #[test]
    fn rendering_tables_apply_to_a_base_file() {
        let conn = Connection::open_in_memory().unwrap();
        // The format-2 base: earlier steps alter its tables (004 adds
        // columns to `urls`).
        conn.execute_batch(crate::crawl::store::SCHEMA).unwrap();
        apply_all(&conn).unwrap();
        for table in ["renders", "rendered_pages", "render_resources", "screenshots"] {
            let n: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
                    [table],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(n, 1, "{table}");
        }
        assert_eq!(version(&conn).unwrap(), LATEST);
    }

    #[test]
    fn additive_steps_keep_read_compat() {
        assert_eq!(read_compat_of(LATEST), BASE);
    }
}
