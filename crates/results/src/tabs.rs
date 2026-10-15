//! The result tabs, their columns and filters, defined once (plan D2).
//!
//! A tab is data: which URLs it lists (a SQL condition on `urls u`), its
//! columns (SQL expressions) and its filters (SQL conditions). The desktop
//! app and `spiderlens export` both read these definitions, so a view and
//! its export always list the same rows (FR-031).
//!
//! Every column and filter names the data it needs (plan D3). Data the crawl
//! file does not have yet is reported as unavailable instead of silently
//! empty. Page content comes from spec 005's tables (`pages`, `page_text`,
//! `directives`, extended `links`), read through primary-key lookups so a
//! column costs one index probe per row on screen.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Data a column or filter needs from the crawl file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum Need {
    /// Stored by every crawl file.
    Base,
    /// Page content: meta description, headings, canonicals, directives,
    /// images, word count, anchor text (spec 005).
    Extraction,
    /// Request attempts (spec 004).
    Attempts,
    /// Response time and size.
    Timing,
    /// The received HTML (spec 005 FR-030).
    RawHtml,
    /// Every sitemap file and entry the crawl read (spec 011).
    SitemapEntries,
    /// The cookie audit (spec 025).
    Cookies,
    /// AI search check results (spec 015).
    AiChecks,
}

impl Need {
    pub fn unavailable_reason(self) -> &'static str {
        match self {
            Self::Base => "",
            Self::Extraction => {
                "Page content (descriptions, headings, canonicals, directives, images) was not collected in this crawl file; it was made before SpiderLens read page content."
            }
            Self::Attempts => "Request attempts are not recorded in this crawl yet.",
            Self::Timing => {
                "Response time and size were not collected in this crawl file; it was made before SpiderLens recorded them."
            }
            Self::RawHtml => "This crawl does not store page HTML.",
            Self::SitemapEntries => {
                "Sitemap entries were not recorded in this crawl file; it was made before SpiderLens kept them."
            }
            Self::Cookies => "This crawl was made before SpiderLens recorded the cookies each page sets.",
            Self::AiChecks => "AI checks have not run on this crawl. Run them from the AI search tab.",
        }
    }

    /// The id used in the crawl file's `data_features` meta list.
    pub fn feature_id(self) -> &'static str {
        match self {
            Self::Base => "base",
            Self::Extraction => "extraction",
            Self::Attempts => "attempts",
            Self::Timing => "timing",
            Self::RawHtml => "raw_html",
            Self::SitemapEntries => "sitemap_entries",
            Self::Cookies => "cookies",
            Self::AiChecks => "ai_checks",
        }
    }
}

/// How a column's values are compared, aligned and formatted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ValueKind {
    Text,
    Url,
    Int,
    Bytes,
    Ms,
    Status,
    State,
    Bool,
}

impl ValueKind {
    pub fn is_text(self) -> bool {
        matches!(self, Self::Text | Self::Url | Self::State)
    }
}

/// A temporary table some columns and filters read, built once per data
/// version (plan D6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Derived {
    /// `temp.titles(id, t, len, k, n, same_h1, count)`: one row per Page
    /// titles row (an HTML page fetched successfully) with its normalised
    /// title `t`, that title's length, the natural sort key of the title as
    /// written `k`, how many of those pages use `t` (NULL when empty),
    /// whether `t` equals the first H1 ignoring case, and how many titles
    /// the page has (NULL without page content); and `temp.title_occ(v, n)`:
    /// each non-empty `t` and how many of those pages use it. Built in one scan, so the tab's sorts, filters
    /// and counts read stored values instead of calling `norm_ws`, `natkey`
    /// and `content_kind` and probing `pages` per row on every query (T039).
    Titles,
    /// `temp.redirects(id, hops, final_id, is_loop)`: each redirect chain.
    Redirects,
    /// `temp.occ_md(v, n)`: each meta description and how many pages use it.
    OccMeta,
    /// `temp.occ_h1(v, n)`: each first H1 and how many pages use it.
    OccH1,
    /// `temp.occ_h2(v, n)`: each first H2 and how many pages use it.
    OccH2,
}

impl Derived {
    pub(crate) fn table(self) -> &'static str {
        match self {
            Self::Titles => "titles",
            Self::Redirects => "redirects",
            Self::OccMeta => "occ_md",
            Self::OccH1 => "occ_h1",
            Self::OccH2 => "occ_h2",
        }
    }

    /// The `urls u` condition that picks exactly the rows this table holds
    /// when it is a tab's row source.
    pub(crate) fn rows_eligible(self) -> &'static str {
        match self {
            Self::Titles => HTML_OK,
            Self::Redirects | Self::OccMeta | Self::OccH1 | Self::OccH2 => "1",
        }
    }
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct ColumnDef {
    pub id: &'static str,
    pub label: &'static str,
    #[serde(skip)]
    #[ts(skip)]
    pub expr: &'static str,
    pub kind: ValueKind,
    pub width: u16,
    pub default_visible: bool,
    pub need: Need,
    #[serde(skip)]
    #[ts(skip)]
    pub derived: Option<Derived>,
    /// A stored `natkey(expr)` to order by instead of computing it per row.
    #[serde(skip)]
    #[ts(skip)]
    pub key: Option<&'static str>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct FilterDef {
    pub id: &'static str,
    pub label: &'static str,
    #[serde(skip)]
    #[ts(skip)]
    pub cond: &'static str,
    pub need: Need,
    #[serde(skip)]
    #[ts(skip)]
    pub derived: Option<Derived>,
    /// Shown only while it matches something (or is the active filter), so
    /// a long list of rarely used filters does not crowd the strip.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub hide_when_empty: Option<bool>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct TabDef {
    pub id: &'static str,
    pub title: &'static str,
    #[serde(skip)]
    #[ts(skip)]
    pub eligible: &'static str,
    pub columns: Vec<ColumnDef>,
    pub filters: Vec<FilterDef>,
    /// Searched besides Address (FR-016).
    pub search_column: Option<&'static str>,
    /// Offers the internal / external switch (Response codes).
    pub scoped: bool,
    /// The tab's rows are the rows of this derived table, `x`, joined to
    /// `urls u` (its `eligible` is then already applied).
    #[serde(skip)]
    #[ts(skip)]
    pub rows: Option<Derived>,
}

