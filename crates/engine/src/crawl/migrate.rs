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
    // Spec 015 (data-model.md): per-host site files fetched by the crawl and
    // the AI checks' results, written by `spiderlens_aeo::analyze`.
    Step {
        version: 6,
        additive: true,
        sql: AEO_TABLES,
    },
    // The cookie audit (spec 025 data-model.md): attributes of each cookie a
    // response set, never its value.
    Step {
        version: 7,
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
    // Spec 011: every sitemap file and entry the crawl read
    // (specs/011-sitemaps-orphan-pages/data-model.md).
    Step {
        version: 8,
        additive: true,
        sql: "CREATE TABLE sitemap_files (
        id INTEGER PRIMARY KEY,
        location TEXT UNIQUE NOT NULL,
        source TEXT NOT NULL,
        parent_id INTEGER,
        level INTEGER NOT NULL,
        base_url TEXT,
        kind TEXT,
        http_status INTEGER,
        final_url TEXT,
        redirected INTEGER,
        compressed INTEGER,
        bytes INTEGER,
        entry_count INTEGER NOT NULL DEFAULT 0,
        over_limit INTEGER,
        error_class TEXT,
        error TEXT,
        error_line INTEGER,
        read_at INTEGER
    );
    CREATE TABLE sitemap_entries (
        id INTEGER PRIMARY KEY,
        file_id INTEGER NOT NULL,
        pos INTEGER NOT NULL,
        kind TEXT NOT NULL,
        loc TEXT NOT NULL,
        url_id INTEGER,
        invalid TEXT,
        lastmod_raw TEXT,
        lastmod INTEGER,
        changefreq TEXT,
        priority TEXT
    );
    CREATE INDEX sitemap_entries_url ON sitemap_entries(url_id);
    CREATE INDEX sitemap_entries_file ON sitemap_entries(file_id, pos);
    CREATE TABLE sitemap_refs (
        entry_id INTEGER NOT NULL,
        kind TEXT NOT NULL,
        raw TEXT NOT NULL,
        url TEXT,
        url_id INTEGER,
        lang TEXT
    );
    CREATE INDEX sitemap_refs_entry ON sitemap_refs(entry_id);",
    },
    Step {
        // External integrations (spec 024, data-model.md). Tables only.
        version: 9,
        additive: true,
        sql: INTEGRATIONS,
    },
    // Spec 013: custom extraction and search rules stored with the crawl, one
    // packed results row and one compressed values row per page
    // (data-model.md). `IF NOT EXISTS` because a rerun on an older file
    // creates them without upgrading it (`CrawlStore::ensure_custom_schema`).
    Step {
        version: 10,
        additive: true,
        sql: CUSTOM_SCHEMA,
    },
    // Reading ease counts (spec 009 FR-031); NULL for other languages.
    Step {
        version: 11,
        additive: true,
        sql: "ALTER TABLE pages ADD COLUMN sentence_count INTEGER;
        ALTER TABLE pages ADD COLUMN syllable_count INTEGER;",
    },
];

const AEO_TABLES: &str = r#"
-- Facts a page hook read while the crawl had the page parsed (spec 015
-- reads its content facts here instead of parsing stored HTML again).
CREATE TABLE page_facts (
    url_id INTEGER NOT NULL,
    kind TEXT NOT NULL,
    version INTEGER NOT NULL,
    data BLOB NOT NULL,
    PRIMARY KEY (url_id, kind)
);
CREATE TABLE site_files (
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
CREATE TABLE aeo_hosts (
    host_key TEXT PRIMARY KEY,
    robots TEXT NOT NULL,
    signals TEXT,
    llms_txt TEXT,
    llms_full_txt TEXT,
    tdmrep TEXT
);
CREATE TABLE aeo_bot_access (
    host_key TEXT NOT NULL,
    bot TEXT NOT NULL,
    home_verdict TEXT NOT NULL,
    matched_group TEXT,
    urls_total INTEGER NOT NULL,
    urls_blocked INTEGER NOT NULL,
    rules TEXT,
    PRIMARY KEY (host_key, bot)
);
CREATE TABLE aeo_pages (
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
CREATE TABLE aeo_findings (
    url_id INTEGER,
    rule_id TEXT NOT NULL,
    outcome TEXT NOT NULL,
    points_lost INTEGER NOT NULL,
    evidence TEXT
);
CREATE INDEX aeo_findings_rule ON aeo_findings(rule_id, url_id);
CREATE INDEX aeo_findings_url ON aeo_findings(url_id);
CREATE TABLE aeo_site (
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
CREATE TABLE aeo_probe (
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

/// Step 10's tables.
pub(crate) const CUSTOM_SCHEMA: &str = "CREATE TABLE IF NOT EXISTS custom_rules (
        id INTEGER PRIMARY KEY,
        slot INTEGER NOT NULL UNIQUE,
        kind TEXT NOT NULL,
        name TEXT NOT NULL,
        def_json TEXT NOT NULL,
        pos INTEGER NOT NULL,
        last_run_at TEXT,
        run_pages INTEGER,
        run_state TEXT,
        UNIQUE (kind, name)
    );
    CREATE TABLE IF NOT EXISTS custom_results (
        url_id INTEGER PRIMARY KEY,
        slots BLOB NOT NULL,
        firsts TEXT NOT NULL
    );
    CREATE TABLE IF NOT EXISTS custom_values (
        url_id INTEGER PRIMARY KEY,
        data BLOB NOT NULL
    );";

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
