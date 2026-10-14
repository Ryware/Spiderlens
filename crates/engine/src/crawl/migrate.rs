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
    // The cookie audit (spec 025 data-model.md): attributes of each cookie a
    // response set, never its value.
    Step {
        version: 6,
        additive: true,
        sql: "CREATE TABLE cookies (
        url_id INTEGER NOT NULL,
        name TEXT NOT NULL,
        domain TEXT NOT NULL,
        host_only INTEGER NOT NULL,
        path TEXT NOT NULL,
        expires INTEGER,
        max_age INTEGER,
        secure INTEGER NOT NULL,
        http_only INTEGER NOT NULL,
        same_site TEXT,
        size INTEGER NOT NULL,
        party TEXT NOT NULL,
        source TEXT NOT NULL,
        extra TEXT
    );
    CREATE INDEX cookies_url ON cookies(url_id);
    CREATE INDEX cookies_key ON cookies(name, domain, path);",
    },
];

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
    fn additive_steps_keep_read_compat() {
        assert_eq!(read_compat_of(LATEST), BASE);
    }
}