impl TabDef {
    /// The FROM clause that lists the tab's candidate rows as `u` (and `x`).
    pub(crate) fn source_sql(&self) -> String {
        match self.rows {
            // CROSS JOIN keeps the derived table outermost: it is the
            // smaller one and already in id order.
            Some(d) => format!("temp.{} x CROSS JOIN urls u ON u.id = x.id", d.table()),
            None => "urls u NOT INDEXED".into(),
        }
    }

    /// The join that adds `x` to rows already picked by id as `t`.
    pub(crate) fn join_sql(&self) -> String {
        match self.rows {
            Some(d) => format!(" LEFT JOIN temp.{} x ON x.id = t.id", d.table()),
            None => String::new(),
        }
    }

    /// The `urls u` condition for the tab's rows without its row source,
    /// for one-pass counts over `urls` (T014).
    pub(crate) fn eligible_on_urls(&self) -> &'static str {
        self.rows.map_or(self.eligible, Derived::rows_eligible)
    }

    pub fn column(&self, id: &str) -> Option<&ColumnDef> {
        self.columns.iter().find(|c| c.id == id)
    }

    pub fn filter(&self, id: &str) -> Option<&FilterDef> {
        self.filters.iter().find(|f| f.id == id)
    }
}

/// A failure a later attempt may fix (spec 004 FR-012). Mirrors the engine's
/// `TRANSIENT_FAILURE`, which picks the rows "Retry failed URLs" re-queues.
const TRANSIENT: &str = "u.fetched_at IS NOT NULL AND (\
    u.error_class IN ('dns_temporary', 'connection', 'timeout', 'host_unavailable') \
    OR (u.error_class = 'content_problem' AND u.error_reason = 'truncated') \
    OR (u.error_class IS NULL AND (u.status = 429 OR (u.status BETWEEN 500 AND 599 AND u.status NOT IN (501, 505)))))";

/// Every other error: retrying cannot help.
const PERMANENT: &str = "u.fetched_at IS NOT NULL AND (\
    (u.error_class IS NOT NULL AND u.error_class NOT IN ('dns_temporary', 'connection', 'timeout', 'host_unavailable') \
     AND NOT (u.error_class = 'content_problem' AND u.error_reason IS 'truncated')) \
    OR (u.error_class IS NULL AND (u.status BETWEEN 400 AND 428 OR u.status BETWEEN 430 AND 499 \
        OR u.status IN (501, 505) OR u.status < 100 OR u.status > 599)))";

/// Internal pages that were fetched successfully as HTML: the rows of the
/// content tabs (FR-010).
const HTML_OK: &str = "u.internal = 1 AND u.state = 'crawled' AND u.error_class IS NULL \
     AND u.status BETWEEN 200 AND 299 AND content_kind(u.content_type) = 'html'";

const INTERNAL: &str = "u.internal = 1 AND u.state = 'crawled'";
const EXTERNAL: &str = "u.internal = 0 AND u.state IN ('crawled', 'not_crawled')";
/// URLs with a response, plus URLs blocked by robots.txt or by the crawl's
/// rules (Assumptions: blocked URLs are listed here, not counted as errors).
const RESPONSES: &str = "u.state = 'crawled' \
     OR (u.check_only = 1 AND u.state = 'not_crawled' AND (u.fetched_at IS NOT NULL OR u.reason_detail IS NOT NULL)) \
     OR u.reason IN ('blocked_by_robots', 'robots_unavailable', 'excluded', 'not_included', 'outside_start_folder')";

const NORM_TITLE: &str = "norm_ws(u.title)";

/// Page content, one primary-key lookup each (spec 005 data model).
const MD: &str = "(SELECT p.meta_description FROM pages p WHERE p.url_id = u.id)";
const H1_1: &str = "(SELECT p.h1_1 FROM pages p WHERE p.url_id = u.id)";
/// The alt text of the first place an image is used. NULL when the
/// attribute is missing, `''` when it is empty.
const IMG_ALT: &str =
    "(SELECT l.alt FROM links l WHERE l.to_id = u.id AND l.kind = 'image' ORDER BY l.rowid LIMIT 1)";

const fn col(
    id: &'static str,
    label: &'static str,
    expr: &'static str,
    kind: ValueKind,
    width: u16,
) -> ColumnDef {
    ColumnDef {
        id,
        label,
        expr,
        kind,
        width,
        default_visible: true,
        need: Need::Base,
        derived: None,
        key: None,
    }
}

impl ColumnDef {
    const fn hidden(mut self) -> Self {
        self.default_visible = false;
        self
    }
    const fn needs(mut self, need: Need) -> Self {
        self.need = need;
        self
    }
    const fn reads(mut self, d: Derived) -> Self {
        self.derived = Some(d);
        self
    }
    const fn sorted_by(mut self, key: &'static str) -> Self {
        self.key = Some(key);
        self
    }
}

const fn filter(id: &'static str, label: &'static str, cond: &'static str) -> FilterDef {
    FilterDef {
        id,
        label,
        cond,
        need: Need::Base,
        derived: None,
        hide_when_empty: None,
    }
}

