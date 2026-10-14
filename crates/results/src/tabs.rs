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
    /// Crawl Analysis results such as Link Score (spec 023), calculated
    /// once fetching has ended.
    Analysis,
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
            Self::Analysis => "Calculated when the crawl ends.",
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
            Self::Analysis => "analysis",
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
    /// Link Score: 0-100, shown whole in the table, one decimal elsewhere.
    Score,
    /// A share of pages, 0-100.
    Percent,
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
    /// `temp.occ_title(v, n)`: each normalised title and how many pages use it.
    OccTitle,
    /// `temp.redirects(id, hops, final_id, is_loop)`: each redirect chain.
    Redirects,
    /// `temp.occ_md(v, n)`: each meta description and how many pages use it.
    OccMeta,
    /// `temp.occ_h1(v, n)`: each first H1 and how many pages use it.
    OccH1,
    /// `temp.occ_h2(v, n)`: each first H2 and how many pages use it.
    OccH2,
    /// `link_metrics`: Link Score and link counts (spec 023), built in
    /// `temp` when the file holds no current copy.
    LinkMetrics,
    /// `temp.viz_crawl_tree`: each URL's crawl tree parent (spec 022).
    VizCrawlTree,
    /// `temp.viz_dirs` and `temp.viz_dir_urls`: the directory tree.
    VizDirs,
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
}

impl TabDef {
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
pub(crate) const HTML_OK: &str = "u.internal = 1 AND u.state = 'crawled' AND u.error_class IS NULL \
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
}

/// A spec 023 link metric, read from `link_metrics` (unqualified: a fresh
/// copy in `temp` shadows a stored one, plan D5).
macro_rules! lm {
    ($id:literal, $label:literal, $field:literal, $kind:expr, $width:literal $(,)?) => {
        col(
            $id,
            $label,
            concat!("(SELECT ", $field, " FROM link_metrics m WHERE m.url_id = u.id)"),
            $kind,
            $width,
        )
        .needs(Need::Analysis)
        .reads(Derived::LinkMetrics)
    };
}

const fn filter(id: &'static str, label: &'static str, cond: &'static str) -> FilterDef {
    FilterDef {
        id,
        label,
        cond,
        need: Need::Base,
        derived: None,
    }
}

/// A Links tab filter on the spec 023 metrics.
const fn link_filter(id: &'static str, label: &'static str, cond: &'static str) -> FilterDef {
    filter(id, label, cond)
        .needs(Need::Analysis)
        .reads(Derived::LinkMetrics)
}

