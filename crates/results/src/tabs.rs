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
    /// Sentence and syllable counts for the readability score (spec 009
    /// FR-031).
    Readability,
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
            Self::Readability => {
                "Readability was not measured in this crawl file; it was made before SpiderLens scored readability."
            }
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
            Self::Readability => "readability",
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
    /// A URL's issues: `(4 - highest priority) * 1,000,000 + count`, so
    /// sorting descending puts the most urgent first (spec 009 FR-018).
    /// Shown as "High · 3".
    Issues,
}

/// Reads an [`ValueKind::Issues`] value: the highest priority and the count.
pub fn decode_issues(v: i64) -> (spiderlens_config::Priority, i64) {
    let p = spiderlens_config::Priority::ALL[(4 - (v / 1_000_000)).clamp(0, 3) as usize];
    (p, v % 1_000_000)
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
    /// `temp.occ_text(v, n)`: each visible text fingerprint and how many
    /// pages share it (spec 009).
    OccText,
    /// `temp.page_flags(url_id, flags)`: per linking page, which kinds of
    /// links it has ([`page_flag`] bits), from one pass over `links`
    /// (spec 009).
    PageFlags,
    /// `temp.issue_hits(rule, url_id)` for the issue settings in force
    /// (spec 009 plan D4).
    IssueHits,
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
    /// The issue rule whose threshold this filter uses, and its label with
    /// `{n}` for the value (spec 009 FR-005, US4 scenario 1).
    #[serde(skip)]
    #[ts(skip)]
    pub threshold: Option<(&'static str, &'static str)>,
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

/// Internal pages that were fetched successfully as HTML: the rows of the
/// content tabs (FR-010).
pub(crate) const HTML_OK: &str = "u.internal = 1 AND u.state = 'crawled' AND u.error_class IS NULL \
     AND u.status BETWEEN 200 AND 299 AND content_kind(u.content_type) = 'html'";

pub(crate) const INTERNAL: &str = "u.internal = 1 AND u.state = 'crawled'";
const EXTERNAL: &str = "u.internal = 0 AND u.state IN ('crawled', 'not_crawled')";
/// URLs with a response, plus URLs blocked by robots.txt or by the crawl's
/// rules (Assumptions: blocked URLs are listed here, not counted as errors).
pub(crate) const RESPONSES: &str = "u.state = 'crawled' \
     OR (u.check_only = 1 AND u.state = 'not_crawled' AND (u.fetched_at IS NOT NULL OR u.reason_detail IS NOT NULL)) \
     OR u.reason IN ('blocked_by_robots', 'robots_unavailable', 'excluded', 'not_included', 'outside_start_folder')";

/// Image URLs: found in img elements first, or answering with an image type.
pub(crate) const IMAGES: &str = "(u.source = 'image' OR content_kind(u.content_type) = 'image') AND u.state IN ('crawled', 'not_crawled')";

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

const fn filter(id: &'static str, label: &'static str, cond: &'static str) -> FilterDef {
    FilterDef {
        id,
        label,
        cond,
        need: Need::Base,
        derived: None,
        threshold: None,
    }
}

impl FilterDef {
    /// The label with the threshold in force.
    pub fn label_with(&self, values: &crate::sql::Thresholds) -> String {
        match self.threshold {
            Some((rule, template)) => values.get(rule).map_or_else(
                || self.label.to_string(),
                |v| template.replace("{n}", &crate::issues::fmt_num(*v)),
            ),
            None => self.label.to_string(),
        }
    }
    const fn thr(mut self, rule: &'static str, template: &'static str) -> Self {
        self.threshold = Some((rule, template));
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
    pub const ISSUES: ColumnDef = col(
        "issues",
        "Issues",
        "(SELECT (4 - MIN(r.prio)) * 1000000 + COUNT(*) FROM temp.issue_hits h \
         JOIN temp.issue_rules r ON r.idx = h.rule WHERE h.url_id = u.id)",
        ValueKind::Issues,
        110,
    )
    .hidden()
    .reads(Derived::IssueHits);
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
        col("attempts", "Attempts", "NULL", ValueKind::Int, 90).needs(Need::Attempts);
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
    ($id:literal, $label:literal, $occ:expr, $rule:literal, $missing:expr, $duplicate:expr, $over:expr, $multiple:expr) => {
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
                filter("missing", "Missing", $missing),
                filter("duplicate", "Duplicate", $duplicate).reads($occ),
                filter("over_70", "Over 70 characters", $over).thr($rule, "Over {n} characters"),
                filter("multiple", "Multiple", $multiple),
            ],
            $id,
        )
    };
}