impl FilterDef {
    const fn when_present(mut self) -> Self {
        self.hide_when_empty = Some(true);
        self
    }
    const fn needs(mut self, need: Need) -> Self {
        self.need = need;
        self
    }
    const fn reads(mut self, d: Derived) -> Self {
        self.derived = Some(d);
        self
    }
}

/// Columns many tabs share.
mod c {
    use super::*;

    pub const ADDRESS: ColumnDef = col("address", "Address", "u.url", ValueKind::Url, 420);
    pub const CONTENT_TYPE: ColumnDef = col(
        "content_type",
        "Content type",
        "u.content_type",
        ValueKind::Text,
        170,
    );
    pub const STATUS_CODE: ColumnDef = col("status_code", "Status code", "u.status", ValueKind::Status, 96);
    pub const STATUS: ColumnDef = col("status", "Status", "status_text(u.status)", ValueKind::Text, 140);
    pub const FINAL_STATE: ColumnDef = col(
        "final_state",
        "Final state",
        "final_state(u.state, u.reason, u.reason_detail, u.check_only, u.fetched_at, u.error_class, u.status)",
        ValueKind::State,
        150,
    );
    pub const INDEXABILITY: ColumnDef = col(
        "indexability",
        "Indexability",
        "indexability(u.state, u.error_class, u.status, u.content_type, u.noindex, u.canonical, u.url)",
        ValueKind::Text,
        200,
    )
    .needs(Need::Extraction);
    pub const TITLE: ColumnDef = col("title", "Title", "u.title", ValueKind::Text, 320);
    pub const META_DESCRIPTION: ColumnDef =
        col("meta_description", "Meta description", MD, ValueKind::Text, 320).needs(Need::Extraction);
    pub const H1: ColumnDef = col("h1", "H1", H1_1, ValueKind::Text, 260).needs(Need::Extraction);
    pub const WORD_COUNT: ColumnDef = col(
        "word_count",
        "Word count",
        "(SELECT p.word_count FROM pages p WHERE p.url_id = u.id)",
        ValueKind::Int,
        100,
    )
    .needs(Need::Extraction);
    pub const SIZE: ColumnDef = col("size", "Size", "u.size_bytes", ValueKind::Bytes, 90).needs(Need::Timing);
    pub const RESPONSE_TIME: ColumnDef = col(
        "response_time",
        "Response time",
        "u.response_ms",
        ValueKind::Ms,
        120,
    )
    .needs(Need::Timing);
    pub const DEPTH: ColumnDef = col("depth", "Depth", "u.depth", ValueKind::Int, 70);
    pub const INLINKS: ColumnDef = col(
        "inlinks",
        "Inlinks",
        "(SELECT COUNT(*) FROM links l WHERE l.to_id = u.id)",
        ValueKind::Int,
        80,
    );
    pub const OUTLINKS: ColumnDef = col(
        "outlinks",
        "Outlinks",
        "(SELECT COUNT(*) FROM links l WHERE l.from_id = u.id)",
        ValueKind::Int,
        80,
    );
    pub const REDIRECT_TARGET: ColumnDef = col(
        "redirect_target",
        "Redirect target",
        "(SELECT r.url FROM urls r WHERE r.id = u.redirect_to)",
        ValueKind::Url,
        320,
    );
    pub const REDIRECT_HOPS: ColumnDef = col(
        "redirect_hops",
        "Redirect hops",
        "(SELECT r.hops FROM temp.redirects r WHERE r.id = u.id)",
        ValueKind::Int,
        110,
    )
    .reads(Derived::Redirects);
    pub const ERROR_CLASS: ColumnDef = col(
        "error_class",
        "Error class",
        "error_label(u.error_class)",
        ValueKind::Text,
        170,
    );
    pub const ATTEMPTS: ColumnDef =
        col("attempts", "Attempts", "u.attempts", ValueKind::Int, 90).needs(Need::Attempts);
    pub const FOUND_ON: ColumnDef = col(
        "found_on",
        "Found on",
        "(SELECT s.url FROM urls s WHERE s.id = u.source_id)",
        ValueKind::Url,
        320,
    )
    .hidden();
    pub const SOURCE: ColumnDef = col("source", "Discovered via", "u.source", ValueKind::Text, 120).hidden();
    pub const IN_SITEMAP: ColumnDef =
        col("in_sitemap", "In sitemap", "u.in_sitemap", ValueKind::Bool, 100).hidden();
}

/// Filters by content type (Internal and External tabs).
const TYPE_FILTERS: [FilterDef; 7] = [
    filter("all", "All", "1"),
    filter("html", "HTML", "content_kind(u.content_type) = 'html'"),
    filter(
        "javascript",
        "JavaScript",
        "content_kind(u.content_type) = 'javascript'",
    ),
    filter("css", "CSS", "content_kind(u.content_type) = 'css'"),
    filter("images", "Images", "content_kind(u.content_type) = 'image'"),
    filter("pdf", "PDF", "content_kind(u.content_type) = 'pdf'"),
    filter("other", "Other", "content_kind(u.content_type) = 'other'"),
];

/// A content tab (FR-010): HTML pages fetched successfully, with All first.
fn content_tab(
    id: &'static str,
    title: &'static str,
    columns: Vec<ColumnDef>,
    filters: Vec<FilterDef>,
    search_column: &'static str,
) -> TabDef {
    let mut fs = vec![filter("all", "All", "1")];
    fs.extend(filters.into_iter().map(|f| f.needs(Need::Extraction)));
    TabDef {
        id,
        title,
        eligible: HTML_OK,
        columns,
        filters: fs,
        search_column: Some(search_column),
        scoped: false,
        rows: None,
    }
}

