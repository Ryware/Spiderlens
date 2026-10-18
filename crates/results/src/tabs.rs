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

use std::borrow::Cow;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::values::Values;

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
    /// The crawl rendered pages (spec 014): rendered values, the
    /// JavaScript tab.
    Rendering,
    /// The rendered HTML is stored (spec 014 FR-032).
    RenderedHtml,
    /// Screenshots are stored (spec 014 FR-033).
    Screenshots,
    /// Forms, actions and agent links (spec 016).
    Agents,
    /// AMP checks (spec 027 part E).
    Amp,
    /// What was read from PDFs (spec 027 part C).
    Pdf,
    /// Sentence and syllable counts for the readability score (spec 009
    /// FR-031).
    Readability,
    /// Search Console page data fetched after the crawl (spec 024).
    SearchConsole,
    /// Every sitemap file and entry the crawl read (spec 011).
    SitemapEntries,
    /// The `sitemap_audit` pass's problems (spec 011 part b), written once
    /// the crawl has ended.
    SitemapAudit,
    /// The `orphans` pass's classes (spec 011 part b).
    Orphans,
    /// Crawl Analysis results such as Link Score (spec 023), calculated
    /// once fetching has ended.
    Analysis,
    /// The cookie audit (spec 025).
    Cookies,
    /// AI search check results (spec 015).
    AiChecks,
    /// The spelling pass's results (spec 027 part D).
    Spelling,
    /// Main content signatures for near duplicate analysis (spec 020).
    NearDup,
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
            Self::Rendering => "This crawl was not rendered.",
            Self::RenderedHtml => "This crawl does not store rendered HTML.",
            Self::Screenshots => "This crawl did not take screenshots.",
            Self::Agents => {
                "Forms and actions were not collected in this crawl file; it was made before SpiderLens checked agent readiness."
            }
            Self::Amp => "This crawl did not check AMP. Turn on Check AMP in the crawl settings to check it.",
            Self::Pdf => {
                "This crawl did not read PDFs. Turn on Crawl PDFs in the crawl settings to read them."
            }
            Self::Readability => {
                "Readability was not measured in this crawl file; it was made before SpiderLens scored readability."
            }
            Self::SearchConsole => {
                "No Search Console data in this crawl yet. Fetch it from Integrations after connecting your Google account."
            }
            Self::SitemapEntries => {
                "Sitemap entries were not recorded in this crawl file; it was made before SpiderLens kept them."
            }
            Self::SitemapAudit => "Checked when the crawl ends.",
            Self::Orphans => {
                "Found when the crawl ends. Orphan pages need a Spider mode crawl that read sitemaps."
            }
            Self::Analysis => "Calculated when the crawl ends.",
            Self::Cookies => "This crawl was made before SpiderLens recorded the cookies each page sets.",
            Self::AiChecks => "AI checks have not run on this crawl. Run them from the AI search tab.",
            Self::Spelling => "Spelling has not been checked for this crawl. Run Check spelling to check it.",
            Self::NearDup => {
                "Near duplicate data was not collected in this crawl file; it was made before SpiderLens compared page content."
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
            Self::Rendering => "rendering",
            Self::RenderedHtml => "rendered_html",
            Self::Screenshots => "screenshots",
            Self::Agents => "agents",
            Self::Amp => "amp",
            Self::Pdf => "pdf",
            Self::Readability => "readability",
            Self::SearchConsole => "search_console",
            Self::SitemapEntries => "sitemap_entries",
            Self::SitemapAudit => "sitemap_audit",
            Self::Orphans => "orphans",
            Self::Analysis => "analysis",
            Self::Cookies => "cookies",
            Self::AiChecks => "ai_checks",
            Self::Spelling => "spelling",
            Self::NearDup => "near_dup",
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
    /// A share, 0-100 (also Search Console CTR).
    Percent,
    /// A number with one decimal (average position).
    Decimal,
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
    /// `link_metrics`: Link Score and link counts (spec 023), built in
    /// `temp` when the file holds no current copy.
    LinkMetrics,
    /// `temp.viz_crawl_tree`: each URL's crawl tree parent (spec 022).
    VizCrawlTree,
    /// `temp.viz_dirs` and `temp.viz_dir_urls`: the directory tree.
    VizDirs,
    /// `link_positions`: where on its page each body link sits (spec 023
    /// US4), from the stored HTML.
    LinkPositions,
    /// `temp.gone_to(to_id, n)`: raw links to each URL that are gone after
    /// rendering (spec 014). Read by Inlinks in rendered mode.
    GoneTo,
}

impl Derived {
    pub fn table(self) -> &'static str {
        match self {
            Self::Titles => "titles",
            Self::Redirects => "redirects",
            Self::OccMeta => "occ_md",
            Self::OccH1 => "occ_h1",
            Self::OccH2 => "occ_h2",
            Self::OccText => "occ_text",
            Self::PageFlags => "page_flags",
            Self::IssueHits => "issue_hits",
            Self::LinkMetrics => "link_metrics",
            Self::LinkPositions => "link_positions",
            Self::VizCrawlTree => "viz_crawl_tree",
            Self::VizDirs => "viz_dirs",
            Self::GoneTo => "gone_to",
        }
    }

    /// The `urls u` condition that picks exactly the rows this table holds
    /// when it is a tab's row source.
    pub(crate) fn rows_eligible(self) -> &'static str {
        match self {
            Self::Titles => HTML_OK,
            Self::Redirects
            | Self::OccMeta
            | Self::OccH1
            | Self::OccH2
            | Self::OccText
            | Self::PageFlags
            | Self::IssueHits
            | Self::LinkMetrics
            | Self::LinkPositions
            | Self::VizCrawlTree
            | Self::VizDirs
            | Self::GoneTo => "1",
        }
    }
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct ColumnDef {
    #[ts(type = "string")]
    pub id: Cow<'static, str>,
    #[ts(type = "string")]
    pub label: Cow<'static, str>,
    #[serde(skip)]
    #[ts(skip)]
    pub expr: Cow<'static, str>,
    pub kind: ValueKind,
    pub width: u16,
    pub default_visible: bool,
    pub need: Need,
    #[serde(skip)]
    #[ts(skip)]
    pub derived: Option<Derived>,
    /// The expression in rendered mode, when `expr` reads a table the
    /// rendered values do not shadow (links, page text, directives; spec
    /// 014 research R11 fallback).
    #[serde(skip)]
    #[ts(skip)]
    pub rendered: Option<&'static str>,
    /// The derived table `rendered` reads, when it differs from `derived`.
    #[serde(skip)]
    #[ts(skip)]
    pub rendered_derived: Option<Derived>,
    /// Sorted by this before the value (custom columns: numbers first,
    /// numerically).
    #[serde(skip)]
    #[ts(skip)]
    pub sort_key: Option<Cow<'static, str>>,
    /// A stored `natkey(expr)` to order by instead of computing it per row.
    #[serde(skip)]
    #[ts(skip)]
    pub key: Option<&'static str>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct FilterDef {
    #[ts(type = "string")]
    pub id: Cow<'static, str>,
    #[ts(type = "string")]
    pub label: Cow<'static, str>,
    #[serde(skip)]
    #[ts(skip)]
    pub cond: Cow<'static, str>,
    pub need: Need,
    #[serde(skip)]
    #[ts(skip)]
    pub derived: Option<Derived>,
    /// The condition in rendered mode, see [`ColumnDef::rendered`].
    #[serde(skip)]
    #[ts(skip)]
    pub rendered: Option<&'static str>,
    /// The issue rule whose threshold this filter uses, and its label with
    /// `{n}` for the value (spec 009 FR-005, US4 scenario 1).
    #[serde(skip)]
    #[ts(skip)]
    pub threshold: Option<(&'static str, &'static str)>,
    /// Shown only while it matches something (or is the active filter), so
    /// a long list of rarely used filters does not crowd the strip.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub hide_when_empty: Option<bool>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct TabDef {
    #[ts(type = "string")]
    pub id: Cow<'static, str>,
    #[ts(type = "string")]
    pub title: Cow<'static, str>,
    #[serde(skip)]
    #[ts(skip)]
    pub eligible: Cow<'static, str>,
    pub columns: Vec<ColumnDef>,
    pub filters: Vec<FilterDef>,
    /// Searched besides Address (FR-016).
    #[ts(type = "string | null")]
    pub search_column: Option<Cow<'static, str>>,
    /// Offers the internal / external switch (Response codes).
    pub scoped: bool,
    /// The tab's rows are the rows of this derived table, `x`, joined to
    /// `urls u` (its `eligible` is then already applied).
    #[serde(skip)]
    #[ts(skip)]
    pub rows: Option<Derived>,
}