/// Every tab in display order (FR-007, FR-009).
pub fn tabs() -> &'static [TabDef] {
    static TABS: std::sync::LazyLock<Vec<TabDef>> = std::sync::LazyLock::new(|| {
        let mut tabs = build();
        // Every tab can show each URL's issues (spec 009 FR-018).
        for t in &mut tabs {
            t.columns.push(c::ISSUES);
        }
        tabs
    });
    &TABS
}

pub fn tab(id: &str) -> Option<&'static TabDef> {
    tabs().iter().chain(issue_tabs()).find(|t| t.id == id)
}

/// One tab per issue rule, `issue:<rule id>` (spec 009 FR-015): the URLs
/// the rule flags, with its evidence. Views, counts, copy and export work on
/// them as on any tab, so the CLI exports an issue with the same rows.
pub fn issue_tabs() -> &'static [TabDef] {
    static TABS: std::sync::LazyLock<Vec<TabDef>> = std::sync::LazyLock::new(build_issue_tabs);
    &TABS
}

fn build_issue_tabs() -> Vec<TabDef> {
    let leak = |s: String| -> &'static str { Box::leak(s.into_boxed_str()) };
    crate::issues::catalogue()
        .iter()
        .enumerate()
        .map(|(i, r)| {
            let mut evidence = col("evidence", "Evidence", r.evidence, ValueKind::Text, 320).needs(r.need);
            evidence.derived = r.derived.first().copied();
            let mut all = filter("all", "All", "1").needs(r.need);
            all.derived = Some(Derived::IssueHits);
            let mut columns = vec![c::ADDRESS, evidence, c::STATUS_CODE];
            if r.scope == crate::issues::Scope::Pages {
                columns.push(c::TITLE);
                columns.push(c::INDEXABILITY);
            } else {
                columns.push(c::CONTENT_TYPE);
                columns.push(c::FINAL_STATE);
            }
            columns.extend([c::INLINKS, c::DEPTH.hidden(), c::FOUND_ON]);
            TabDef {
                id: leak(format!("issue:{}", r.id)),
                title: r.name,
                eligible: leak(format!(
                    "u.id IN (SELECT h.url_id FROM temp.issue_hits h WHERE h.rule = {i})"
                )),
                columns,
                filters: vec![all],
                search_column: Some("evidence"),
                scoped: false,
            }
        })
        .collect()
}

