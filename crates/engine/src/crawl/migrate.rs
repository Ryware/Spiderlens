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
    // Spec 027 part C: what was read from each PDF.
    Step {
        version: 12,
        additive: true,
        sql: "CREATE TABLE pdfs (
        url_id INTEGER PRIMARY KEY,
        status TEXT NOT NULL,
        status_reason TEXT,
        title TEXT,
        author TEXT,
        subject TEXT,
        keywords TEXT,
        creator TEXT,
        producer TEXT,
        created_at TEXT,
        modified_at TEXT,
        page_count INTEGER,
        pages_read INTEGER,
        word_count INTEGER,
        lang TEXT,
        encrypted INTEGER,
        no_text INTEGER,
        tagged INTEGER,
        link_count INTEGER,
        text_stored INTEGER
    );",
    },
    // Spec 027 part D: spelling, written by the spelling pass after a crawl.
    Step {
        version: 13,
        additive: true,
        sql: "CREATE TABLE spell_pages (
        url_id INTEGER PRIMARY KEY,
        lang TEXT,
        status TEXT NOT NULL,
        words INTEGER,
        errors INTEGER,
        lang_mismatch INTEGER
    );
    CREATE TABLE spell_findings (
        url_id INTEGER NOT NULL,
        kind TEXT NOT NULL,
        word TEXT NOT NULL,
        source TEXT NOT NULL,
        pos INTEGER NOT NULL,
        context TEXT NOT NULL,
        suggestions TEXT,
        block INTEGER,
        site_wide INTEGER
    );
    CREATE INDEX spell_findings_url ON spell_findings(url_id);
    CREATE INDEX spell_findings_word ON spell_findings(word);",
    },
    // Spec 027 part E: AMP pages and their failed checks.
    Step {
        version: 14,
        additive: true,
        sql: "CREATE TABLE amp_pages (
        url_id INTEGER PRIMARY KEY,
        format TEXT NOT NULL,
        failures INTEGER NOT NULL,
        style_bytes INTEGER
    );
    CREATE TABLE amp_checks (
        url_id INTEGER NOT NULL,
        check_id TEXT NOT NULL,
        pos INTEGER,
        detail TEXT NOT NULL
    );
    CREATE INDEX amp_checks_url ON amp_checks(url_id, check_id);",
    },
    Step {
        // Spec 016: forms, actions, agent entry points and agent readiness scores.
        version: 15,
        additive: true,
        sql: AGENTS,
    },
    // Spec 014: JavaScript rendering.
    Step {
        version: 16,
        additive: true,
        sql: RENDERING,
    },
    // Near duplicate content (spec 020, data-model.md): main content
    // columns, one signature per page, and the analysis pass's output
    // tables (filled after the crawl by `spiderlens-results`).
    Step {
        version: 17,
        additive: true,
        sql: "
        ALTER TABLE pages ADD COLUMN main_words INTEGER;
        ALTER TABLE pages ADD COLUMN main_hash INTEGER;
        ALTER TABLE pages ADD COLUMN main_source TEXT;
        CREATE TABLE content_sig (url_id INTEGER PRIMARY KEY, sig BLOB NOT NULL);
        CREATE TABLE near_dup_pages (
            url_id INTEGER PRIMARY KEY,
            status TEXT NOT NULL,
            closest_id INTEGER,
            closest_sim INTEGER,
            near_count INTEGER NOT NULL DEFAULT 0,
            near_count_is_min INTEGER NOT NULL DEFAULT 0,
            cluster INTEGER
        );
        CREATE INDEX near_dup_pages_cluster ON near_dup_pages(cluster) WHERE cluster IS NOT NULL;
        CREATE TABLE near_dup_pairs (
            url_id INTEGER NOT NULL,
            other_id INTEGER NOT NULL,
            sim INTEGER NOT NULL,
            exact INTEGER NOT NULL,
            PRIMARY KEY (url_id, other_id)
        ) WITHOUT ROWID;
        CREATE TABLE near_dup_clusters (
            id INTEGER PRIMARY KEY,
            size INTEGER NOT NULL,
            min_sim INTEGER NOT NULL,
            max_sim INTEGER NOT NULL,
            representative_id INTEGER NOT NULL,
            exact INTEGER NOT NULL
        );
        ",
    },
    // Spec 011 part b (data-model.md): the outputs of the `sitemap_audit`
    // and `orphans` Crawl Analysis passes, and an index for looking up the
    // images sitemaps list. `IF NOT EXISTS` because a pass creates its table
    // itself when it runs on a file from before this step.
    Step {
        version: 18,
        additive: true,
        sql: SITEMAP_PASSES,
    },
    // Spec 026: accessibility and mobile usability results.
    Step {
        version: 19,
        additive: true,
        sql: A11Y_TABLES,
    },
    // Spec 023 stage 2 (data-model.md): the outputs of the `link_positions`
    // and `link_metrics` Crawl Analysis passes. `IF NOT EXISTS` because a
    // pass creates its table itself when it runs on a file from before this
    // step. Link Score is stored as `link_score`: `link_metrics` is already
    // the spec 024 provider table in `main`.
    Step {
        version: 20,
        additive: true,
        sql: LINK_PASSES,
    },
    // Spec 016: what the MCP discovery request of each endpoint answered.
    Step {
        version: 21,
        additive: true,
        sql: "CREATE TABLE agent_mcp_checks (
    host_key TEXT NOT NULL,
    url TEXT NOT NULL,
    status INTEGER,
    answered INTEGER NOT NULL,
    protocol_versions TEXT,
    requires_sign_in INTEGER NOT NULL,
    error TEXT,
    checked_at INTEGER,
    PRIMARY KEY (host_key, url)
);",
    },
];