fn ext(id: &'static str, label: &'static str, expr: &'static str, kind: ValueKind, width: u16) -> ColumnDef {
    col(id, label, expr, kind, width).needs(Need::Extraction)
}

/// A robots directive that applies to this crawl: for all crawlers, or for
/// the crawl's robots token (FR-013).
macro_rules! directive {
    ($names:literal) => {
        concat!(
            "EXISTS (SELECT 1 FROM directives d WHERE d.url_id = u.id \
             AND (d.crawler = 'all' OR d.crawler = robots_token()) AND d.directive IN (",
            $names,
            "))"
        )
    };
}

/// The directives from one source, as written: `noindex, max-snippet:50`,
/// with the crawler named when it is not all of them.
macro_rules! directives_from {
    ($source:literal) => {
        concat!(
            "(SELECT group_concat(v, ', ') FROM (SELECT CASE WHEN d.crawler = 'all' THEN '' ELSE d.crawler || ': ' END \
             || d.directive || COALESCE(':' || d.value, '') AS v FROM directives d \
             WHERE d.url_id = u.id AND d.source = '",
            $source,
            "' ORDER BY d.pos))"
        )
    };
}

/// The first canonical declared in one place, as an absolute URL.
macro_rules! canonical_from {
    ($source:literal) => {
        concat!(
            "(SELECT COALESCE(t.url, l.target) FROM links l LEFT JOIN urls t ON t.id = l.to_id \
             WHERE l.from_id = u.id AND l.kind = 'canonical' AND l.source = '",
            $source,
            "' ORDER BY l.pos LIMIT 1)"
        )
    };
}

/// Filters on an image's uses: any page that shows it this way.
macro_rules! image_use {
    ($cond:literal) => {
        concat!(
            "EXISTS (SELECT 1 FROM links l WHERE l.to_id = u.id AND l.kind = 'image' AND ",
            $cond,
            ")"
        )
    };
}

/// Columns and filters of the H1 and H2 tabs, which differ only in level.
macro_rules! heading_tab {
    ($id:literal, $label:literal, $occ:expr) => {
        content_tab(
            $id,
            $label,
            vec![
                c::ADDRESS,
                ext(
                    $id,
                    $label,
                    concat!("(SELECT p.", $id, "_1 FROM pages p WHERE p.url_id = u.id)"),
                    ValueKind::Text,
                    300,
                ),
                ext(
                    concat!($id, "_length"),
                    concat!($label, " length"),
                    concat!(
                        "length((SELECT p.",
                        $id,
                        "_1 FROM pages p WHERE p.url_id = u.id))"
                    ),
                    ValueKind::Int,
                    90,
                ),
                ext(
                    concat!($id, "_2"),
                    concat!($label, " (second)"),
                    concat!("(SELECT p.", $id, "_2 FROM pages p WHERE p.url_id = u.id)"),
                    ValueKind::Text,
                    260,
                ),
                ext(
                    "occurrences",
                    "Occurrences",
                    concat!(
                        "(SELECT o.n FROM temp.occ_",
                        $id,
                        " o WHERE o.v = (SELECT p.",
                        $id,
                        "_1 FROM pages p WHERE p.url_id = u.id))"
                    ),
                    ValueKind::Int,
                    110,
                )
                .reads($occ),
            ],
            vec![
                filter(
                    "missing",
                    "Missing",
                    concat!(
                        "COALESCE((SELECT p.",
                        $id,
                        "_1 FROM pages p WHERE p.url_id = u.id), '') = ''"
                    ),
                ),
                filter(
                    "duplicate",
                    "Duplicate",
                    concat!(
                        "(SELECT p.",
                        $id,
                        "_1 FROM pages p WHERE p.url_id = u.id) IN (SELECT o.v FROM temp.occ_",
                        $id,
                        " o WHERE o.n > 1)"
                    ),
                )
                .reads($occ),
                filter(
                    "over_70",
                    "Over 70 characters",
                    concat!(
                        "length((SELECT p.",
                        $id,
                        "_1 FROM pages p WHERE p.url_id = u.id)) > 70"
                    ),
                ),
                filter(
                    "multiple",
                    "Multiple",
                    concat!(
                        "(SELECT p.",
                        $id,
                        "_count FROM pages p WHERE p.url_id = u.id) > 1"
                    ),
                ),
            ],
            $id,
        )
    };
}

/// An `aeo_pages` value of the row's URL (spec 015).
macro_rules! ai {
    ($field:literal) => {
        concat!("(SELECT a.", $field, " FROM aeo_pages a WHERE a.url_id = u.id)")
    };
}

/// A failed AI check with a tab filter: bit `$bit` of `aeo_pages.flags`
/// (spec 015 contracts/aeo-crate.md; bits are append-only).
macro_rules! ai_flag {
    ($id:literal, $label:literal, $bit:literal) => {
        filter(
            $id,
            $label,
            concat!(
                "(SELECT a.flags FROM aeo_pages a WHERE a.url_id = u.id) & (1 << ",
                $bit,
                ") != 0"
            ),
        )
        .needs(Need::AiChecks)
    };
}

