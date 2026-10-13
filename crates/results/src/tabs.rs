//! The result tabs, their columns and filters, defined once (plan D2).
//!
//! A tab is data: which URLs it lists (a SQL condition on `urls u`), its
//! columns (SQL expressions) and its filters (SQL conditions). The desktop
//! app and `spiderlens export` both read these definitions, so a view and
//! its export always list the same rows (FR-031).
//!
//! Every column and filter names the data it needs (plan D3). Data the crawl
//! file does not have yet is reported as unavailable instead of silently
//! empty; extraction-based expressions are `NULL` until spec 005 stores them.

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
}

impl Need {
    pub fn unavailable_reason(self) -> &'static str {
        match self {
            Self::Base => "",
            Self::Extraction => {
                "Page content (descriptions, headings, canonicals, directives, images) is not collected yet."
            }
            Self::Attempts => "Request attempts are not recorded in this crawl yet.",
            Self::Timing => "Response time and size are not recorded in this crawl yet.",
            Self::RawHtml => "This crawl does not store page HTML.",
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
    }
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
    pub const INDEXABILITY: ColumnDef =
        col("indexability", "Indexability", "NULL", ValueKind::Text, 130).needs(Need::Extraction);
    pub const TITLE: ColumnDef = col("title", "Title", "u.title", ValueKind::Text, 320);
    pub const META_DESCRIPTION: ColumnDef = col(
        "meta_description",
        "Meta description",
        "NULL",
        ValueKind::Text,
        320,
    )
    .needs(Need::Extraction);
    pub const H1: ColumnDef = col("h1", "H1", "NULL", ValueKind::Text, 260).needs(Need::Extraction);
    pub const WORD_COUNT: ColumnDef =
        col("word_count", "Word count", "NULL", ValueKind::Int, 100).needs(Need::Extraction);
    pub const SIZE: ColumnDef = col("size", "Size", "NULL", ValueKind::Bytes, 90).needs(Need::Timing);
    pub const RESPONSE_TIME: ColumnDef =
        col("response_time", "Response time", "NULL", ValueKind::Ms, 120).needs(Need::Timing);
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

/// Text tabs that wait for page extraction: All lists the eligible pages,
/// every other filter needs extracted content.
fn content_tab(
    id: &'static str,
    title: &'static str,
    columns: Vec<ColumnDef>,
    filters: &[(&'static str, &'static str)],
    search_column: &'static str,
) -> TabDef {
    let mut fs = vec![filter("all", "All", "1")];
    fs.extend(
        filters
            .iter()
            .map(|(fid, label)| filter(fid, label, "0").needs(Need::Extraction)),
    );
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

fn ext(id: &'static str, label: &'static str, kind: ValueKind, width: u16) -> ColumnDef {
    col(id, label, "NULL", kind, width).needs(Need::Extraction)
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
                ext("title_2", "Title (second)", Text, 260).hidden(),
                ext("h1", "H1", Text, 260).hidden(),
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
                filter("same_as_h1", "Same as H1", "0").needs(Need::Extraction),
                filter("multiple", "Multiple", "0").needs(Need::Extraction),
            ],
            search_column: Some("title"),
            scoped: false,
        },
        content_tab(
            "meta-description",
            "Meta description",
            vec![
                c::ADDRESS,
                ext("meta_description", "Meta description", Text, 360),
                ext("length", "Length", Int, 80),
                ext("occurrences", "Occurrences", Int, 110),
            ],
            &[
                ("missing", "Missing"),
                ("duplicate", "Duplicate"),
                ("over_155", "Over 155 characters"),
                ("below_70", "Below 70 characters"),
                ("multiple", "Multiple"),
            ],
            "meta_description",
        ),
        content_tab(
            "h1",
            "H1",
            vec![
                c::ADDRESS,
                ext("h1", "H1", Text, 300),
                ext("h1_length", "H1 length", Int, 90),
                ext("h1_2", "H1 (second)", Text, 260),
                ext("occurrences", "Occurrences", Int, 110),
            ],
            &[
                ("missing", "Missing"),
                ("duplicate", "Duplicate"),
                ("over_70", "Over 70 characters"),
                ("multiple", "Multiple"),
            ],
            "h1",
        ),
        content_tab(
            "h2",
            "H2",
            vec![
                c::ADDRESS,
                ext("h2", "H2", Text, 300),
                ext("h2_length", "H2 length", Int, 90),
                ext("h2_2", "H2 (second)", Text, 260),
                ext("occurrences", "Occurrences", Int, 110),
            ],
            &[
                ("missing", "Missing"),
                ("duplicate", "Duplicate"),
                ("over_70", "Over 70 characters"),
                ("multiple", "Multiple"),
            ],
            "h2",
        ),
        TabDef {
            id: "images",
            title: "Images",
            // Image URLs: found in img elements first, or answering with an image type.
            eligible: "(u.source = 'image' OR content_kind(u.content_type) = 'image') AND u.state IN ('crawled', 'not_crawled')",
            columns: vec![
                c::ADDRESS,
                ext("alt", "Alt text", Text, 280),
                ext("alt_length", "Alt length", Int, 90),
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
                filter("missing_alt", "Missing alt attribute", "0").needs(Need::Extraction),
                filter("empty_alt", "Empty alt text", "0").needs(Need::Extraction),
                filter("alt_over_100", "Alt text over 100 characters", "0").needs(Need::Extraction),
                filter("over_100kb", "Over 100 KB", "0").needs(Need::Timing),
            ],
            search_column: Some("alt"),
            scoped: false,
        },
        content_tab(
            "canonicals",
            "Canonicals",
            vec![
                c::ADDRESS,
                ext("canonical_html", "Canonical (HTML)", Url, 320),
                ext("canonical_http", "Canonical (HTTP header)", Url, 320),
                c::INDEXABILITY,
            ],
            &[
                ("contains", "Contains canonical"),
                ("self", "Self-referencing"),
                ("canonicalised", "Canonicalised"),
                ("missing", "Missing"),
                ("conflicting", "Multiple or conflicting"),
            ],
            "canonical_html",
        ),
        content_tab(
            "directives",
            "Directives",
            vec![
                c::ADDRESS,
                ext("meta_robots", "Meta robots", Text, 220),
                ext("x_robots_tag", "X-Robots-Tag", Text, 220),
                c::INDEXABILITY,
            ],
            &[
                ("index", "Index"),
                ("noindex", "Noindex"),
                ("follow", "Follow"),
                ("nofollow", "Nofollow"),
                ("none", "None"),
                ("noarchive", "Noarchive"),
                ("nosnippet", "Nosnippet"),
                ("max_preview", "Max-snippet / max-image-preview / max-video-preview"),
                ("unavailable_after", "Unavailable_after"),
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
    }
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