/// Step 19's tables (spec 026 data-model.md). They stay empty when the
/// accessibility level is Off.
const A11Y_TABLES: &str = "
CREATE TABLE a11y_audits (
    url_id INTEGER PRIMARY KEY,
    static_state TEXT,
    full_state TEXT,
    full_reason TEXT,
    check_ms INTEGER,
    critical INTEGER,
    serious INTEGER,
    moderate INTEGER,
    minor INTEGER,
    needs_review INTEGER,
    checks_failed INTEGER,
    elements INTEGER,
    lowest_level TEXT
);
CREATE TABLE a11y_results (
    id INTEGER PRIMARY KEY,
    url_id INTEGER NOT NULL,
    check_id TEXT NOT NULL,
    kind TEXT NOT NULL,
    source TEXT NOT NULL,
    impact TEXT,
    wcag TEXT,
    nodes_total INTEGER
);
CREATE INDEX a11y_results_url ON a11y_results(url_id);
CREATE INDEX a11y_results_check ON a11y_results(check_id, kind);
CREATE TABLE a11y_nodes (
    result_id INTEGER NOT NULL,
    pos INTEGER NOT NULL,
    selector TEXT,
    excerpt TEXT,
    data TEXT,
    x REAL,
    y REAL,
    w REAL,
    h REAL
);
CREATE INDEX a11y_nodes_result ON a11y_nodes(result_id);
CREATE TABLE mobile_measures (
    url_id INTEGER PRIMARY KEY,
    viewport TEXT,
    viewport_state TEXT,
    plugins INTEGER,
    window TEXT,
    measured TEXT,
    content_px REAL,
    window_px REAL,
    widest TEXT,
    small_text_share REAL,
    small_targets INTEGER,
    examples TEXT
);
";

/// Step 20's tables. The passes in `spiderlens-results`
/// (`analysis::link_positions`, `analysis::link_metrics`) create the same
/// two tables in `temp` (Link Score as `link_metrics`, which the column
/// expressions name) for a file they cannot write.
const LINK_PASSES: &str = "
CREATE TABLE IF NOT EXISTS link_positions (
    from_id INTEGER NOT NULL,
    pos INTEGER NOT NULL,
    position INTEGER NOT NULL,
    rule INTEGER,
    PRIMARY KEY (from_id, pos)
) WITHOUT ROWID;
CREATE TABLE IF NOT EXISTS link_score (
    url_id INTEGER PRIMARY KEY,
    score REAL NOT NULL,
    rank INTEGER NOT NULL,
    unique_inlinks INTEGER NOT NULL,
    followed_inlinks INTEGER NOT NULL,
    all_inlinks INTEGER NOT NULL,
    unique_outlinks INTEGER NOT NULL,
    external_outlinks INTEGER NOT NULL,
    pct_linking REAL NOT NULL
);
";

/// Step 18's tables. The passes in `spiderlens-results`
/// (`sitemaps::audit`, `sitemaps::orphans`) create the same two tables in
/// `temp` for a file they cannot write.
const SITEMAP_PASSES: &str = "
CREATE TABLE IF NOT EXISTS sitemap_problems (
    rule TEXT NOT NULL,
    file_id INTEGER,
    entry_id INTEGER,
    url_id INTEGER,
    detail TEXT
);
CREATE INDEX IF NOT EXISTS sitemap_problems_rule ON sitemap_problems(rule, url_id);
CREATE INDEX IF NOT EXISTS sitemap_problems_url ON sitemap_problems(url_id);
CREATE TABLE IF NOT EXISTS orphans (
    url_id INTEGER PRIMARY KEY,
    class TEXT NOT NULL,
    sources TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS sitemap_refs_url ON sitemap_refs(url_id);
";

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