fn ai_search_tab() -> TabDef {
    use ValueKind::*;
    let ai_col = |id, label, expr, kind, width| col(id, label, expr, kind, width).needs(Need::AiChecks);
    TabDef {
        id: "ai-search",
        title: "AI search",
        eligible: HTML_OK,
        columns: vec![
            c::ADDRESS,
            ai_col("ai_score", "AI score", ai!("score"), Int, 90),
            ai_col("page_type", "Page type", ai!("page_type"), Text, 100),
            ai_col("ai_access", "Access", ai!("access"), Int, 80),
            ai_col("ai_readable", "Readable without JS", ai!("readable"), Int, 90),
            ai_col("ai_answer", "Answer readiness", ai!("answer"), Int, 90),
            ai_col("ai_trust", "Trust", ai!("trust"), Int, 70),
            ai_col("ai_structured", "Structured data", ai!("structured"), Int, 90),
            ai_col(
                "blocked_crawlers",
                "Blocked AI crawlers",
                "ai_bot_names((SELECT a.blocked_bots FROM aeo_pages a WHERE a.url_id = u.id))",
                Text,
                240,
            ),
            ai_col("published", "Published", ai!("published"), Text, 110),
            ai_col("modified", "Modified", ai!("modified"), Text, 110),
            ai_col("author", "Author", ai!("author"), Text, 160),
            ai_col("version_used", "Version used", ai!("version_used"), Text, 100).hidden(),
            ai_col("provisional", "Provisional", ai!("provisional"), Bool, 90).hidden(),
            ai_col(
                "page_type_reason",
                "Page type reason",
                ai!("page_type_reason"),
                Text,
                220,
            )
            .hidden(),
            c::TITLE.hidden(),
        ],
        filters: vec![
            filter("all", "All", "1"),
            filter(
                "score_below_50",
                "AI score below 50",
                concat!(ai!("score"), " < 50"),
            )
            .needs(Need::AiChecks),
            ai_flag!("search_blocked", "Blocked for AI search crawlers", 0),
            ai_flag!("training_only_blocked", "Blocked for AI training only", 1),
            ai_flag!("user_fetch_blocked", "Blocked for user-requested fetches", 2),
            ai_flag!("snippet_restricted", "Snippet restricted", 3),
            ai_flag!("js_content", "Main content missing without JavaScript", 4),
            ai_flag!("js_suspected", "Possibly built with JavaScript", 5),
            ai_flag!("question_no_answer", "Question without a direct answer", 6),
            ai_flag!("unstructured", "Long text without structure", 7),
            ai_flag!("no_facts", "No facts with sources", 8),
            ai_flag!(
                "sd_not_visible",
                "Structured data content not visible on the page",
                9
            ),
            ai_flag!("no_date", "No date", 10),
            ai_flag!("dates_disagree", "Dates disagree", 11),
            ai_flag!("stale", "Not updated in 12 months", 12),
            ai_flag!("no_author", "No author", 13),
            ai_flag!("sd_errors", "Structured data errors", 14),
            ai_flag!("sd_warnings", "Structured data warnings", 15),
        ],
        search_column: Some("author"),
        scoped: false,
        rows: None,
    }
}

/// Every tab in display order (FR-007, FR-009).
pub fn tabs() -> &'static [TabDef] {
    static TABS: std::sync::LazyLock<Vec<TabDef>> = std::sync::LazyLock::new(build);
    &TABS
}

pub fn tab(id: &str) -> Option<&'static TabDef> {
    tabs().iter().find(|t| t.id == id)
}