fn build() -> Vec<TabDef> {
    use ValueKind::*;
    // Filters that test what an issue rule tests share its condition
    // (spec 009 FR-005).
    use crate::issues::rules::{
        canonicals as ca, headings as hd, images as im, meta as me, titles as ti, url as ur,
    };
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
                    "(u.error_class = 'redirect_loop' OR u.id IN (SELECT r.id FROM temp.redirects r WHERE r.is_loop OR r.hops > redirect_limit()))",
                )
                .reads(Derived::Redirects),
                filter("client_error", "Client error (4xx)", "u.error_class IS NULL AND u.status BETWEEN 400 AND 499"),
                filter("server_error", "Server error (5xx)", "u.error_class IS NULL AND u.status BETWEEN 500 AND 599"),
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
                filter("non_ascii", "Non-ASCII characters", ur::NON_ASCII),
                filter("underscores", "Underscores", ur::UNDERSCORES),
                filter("uppercase", "Uppercase", ur::UPPERCASE),
                filter("parameters", "Contains query parameters", ur::PARAMETERS),
                filter("multiple_slashes", "Multiple slashes", ur::MULTIPLE_SLASHES),
                filter("spaces", "Contains spaces", ur::SPACES),
                filter("over_115", "Over 115 characters", ur::OVER).thr("url.over", "Over {n} characters"),
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
                filter("missing", "Missing", ti::MISSING),
                filter("duplicate", "Duplicate", ti::DUPLICATE).reads(Derived::OccTitle),
                filter("over_60", "Over 60 characters", ti::OVER).thr("title.over", "Over {n} characters"),
                filter("below_30", "Below 30 characters", ti::BELOW).thr("title.below", "Below {n} characters"),
                filter("same_as_h1", "Same as H1", ti::SAME_AS_H1).needs(Need::Extraction),
                filter("multiple", "Multiple", ti::MULTIPLE).needs(Need::Extraction),
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
                filter("missing", "Missing", me::MISSING),
                filter("duplicate", "Duplicate", me::DUPLICATE).reads(Derived::OccMeta),
                filter("over_155", "Over 155 characters", me::OVER).thr("meta.over", "Over {n} characters"),
                filter("below_70", "Below 70 characters", me::BELOW).thr("meta.below", "Below {n} characters"),
                filter("multiple", "Multiple", me::MULTIPLE),
            ],
            "meta_description",
        ),
        heading_tab!("h1", "H1", Derived::OccH1, "h1.over", hd::H1_MISSING, hd::H1_DUPLICATE, hd::H1_OVER, hd::H1_MULTIPLE),
        heading_tab!("h2", "H2", Derived::OccH2, "h2.over", hd::H2_MISSING, hd::H2_DUPLICATE, hd::H2_OVER, hd::H2_MULTIPLE),
        TabDef {
            id: "images",
            title: "Images",
            eligible: IMAGES,
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
                filter("missing_alt", "Missing alt attribute", im::MISSING_ALT).needs(Need::Extraction),
                filter("empty_alt", "Empty alt text", image_use!("l.alt = ''")).needs(Need::Extraction),
                filter("alt_over_100", "Alt text over 100 characters", im::ALT_OVER)
                    .needs(Need::Extraction)
                    .thr("image.alt_over", "Alt text over {n} characters"),
                filter("over_100kb", "Over 100 KB", im::LARGE).needs(Need::Timing).thr("image.large", "Over {n} bytes"),
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
                filter("canonicalised", "Canonicalised", ca::CANONICALISED),
                filter("missing", "Missing", ca::MISSING),
                filter("conflicting", "Multiple or conflicting", ca::CONFLICTING),
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
        Derived::OccText => conn.execute_batch(&format!(
            "DROP TABLE IF EXISTS temp.occ_text;
             CREATE TEMP TABLE occ_text(v INTEGER PRIMARY KEY, n INTEGER NOT NULL);
             INSERT INTO temp.occ_text SELECT p.text_hash, COUNT(*) FROM pages p JOIN urls u ON u.id = p.url_id
               WHERE {HTML_OK} AND p.text_hash IS NOT NULL AND p.word_count > 0 GROUP BY 1;"
        )),
        Derived::PageFlags => page_flags(conn),
        // Built by the reader or export, which know the issue settings.
        Derived::IssueHits => Ok(()),
    }
}

/// Bits of `temp.page_flags.flags`.
pub(crate) mod page_flag {
    /// An anchor to an internal URL that failed or returned 4xx/5xx.
    pub const BROKEN_INTERNAL: u32 = 1;
    /// An anchor to an external URL that failed or returned 4xx/5xx.
    pub const BROKEN_EXTERNAL: u32 = 2;
    /// An anchor to an internal 3xx.
    pub const TO_REDIRECT: u32 = 4;
    /// A nofollow anchor to an internal URL.
    pub const NOFOLLOW_INTERNAL: u32 = 8;
    /// Any anchor to an internal URL.
    pub const INTERNAL_OUT: u32 = 16;
    /// An image whose address is http:.
    pub const HTTP_IMAGE: u32 = 32;
    /// An anchor whose text says nothing ("click here").
    pub const WEAK_ANCHOR: u32 = 64;
    /// A canonical link outside the head.
    pub const CANONICAL_IN_BODY: u32 = 128;
    /// Any hreflang link.
    pub const HREFLANG: u32 = 256;
    /// An hreflang link to the page itself.
    pub const HREFLANG_SELF: u32 = 512;
    /// An hreflang link for x-default.
    pub const HREFLANG_X_DEFAULT: u32 = 1024;
    /// An hreflang link with a missing or invalid language code.
    pub const HREFLANG_BAD_CODE: u32 = 2048;
    /// A canonical, hreflang or pagination link to another URL that was
    /// fetched and failed, did not return 2xx, is noindex or canonicalised
    /// elsewhere (`issues::rules::target_not_ok!`).
    pub const CANONICAL_BAD_TARGET: u32 = 4096;
    pub const HREFLANG_BAD_TARGET: u32 = 8192;
    pub const PAGINATION_BAD_TARGET: u32 = 16384;
    /// A <title> in the body (from `page_text`).
    pub const TITLE_IN_BODY: u32 = 32768;
    /// A heading more than one level below the one before it.
    pub const HEADING_SKIP: u32 = 65536;
}