impl FilterDef {
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
    pub const LINK_SCORE: ColumnDef = lm!(
        "link_score",
        "Link Score",
        "round(m.score, 1)",
        ValueKind::Score,
        100
    );
    pub const LINK_RANK: ColumnDef = lm!("link_rank", "Link Score rank", "m.rank", ValueKind::Int, 120);
    pub const UNIQUE_INLINKS: ColumnDef = lm!(
        "unique_inlinks",
        "Unique inlinks",
        "m.unique_inlinks",
        ValueKind::Int,
        120
    );
    pub const FOLLOWED_INLINKS: ColumnDef = lm!(
        "followed_inlinks",
        "Unique followed inlinks",
        "m.followed_inlinks",
        ValueKind::Int,
        170
    );
    pub const ALL_INLINKS: ColumnDef =
        lm!("all_inlinks", "All inlinks", "m.all_inlinks", ValueKind::Int, 100);
    pub const UNIQUE_OUTLINKS: ColumnDef = lm!(
        "unique_outlinks",
        "Unique outlinks",
        "m.unique_outlinks",
        ValueKind::Int,
        120
    );
    pub const EXTERNAL_OUTLINKS: ColumnDef = lm!(
        "external_outlinks",
        "Unique external outlinks",
        "m.external_outlinks",
        ValueKind::Int,
        170,
    );
    pub const PCT_LINKING: ColumnDef = lm!(
        "pct_linking",
        "% of pages linking",
        "round(m.pct_linking, 1)",
        ValueKind::Percent,
        140
    );
    pub const OCCURRENCES_TITLE: ColumnDef = col(
        "occurrences",
        "Occurrences",
        "(SELECT o.n FROM temp.occ_title o WHERE o.v = norm_ws(u.title))",
        ValueKind::Int,
        110,
    )
    .reads(Derived::OccTitle);
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
                c::LINK_SCORE,
                c::INLINKS,
                c::OUTLINKS,
                c::LINK_RANK.hidden(),
                c::UNIQUE_INLINKS.hidden(),
                c::FOLLOWED_INLINKS.hidden(),
                c::ALL_INLINKS.hidden(),
                c::UNIQUE_OUTLINKS.hidden(),
                c::EXTERNAL_OUTLINKS.hidden(),
                c::PCT_LINKING.hidden(),
                c::REDIRECT_TARGET.hidden(),
                c::ERROR_CLASS.hidden(),
                c::FOUND_ON,
                c::SOURCE,
                c::IN_SITEMAP,
            ],
            filters: TYPE_FILTERS.to_vec(),
            search_column: Some("title"),
            scoped: false,
        },
        TabDef {
            id: "links",
            title: "Links",
            eligible: HTML_OK,
            columns: vec![
                c::ADDRESS,
                c::LINK_SCORE,
                col("depth", "Crawl depth", "u.depth", Int, 100),
                c::UNIQUE_INLINKS,
                c::FOLLOWED_INLINKS,
                c::UNIQUE_OUTLINKS,
                c::INDEXABILITY,
                c::LINK_RANK.hidden(),
                c::ALL_INLINKS.hidden(),
                c::EXTERNAL_OUTLINKS.hidden(),
                c::PCT_LINKING.hidden(),
                c::TITLE.hidden(),
            ],
            filters: vec![
                filter("all", "All", "1"),
                filter("depth_4", "Crawl depth 4 or more", "u.depth >= 4"),
                link_filter(
                    "one_followed",
                    "One followed inlink",
                    "(SELECT m.followed_inlinks FROM link_metrics m WHERE m.url_id = u.id) = 1",
                ),
                link_filter(
                    "no_followed",
                    "No followed inlinks",
                    "u.source <> 'start' AND (SELECT m.followed_inlinks FROM link_metrics m WHERE m.url_id = u.id) = 0",
                ),
                link_filter(
                    "no_outlinks",
                    "No internal outlinks",
                    "(SELECT m.unique_outlinks FROM link_metrics m WHERE m.url_id = u.id) = 0",
                ),
                filter(
                    "nofollow_outlinks",
                    "Nofollow internal outlinks",
                    "EXISTS (SELECT 1 FROM links l JOIN urls t ON t.id = l.to_id WHERE l.from_id = u.id \
                     AND l.kind = 'anchor' AND t.internal = 1 AND (l.nofollow = 1 OR l.page_nofollow = 1))",
                ),
            ],
            search_column: None,
            scoped: false,
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
            ],
            search_column: None,
            scoped: true,
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
        },
        TabDef {
            id: "page-titles",
            title: "Page titles",
            eligible: HTML_OK,
            columns: vec![
                c::ADDRESS,
                c::TITLE,
                col("title_length", "Title length", "length(norm_ws(u.title))", Int, 100),
                c::OCCURRENCES_TITLE,
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
                filter("missing", "Missing", "(u.title IS NULL OR norm_ws(u.title) = '')"),
                filter(
                    "duplicate",
                    "Duplicate",
                    "norm_ws(u.title) IN (SELECT o.v FROM temp.occ_title o WHERE o.n > 1)",
                )
                .reads(Derived::OccTitle),
                filter("over_60", "Over 60 characters", "length(norm_ws(u.title)) > 60"),
                filter(
                    "below_30",
                    "Below 30 characters",
                    "norm_ws(u.title) <> '' AND length(norm_ws(u.title)) < 30",
                ),
                filter(
                    "same_as_h1",
                    "Same as H1",
                    "norm_ws(u.title) <> '' AND lower(norm_ws(u.title)) = lower((SELECT p.h1_1 FROM pages p WHERE p.url_id = u.id))",
                )
                .needs(Need::Extraction),
                filter(
                    "multiple",
                    "Multiple",
                    "(SELECT p.title_count FROM pages p WHERE p.url_id = u.id) > 1",
                )
                .needs(Need::Extraction),
            ],
            search_column: Some("title"),
            scoped: false,
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
    ]
}

/// Builds a derived table on `conn` (dropping an older copy).
pub(crate) fn build_derived(conn: &rusqlite::Connection, d: Derived) -> rusqlite::Result<()> {
    match d {
        Derived::OccTitle => conn.execute_batch(&format!(
            "DROP TABLE IF EXISTS temp.occ_title;
             CREATE TEMP TABLE occ_title(v TEXT PRIMARY KEY, n INTEGER NOT NULL) WITHOUT ROWID;
             INSERT INTO temp.occ_title SELECT {NORM_TITLE}, COUNT(*) FROM urls u
               WHERE {HTML_OK} AND {NORM_TITLE} <> '' GROUP BY 1;"
        )),
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
        Derived::LinkMetrics => crate::analysis::link_metrics::build(conn).map(|_| ()),
        Derived::VizCrawlTree => crate::viz::crawl_tree::build(conn),
        Derived::VizDirs => crate::viz::dirs::build(conn),
    }
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