fn build() -> Vec<TabDef> {
    use ValueKind::*;
    vec![
        TabDef {
            id: "internal",
            title: "Internal",
            eligible: INTERNAL,
            columns: vec![
                c::ADDRESS,
                c::CONTENT_TYPE,
                c::STATUS_CODE,
                c::STATUS,
                c::FINAL_STATE,
                c::INDEXABILITY,
                c::TITLE,
                c::META_DESCRIPTION,
                c::H1,
                c::WORD_COUNT,
                c::SIZE,
                c::RESPONSE_TIME,
                c::DEPTH,
                c::INLINKS,
                c::OUTLINKS,
                c::REDIRECT_TARGET.hidden(),
                c::ERROR_CLASS.hidden(),
                c::FOUND_ON,
                c::SOURCE,
                c::IN_SITEMAP,
            ],
            filters: TYPE_FILTERS.to_vec(),
            search_column: Some("title"),
            scoped: false,
            rows: None,
        },
        TabDef {
            id: "external",
            title: "External",
            eligible: EXTERNAL,
            columns: vec![
                c::ADDRESS,
                c::CONTENT_TYPE,
                c::STATUS_CODE,
                c::STATUS,
                c::FINAL_STATE,
                c::INLINKS,
                c::REDIRECT_TARGET.hidden(),
                c::ERROR_CLASS.hidden(),
                c::FOUND_ON,
            ],
            filters: {
                let mut f = TYPE_FILTERS.to_vec();
                f.push(filter("not_checked", "Not checked", "u.fetched_at IS NULL"));
                f
            },
            search_column: None,
            scoped: false,
            rows: None,
        },
        TabDef {
            id: "response-codes",
            title: "Response codes",
            eligible: RESPONSES,
            columns: vec![
                c::ADDRESS,
                c::STATUS_CODE,
                c::STATUS,
                c::FINAL_STATE,
                c::ERROR_CLASS,
                c::ATTEMPTS,
                c::REDIRECT_TARGET,
                c::REDIRECT_HOPS,
                c::INLINKS,
                c::CONTENT_TYPE.hidden(),
                c::FOUND_ON,
            ],
            filters: vec![
                filter("all", "All", "1"),
                filter(
                    "blocked_robots",
                    "Blocked by robots.txt",
                    "(u.reason IN ('blocked_by_robots', 'robots_unavailable') \
                     OR (u.check_only = 1 AND u.fetched_at IS NULL AND u.reason_detail IS NOT NULL))",
                ),
                filter(
                    "blocked_config",
                    "Blocked by configuration",
                    "u.reason IN ('excluded', 'not_included', 'outside_start_folder')",
                ),
                filter("no_response", "No response", "u.error_class IS NOT NULL"),
                filter("success", "Success (2xx)", "u.error_class IS NULL AND u.status BETWEEN 200 AND 299"),
                filter("redirection", "Redirection (3xx)", "u.error_class IS NULL AND u.status BETWEEN 300 AND 399"),
                filter(
                    "redirect_chain",
                    "Redirect loop or chain over limit",
                    "(u.error_class IN ('redirect_loop', 'redirect_problem') OR u.id IN (SELECT r.id FROM temp.redirects r WHERE r.is_loop OR r.hops > redirect_limit()))",
                )
                .reads(Derived::Redirects),
                filter("client_error", "Client error (4xx)", "u.error_class IS NULL AND u.status BETWEEN 400 AND 499"),
                filter("server_error", "Server error (5xx)", "u.error_class IS NULL AND u.status BETWEEN 500 AND 599"),
                // Spec 004 FR-012: the failures "Retry failed URLs" picks up,
                // and the ones it never will. Same condition as the engine.
                filter("transient", "Failed, may work later", TRANSIENT).needs(Need::Attempts),
                filter("permanent", "Failed, permanent", PERMANENT).needs(Need::Attempts),
                filter("retried", "Needed more than one attempt", "u.attempts > 1").needs(Need::Attempts),
                // One filter per error class (spec 004 FR-012), listed only
                // when the crawl has such errors.
                filter("class_dns", "DNS lookup failed", "u.error_class = 'dns'").when_present(),
                filter("class_dns_temporary", "DNS temporarily unavailable", "u.error_class = 'dns_temporary'")
                    .when_present(),
                filter("class_connection", "Connection failed", "u.error_class = 'connection'").when_present(),
                filter("class_timeout", "Timed out", "u.error_class = 'timeout'").when_present(),
                filter("class_tls_invalid", "Invalid TLS certificate", "u.error_class = 'tls_invalid'").when_present(),
                filter(
                    "class_redirect_problem",
                    "Redirect problem",
                    "u.error_class IN ('redirect_problem', 'redirect_loop')",
                )
                .when_present(),
                filter("class_content_problem", "Content problem", "u.error_class = 'content_problem'").when_present(),
                filter("class_malformed_url", "Malformed URL", "u.error_class = 'malformed_url'").when_present(),
                filter("class_host_unavailable", "Host unavailable", "u.error_class = 'host_unavailable'")
                    .when_present(),
                filter("class_http_429", "Too many requests (429)", "u.error_class IS NULL AND u.status = 429")
                    .when_present(),
            ],
            search_column: None,
            scoped: true,
            rows: None,
        },
        TabDef {
            id: "url",
            title: "URL",
            eligible: INTERNAL,
            columns: vec![
                c::ADDRESS,
                col("length", "Length", "length(u.url)", Int, 80),
                col(
                    "query_string",
                    "Query string",
                    "CASE WHEN instr(u.url, '?') > 0 THEN substr(u.url, instr(u.url, '?') + 1) END",
                    Text,
                    220,
                ),
                c::DEPTH,
                c::STATUS_CODE.hidden(),
            ],
            filters: vec![
                filter("all", "All", "1"),
                filter("non_ascii", "Non-ASCII characters", "url_check(u.url, 'non_ascii')"),
                filter("underscores", "Underscores", "url_check(u.url, 'underscore')"),
                filter("uppercase", "Uppercase", "url_check(u.url, 'uppercase')"),
                filter("parameters", "Contains query parameters", "instr(u.url, '?') > 0"),
                filter("multiple_slashes", "Multiple slashes", "url_check(u.url, 'multiple_slashes')"),
                filter("spaces", "Contains spaces", "url_check(u.url, 'spaces')"),
                filter("over_115", "Over 115 characters", "length(u.url) > 115"),
            ],
            search_column: None,
            scoped: false,
            rows: None,
        },
        TabDef {
            id: "page-titles",
            title: "Page titles",
            // temp.titles holds exactly the HTML_OK rows.
            eligible: "1",
            columns: vec![
                c::ADDRESS,
                c::TITLE.sorted_by("x.k"),
                col("title_length", "Title length", "x.len", Int, 100),
                col("occurrences", "Occurrences", "x.n", Int, 110),
                ext(
                    "title_2",
                    "Title (second)",
                    "(SELECT t.text FROM page_text t WHERE t.url_id = u.id AND t.kind = 'title' AND t.pos = 1)",
                    Text,
                    260,
                )
                .hidden(),
                c::H1.hidden(),
                c::INDEXABILITY.hidden(),
            ],
            filters: vec![
                filter("all", "All", "1"),
                filter("missing", "Missing", "COALESCE(x.t, '') = ''"),
                filter("duplicate", "Duplicate", "x.n > 1"),
                filter("over_60", "Over 60 characters", "x.len > 60"),
                filter("below_30", "Below 30 characters", "x.t <> '' AND x.len < 30"),
                filter("same_as_h1", "Same as H1", "x.same_h1").needs(Need::Extraction),
                filter("multiple", "Multiple", "x.count > 1").needs(Need::Extraction),
            ],
            search_column: Some("title"),
            scoped: false,
            rows: Some(Derived::Titles),
        },
        content_tab(
            "meta-description",
            "Meta description",
            vec![
                c::ADDRESS,
                ext("meta_description", "Meta description", MD, Text, 360),
                ext("length", "Length", "length((SELECT p.meta_description FROM pages p WHERE p.url_id = u.id))", Int, 80),
                ext(
                    "occurrences",
                    "Occurrences",
                    "(SELECT o.n FROM temp.occ_md o WHERE o.v = (SELECT p.meta_description FROM pages p WHERE p.url_id = u.id))",
                    Int,
                    110,
                )
                .reads(Derived::OccMeta),
            ],
            vec![
                filter("missing", "Missing", "COALESCE((SELECT p.meta_description FROM pages p WHERE p.url_id = u.id), '') = ''"),
                filter(
                    "duplicate",
                    "Duplicate",
                    "(SELECT p.meta_description FROM pages p WHERE p.url_id = u.id) IN (SELECT o.v FROM temp.occ_md o WHERE o.n > 1)",
                )
                .reads(Derived::OccMeta),
                filter(
                    "over_155",
                    "Over 155 characters",
                    "length((SELECT p.meta_description FROM pages p WHERE p.url_id = u.id)) > 155",
                ),
                filter(
                    "below_70",
                    "Below 70 characters",
                    "length((SELECT p.meta_description FROM pages p WHERE p.url_id = u.id)) BETWEEN 1 AND 69",
                ),
                filter(
                    "multiple",
                    "Multiple",
                    "(SELECT p.meta_description_count FROM pages p WHERE p.url_id = u.id) > 1",
                ),
            ],
            "meta_description",
        ),
        heading_tab!("h1", "H1", Derived::OccH1),
        heading_tab!("h2", "H2", Derived::OccH2),
        TabDef {
            id: "images",
            title: "Images",
            // Image URLs: found in img elements first, or answering with an image type.
            eligible: "(u.source = 'image' OR content_kind(u.content_type) = 'image') AND u.state IN ('crawled', 'not_crawled')",
            columns: vec![
                c::ADDRESS,
                ext("alt", "Alt text", IMG_ALT, Text, 280),
                ext(
                    "alt_length",
                    "Alt length",
                    "length((SELECT l.alt FROM links l WHERE l.to_id = u.id AND l.kind = 'image' ORDER BY l.rowid LIMIT 1))",
                    Int,
                    90,
                ),
                c::STATUS_CODE,
                c::SIZE,
                col(
                    "found_on_count",
                    "Found on",
                    "(SELECT COUNT(DISTINCT l.from_id) FROM links l WHERE l.to_id = u.id AND l.kind = 'image')",
                    Int,
                    90,
                ),
                c::FINAL_STATE.hidden(),
            ],
            filters: vec![
                filter("all", "All", "1"),
                filter("missing_alt", "Missing alt attribute", image_use!("l.alt IS NULL")).needs(Need::Extraction),
                filter("empty_alt", "Empty alt text", image_use!("l.alt = ''")).needs(Need::Extraction),
                filter("alt_over_100", "Alt text over 100 characters", image_use!("length(l.alt) > 100"))
                    .needs(Need::Extraction),
                filter("over_100kb", "Over 100 KB", "u.size_bytes > 102400").needs(Need::Timing),
            ],
            search_column: Some("alt"),
            scoped: false,
            rows: None,
        },
        content_tab(
            "canonicals",
            "Canonicals",
            vec![
                c::ADDRESS,
                ext("canonical_html", "Canonical (HTML)", canonical_from!("html"), Url, 320),
                ext("canonical_http", "Canonical (HTTP header)", canonical_from!("header"), Url, 320),
                c::INDEXABILITY,
            ],
            vec![
                filter("contains", "Contains canonical", "u.canonical IS NOT NULL"),
                filter("self", "Self-referencing", "u.canonical = u.url"),
                filter("canonicalised", "Canonicalised", "u.canonical <> u.url"),
                filter("missing", "Missing", "u.canonical IS NULL"),
                filter("conflicting", "Multiple or conflicting", "u.canonical_count > 1"),
            ],
            "canonical_html",
        ),
        content_tab(
            "directives",
            "Directives",
            vec![
                c::ADDRESS,
                ext("meta_robots", "Meta robots", directives_from!("meta"), Text, 220),
                ext("x_robots_tag", "X-Robots-Tag", directives_from!("header"), Text, 220),
                c::INDEXABILITY,
            ],
            vec![
                filter("index", "Index", "COALESCE(u.noindex, 0) = 0"),
                filter("noindex", "Noindex", "u.noindex = 1"),
                filter("follow", "Follow", "COALESCE(u.nofollow, 0) = 0"),
                filter("nofollow", "Nofollow", "u.nofollow = 1"),
                filter("none", "None", directive!("'none'")),
                filter("noarchive", "Noarchive", directive!("'noarchive'")),
                filter("nosnippet", "Nosnippet", directive!("'nosnippet'")),
                filter(
                    "max_preview",
                    "Max-snippet / max-image-preview / max-video-preview",
                    directive!("'max-snippet', 'max-image-preview', 'max-video-preview'"),
                ),
                filter("unavailable_after", "Unavailable_after", directive!("'unavailable_after'")),
            ],
            "meta_robots",
        ),
        ai_search_tab(),
    ]
}

