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
    /// `temp.link_stats(url_id, flags)`: per linking page, which kinds of
    /// links it has ([`link_flag`] bits), from one pass over `links`
    /// (spec 009).
    LinkStats,
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
    static TABS: std::sync::LazyLock<Vec<TabDef>> = std::sync::LazyLock::new(build);
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
        Derived::LinkStats => link_stats(conn),
        // Built by the reader or export, which know the issue settings.
        Derived::IssueHits => Ok(()),
    }
}

/// Bits of `temp.link_stats.flags`.
pub(crate) mod link_flag {
    /// An anchor to an internal URL that failed or returned 4xx/5xx.
    pub const BROKEN_INTERNAL: u8 = 1;
    /// An anchor to an external URL that failed or returned 4xx/5xx.
    pub const BROKEN_EXTERNAL: u8 = 2;
    /// An anchor to an internal 3xx.
    pub const TO_REDIRECT: u8 = 4;
    /// A nofollow anchor to an internal URL.
    pub const NOFOLLOW_INTERNAL: u8 = 8;
    /// Any anchor to an internal URL.
    pub const INTERNAL_OUT: u8 = 16;
    /// An image whose address is http:.
    pub const HTTP_IMAGE: u8 = 32;
}

/// Builds `temp.link_stats` in Rust: one sequential scan of `links` with
/// each target's class looked up in an array by URL id. A SQL join probes
/// the urls b-tree once per link, which on 10 million links takes tens of
/// seconds; this takes a few (spec 009 SC-003).
fn link_stats(conn: &rusqlite::Connection) -> rusqlite::Result<()> {
    use link_flag::*;
    // Target classes, by URL id: bit 0 internal, 1 broken, 2 redirect, 3 http.
    let max_id: i64 = conn.query_row("SELECT COALESCE(MAX(id), 0) FROM urls", [], |r| r.get(0))?;
    let len = usize::try_from(max_id).unwrap_or(0) + 1;
    let mut class = vec![0u8; len];
    {
        let mut stmt = conn.prepare(
            "SELECT id, (internal = 1)
               | (COALESCE(error_class IS NOT NULL OR status >= 400, 0) << 1)
               | (COALESCE(error_class IS NULL AND status BETWEEN 300 AND 399, 0) << 2)
               | ((url LIKE 'http:%') << 3)
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
    let mut flags = vec![0u8; len];
    {
        // kind: 1 anchor, 2 image. An image without a URL row is judged by
        // its written address.
        let mut stmt = conn.prepare(
            "SELECT from_id, to_id, CASE kind WHEN 'anchor' THEN 1 WHEN 'image' THEN 2 ELSE 0 END, nofollow,
               to_id IS NULL AND kind = 'image' AND target LIKE 'http:%'
             FROM links NOT INDEXED WHERE kind IN ('anchor', 'image')",
        )?;
        let mut rows = stmt.query([])?;
        while let Some(r) = rows.next()? {
            let Some(from) = usize::try_from(r.get::<_, i64>(0)?).ok().filter(|&i| i < len) else {
                continue;
            };
            let to: Option<i64> = r.get(1)?;
            let t = to
                .and_then(|t| usize::try_from(t).ok())
                .and_then(|t| class.get(t).copied());
            let kind: i64 = r.get(2)?;
            let mut f = 0u8;
            match (kind, t) {
                (1, Some(c)) => {
                    let internal = c & 1 != 0;
                    if c & 2 != 0 {
                        f |= if internal {
                            BROKEN_INTERNAL
                        } else {
                            BROKEN_EXTERNAL
                        };
                    }
                    if internal {
                        f |= INTERNAL_OUT;
                        if c & 4 != 0 {
                            f |= TO_REDIRECT;
                        }
                        if r.get::<_, i64>(3)? == 1 {
                            f |= NOFOLLOW_INTERNAL;
                        }
                    }
                }
                (2, Some(c)) if c & 8 != 0 => f |= HTTP_IMAGE,
                (2, None) if r.get::<_, bool>(4)? => f |= HTTP_IMAGE,
                _ => {}
            }
            flags[from] |= f;
        }
    }
    conn.execute_batch(
        "DROP TABLE IF EXISTS temp.link_stats;
         CREATE TEMP TABLE link_stats(url_id INTEGER PRIMARY KEY, flags INTEGER NOT NULL);
         SAVEPOINT link_stats;",
    )?;
    {
        let mut ins = conn.prepare("INSERT INTO temp.link_stats VALUES (?1, ?2)")?;
        for (id, f) in flags.iter().enumerate().filter(|(_, f)| **f != 0) {
            ins.execute((id as i64, *f))?;
        }
    }
    conn.execute_batch("RELEASE link_stats;")
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