/// Link target classes, by URL id (see [`page_flags`]).
mod target_class {
    pub const INTERNAL: u8 = 1;
    pub const BROKEN: u8 = 2;
    pub const REDIRECT: u8 = 4;
    pub const HTTP: u8 = 8;
    pub const NOT_OK: u8 = 16;
}

/// Flags from the links with rowid in `[lo, hi)` (see [`page_flags`]).
fn scan_links(
    conn: &rusqlite::Connection,
    class: &[u8],
    extra: &str,
    (lo, hi): (i64, i64),
    flags: &mut [u32],
) -> rusqlite::Result<()> {
    use page_flag::*;
    use target_class::*;
    let len = class.len();
    let mut stmt = conn.prepare(&format!(
        "SELECT from_id, to_id, kind, nofollow, target LIKE 'http:%', {extra} FROM links \
         WHERE rowid >= ?1 AND rowid < ?2 AND kind IN ('anchor', 'image', 'canonical', 'hreflang', 'next', 'prev')"
    ))?;
    let mut rows = stmt.query((lo, hi))?;
    while let Some(r) = rows.next()? {
        let Some(from) = usize::try_from(r.get::<_, i64>(0)?).ok().filter(|&i| i < len) else {
            continue;
        };
        let to = r.get::<_, Option<i64>>(1)?.and_then(|t| usize::try_from(t).ok());
        let c = to.and_then(|t| class.get(t).copied());
        let other = to.is_some_and(|t| t != from);
        let not_ok = other && c.is_some_and(|c| c & NOT_OK != 0);
        let kind = r.get_ref(2)?.as_str().unwrap_or("");
        let mut f = 0u32;
        match kind {
            "anchor" => {
                if let Some(c) = c {
                    let internal = c & INTERNAL != 0;
                    if c & BROKEN != 0 {
                        f |= if internal {
                            BROKEN_INTERNAL
                        } else {
                            BROKEN_EXTERNAL
                        };
                    }
                    if internal {
                        f |= INTERNAL_OUT;
                        if c & REDIRECT != 0 {
                            f |= TO_REDIRECT;
                        }
                        if r.get::<_, i64>(3)? == 1 {
                            f |= NOFOLLOW_INTERNAL;
                        }
                    }
                }
                if r.get_ref(5)?
                    .as_str_or_null()?
                    .is_some_and(crate::sql::weak_anchor)
                {
                    f |= WEAK_ANCHOR;
                }
            }
            "image" => {
                let http = match c {
                    Some(c) => c & HTTP != 0,
                    None => to.is_none() && r.get::<_, Option<bool>>(4)?.unwrap_or(false),
                };
                if http {
                    f |= HTTP_IMAGE;
                }
            }
            "canonical" => {
                if r.get::<_, Option<i64>>(6)? == Some(0) {
                    f |= CANONICAL_IN_BODY;
                }
                if not_ok {
                    f |= CANONICAL_BAD_TARGET;
                }
            }
            "hreflang" => {
                f |= HREFLANG;
                if to == Some(from) {
                    f |= HREFLANG_SELF;
                }
                let lang = r.get_ref(7)?.as_str_or_null()?;
                if lang.is_some_and(|l| l.eq_ignore_ascii_case("x-default")) {
                    f |= HREFLANG_X_DEFAULT;
                }
                if !lang.is_some_and(crate::sql::lang_code_ok) {
                    f |= HREFLANG_BAD_CODE;
                }
                if not_ok {
                    f |= HREFLANG_BAD_TARGET;
                }
            }
            _ => {
                if not_ok {
                    f |= PAGINATION_BAD_TARGET;
                }
            }
        }
        flags[from] |= f;
    }
    Ok(())
}