/// Builds a derived table on `conn` (dropping an older copy).
pub(crate) fn build_derived(
    conn: &rusqlite::Connection,
    d: Derived,
    features: &crate::Features,
) -> rusqlite::Result<()> {
    match d {
        Derived::Titles => {
            conn.execute_batch(
                "DROP TABLE IF EXISTS temp.titles;
                 DROP TABLE IF EXISTS temp.title_occ;
                 CREATE TEMP TABLE titles(id INTEGER PRIMARY KEY, t TEXT, len INTEGER, k TEXT, n INTEGER,
                   same_h1 INTEGER, count INTEGER);
                 CREATE TEMP TABLE title_occ(v TEXT PRIMARY KEY, n INTEGER NOT NULL) WITHOUT ROWID;",
            )?;
            append_titles(conn, features)
        }
        Derived::Redirects => conn.execute_batch(
            "DROP TABLE IF EXISTS temp.redirects;
             CREATE TEMP TABLE redirects(id INTEGER PRIMARY KEY, hops INTEGER NOT NULL, final_id INTEGER, is_loop INTEGER NOT NULL);
             WITH RECURSIVE c(start, cur, n, path) AS (
               SELECT id, redirect_to, 1, ',' || id || ',' FROM urls WHERE redirect_to IS NOT NULL
               UNION ALL
               SELECT c.start, r.redirect_to, c.n + 1, c.path || c.cur || ','
               FROM c JOIN urls r ON r.id = c.cur
               WHERE r.redirect_to IS NOT NULL AND instr(c.path, ',' || c.cur || ',') = 0 AND c.n < 100
             )
             INSERT INTO temp.redirects
               SELECT start, MAX(n), cur, instr(path, ',' || cur || ',') > 0 FROM c GROUP BY start;",
        ),
        Derived::OccMeta => occ(conn, "occ_md", "meta_description"),
        Derived::OccH1 => occ(conn, "occ_h1", "h1_1"),
        Derived::OccH2 => occ(conn, "occ_h2", "h2_1"),
    }
}