impl ColumnDef {
    /// The SQL expression for these values.
    pub(crate) fn sql(&self, values: Values) -> &str {
        match (values, self.rendered) {
            (Values::Rendered, Some(r)) => r,
            _ => &self.expr,
        }
    }
}

impl FilterDef {
    /// The SQL condition for these values.
    pub(crate) fn sql(&self, values: Values) -> &str {
        match (values, self.rendered) {
            (Values::Rendered, Some(r)) => r,
            _ => &self.cond,
        }
    }
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
    pub fn eligible_on_urls(&self) -> &str {
        match self.rows {
            Some(d) => d.rows_eligible(),
            None => &self.eligible,
        }
    }

    /// The condition on `urls` for the rows the All view lists: the
    /// eligible rows, narrowed when the All filter is narrower (the
    /// Sitemaps tab also holds pages missing from every sitemap).
    pub fn all_on_urls(&self) -> Cow<'_, str> {
        let eligible = self.eligible_on_urls();
        match self.filter("all").map(|f| f.cond.as_ref()) {
            Some(all) if self.rows.is_none() && all != "1" => Cow::Owned(format!("({eligible}) AND ({all})")),
            _ => Cow::Borrowed(eligible),
        }
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
macro_rules! md_expr {
    () => {
        "(SELECT p.meta_description FROM pages p WHERE p.url_id = u.id)"
    };
}
const MD: &str = md_expr!();

/// Width of a title or description as a search result draws it (spec 027
/// part A). Limits are the spec's defaults (FR-A03): title 600 desktop,
/// 580 mobile, short under 200; description 990 desktop, 680 mobile.
macro_rules! px {
    ($expr:expr, $style:literal) => {
        concat!("px_width(", $expr, ", '", $style, "')")
    };
}
const TITLE_PX: &str = px!("u.title", "title");
const DESC_PX: &str = px!(md_expr!(), "desc");
const H1_1: &str = "(SELECT p.h1_1 FROM pages p WHERE p.url_id = u.id)";
/// The alt text of the first place an image is used. NULL when the
/// attribute is missing, `''` when it is empty.
const IMG_ALT: &str =
    "(SELECT l.alt FROM links l WHERE l.to_id = u.id AND l.kind = 'image' ORDER BY l.rowid LIMIT 1)";

/// The page has a rendered version (spec 014): its rendered values replace
/// the raw ones in rendered mode. Only in rendered expressions, where `u`
/// is the rendered `urls` view, which shares one `rendered_pages` lookup
/// per row between this flag and the rendered fields.
macro_rules! rendered_page {
    () => {
        "u.rendered_row = 1"
    };
}

/// A raw link row of `l` that is still in its page after rendering.
macro_rules! live_raw {
    () => {
        "NOT EXISTS (SELECT 1 FROM main.raw_only_links g WHERE g.from_id = l.from_id AND g.pos = l.pos)"
    };
}

// Link counts in rendered mode: the raw links, minus the raw links gone
// after rendering, plus the links found only after rendering. The raw and
// rendered parts are index-only counts; the gone part is a lookup in a
// small table (`temp.gone_to`) or a count of `raw_only_links` by page
// (every raw reference has a link row at its position, so each
// `raw_only_links` row is one link). Filtering
// the raw links with `live_raw!` instead read every link row and made
// sorting by Inlinks 20 times slower on 100,000 pages; a UNION view of
// both link tables is worse still (T030, benchmarks.md, research R11).
// A delta table holding the rendered links too was faster to read but
// grows with `rendered_links`, which on a script-only site holds every
// link, and is rebuilt at each refresh of a running crawl.
const INLINKS_RENDERED: &str = "((SELECT COUNT(*) FROM links l WHERE l.to_id = u.id) \
     - COALESCE((SELECT g.n FROM temp.gone_to g WHERE g.to_id = u.id), 0) \
     + (SELECT COUNT(*) FROM rendered_links l WHERE l.to_id = u.id))";
const OUTLINKS_RENDERED: &str = "((SELECT COUNT(*) FROM links l WHERE l.from_id = u.id) \
     - (SELECT COUNT(*) FROM main.raw_only_links g WHERE g.from_id = u.id) \
     + (SELECT COUNT(*) FROM rendered_links l WHERE l.from_id = u.id))";

const IMG_ALT_RENDERED: &str = concat!(
    "(SELECT a FROM (SELECT l.alt AS a, l.rowid AS k FROM links l WHERE l.to_id = u.id AND l.kind = 'image' AND ",
    live_raw!(),
    " UNION ALL SELECT l.alt, 1099511627776 + l.rowid FROM rendered_links l \
     WHERE l.to_id = u.id AND l.kind = 'image') ORDER BY k LIMIT 1)"
);

const IMG_ALT_LENGTH_RENDERED: &str = concat!(
    "length((SELECT a FROM (SELECT l.alt AS a, l.rowid AS k FROM links l WHERE l.to_id = u.id AND l.kind = 'image' AND ",
    live_raw!(),
    " UNION ALL SELECT l.alt, 1099511627776 + l.rowid FROM rendered_links l \
     WHERE l.to_id = u.id AND l.kind = 'image') ORDER BY k LIMIT 1))"
);

pub(crate) const fn col(
    id: &'static str,
    label: &'static str,
    expr: &'static str,
    kind: ValueKind,
    width: u16,
) -> ColumnDef {
    ColumnDef {
        id: Cow::Borrowed(id),
        label: Cow::Borrowed(label),
        expr: Cow::Borrowed(expr),
        kind,
        width,
        default_visible: true,
        need: Need::Base,
        derived: None,
        rendered: None,
        rendered_derived: None,
        sort_key: None,
        key: None,
    }
}

impl ColumnDef {
    const fn with_rendered(mut self, expr: &'static str) -> Self {
        self.rendered = Some(expr);
        self
    }
    const fn with_rendered_reads(mut self, expr: &'static str, d: Derived) -> Self {
        self.rendered = Some(expr);
        self.rendered_derived = Some(d);
        self
    }
    /// The derived table the column reads for `values`.
    pub(crate) fn derived_in(&self, values: Values) -> Option<Derived> {
        match (values, self.rendered_derived) {
            (Values::Rendered, Some(d)) => Some(d),
            _ => self.derived,
        }
    }
    pub(crate) const fn hidden(mut self) -> Self {
        self.default_visible = false;
        self
    }
    pub(crate) const fn needs(mut self, need: Need) -> Self {
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

/// A fact read from a PDF (spec 027 part C).
macro_rules! pdf_fact {
    ($col:literal) => {
        concat!("(SELECT p.", $col, " FROM pdfs p WHERE p.url_id = u.id)")
    };
}

const fn pdf_col(
    id: &'static str,
    label: &'static str,
    expr: &'static str,
    kind: ValueKind,
    width: u16,
) -> ColumnDef {
    col(id, label, expr, kind, width).needs(Need::Pdf)
}

const fn pdf_filter(id: &'static str, label: &'static str, cond: &'static str) -> FilterDef {
    filter(id, label, cond).needs(Need::Pdf)
}

/// A spelling result for the page (spec 027 part D).
macro_rules! spell_fact {
    ($col:literal) => {
        concat!("(SELECT s.", $col, " FROM spell_pages s WHERE s.url_id = u.id)")
    };
}

/// Pages with at least one finding matching `cond` (on `f`).
macro_rules! has_finding {
    ($cond:literal) => {
        concat!(
            "EXISTS (SELECT 1 FROM spell_findings f WHERE f.url_id = u.id AND ",
            $cond,
            ")"
        )
    };
}

const fn spell_col(
    id: &'static str,
    label: &'static str,
    expr: &'static str,
    kind: ValueKind,
    width: u16,
) -> ColumnDef {
    col(id, label, expr, kind, width).needs(Need::Spelling)
}

const fn spell_filter(id: &'static str, label: &'static str, cond: &'static str) -> FilterDef {
    filter(id, label, cond).needs(Need::Spelling)
}

/// AMP pages whose `check` failed (spec 027 part E).
macro_rules! amp_failed {
    ($check:literal) => {
        concat!(
            "EXISTS (SELECT 1 FROM amp_checks a WHERE a.url_id = u.id AND a.check_id = '",
            $check,
            "')"
        )
    };
}

/// An AMP page whose canonical page has no `amphtml` link back to it.
const AMP_UNPAIRED: &str = "EXISTS (SELECT 1 FROM amp_pages ap WHERE ap.url_id = u.id AND ap.format = 'website') \
     AND u.canonical IS NOT NULL AND u.canonical <> u.url \
     AND NOT EXISTS (SELECT 1 FROM urls c JOIN links l ON l.from_id = c.id \
         WHERE c.url = u.canonical AND l.kind = 'amphtml' AND l.to_id = u.id)";

/// Reached through an `amphtml` link but not marked as AMP.
const AMP_UNMARKED: &str = "NOT EXISTS (SELECT 1 FROM amp_pages ap WHERE ap.url_id = u.id) \
     AND EXISTS (SELECT 1 FROM links l WHERE l.to_id = u.id AND l.kind = 'amphtml')";

const fn amp_col(
    id: &'static str,
    label: &'static str,
    expr: &'static str,
    kind: ValueKind,
    width: u16,
) -> ColumnDef {
    col(id, label, expr, kind, width).needs(Need::Amp)
}

const fn amp_filter(id: &'static str, label: &'static str, cond: &'static str) -> FilterDef {
    filter(id, label, cond).needs(Need::Amp)
}

pub(crate) const fn filter(id: &'static str, label: &'static str, cond: &'static str) -> FilterDef {
    FilterDef {
        id: Cow::Borrowed(id),
        label: Cow::Borrowed(label),
        cond: Cow::Borrowed(cond),
        need: Need::Base,
        derived: None,
        rendered: None,
        threshold: None,
        hide_when_empty: None,
    }
}

/// A Links tab filter on the spec 023 metrics.
const fn link_filter(id: &'static str, label: &'static str, cond: &'static str) -> FilterDef {
    filter(id, label, cond)
        .needs(Need::Analysis)
        .reads(Derived::LinkMetrics)
}

impl FilterDef {
    const fn with_rendered(mut self, cond: &'static str) -> Self {
        self.rendered = Some(cond);
        self
    }
    /// The label with the threshold in force.
    pub fn label_with(&self, values: &crate::sql::Thresholds) -> String {
        match self.threshold {
            Some((rule, template)) => values.get(rule).map_or_else(
                || self.label.to_string(),
                |v| {
                    let mut label = template.replace("{n}", &crate::issues::fmt_num(*v));
                    // A second value, for labels with two (spec 014 FR-027).
                    if let Some(w) = values.get(format!("{rule}.2").as_str()) {
                        label = label.replace("{w}", &crate::issues::fmt_num(*w));
                    }
                    label
                },
            ),
            None => self.label.to_string(),
        }
    }
    const fn thr(mut self, rule: &'static str, template: &'static str) -> Self {
        self.threshold = Some((rule, template));
        self
    }
    const fn when_present(mut self) -> Self {
        self.hide_when_empty = Some(true);
        self
    }
    pub(crate) const fn needs(mut self, need: Need) -> Self {
        self.need = need;
        self
    }
    const fn reads(mut self, d: Derived) -> Self {
        self.derived = Some(d);
        self
    }
}

/// Search Console page data (spec 024), one primary-key lookup each.
const GSC_CLICKS: &str = "(SELECT g.clicks FROM gsc_pages g WHERE g.url_id = u.id)";
const GSC_IMPRESSIONS: &str = "(SELECT g.impressions FROM gsc_pages g WHERE g.url_id = u.id)";
const GSC_CTR: &str = "(SELECT g.ctr * 100 FROM gsc_pages g WHERE g.url_id = u.id)";
const GSC_POSITION: &str = "(SELECT g.position FROM gsc_pages g WHERE g.url_id = u.id AND g.impressions > 0)";
const GSC_DATA: &str = "(CASE WHEN EXISTS (SELECT 1 FROM integration_status s WHERE s.url_id = u.id \
     AND s.provider = 'search_console' AND s.reason = 'no_data') THEN 'No data from Google' \
     WHEN EXISTS (SELECT 1 FROM gsc_pages g WHERE g.url_id = u.id) THEN 'Has data' ELSE 'Not fetched' END)";

/// Columns many tabs share.
pub mod c {
    use super::*;

    pub const ADDRESS: ColumnDef = col("address", "Address", "u.url", ValueKind::Url, 420);
    /// Spec 027 FR-B03: the URL's segment, in every tab.
    pub const SEGMENT: ColumnDef = col(
        "segment",
        "Segment",
        crate::segments::SEGMENT_EXPR,
        ValueKind::Text,
        140,
    );
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
    pub const GSC_CLICKS: ColumnDef = col(
        "gsc_clicks",
        "Clicks (Search Console)",
        super::GSC_CLICKS,
        ValueKind::Int,
        120,
    )
    .needs(Need::SearchConsole);
    pub const GSC_IMPRESSIONS: ColumnDef = col(
        "gsc_impressions",
        "Impressions (Search Console)",
        super::GSC_IMPRESSIONS,
        ValueKind::Int,
        140,
    )
    .needs(Need::SearchConsole);
    pub const GSC_CTR: ColumnDef = col(
        "gsc_ctr",
        "CTR (Search Console)",
        super::GSC_CTR,
        ValueKind::Percent,
        110,
    )
    .needs(Need::SearchConsole);
    pub const GSC_POSITION: ColumnDef = col(
        "gsc_position",
        "Avg. position (Search Console)",
        super::GSC_POSITION,
        ValueKind::Decimal,
        120,
    )
    .needs(Need::SearchConsole);
    pub const GSC_DATA: ColumnDef = col(
        "gsc_data",
        "Search Console data",
        super::GSC_DATA,
        ValueKind::Text,
        150,
    )
    .needs(Need::SearchConsole);
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
    /// Flesch reading ease, 0 to 100 (spec 009 FR-031).
    pub const READABILITY: ColumnDef = col(
        "readability",
        "Readability",
        concat!(
            "CAST(round(",
            crate::issues::rules::content::readability!(),
            ") AS INTEGER)"
        ),
        ValueKind::Int,
        100,
    )
    .hidden()
    .needs(Need::Readability);
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
    )
    .with_rendered_reads(INLINKS_RENDERED, Derived::GoneTo);
    pub const OUTLINKS: ColumnDef = col(
        "outlinks",
        "Outlinks",
        "(SELECT COUNT(*) FROM links l WHERE l.from_id = u.id)",
        ValueKind::Int,
        80,
    )
    .with_rendered(OUTLINKS_RENDERED);
    /// Whether a row shows rendered or raw values (spec 014 FR-021).
    pub const VALUES: ColumnDef = col("values", "Values", "'Raw'", ValueKind::Text, 90)
        .hidden()
        .with_rendered(concat!(
            "CASE WHEN ",
            rendered_page!(),
            " THEN 'Rendered' ELSE 'Raw' END"
        ));
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
    let mut columns = columns;
    columns.push(c::VALUES);
    TabDef {
        id: Cow::Borrowed(id),
        title: Cow::Borrowed(title),
        eligible: Cow::Borrowed(HTML_OK),
        columns,
        filters: fs,
        search_column: Some(Cow::Borrowed(search_column)),
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

/// [`directive!`] in rendered mode: the rendered page's meta directives,
/// with the header ones, which rendering does not change (FR-019).
macro_rules! directive_rendered {
    ($names:literal) => {
        concat!(
            "(EXISTS (SELECT 1 FROM directives d WHERE d.url_id = u.id AND (d.source = 'header' OR NOT ",
            rendered_page!(),
            ") AND (d.crawler = 'all' OR d.crawler = robots_token()) AND d.directive IN (",
            $names,
            ")) OR EXISTS (SELECT 1 FROM rendered_directives d WHERE d.url_id = u.id AND d.origin <> 'raw' \
             AND (d.crawler = 'all' OR d.crawler = robots_token()) AND d.directive IN (",
            $names,
            ")))"
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

/// The rendered page's meta directives as written, else the raw ones.
const META_DIRECTIVES_RENDERED: &str = concat!(
    "CASE WHEN ",
    rendered_page!(),
    " THEN (SELECT group_concat(v, ', ') FROM (SELECT CASE WHEN d.crawler = 'all' THEN '' ELSE d.crawler || ': ' END \
     || d.directive || COALESCE(':' || d.value, '') AS v FROM rendered_directives d \
     WHERE d.url_id = u.id AND d.origin <> 'raw' ORDER BY d.pos)) ELSE ",
    directives_from!("meta"),
    " END"
);

/// [`canonical_from!`] in rendered mode: raw canonicals still there, then
/// those found only after rendering.
macro_rules! canonical_from_rendered {
    ($source:literal) => {
        concat!(
            "(SELECT x FROM (SELECT COALESCE(t.url, l.target) AS x, l.pos AS p FROM links l \
             LEFT JOIN urls t ON t.id = l.to_id WHERE l.from_id = u.id AND l.kind = 'canonical' AND l.source = '",
            $source,
            "' AND ",
            live_raw!(),
            " UNION ALL SELECT COALESCE(t.url, l.target), l.pos FROM rendered_links l \
             LEFT JOIN urls t ON t.id = l.to_id WHERE l.from_id = u.id AND l.kind = 'canonical' AND l.source = '",
            $source,
            "') ORDER BY p LIMIT 1)"
        )
    };
}

/// [`image_use!`] in rendered mode.
macro_rules! image_use_rendered {
    ($cond:literal) => {
        concat!(
            "(EXISTS (SELECT 1 FROM links l WHERE l.to_id = u.id AND l.kind = 'image' AND ",
            live_raw!(),
            " AND ",
            $cond,
            ") OR EXISTS (SELECT 1 FROM rendered_links l WHERE l.to_id = u.id AND l.kind = 'image' AND ",
            $cond,
            "))"
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

/// URLs with at least one issue under the settings in force.
const WITH_ISSUES: FilterDef = filter(
    "with_issues",
    "With issues",
    "EXISTS (SELECT 1 FROM temp.issue_hits h WHERE h.url_id = u.id)",
)
.reads(Derived::IssueHits);
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
        id: Cow::Borrowed("ai-search"),
        title: Cow::Borrowed("AI search"),
        eligible: Cow::Borrowed(HTML_OK),
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
        search_column: Some(Cow::Borrowed("author")),
        scoped: false,
        rows: None,
    }
}

/// Every tab in display order (FR-007, FR-009).
pub fn tabs() -> &'static [TabDef] {
    static TABS: std::sync::LazyLock<Vec<TabDef>> = std::sync::LazyLock::new(|| {
        let mut tabs = build();
        // Every tab can show each URL's issues and keep the URLs that have
        // some (spec 009 FR-018).
        for t in &mut tabs {
            t.columns.push(c::ISSUES);
            t.filters.push(WITH_ISSUES);
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

/// The 007 tab that shows what a category's rules test.
fn home_tab(c: crate::issues::Category) -> Option<&'static str> {
    use crate::issues::Category::*;
    Some(match c {
        ResponseCodes => "response-codes",
        PageTitles => "page-titles",
        MetaDescription => "meta-description",
        H1 => "h1",
        H2 => "h2",
        Images => "images",
        Canonicals => "canonicals",
        Directives => "directives",
        Url => "url",
        Sitemaps => "sitemaps",
        _ => return None,
    })
}

fn build_issue_tabs() -> Vec<TabDef> {
    crate::issues::catalogue()
        .iter()
        .enumerate()
        .map(|(i, r)| {
            let mut evidence = col("evidence", "Evidence", r.evidence, ValueKind::Text, 320).needs(r.need);
            evidence.derived = r.derived.first().copied();
            let mut all = filter("all", "All", "1").needs(r.need);
            all.derived = Some(Derived::IssueHits);
            let mut columns = vec![c::ADDRESS, evidence];
            // The columns of the rule's own 007 tab, such as the title and
            // its length for title rules (FR-015), then the common ones.
            if let Some(home) = home_tab(r.category).and_then(|id| tabs().iter().find(|t| t.id == id)) {
                // Issue tabs list `urls u` only, so a column that reads the
                // home tab's row table `x` looks its row up instead.
                columns.extend(
                    home.columns
                        .iter()
                        .filter(|c| c.id != "address" && c.id != "issues")
                        .map(|c| match home.rows {
                            Some(d) => ColumnDef {
                                expr: if c.expr.contains("x.") {
                                    Cow::Owned(format!(
                                        "(SELECT {} FROM temp.{} x WHERE x.id = u.id)",
                                        c.expr,
                                        d.table()
                                    ))
                                } else {
                                    c.expr.clone()
                                },
                                derived: if c.expr.contains("x.") { Some(d) } else { c.derived },
                                key: None,
                                ..c.clone()
                            },
                            None => c.clone(),
                        }),
                );
            }
            let mut common = vec![c::STATUS_CODE];
            if r.scope == crate::issues::Scope::Pages {
                common.extend([c::TITLE, c::INDEXABILITY]);
            } else {
                common.extend([c::CONTENT_TYPE, c::FINAL_STATE]);
            }
            common.extend([c::INLINKS, c::DEPTH.hidden(), c::FOUND_ON]);
            for col in common {
                if !columns.iter().any(|c| c.id == col.id) {
                    columns.push(col);
                }
            }
            TabDef {
                id: Cow::Owned(format!("issue:{}", r.id)),
                title: Cow::Borrowed(r.name),
                eligible: Cow::Owned(format!(
                    "u.id IN (SELECT h.url_id FROM temp.issue_hits h WHERE h.rule = {i})"
                )),
                columns,
                filters: vec![all],
                search_column: Some(Cow::Borrowed("evidence")),
                scoped: false,
                rows: None,
            }
        })
        .collect()
}

/// A fixed tab, or one of a file's custom tabs (spec 013).
pub(crate) fn find(id: &str, custom: &[TabDef]) -> Option<Cow<'static, TabDef>> {
    tab(id)
        .map(Cow::Borrowed)
        .or_else(|| custom.iter().find(|t| t.id == id).cloned().map(Cow::Owned))
}

fn build() -> Vec<TabDef> {
    let mut tabs = build_tabs();
    for t in &mut tabs {
        t.columns.push(c::SEGMENT.hidden());
    }
    tabs
}

fn build_tabs() -> Vec<TabDef> {
    use ValueKind::*;
    // Filters that test what an issue rule tests share its condition
    // (spec 009 FR-005).
    use crate::issues::rules::{canonicals as ca, headings as hd, images as im, meta as me, url as ur};
    vec![
        TabDef {
            id: Cow::Borrowed("internal"),
            title: Cow::Borrowed("Internal"),
            eligible: Cow::Borrowed(INTERNAL),
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
                c::READABILITY,
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
                c::VALUES,
                c::GSC_CLICKS.hidden(),
                c::GSC_IMPRESSIONS.hidden(),
                c::GSC_CTR.hidden(),
                c::GSC_POSITION.hidden(),
            ],
            filters: TYPE_FILTERS.to_vec(),
            search_column: Some(Cow::Borrowed("title")),
            scoped: false,
            rows: None,
        },
        TabDef {
            id: Cow::Borrowed("links"),
            title: Cow::Borrowed("Links"),
            eligible: Cow::Borrowed(HTML_OK),
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
                // Positions 1, 2 and 5: navigation, header, footer (US4).
                filter(
                    "boilerplate_only",
                    "Linked only from navigation, header or footer",
                    "EXISTS (SELECT 1 FROM links l WHERE l.to_id = u.id AND l.kind = 'anchor' AND l.from_id <> u.id) \
                     AND NOT EXISTS (SELECT 1 FROM links l LEFT JOIN link_positions p \
                     ON p.from_id = l.from_id AND p.pos = l.pos WHERE l.to_id = u.id AND l.kind = 'anchor' \
                     AND l.from_id <> u.id AND COALESCE(p.position, 0) NOT IN (1, 2, 5))",
                )
                .needs(Need::Analysis)
                .reads(Derived::LinkPositions),
            ],
            search_column: None,
            scoped: false,
            rows: None,
        },
        TabDef {
            id: Cow::Borrowed("external"),
            title: Cow::Borrowed("External"),
            eligible: Cow::Borrowed(EXTERNAL),
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
            id: Cow::Borrowed("response-codes"),
            title: Cow::Borrowed("Response codes"),
            eligible: Cow::Borrowed(RESPONSES),
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
            id: Cow::Borrowed("url"),
            title: Cow::Borrowed("URL"),
            eligible: Cow::Borrowed(INTERNAL),
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
            rows: None,
        },
        TabDef {
            id: Cow::Borrowed("page-titles"),
            title: Cow::Borrowed("Page titles"),
            // temp.titles holds exactly the HTML_OK rows.
            eligible: Cow::Borrowed("1"),
            columns: vec![
                c::ADDRESS,
                c::TITLE.sorted_by("x.k"),
                col("title_length", "Title length", "x.len", Int, 100),
                col("occurrences", "Occurrences", "x.n", Int, 110),
                col("title_px", "Title pixels", TITLE_PX, Int, 100),
                col("title_cut_desktop", "Cut on desktop", concat!("(", px!("u.title", "title"), " > 600)"), Bool, 120),
                col("title_cut_mobile", "Cut on mobile", concat!("(", px!("u.title", "title"), " > 580)"), Bool, 120),
                ext(
                    "title_2",
                    "Title (second)",
                    "(SELECT t.text FROM page_text t WHERE t.url_id = u.id AND t.kind = 'title' AND t.pos = 1)",
                    Text,
                    260,
                )
                .with_rendered(concat!(
                    "CASE WHEN ",
                    rendered_page!(),
                    " THEN (SELECT t.text FROM rendered_text t WHERE t.url_id = u.id AND t.kind = 'title' \
                     AND t.pos = 1 AND t.origin <> 'raw') \
                     ELSE (SELECT t.text FROM page_text t WHERE t.url_id = u.id AND t.kind = 'title' AND t.pos = 1) END"
                ))
                .hidden(),
                c::H1.hidden(),
                c::INDEXABILITY.hidden(),
                c::VALUES,
            ],
            filters: vec![
                filter("all", "All", "1"),
                filter("missing", "Missing", "COALESCE(x.t, '') = ''"),
                filter("duplicate", "Duplicate", "x.n > 1"),
                filter("over_60", "Over 60 characters", "x.len > thr('title.over')").thr("title.over", "Over {n} characters"),
                filter("px_over_desktop", "Over 600 pixels (desktop)", concat!(px!("u.title", "title"), " > 600")),
                filter("px_over_mobile", "Over 580 pixels (mobile)", concat!(px!("u.title", "title"), " > 580")),
                filter("below_30", "Below 30 characters", "x.t <> '' AND x.len < thr('title.below')")
                    .thr("title.below", "Below {n} characters"),
                filter("px_below", "Below 200 pixels", concat!("x.t <> '' AND ", px!("u.title", "title"), " < 200")),
                filter("same_as_h1", "Same as H1", "x.same_h1").needs(Need::Extraction),
                filter("multiple", "Multiple", "x.count > 1").needs(Need::Extraction),
            ],
            search_column: Some(Cow::Borrowed("title")),
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
                ext("desc_px", "Pixels", DESC_PX, Int, 90),
                ext("desc_cut_desktop", "Cut on desktop", concat!("(", px!(md_expr!(), "desc"), " > 990)"), Bool, 120),
                ext("desc_cut_mobile", "Cut on mobile", concat!("(", px!(md_expr!(), "desc"), " > 680)"), Bool, 120),
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
                filter("px_over_desktop", "Over 990 pixels (desktop)", concat!(px!(md_expr!(), "desc"), " > 990")),
                filter("px_over_mobile", "Over 680 pixels (mobile)", concat!(px!(md_expr!(), "desc"), " > 680")),
                filter("below_70", "Below 70 characters", me::BELOW).thr("meta.below", "Below {n} characters"),
                filter("multiple", "Multiple", me::MULTIPLE),
            ],
            "meta_description",
        ),
        heading_tab!("h1", "H1", Derived::OccH1, "h1.over", hd::H1_MISSING, hd::H1_DUPLICATE, hd::H1_OVER, hd::H1_MULTIPLE),
        heading_tab!("h2", "H2", Derived::OccH2, "h2.over", hd::H2_MISSING, hd::H2_DUPLICATE, hd::H2_OVER, hd::H2_MULTIPLE),
        TabDef {
            id: Cow::Borrowed("images"),
            title: Cow::Borrowed("Images"),
            eligible: Cow::Borrowed(IMAGES),
            columns: vec![
                c::ADDRESS,
                ext("alt", "Alt text", IMG_ALT, Text, 280).with_rendered(IMG_ALT_RENDERED),
                ext(
                    "alt_length",
                    "Alt length",
                    "length((SELECT l.alt FROM links l WHERE l.to_id = u.id AND l.kind = 'image' ORDER BY l.rowid LIMIT 1))",
                    Int,
                    90,
                )
                .with_rendered(IMG_ALT_LENGTH_RENDERED),
                c::STATUS_CODE,
                c::SIZE,
                col(
                    "found_on_count",
                    "Found on",
                    "(SELECT COUNT(DISTINCT l.from_id) FROM links l WHERE l.to_id = u.id AND l.kind = 'image')",
                    Int,
                    90,
                )
                .with_rendered(concat!(
                    "(SELECT COUNT(DISTINCT f) FROM (SELECT l.from_id AS f FROM links l WHERE l.to_id = u.id \
                     AND l.kind = 'image' AND ",
                    live_raw!(),
                    " UNION ALL SELECT l.from_id FROM rendered_links l WHERE l.to_id = u.id AND l.kind = 'image'))"
                )),
                c::FINAL_STATE.hidden(),
            ],
            filters: vec![
                filter("all", "All", "1"),
                filter("missing_alt", "Missing alt attribute", im::MISSING_ALT)
                    .with_rendered(image_use_rendered!("l.alt IS NULL")).needs(Need::Extraction),
                filter("empty_alt", "Empty alt text", image_use!("l.alt = ''"))
                    .with_rendered(image_use_rendered!("l.alt = ''")).needs(Need::Extraction),
                filter("alt_over_100", "Alt text over 100 characters", im::ALT_OVER)
                    .with_rendered(image_use_rendered!("length(l.alt) > 100"))
                    .needs(Need::Extraction)
                    .thr("image.alt_over", "Alt text over {n} characters"),
                filter("over_100kb", "Over 100 KB", im::LARGE).needs(Need::Timing).thr("image.large", "Over {n} bytes"),
            ],
            search_column: Some(Cow::Borrowed("alt")),
            scoped: false,
            rows: None,
        },
        TabDef {
            id: Cow::Borrowed("pdfs"),
            title: Cow::Borrowed("PDFs"),
            eligible: Cow::Borrowed("u.internal = 1 AND u.state = 'crawled' AND content_kind(u.content_type) = 'pdf'"),
            columns: vec![
                c::ADDRESS,
                pdf_col("pdf_title", "PDF title", pdf_fact!("title"), Text, 280),
                pdf_col("pdf_pages", "Pages", pdf_fact!("page_count"), Int, 80),
                pdf_col("pdf_words", "Words", pdf_fact!("word_count"), Int, 90),
                pdf_col(
                    "pdf_status",
                    "Read",
                    "(SELECT CASE p.status WHEN 'read' THEN 'Read' WHEN 'partly_read' THEN 'Partly read' \
                     WHEN 'encrypted' THEN 'Password protected' WHEN 'over_size_limit' THEN 'Over size limit' \
                     ELSE 'Unreadable' END FROM pdfs p WHERE p.url_id = u.id)",
                    Text,
                    140,
                ),
                c::STATUS_CODE,
                c::SIZE,
                c::INDEXABILITY,
                pdf_col("pdf_author", "Author", pdf_fact!("author"), Text, 160).hidden(),
                pdf_col("pdf_subject", "Subject", pdf_fact!("subject"), Text, 200).hidden(),
                pdf_col("pdf_keywords", "Keywords", pdf_fact!("keywords"), Text, 200).hidden(),
                pdf_col("pdf_creator", "Creator", pdf_fact!("creator"), Text, 160).hidden(),
                pdf_col("pdf_producer", "Producer", pdf_fact!("producer"), Text, 160).hidden(),
                pdf_col("pdf_created", "Created", pdf_fact!("created_at"), Text, 170).hidden(),
                pdf_col("pdf_modified", "Modified", pdf_fact!("modified_at"), Text, 170).hidden(),
                pdf_col("pdf_lang", "Language", pdf_fact!("lang"), Text, 90).hidden(),
                pdf_col("pdf_tagged", "Tagged", pdf_fact!("tagged"), Bool, 80).hidden(),
                pdf_col("pdf_links", "Links", pdf_fact!("link_count"), Int, 80).hidden(),
                pdf_col("pdf_reason", "Read detail", pdf_fact!("status_reason"), Text, 240).hidden(),
            ],
            filters: vec![
                filter("all", "All", "1"),
                pdf_filter("missing_title", "Missing title", concat!("COALESCE(trim(", pdf_fact!("title"), "), '') = ''")),
                pdf_filter("no_text", "No text (scanned)", "(SELECT p.no_text FROM pdfs p WHERE p.url_id = u.id) = 1"),
                pdf_filter("encrypted", "Password protected", "(SELECT p.status FROM pdfs p WHERE p.url_id = u.id) = 'encrypted'"),
                pdf_filter(
                    "size_limit",
                    "Over the PDF size limit",
                    "(SELECT p.status FROM pdfs p WHERE p.url_id = u.id) = 'over_size_limit'",
                ),
                pdf_filter(
                    "unreadable",
                    "Unreadable or partly read",
                    "(SELECT p.status FROM pdfs p WHERE p.url_id = u.id) IN ('failed', 'partly_read')",
                ),
                pdf_filter("untagged", "Not tagged", "(SELECT p.tagged FROM pdfs p WHERE p.url_id = u.id) = 0"),
                filter("noindex", "Noindex", "u.noindex = 1"),
            ],
            search_column: Some(Cow::Borrowed("pdf_title")),
            scoped: false,
            rows: None,
        },
        TabDef {
            id: Cow::Borrowed("spelling"),
            title: Cow::Borrowed("Spelling"),
            eligible: Cow::Borrowed(HTML_OK),
            columns: vec![
                c::ADDRESS,
                spell_col("spell_errors", "Spelling errors", spell_fact!("errors"), Int, 110),
                spell_col(
                    "spell_words",
                    "Misspelt words",
                    "(SELECT group_concat(word, ', ') FROM (SELECT DISTINCT f.word FROM spell_findings f \
                     WHERE f.url_id = u.id ORDER BY f.pos LIMIT 8))",
                    Text,
                    320,
                ),
                spell_col("spell_lang", "Dictionary", spell_fact!("lang"), Text, 100),
                spell_col(
                    "spell_status",
                    "Checked",
                    "(SELECT CASE s.status WHEN 'checked' THEN 'Checked' \
                     WHEN 'partly_checked' THEN 'Title and headings only' \
                     WHEN 'no_dictionary' THEN 'No dictionary' ELSE 'No text' END \
                     FROM spell_pages s WHERE s.url_id = u.id)",
                    Text,
                    170,
                ),
                c::TITLE,
                spell_col("spell_checked_words", "Words checked", spell_fact!("words"), Int, 110).hidden(),
                c::STATUS_CODE.hidden(),
            ],
            filters: vec![
                filter("all", "All", "1"),
                spell_filter("errors", "Spelling errors", concat!(spell_fact!("errors"), " > 0")),
                spell_filter("in_title", "In title", has_finding!("f.source = 'title'")),
                spell_filter("in_description", "In meta description", has_finding!("f.source = 'description'")),
                spell_filter("in_headings", "In headings", has_finding!("f.source = 'heading'")),
                spell_filter("in_body", "In body text", has_finding!("f.source = 'body' AND f.site_wide = 0")),
                spell_filter("site_wide", "Site-wide (template)", has_finding!("f.site_wide = 1")),
                spell_filter(
                    "lang_mismatch",
                    "Language mismatch",
                    concat!(spell_fact!("lang_mismatch"), " = 1"),
                ),
                spell_filter(
                    "no_dictionary",
                    "No dictionary",
                    concat!(spell_fact!("status"), " = 'no_dictionary'"),
                ),
                spell_filter(
                    "partly_checked",
                    "Body not checked",
                    concat!(spell_fact!("status"), " = 'partly_checked'"),
                ),
            ],
            search_column: Some(Cow::Borrowed("spell_words")),
            scoped: false,
            rows: None,
        },
        TabDef {
            id: Cow::Borrowed("amp"),
            title: Cow::Borrowed("AMP"),
            // AMP pages, and pages an amphtml link points to.
            eligible: Cow::Borrowed(
                "u.internal = 1 AND u.state = 'crawled' AND (\
                EXISTS (SELECT 1 FROM amp_pages ap WHERE ap.url_id = u.id) \
                OR EXISTS (SELECT 1 FROM links l WHERE l.to_id = u.id AND l.kind = 'amphtml'))",
            ),
            columns: vec![
                c::ADDRESS,
                amp_col(
                    "amp_failures",
                    "Failed checks",
                    "(SELECT COUNT(*) FROM amp_checks a WHERE a.url_id = u.id)",
                    Int,
                    110,
                ),
                amp_col(
                    "amp_checks",
                    "Checks failed",
                    "(SELECT group_concat(DISTINCT a.check_id) FROM amp_checks a WHERE a.url_id = u.id)",
                    Text,
                    260,
                ),
                col("amp_canonical", "Canonical", "u.canonical", Url, 320),
                amp_col(
                    "amp_format",
                    "Format",
                    "(SELECT CASE ap.format WHEN 'email' THEN 'AMP for email' ELSE 'AMP' END \
                     FROM amp_pages ap WHERE ap.url_id = u.id)",
                    Text,
                    110,
                ),
                c::STATUS_CODE,
                amp_col(
                    "amp_style_bytes",
                    "Custom CSS",
                    "(SELECT ap.style_bytes FROM amp_pages ap WHERE ap.url_id = u.id)",
                    Bytes,
                    100,
                )
                .hidden(),
                c::INDEXABILITY.hidden(),
            ],
            filters: vec![
                // Even All needs AMP data: older files have no AMP tables.
                amp_filter("all", "All", "1"),
                amp_filter(
                    "failing",
                    "Failing a check",
                    "EXISTS (SELECT 1 FROM amp_checks a WHERE a.url_id = u.id)",
                ),
                amp_filter("missing_marker", "Not marked as AMP", AMP_UNMARKED),
                amp_filter("runtime", "Missing runtime script", amp_failed!("runtime")),
                amp_filter("boilerplate", "Missing boilerplate", amp_failed!("boilerplate")),
                amp_filter("viewport", "Missing viewport", amp_failed!("viewport")),
                amp_filter("canonical", "Missing canonical", amp_failed!("canonical")),
                amp_filter("unpaired", "Canonical does not link back", AMP_UNPAIRED),
                amp_filter("author_script", "Author scripts", amp_failed!("author_script")),
                amp_filter("style_size", "Custom CSS over 75,000 bytes", amp_failed!("style_size")),
                amp_filter("disallowed", "Disallowed elements", amp_failed!("disallowed")),
            ],
            search_column: None,
            scoped: false,
            rows: None,
        },
        content_tab(
            "canonicals",
            "Canonicals",
            vec![
                c::ADDRESS,
                ext("canonical_html", "Canonical (HTML)", canonical_from!("html"), Url, 320)
                    .with_rendered(canonical_from_rendered!("html")),
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
                ext("meta_robots", "Meta robots", directives_from!("meta"), Text, 220)
                    .with_rendered(META_DIRECTIVES_RENDERED),
                ext("x_robots_tag", "X-Robots-Tag", directives_from!("header"), Text, 220),
                c::INDEXABILITY,
            ],
            vec![
                filter("index", "Index", "COALESCE(u.noindex, 0) = 0"),
                filter("noindex", "Noindex", "u.noindex = 1"),
                filter("follow", "Follow", "COALESCE(u.nofollow, 0) = 0"),
                filter("nofollow", "Nofollow", "u.nofollow = 1"),
                filter("none", "None", directive!("'none'"))
                    .with_rendered(directive_rendered!("'none'")),
                filter("noarchive", "Noarchive", directive!("'noarchive'"))
                    .with_rendered(directive_rendered!("'noarchive'")),
                filter("nosnippet", "Nosnippet", directive!("'nosnippet'"))
                    .with_rendered(directive_rendered!("'nosnippet'")),
                filter(
                    "max_preview",
                    "Max-snippet / max-image-preview / max-video-preview",
                    directive!("'max-snippet', 'max-image-preview', 'max-video-preview'"),
                )
                .with_rendered(directive_rendered!(
                    "'max-snippet', 'max-image-preview', 'max-video-preview'"
                )),
                filter("unavailable_after", "Unavailable_after", directive!("'unavailable_after'"))
                    .with_rendered(directive_rendered!("'unavailable_after'")),
            ],
            "meta_robots",
        ),
        crate::agents::tab(),
        crate::sitemaps::tab(),
        TabDef {
            id: Cow::Borrowed("search-console"),
            title: Cow::Borrowed("Search Console"),
            eligible: Cow::Borrowed(HTML_OK),
            columns: vec![
                c::ADDRESS,
                c::GSC_CLICKS,
                c::GSC_IMPRESSIONS,
                c::GSC_CTR,
                c::GSC_POSITION,
                c::INDEXABILITY,
                c::TITLE,
                c::INLINKS,
                c::DEPTH,
                c::GSC_DATA.hidden(),
            ],
            filters: [
                filter("all", "All", "1"),
                filter("clicks", "With clicks", GSC_WITH_CLICKS),
                filter("no_impressions", "Indexable, no impressions", GSC_INDEXABLE_NO_IMPRESSIONS),
                filter("clicks_not_indexable", "Has clicks, not indexable", GSC_CLICKS_NOT_INDEXABLE),
                filter("no_data", "No data from Google", GSC_NO_DATA),
            ]
            .into_iter()
            .map(|f| f.needs(Need::SearchConsole))
            .collect(),
            search_column: Some(Cow::Borrowed("title")),
            scoped: false,
            rows: None,
        },
        ai_search_tab(),
        near_dup_tab(),
        javascript_tab(),
    ]
}

const GSC_WITH_CLICKS: &str = "COALESCE((SELECT g.clicks FROM gsc_pages g WHERE g.url_id = u.id), 0) > 0";
const GSC_INDEXABLE_NO_IMPRESSIONS: &str = "indexability(u.state, u.error_class, u.status, u.content_type, u.noindex, u.canonical, u.url) = 'Indexable' \
     AND COALESCE((SELECT g.impressions FROM gsc_pages g WHERE g.url_id = u.id), 0) = 0";
const GSC_CLICKS_NOT_INDEXABLE: &str = "indexability(u.state, u.error_class, u.status, u.content_type, u.noindex, u.canonical, u.url) <> 'Indexable' \
     AND COALESCE((SELECT g.clicks FROM gsc_pages g WHERE g.url_id = u.id), 0) > 0";
const GSC_NO_DATA: &str = "EXISTS (SELECT 1 FROM integration_status s WHERE s.url_id = u.id \
     AND s.provider = 'search_console' AND s.reason = 'no_data')";

/// One value from a page's near duplicate row (spec 020).
macro_rules! nd {
    ($field:literal) => {
        concat!(
            "(SELECT n.",
            $field,
            " FROM near_dup_pages n WHERE n.url_id = u.id)"
        )
    };
}

/// The Content tab: near and exact duplicate content (spec 020 FR-017).
fn near_dup_tab() -> TabDef {
    use ValueKind::*;
    let nd_col = |id, label, expr, kind, width| col(id, label, expr, kind, width).needs(Need::NearDup);
    TabDef {
        id: Cow::Borrowed("content"),
        title: Cow::Borrowed("Content"),
        eligible: Cow::Borrowed(HTML_OK),
        columns: vec![
            c::ADDRESS,
            nd_col(
                "main_words",
                "Main content words",
                "(SELECT p.main_words FROM pages p WHERE p.url_id = u.id)",
                Int,
                130,
            ),
            nd_col("closest_sim", "Closest similarity", nd!("closest_sim"), Int, 130),
            nd_col(
                "closest_url",
                "Closest match",
                "(SELECT c.url FROM near_dup_pages n JOIN urls c ON c.id = n.closest_id WHERE n.url_id = u.id)",
                Url,
                320,
            ),
            nd_col("near_count", "Near duplicates", nd!("near_count"), Int, 120),
            nd_col("cluster", "Cluster", nd!("cluster"), Int, 80),
            nd_col(
                "exact_content",
                "Exact duplicate",
                "EXISTS (SELECT 1 FROM near_dup_pairs q WHERE q.url_id = u.id AND q.exact = 1)",
                Bool,
                110,
            ),
            nd_col(
                "compare_status",
                "Compared",
                "(SELECT CASE n.status WHEN 'compared' THEN 'Compared' WHEN 'too_short' THEN 'Too little content' \
                 WHEN 'not_indexable' THEN 'Not indexable' ELSE 'No data' END FROM near_dup_pages n WHERE n.url_id = u.id)",
                Text,
                140,
            )
            .hidden(),
            c::WORD_COUNT.hidden(),
            c::INDEXABILITY.hidden(),
        ],
        filters: vec![
            filter("all", "All", "1"),
            filter(
                "near_duplicates",
                "Near duplicates",
                "u.id IN (SELECT n.url_id FROM near_dup_pages n WHERE n.near_count > 0)",
            )
            .needs(Need::NearDup),
            filter(
                "exact_duplicates",
                "Exact duplicates",
                "(SELECT p.text_hash FROM pages p WHERE p.url_id = u.id) IN (SELECT o.v FROM temp.occ_text o WHERE o.n > 1)",
            )
            .needs(Need::Extraction)
            .reads(Derived::OccText),
            filter(
                "not_compared",
                "Not compared",
                "u.id IN (SELECT n.url_id FROM near_dup_pages n WHERE n.status IN ('not_indexable', 'no_signature'))",
            )
            .needs(Need::NearDup),
            filter(
                "low_content",
                "Low content",
                "u.id IN (SELECT n.url_id FROM near_dup_pages n WHERE n.status = 'too_short')",
            )
            .needs(Need::NearDup),
        ],
        search_column: Some(Cow::Borrowed("closest_url")),
        scoped: false,
        rows: None,
    }
}

/// Builds a derived table on `conn` (dropping an older copy).
pub fn build_derived(
    conn: &rusqlite::Connection,
    d: Derived,
    features: &crate::Features,
) -> rusqlite::Result<()> {
    let links = &features.link_settings;
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
        Derived::GoneTo => conn.execute_batch(
            "DROP TABLE IF EXISTS temp.gone_to;
             CREATE TEMP TABLE gone_to(to_id INTEGER PRIMARY KEY, n INTEGER NOT NULL);
             INSERT INTO temp.gone_to SELECT l.to_id, COUNT(*) FROM main.raw_only_links g
               CROSS JOIN links l ON l.from_id = g.from_id AND l.pos = g.pos
               WHERE l.to_id IS NOT NULL GROUP BY l.to_id;",
        ),
        Derived::OccText => conn.execute_batch(&format!(
            "DROP TABLE IF EXISTS temp.occ_text;
             CREATE TEMP TABLE occ_text(v INTEGER PRIMARY KEY, n INTEGER NOT NULL) WITHOUT ROWID;
             INSERT INTO temp.occ_text SELECT p.text_hash, COUNT(*) FROM pages p JOIN urls u ON u.id = p.url_id
               WHERE {HTML_OK} AND p.text_hash IS NOT NULL AND p.word_count > 0 GROUP BY 1;"
        )),
        Derived::PageFlags => page_flags(conn),
        // Built by the reader or export, which know the issue settings.
        Derived::IssueHits => Ok(()),
        Derived::LinkMetrics => crate::analysis::Watch::quiet(|w| {
            crate::analysis::link_metrics::build(conn, links.weighting.as_ref(), w).map(|_| ())
        }),
        Derived::LinkPositions => crate::analysis::Watch::quiet(|w| {
            crate::analysis::link_positions::build(conn, &links.position_rules, w).map(|_| ())
        }),
        Derived::VizCrawlTree => crate::viz::crawl_tree::build(conn),
        Derived::VizDirs => crate::viz::dirs::build(conn),
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

/// One value from a page's `renders` row (spec 014).
macro_rules! rx {
    ($field:literal) => {
        concat!("(SELECT r.", $field, " FROM renders r WHERE r.url_id = u.id)")
    };
}

/// One value from a page's `render_comparisons` row (spec 014).
macro_rules! cx {
    ($field:literal) => {
        concat!(
            "(SELECT c.",
            $field,
            " FROM render_comparisons c WHERE c.url_id = u.id)"
        )
    };
}

/// A condition on a page's `renders` row.
macro_rules! on_render {
    ($cond:literal) => {
        concat!(
            "EXISTS (SELECT 1 FROM renders r WHERE r.url_id = u.id AND ",
            $cond,
            ")"
        )
    };
}

/// A condition on a page's `render_comparisons` row.
macro_rules! on_cmp {
    ($cond:literal) => {
        concat!(
            "EXISTS (SELECT 1 FROM render_comparisons c WHERE c.url_id = u.id AND ",
            $cond,
            ")"
        )
    };
}

/// The rows of the JavaScript tab: pages with a `renders` row (spec 014
/// FR-025). Reads no rendered values, so it ignores the Raw / Rendered
/// switch.
const JS_ELIGIBLE: &str = "EXISTS (SELECT 1 FROM renders r WHERE r.url_id = u.id)";

/// "Contains JavaScript content": rendered words exceed the raw count by
/// the stored percentage and word count (FR-027).
const JS_CONTENT: &str = on_cmp!(
    "c.words_rendered - c.words_raw >= js_content_words() \
     AND c.words_rendered * 100 > c.words_raw * (100 + js_content_percent())"
);

/// "Content hidden from non-rendering crawlers": dependence over the stored
/// percentage, or a title or first H1 that exists only after rendering
/// (FR-030).
const JS_HIDDEN: &str =
    on_cmp!("(c.dependence > hidden_content_percent() OR c.title_diff = 'added' OR c.h1_diff = 'added')");

/// The JavaScript tab: what rendering changed on each rendered page (spec
/// 014 FR-025 and FR-026).
fn javascript_tab() -> TabDef {
    use ValueKind::*;
    let js_col = |id, label, expr, kind, width| col(id, label, expr, kind, width).needs(Need::Rendering);
    let js_filter = |id, label, cond| filter(id, label, cond).needs(Need::Rendering);
    TabDef {
        id: Cow::Borrowed("javascript"),
        title: Cow::Borrowed("JavaScript"),
        eligible: Cow::Borrowed(JS_ELIGIBLE),
        columns: vec![
            c::ADDRESS,
            js_col(
                "render_status",
                "Rendering status",
                "(SELECT CASE r.state WHEN 'rendered' THEN 'Rendered' WHEN 'timed_out' THEN 'Timed out' \
                 WHEN 'failed' THEN 'Failed' WHEN 'not_rendered' THEN 'Not rendered' \
                 WHEN 'queued' THEN 'Queued' WHEN 'rendering' THEN 'Rendering' ELSE r.state END \
                 || CASE WHEN r.reason IS NULL THEN '' ELSE ' (' || replace(r.reason, '_', ' ') || ')' END \
                 FROM renders r WHERE r.url_id = u.id)",
                Text,
                190,
            ),
            js_col("render_time", "Render time", rx!("render_ms"), Ms, 110),
            js_col("words_raw", "Word count (raw)", cx!("words_raw"), Int, 130),
            js_col(
                "words_rendered",
                "Word count (rendered)",
                cx!("words_rendered"),
                Int,
                160,
            ),
            js_col("words_change", "Word count change", cx!("words_change"), Int, 140),
            js_col("links_raw", "Links (raw)", cx!("links_raw"), Int, 100),
            js_col(
                "links_rendered",
                "Links (rendered)",
                cx!("links_rendered"),
                Int,
                130,
            ),
            js_col(
                "js_links",
                "JavaScript-only links",
                cx!("links_rendered_only"),
                Int,
                150,
            ),
            js_col(
                "js_dependence",
                "JavaScript dependence",
                "(SELECT round(c.dependence, 1) FROM render_comparisons c WHERE c.url_id = u.id)",
                Percent,
                150,
            ),
            js_col("js_errors", "JavaScript errors", rx!("console_errors"), Int, 130),
            js_col(
                "blocked_resources",
                "Blocked resources",
                rx!("resources_blocked"),
                Int,
                130,
            ),
            js_col(
                "failed_resources",
                "Failed resources",
                rx!("resources_failed"),
                Int,
                130,
            )
            .hidden(),
            js_col(
                "js_warnings",
                "JavaScript warnings",
                rx!("console_warnings"),
                Int,
                140,
            )
            .hidden(),
            js_col("js_redirect", "JavaScript redirect", rx!("js_redirect"), Url, 320).hidden(),
            js_col(
                "bot_challenge",
                "Possible bot challenge",
                rx!("bot_challenge"),
                Bool,
                150,
            )
            .hidden(),
        ],
        filters: vec![
            js_filter("all", "All", "1"),
            js_filter("rendered", "Rendered", on_render!("r.state = 'rendered'")),
            js_filter(
                "not_rendered",
                "Not rendered",
                on_render!("r.state = 'not_rendered'"),
            ),
            js_filter(
                "timed_out",
                "Render timed out",
                on_render!("r.state = 'timed_out'"),
            ),
            js_filter("failed", "Render failed", on_render!("r.state = 'failed'")),
            js_filter(
                "js_links",
                "Contains JavaScript links",
                on_cmp!("c.links_rendered_only > 0"),
            ),
            js_filter("js_content", "Contains JavaScript content", JS_CONTENT).thr(
                "render.js_content",
                "Contains JavaScript content (over {n}% and {w} words)",
            ),
            js_filter(
                "title_only_rendered",
                "Title only in rendered HTML",
                on_cmp!("c.title_diff = 'added'"),
            ),
            js_filter(
                "title_updated",
                "Title updated by JavaScript",
                on_cmp!("c.title_diff = 'changed'"),
            ),
            js_filter(
                "description_only_rendered",
                "Meta description only in rendered HTML",
                on_cmp!("c.description_diff = 'added'"),
            ),
            js_filter(
                "description_updated",
                "Meta description updated by JavaScript",
                on_cmp!("c.description_diff = 'changed'"),
            ),
            js_filter(
                "h1_only_rendered",
                "H1 only in rendered HTML",
                on_cmp!("c.h1_diff = 'added'"),
            ),
            js_filter(
                "h1_updated",
                "H1 updated by JavaScript",
                on_cmp!("c.h1_diff = 'changed'"),
            ),
            js_filter(
                "canonical_only_rendered",
                "Canonical only in rendered HTML",
                on_cmp!("c.canonical_diff = 'added'"),
            ),
            js_filter(
                "canonical_mismatch",
                "Canonical mismatch",
                on_cmp!("c.canonical_diff = 'changed'"),
            ),
            js_filter(
                "noindex_only_original",
                "Noindex only in original HTML",
                on_cmp!("c.noindex_diff = 'removed'"),
            ),
            js_filter(
                "noindex_only_rendered",
                "Noindex only in rendered HTML",
                on_cmp!("c.noindex_diff = 'added'"),
            ),
            js_filter(
                "nofollow_only_original",
                "Nofollow only in original HTML",
                on_cmp!("c.nofollow_diff = 'removed'"),
            ),
            js_filter(
                "nofollow_only_rendered",
                "Nofollow only in rendered HTML",
                on_cmp!("c.nofollow_diff = 'added'"),
            ),
            js_filter(
                "structured_only_rendered",
                "Structured data only in rendered HTML",
                on_cmp!("c.structured_rendered_only > 0"),
            ),
            js_filter(
                "js_redirect",
                "JavaScript redirect",
                on_render!("r.js_redirect IS NOT NULL"),
            ),
            js_filter(
                "js_errors",
                "JavaScript errors",
                on_render!("r.console_errors > 0"),
            ),
            js_filter(
                "failed_resources",
                "Has failed resources",
                on_render!("r.resources_failed > 0"),
            ),
            js_filter(
                "blocked_resources",
                "Has blocked resources",
                on_render!("r.resources_blocked > 0"),
            ),
            js_filter(
                "bot_challenge",
                "Possible bot challenge",
                on_render!("r.bot_challenge = 1"),
            ),
            js_filter(
                "hidden_content",
                "Content hidden from non-rendering crawlers",
                JS_HIDDEN,
            )
            .thr(
                "render.hidden",
                "Content hidden from non-rendering crawlers (over {n}%)",
            ),
        ],
        search_column: Some(Cow::Borrowed("address")),
        scoped: false,
        rows: None,
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
            assert!(tab_ids.insert(&t.id), "duplicate tab {}", t.id);
            let cols: HashSet<_> = t.columns.iter().map(|c| &c.id).collect();
            assert_eq!(cols.len(), t.columns.len(), "duplicate column in {}", t.id);
            let fs: HashSet<_> = t.filters.iter().map(|f| &f.id).collect();
            assert_eq!(fs.len(), t.filters.len(), "duplicate filter in {}", t.id);
            assert_eq!(t.filters[0].id, "all");
            assert_eq!(t.columns[0].id, "address");
            if let Some(s) = &t.search_column {
                assert!(
                    t.column(s.as_ref()).is_some(),
                    "{} searches unknown column {s}",
                    t.id
                );
            }
        }
        assert_eq!(tab_ids.len(), 21);
    }

    #[test]
    fn javascript_tab_has_the_26_filters_with_thresholds_in_labels() {
        // FR-026, FR-027, FR-030.
        let t = tab("javascript").unwrap();
        // The 26 of FR-026, plus the "With issues" filter every tab has.
        assert_eq!(t.filters.len(), 27);
        let mut values = crate::features::RenderThresholds::default().as_map();
        let label = |id: &str, values: &crate::sql::Thresholds| t.filter(id).unwrap().label_with(values);
        assert_eq!(
            label("js_content", &values),
            "Contains JavaScript content (over 20% and 50 words)"
        );
        assert_eq!(
            label("hidden_content", &values),
            "Content hidden from non-rendering crawlers (over 50%)"
        );
        values.insert("render.js_content", 35.0);
        values.insert("render.js_content.2", 120.0);
        assert_eq!(
            label("js_content", &values),
            "Contains JavaScript content (over 35% and 120 words)"
        );
        assert_eq!(label("rendered", &values), "Rendered");
        assert!(
            t.filters
                .iter()
                .all(|f| f.id == "with_issues" || f.need == Need::Rendering)
        );
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
    fn issue_tabs_show_their_categorys_columns() {
        // FR-015: a title rule shows the title and its length.
        let t = tab("issue:title.over").unwrap();
        let ids: Vec<_> = t.columns.iter().map(|c| c.id.as_ref()).collect();
        assert_eq!(&ids[..4], ["address", "evidence", "title", "title_length"]);
        for t in issue_tabs() {
            let unique: HashSet<_> = t.columns.iter().map(|c| &c.id).collect();
            assert_eq!(unique.len(), t.columns.len(), "duplicate column in {}", t.id);
        }
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