/// Builds `temp.page_flags` in Rust: one sequential scan of `links` (and
/// one of `page_text`) with
/// each target's class looked up in an array by URL id. A SQL join probes
/// the urls b-tree once per link, and a rule per page reads that page's
/// links again; on 10 million links either takes tens of seconds, this a
/// few (spec 009 SC-003).
fn page_flags(conn: &rusqlite::Connection) -> rusqlite::Result<()> {
    page_flags_with(conn, 200_000)
}

/// [`page_flags`], splitting the links scan from `min_parallel` links.
fn page_flags_with(conn: &rusqlite::Connection, min_parallel: i64) -> rusqlite::Result<()> {
    use page_flag::*;
    let max_id: i64 = conn.query_row("SELECT COALESCE(MAX(id), 0) FROM urls", [], |r| r.get(0))?;
    let len = usize::try_from(max_id).unwrap_or(0) + 1;
    let mut class = vec![0u8; len];
    {
        let mut stmt = conn.prepare(
            "SELECT id, (internal = 1)
               | (COALESCE(error_class IS NOT NULL OR status >= 400, 0) << 1)
               | (COALESCE(error_class IS NULL AND status BETWEEN 300 AND 399, 0) << 2)
               | ((url LIKE 'http:%') << 3)
               | (COALESCE(fetched_at IS NOT NULL AND (error_class IS NOT NULL OR status IS NULL
                   OR status NOT BETWEEN 200 AND 299 OR noindex = 1 OR (canonical IS NOT NULL AND canonical <> url)), 0) << 4)
             FROM urls",
        )?;
        let mut rows = stmt.query([])?;
        while let Some(r) = rows.next()? {
            let id: i64 = r.get(0)?;
            if let Some(c) = usize::try_from(id).ok().and_then(|i| class.get_mut(i)) {
                *c = r.get(1)?;
            }
        }
    }
    // Files from before page extraction have no link text, head or language.
    let extracted = conn
        .prepare("SELECT 1 FROM pragma_table_info('links') WHERE name = 'in_head'")?
        .exists([])?;
    let extra = if extracted {
        "text, in_head, lang"
    } else {
        "NULL, NULL, NULL"
    };
    let mut flags = vec![0u32; len];
    let max_link: i64 = conn.query_row("SELECT COALESCE(MAX(rowid), 0) FROM links", [], |r| r.get(0))?;
    // Large files: split the scan by rowid over a few read-only connections.
    // During a live crawl each sees its own snapshot; counts then are
    // provisional anyway (FR-019).
    let threads = std::thread::available_parallelism().map_or(1, |n| n.get()).min(4) as i64;
    let path = conn
        .path()
        .filter(|p| !p.is_empty())
        .map(std::path::PathBuf::from);
    match path {
        Some(path) if threads > 1 && max_link >= min_parallel => {
            let step = max_link / threads + 1;
            let parts = std::thread::scope(|sc| {
                let handles: Vec<_> = (0..threads)
                    .map(|t| {
                        let (class, path, extra) = (&class, &path, extra);
                        sc.spawn(move || -> rusqlite::Result<Vec<u32>> {
                            let c = spiderlens_engine::crawl::store::connect_read_only(path)
                                .map_err(|e| rusqlite::Error::UserFunctionError(e.to_string().into()))?;
                            let mut f = vec![0u32; class.len()];
                            scan_links(&c, class, extra, (t * step, (t + 1) * step), &mut f)?;
                            Ok(f)
                        })
                    })
                    .collect();
                handles
                    .into_iter()
                    .map(|h| h.join().expect("link scan thread"))
                    .collect::<rusqlite::Result<Vec<_>>>()
            })?;
            for part in parts {
                for (f, p) in flags.iter_mut().zip(part) {
                    *f |= p;
                }
            }
        }
        _ => scan_links(conn, &class, extra, (0, max_link + 1), &mut flags)?,
    }
    // Titles and headings, in page order (the page_text_url index).
    let has_text = conn
        .prepare("SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'page_text'")?
        .exists([])?;
    if has_text {
        let mut stmt = conn.prepare(
            "SELECT url_id, kind = 'title', level, in_head FROM page_text \
             WHERE kind IN ('heading', 'title') ORDER BY url_id, kind, pos",
        )?;
        let mut rows = stmt.query([])?;
        let mut page = -1i64;
        let mut prev: Option<i64> = None;
        while let Some(r) = rows.next()? {
            let id: i64 = r.get(0)?;
            let Some(f) = usize::try_from(id).ok().and_then(|i| flags.get_mut(i)) else {
                continue;
            };
            if r.get::<_, bool>(1)? {
                if r.get::<_, Option<i64>>(3)? == Some(0) {
                    *f |= TITLE_IN_BODY;
                }
                continue;
            }
            if id != page {
                page = id;
                prev = None;
            }
            let level: Option<i64> = r.get(2)?;
            if let (Some(p), Some(l)) = (prev, level)
                && l > p + 1
            {
                *f |= HEADING_SKIP;
            }
            prev = level;
        }
    }
    conn.execute_batch(
        "DROP TABLE IF EXISTS temp.page_flags;
         CREATE TEMP TABLE page_flags(url_id INTEGER PRIMARY KEY, flags INTEGER NOT NULL);
         SAVEPOINT page_flags;",
    )?;
    {
        let mut ins = conn.prepare("INSERT INTO temp.page_flags VALUES (?1, ?2)")?;
        for (id, f) in flags.iter().enumerate().filter(|(_, f)| **f != 0) {
            ins.execute((id as i64, *f))?;
        }
    }
    conn.execute_batch("RELEASE page_flags;")
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
        assert_eq!(tab_ids.len(), 11);
    }

    #[test]
    fn split_link_scan_gives_the_same_flags() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("f.sqlite");
        let conn = rusqlite::Connection::open(&path).unwrap();
        conn.execute_batch(
            "CREATE TABLE urls(id INTEGER PRIMARY KEY, url TEXT, internal INTEGER, status INTEGER, error_class TEXT,
               fetched_at INTEGER, noindex INTEGER, canonical TEXT);
             CREATE TABLE links(from_id INTEGER, to_id INTEGER, kind TEXT, nofollow INTEGER, target TEXT,
               text TEXT, in_head INTEGER, lang TEXT);
             BEGIN;",
        )
        .unwrap();
        for id in 1..=200i64 {
            let status = [200, 404, 301, 500][id as usize % 4];
            conn.execute(
                "INSERT INTO urls VALUES (?1, 'https://a.test/' || ?1, ?1 % 3 <> 0, ?2, NULL, 1, 0, NULL)",
                (id, status),
            )
            .unwrap();
        }
        for i in 0..5_000i64 {
            let kind = ["anchor", "image", "canonical", "hreflang", "next"][i as usize % 5];
            conn.execute(
                "INSERT INTO links VALUES (?1, ?2, ?3, ?4 % 2, 'http://x', 'click here', ?4 % 3 = 0, 'en')",
                (i % 200 + 1, (i * 7) % 200 + 1, kind, i),
            )
            .unwrap();
        }
        conn.execute_batch("COMMIT").unwrap();
        let read = |min: i64| {
            page_flags_with(&conn, min).unwrap();
            let mut st = conn
                .prepare("SELECT url_id, flags FROM temp.page_flags ORDER BY 1")
                .unwrap();
            st.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)))
                .unwrap()
                .collect::<rusqlite::Result<Vec<_>>>()
                .unwrap()
        };
        let one = read(i64::MAX);
        assert!(one.len() > 100);
        assert_eq!(read(0), one);
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