/// Brings a derived table built from older data up to date without
/// rebuilding it, when it can be; false when it must be rebuilt.
pub(crate) fn update_derived(
    conn: &rusqlite::Connection,
    d: Derived,
    features: &crate::Features,
) -> rusqlite::Result<bool> {
    match d {
        Derived::Titles => append_titles(conn, features).map(|()| true),
        _ => Ok(false),
    }
}

/// Adds the pages not yet in `temp.titles`, counts their titles in
/// `temp.title_occ`, and updates the count of listed pages that share one. Rows already listed never need a second look: the
/// engine writes a response, with its page content in the same
/// transaction, only to a URL in flight, so an HTML page fetched
/// successfully keeps its title, H1 and status for good. A live crawl's
/// refresh then costs one probe per URL plus the new pages.
fn append_titles(conn: &rusqlite::Connection, features: &crate::Features) -> rusqlite::Result<()> {
    // Files without page content may not have the `pages` table.
    let (h1, count, pages) = if features.has(Need::Extraction) {
        ("p.h1_1", "p.title_count", "LEFT JOIN pages p ON p.url_id = u.id")
    } else {
        ("NULL", "NULL", "")
    };
    // Counting with a window function would sort every row twice;
    // grouping the new titles and looking the counts up costs a fifth.
    conn.execute_batch(&format!(
        "DROP TABLE IF EXISTS temp.titles_new;
         CREATE TEMP TABLE titles_new AS
           SELECT u.id AS id, {NORM_TITLE} AS t, natkey(u.title) AS k, {h1} AS h1, {count} AS count
           FROM urls u NOT INDEXED {pages}
           WHERE NOT EXISTS (SELECT 1 FROM temp.titles x WHERE x.id = u.id) AND {HTML_OK};
         INSERT INTO temp.title_occ SELECT t, COUNT(*) FROM temp.titles_new WHERE t <> '' GROUP BY t
           ON CONFLICT(v) DO UPDATE SET n = n + excluded.n;
         UPDATE temp.titles SET n = (SELECT o.n FROM temp.title_occ o WHERE o.v = titles.t)
           WHERE t IN (SELECT t FROM temp.titles_new WHERE t <> '');
         INSERT INTO temp.titles
           SELECT id, t, length(t), k, (SELECT o.n FROM temp.title_occ o WHERE o.v = t),
             t <> '' AND lower(t) = lower(h1), count
           FROM temp.titles_new ORDER BY id;
         DROP TABLE temp.titles_new;"
    ))
}

/// `temp.<table>(v, n)`: each non-empty value of `pages.<field>` on the
/// content tabs' pages, and how many pages use it.
fn occ(conn: &rusqlite::Connection, table: &str, field: &str) -> rusqlite::Result<()> {
    conn.execute_batch(&format!(
        "DROP TABLE IF EXISTS temp.{table};
         CREATE TEMP TABLE {table}(v TEXT PRIMARY KEY, n INTEGER NOT NULL) WITHOUT ROWID;
         INSERT INTO temp.{table} SELECT p.{field}, COUNT(*) FROM pages p JOIN urls u ON u.id = p.url_id
           WHERE {HTML_OK} AND p.{field} <> '' GROUP BY 1;"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn ids_are_unique_and_all_first() {
        let mut tab_ids = HashSet::new();
        for t in tabs() {
            assert!(tab_ids.insert(t.id), "duplicate tab {}", t.id);
            let cols: HashSet<_> = t.columns.iter().map(|c| c.id).collect();
            assert_eq!(cols.len(), t.columns.len(), "duplicate column in {}", t.id);
            let fs: HashSet<_> = t.filters.iter().map(|f| f.id).collect();
            assert_eq!(fs.len(), t.filters.len(), "duplicate filter in {}", t.id);
            assert_eq!(t.filters[0].id, "all");
            assert_eq!(t.columns[0].id, "address");
            if let Some(s) = t.search_column {
                assert!(t.column(s).is_some(), "{} searches unknown column {s}", t.id);
            }
        }
        assert_eq!(tab_ids.len(), 12);
    }

    #[test]
    fn thresholds_are_in_labels() {
        // FR-012: the user can see what "over length" means.
        for t in tabs() {
            for f in &t.filters {
                if f.id.starts_with("over_") || f.id.starts_with("below_") {
                    assert!(f.label.chars().any(|c| c.is_ascii_digit()), "{}/{}", t.id, f.id);
                }
            }
        }
    }
}
