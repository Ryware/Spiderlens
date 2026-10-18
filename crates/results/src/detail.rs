//! Everything known about one URL, for the detail pane (plan D10, FR-019..25).

use std::collections::BTreeMap;

use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};
use spiderlens_engine::extract::body;
use ts_rs::TS;

use crate::reader::Reader;
use crate::tabs::{Derived, Need};
use crate::values::Values;
use crate::{Error, Result, sql};

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct RedirectHop {
    #[ts(type = "number")]
    pub id: i64,
    pub url: String,
    pub status: Option<u16>,
    pub final_state: String,
}

/// What automated accessibility checks can and cannot tell (spec 026
/// FR-025). Shown wherever accessibility results are: the tab, the detail
/// pane, the command line and the exports.
pub const A11Y_DISCLAIMER: &str = "Automated checks find only part of the accessibility problems. \
A page without violations is not necessarily accessible.";

/// One failing element of an accessibility check.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct A11yNodeDetail {
    pub selector: String,
    pub excerpt: String,
    /// Check-specific JSON, such as the colours of a contrast failure.
    pub data: Option<String>,
    /// x, y, width and height in CSS pixels, when the page was rendered.
    pub bbox: Option<Vec<f64>>,
}

/// One check's result on the page.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct A11yCheckDetail {
    pub check_id: String,
    pub title: String,
    /// violation | needs_review
    pub kind: String,
    /// static | full
    pub source: String,
    pub impact: Option<String>,
    /// Space-separated WCAG tags as the checks write them.
    pub wcag: Option<String>,
    /// Failing elements before the cap.
    pub nodes_total: u32,
    /// The rule's public documentation; None for the static checks.
    pub help_url: Option<String>,
    pub nodes: Vec<A11yNodeDetail>,
}

/// The Accessibility section of the detail pane (spec 026 FR-023).
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct A11yDetail {
    pub static_state: Option<String>,
    pub full_state: Option<String>,
    pub full_reason: Option<String>,
    /// Ordered by impact, then check.
    pub checks: Vec<A11yCheckDetail>,
    /// Some check has more failing elements than are stored.
    pub capped: bool,
    pub disclaimer: String,
}

/// An element that failed a mobile measure.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct MobileExample {
    pub selector: String,
    pub w: f64,
    pub h: f64,
}

/// The Mobile section of the detail pane (spec 026 FR-023).
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct MobileDetail {
    pub viewport: Option<String>,
    pub viewport_state: Option<String>,
    pub plugins: Option<u32>,
    pub window: Option<String>,
    pub measured: Option<String>,
    pub content_px: Option<f64>,
    pub window_px: Option<f64>,
    pub widest: Option<String>,
    pub small_text_share: Option<f64>,
    pub small_targets: Option<u32>,
    pub examples: Vec<MobileExample>,
}

/// A readable name for a check id.
pub(crate) fn a11y_title(check_id: &str) -> String {
    if let Some(id) = check_id.strip_prefix("static.") {
        let title = match id {
            "form_label" => "Form field without a label",
            "button_name" => "Button without a name",
            "link_name" => "Link without a name",
            "frame_title" => "Frame without a title",
            "lang_invalid" => "Invalid page language",
            "element_lang_invalid" => "Invalid language on an element",
            "zoom_disabled" => "Pinch zoom is disabled",
            "viewport_missing" => "No viewport meta tag",
            "viewport_width" => "Viewport is not the device width",
            "tabindex_positive" => "Positive tabindex",
            "accesskey_duplicate" => "Duplicate accesskey",
            "meta_refresh_delay" => "Timed page refresh",
            "deprecated_element" => "Blinking or scrolling element",
            "plugin_element" => "Uses object, embed or applet",
            "area_alt" => "Image map area without alt text",
            "input_image_alt" => "Image button without alt text",
            other => return other.replace('_', " "),
        };
        return title.to_string();
    }
    let spaced = check_id.replace('-', " ");
    let mut chars = spaced.chars();
    match chars.next() {
        Some(c) => c.to_uppercase().chain(chars).collect(),
        None => spaced,
    }
}

/// One failed AMP check (spec 027 part E).
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct AmpFailure {
    pub check: String,
    pub detail: String,
    /// The element's position among the page's elements, from 1.
    pub pos: Option<u32>,
}

/// What the AMP checks found on the page.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct AmpDetail {
    /// website | email
    pub format: String,
    pub failures: Vec<AmpFailure>,
    /// Whether the canonical page links back with `amphtml`; None when the
    /// canonical is the page itself or missing.
    pub paired: Option<bool>,
}

/// One misspelt word on the page (spec 027 part D).
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct SpellFinding {
    pub word: String,
    /// title | description | heading | body
    pub source: String,
    /// The sentence around the word.
    pub context: String,
    pub suggestions: Vec<String>,
    /// In a text block on most pages, such as a footer.
    pub site_wide: bool,
}

/// What the spelling pass found on the page.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct SpellDetail {
    /// The dictionary used, or the language without one.
    pub lang: Option<String>,
    /// checked | partly_checked | no_dictionary | no_text
    pub status: String,
    pub words: u32,
    pub errors: u32,
    pub lang_mismatch: bool,
    /// Up to 200, in page order.
    pub findings: Vec<SpellFinding>,
}

/// One URL's spec 023 values (FR-008).
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct LinkMetrics {
    /// 0-100, unrounded.
    pub score: f64,
    pub rank: u32,
    /// Number of scored URLs, for "rank 12 of 4,812".
    pub of: u32,
    pub unique_inlinks: u32,
    pub followed_inlinks: u32,
    pub all_inlinks: u32,
    pub unique_outlinks: u32,
    pub external_outlinks: u32,
    pub pct_linking: f64,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct UrlDetail {
    #[ts(type = "number")]
    pub id: i64,
    pub url: String,
    /// Decoded host and path, for display (Edge case: internationalised URLs).
    pub url_readable: String,
    pub internal: bool,
    pub final_state: String,
    pub status: Option<u16>,
    pub status_text: Option<String>,
    pub content_type: Option<String>,
    pub depth: u32,
    pub error_class: Option<String>,
    pub error_label: Option<String>,
    pub error_message: Option<String>,
    pub not_crawled_reason: Option<String>,
    pub reason_detail: Option<String>,
    pub robots_rule: Option<String>,
    pub found_on: Option<String>,
    pub discovered_via: String,
    pub in_sitemap: bool,
    /// Milliseconds since the Unix epoch.
    #[ts(type = "number | null")]
    pub fetched_at: Option<i64>,
    /// This URL and each hop after it, in order (FR-021). Empty when it does
    /// not redirect.
    pub redirects: Vec<RedirectHop>,
    pub redirect_loop: bool,
    pub redirect_over_limit: bool,
    #[ts(type = "number")]
    pub inlinks: u64,
    #[ts(type = "number")]
    pub outlinks: u64,
    #[ts(type = "number")]
    pub images: u64,
    /// Body size after transfer decoding, in bytes.
    #[ts(type = "number | null")]
    pub size: Option<i64>,
    /// Request sent to last body byte read.
    #[ts(type = "number | null")]
    pub response_ms: Option<i64>,
    /// "Indexable" or "Non-indexable: <reason>".
    pub indexability: Option<String>,
    /// Response headers in received order, names lowercase (FR-020).
    pub headers: Vec<(String, String)>,
    /// Number of headings, for the Headings list.
    #[ts(type = "number")]
    pub headings: u64,
    /// The page's HTML is stored in the crawl file.
    pub html_stored: bool,
    /// Link Score and link counts (spec 023); None for external URLs and
    /// while the crawl runs.
    pub link_metrics: Option<LinkMetrics>,
    /// Cookies this URL's response set, without values (spec 025 FR-027).
    pub cookies: Vec<crate::CookieRow>,
    /// Requests made for this URL across the crawl, retries included
    /// (spec 004 FR-020).
    pub attempts: Option<u32>,
    /// The error's sub-reason, such as "refused" or "expired" (FR-013).
    pub error_reason: Option<String>,
    /// The result came from "Retry failed URLs" (US5.3).
    pub manual_retry: bool,
    /// AMP checks, for AMP pages of a crawl that checked AMP.
    pub amp: Option<AmpDetail>,
    /// Accessibility results, for pages a crawl checked (spec 026).
    pub accessibility: Option<A11yDetail>,
    /// Mobile measures, for pages a crawl checked (spec 026).
    pub mobile: Option<MobileDetail>,
    /// What was read from the document, for a PDF the crawl read.
    pub pdf: Option<PdfDetail>,
    /// Title and meta description as served, for the search preview (spec
    /// 027 part A). `None` for anything that is not an HTML page.
    pub snippet: Option<Snippet>,
    /// Spelling results, when the page was checked.
    pub spelling: Option<SpellDetail>,
    /// Rendering (spec 014): for a page that has a `renders` row.
    pub render: Option<RenderDetail>,
    /// Fields and sections this file has no data for, with the reason.
    #[ts(type = "Record<string, string>")]
    pub unavailable: BTreeMap<String, String>,
}

/// One element that rendering may change, with both values (spec 014
/// FR-024). The values are given only when the element differs.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct ElementChange {
    /// same | added | removed | changed
    pub diff: String,
    pub raw: Option<String>,
    pub rendered: Option<String>,
}

/// How the rendered page differs from the raw one (spec 014 FR-024,
/// FR-029).
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct RenderComparison {
    pub words_raw: u32,
    pub words_rendered: u32,
    #[ts(type = "number")]
    pub words_change: i64,
    pub links_raw: u32,
    pub links_rendered: u32,
    pub links_rendered_only: u32,
    pub links_raw_only: u32,
    pub title: ElementChange,
    pub description: ElementChange,
    pub h1: ElementChange,
    pub canonical: ElementChange,
    pub noindex: ElementChange,
    pub nofollow: ElementChange,
    pub hreflang: ElementChange,
    pub structured_rendered_only: u32,
    /// Overall JavaScript dependence, 0 to 100.
    pub dependence: f64,
    /// Shares of each content type that exist only in the rendered page,
    /// 0 to 1; null when the rendered page has none of that type.
    pub dep_text: Option<f64>,
    pub dep_headings: Option<f64>,
    pub dep_internal_links: Option<f64>,
    pub dep_structured: Option<f64>,
    pub dep_images: Option<f64>,
    pub raw_has_title: bool,
    pub raw_has_description: bool,
    pub raw_has_canonical: bool,
    pub raw_has_directives: bool,
}

/// The rendering section of the detail pane (spec 014 FR-028).
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct RenderDetail {
    /// queued | rendering | rendered | timed_out | failed | not_rendered
    pub state: String,
    pub reason: Option<String>,
    pub reason_label: Option<String>,
    pub reason_detail: Option<String>,
    pub attempts: u32,
    #[ts(type = "number | null")]
    pub render_ms: Option<i64>,
    /// The window used, CSS pixels.
    pub window_w: Option<u32>,
    pub window_h: Option<u32>,
    /// Where a script navigated to, as written.
    pub js_redirect: Option<String>,
    #[ts(type = "number | null")]
    pub js_redirect_id: Option<i64>,
    pub console_errors: u32,
    pub console_warnings: u32,
    pub resources_failed: u32,
    pub resources_blocked: u32,
    pub bot_challenge: bool,
    /// None when the page has no rendered version.
    pub comparison: Option<RenderComparison>,
    pub rendered_html_stored: bool,
    pub screenshot_stored: bool,
}

/// The sentence for a render's reason code.
pub(crate) fn render_reason_label(reason: &str) -> &'static str {
    match reason {
        "crash" => "The browser crashed",
        "renderer_error" => "The browser reported an error",
        "page_too_large" => "The page was too large to render",
        "navigation_error" => "The page could not be loaded in the browser",
        "outside_rules" => "Outside the rendering rules",
        "crawl_stopped" => "The crawl was stopped first",
        "browser_unavailable" => "No usable browser",
        "html_not_stored" => "The raw HTML was not stored",
        _ => "Not rendered",
    }
}

/// One request a rendered page made (spec 014 FR-018).
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct ResourceRow {
    pub url: String,
    /// script, stylesheet, image, media, font, xhr, fetch, document, other
    pub kind: String,
    pub host: String,
    pub third_party: bool,
    /// loaded | cached | failed | blocked_setting | blocked_robots
    pub outcome: String,
    pub status: Option<u16>,
    pub error: Option<String>,
    pub robots_rule: Option<String>,
    #[ts(type = "number | null")]
    pub bytes: Option<i64>,
}

/// One console error or warning of a rendered page (spec 014 FR-043).
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct ConsoleRow {
    /// error | warning | exception
    pub level: String,
    pub message: String,
    pub source_url: Option<String>,
    pub line: Option<u32>,
    pub col: Option<u32>,
}

/// Which stored HTML of a page to read (spec 014 FR-032).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum HtmlVersion {
    /// As the server sent it.
    #[default]
    Raw,
    /// The DOM after scripts ran.
    Rendered,
}

/// The PDF section of the detail pane (spec 027 FR-C02).
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct PdfDetail {
    /// read, partly_read, encrypted, over_size_limit or failed.
    pub status: String,
    pub status_label: String,
    /// Why something could not be read.
    pub status_reason: Option<String>,
    pub title: Option<String>,
    pub author: Option<String>,
    pub subject: Option<String>,
    pub keywords: Option<String>,
    pub creator: Option<String>,
    pub producer: Option<String>,
    pub created: Option<String>,
    pub modified: Option<String>,
    pub lang: Option<String>,
    pub pages: Option<u32>,
    pub pages_read: Option<u32>,
    pub words: Option<u32>,
    pub encrypted: bool,
    pub tagged: bool,
    /// Pages read but no text on them: a scan.
    pub no_text: bool,
    pub links: u32,
    pub text_stored: bool,
}

pub(crate) fn pdf_status_label(status: &str) -> &'static str {
    match status {
        "read" => "Read",
        "partly_read" => "Partly read",
        "encrypted" => "Password protected",
        "over_size_limit" => "Over size limit",
        _ => "Unreadable",
    }
}

/// What a search result is built from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct Snippet {
    /// The primary title (spec 005), `None` when the page has none.
    pub title: Option<String>,
    /// The meta description; `None` when missing, or when the file has no
    /// page content.
    pub description: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ListKind {
    Inlinks,
    Outlinks,
    Images,
    Headings,
    /// Requests a rendered page made (spec 014).
    Resources,
    /// Console errors and warnings of a rendered page (spec 014).
    Console,
}

/// One link in the Inlinks, Outlinks or Images list.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct LinkRow {
    /// The other URL: the source for inlinks, the target otherwise.
    #[ts(type = "number")]
    pub id: i64,
    pub url: String,
    /// The href as written in the page.
    pub raw: String,
    pub kind: String,
    pub nofollow: bool,
    pub followed: bool,
    pub internal: bool,
    pub status: Option<u16>,
    pub final_state: String,
    /// Anchor text, or alt text for images. For images, null means the alt
    /// attribute is missing.
    pub text: Option<String>,
    pub sponsored: bool,
    pub ugc: bool,
    /// Where the anchor sits on its page (spec 023 US4): a `Position` or
    /// "unknown" (HTML not stored). Null when not worked out yet, or not
    /// an anchor.
    pub position: Option<String>,
    /// Where the link was found, for a rendered page (spec 014 FR-028):
    /// raw (only in the raw HTML), rendered (only after rendering) or
    /// both. Null for pages that were not rendered.
    pub origin: Option<String>,
}

/// One heading of a page, in document order (FR-023).
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct HeadingRow {
    pub level: u8,
    pub text: String,
    /// As [`LinkRow::origin`].
    pub origin: Option<String>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct ListPage {
    #[ts(type = "number")]
    pub total: u64,
    pub from: u32,
    pub rows: Vec<LinkRow>,
    /// The rows of the Headings list (empty for the link lists).
    pub headings: Vec<HeadingRow>,
    /// The rows of the Resources list (empty for the other lists).
    pub resources: Vec<ResourceRow>,
    /// The rows of the Console list (empty for the other lists).
    pub console: Vec<ConsoleRow>,
    /// Set when this file cannot list this section.
    pub unavailable: Option<String>,
}

/// Why a page has no HTML to show.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum HtmlMissing {
    /// The crawl ran with "Store page HTML" off.
    NotStored,
    /// Not an HTML page, or no body was received.
    NoBody,
}

/// A page's stored HTML, for the source viewer (T029, spec 005 FR-030).
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct PageHtml {
    /// The source as text; null when `missing` says why there is none.
    pub html: Option<String>,
    pub missing: Option<HtmlMissing>,
    /// Size of the stored body, in bytes.
    #[ts(type = "number")]
    pub bytes: u64,
    /// The encoding the text was decoded with.
    pub encoding: Option<String>,
    /// Some bytes were not valid in that encoding and show as U+FFFD.
    pub encoding_errors: bool,
}

/// Why a page has no screenshot to show.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ScreenshotMissing {
    /// The crawl ran with "Save screenshots" off.
    NotTaken,
    /// This page has none: not rendered, or the capture failed.
    NoImage,
}

/// A rendered page's stored screenshot (spec 014 FR-033).
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct PageScreenshot {
    /// The WebP image, base64 encoded; null when `missing` says why.
    pub webp_base64: Option<String>,
    pub missing: Option<ScreenshotMissing>,
    /// Image size in pixels.
    pub width: u32,
    pub height: u32,
}

const FINAL_STATE: &str =
    "final_state(x.state, x.reason, x.reason_detail, x.check_only, x.fetched_at, x.error_class, x.status)";

impl Reader {
    pub fn detail(&mut self, id: i64) -> Result<UrlDetail> {
        let sql = format!(
            "SELECT x.url, x.internal, {FINAL_STATE}, x.status, x.content_type, x.depth, x.error_class,
                    x.error_message, x.reason, x.reason_detail, x.robots_rule,
                    (SELECT s.url FROM urls s WHERE s.id = x.source_id), x.source, x.in_sitemap, x.fetched_at
             FROM urls x WHERE x.id = ?1"
        );
        let row = self
            .conn
            .query_row(&sql, [id], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, bool>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, Option<u16>>(3)?,
                    r.get::<_, Option<String>>(4)?,
                    r.get::<_, u32>(5)?,
                    r.get::<_, Option<String>>(6)?,
                    r.get::<_, Option<String>>(7)?,
                    r.get::<_, Option<String>>(8)?,
                    r.get::<_, Option<String>>(9)?,
                    r.get::<_, Option<String>>(10)?,
                    r.get::<_, Option<String>>(11)?,
                    r.get::<_, String>(12)?,
                    r.get::<_, bool>(13)?,
                    r.get::<_, Option<i64>>(14)?,
                ))
            })
            .optional()?
            .ok_or_else(|| Error::invalid(format!("URL {id} is not in this crawl")))?;
        let (
            url,
            internal,
            final_state,
            status,
            content_type,
            depth,
            error_class,
            error_message,
            reason,
            reason_detail,
            robots_rule,
            found_on,
            source,
            in_sitemap,
            fetched_at,
        ) = row;
        let (redirects, redirect_loop) = self.redirect_chain(id)?;
        let limit = self.features.redirect_limit as usize;
        let count = |sql: &str| -> rusqlite::Result<u64> {
            self.conn
                .query_row(sql, [id], |r| r.get::<_, i64>(0))
                .map(|n| n as u64)
        };
        let inlinks = count("SELECT COUNT(*) FROM links WHERE to_id = ?1")?;
        let outlinks = count("SELECT COUNT(*) FROM links WHERE from_id = ?1")?;
        let images = count("SELECT COUNT(*) FROM links WHERE from_id = ?1 AND kind = 'image'")?;
        let content = self.features.has(Need::Extraction);
        let headings = if content {
            count("SELECT COUNT(*) FROM page_text WHERE url_id = ?1 AND kind = 'heading'")?
        } else {
            0
        };
        let (indexability, headers) = if content {
            let ix: Option<String> = self.conn.query_row(
                "SELECT indexability(u.state, u.error_class, u.status, u.content_type, u.noindex, u.canonical, u.url)
                 FROM urls u WHERE u.id = ?1",
                [id],
                |r| r.get(0),
            )?;
            let raw: Option<String> = self
                .conn
                .query_row("SELECT headers FROM headers WHERE url_id = ?1", [id], |r| {
                    r.get(0)
                })
                .optional()?;
            let mut headers = raw
                .and_then(|h| serde_json::from_str::<Vec<(String, String)>>(&h).ok())
                .unwrap_or_default();
            // Older files that are only viewed still hold cookie values
            // (spec 025 FR-026).
            spiderlens_engine::crawl::redact::scrub_pairs(&mut headers);
            (ix, headers)
        } else {
            (None, vec![])
        };
        let (size, response_ms) = if self.features.has(Need::Timing) {
            self.conn.query_row(
                "SELECT size_bytes, response_ms FROM urls WHERE id = ?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )?
        } else {
            (None, None)
        };
        let (attempts, error_reason, manual_retry) = if self.features.has(Need::Attempts) {
            self.conn.query_row(
                "SELECT attempts, error_reason, manual_retry FROM urls WHERE id = ?1",
                [id],
                |r| {
                    Ok((
                        r.get(0)?,
                        r.get(1)?,
                        r.get::<_, Option<bool>>(2)?.unwrap_or(false),
                    ))
                },
            )?
        } else {
            (None, None, false)
        };
        let pdf = if self.features.has(Need::Pdf) {
            self.conn
                .query_row(
                    "SELECT status, status_reason, title, author, subject, keywords, creator, producer, created_at,
                     modified_at, lang, page_count, pages_read, word_count, encrypted, tagged, no_text, link_count,
                     text_stored FROM pdfs WHERE url_id = ?1",
                    [id],
                    |r| {
                        let status: String = r.get(0)?;
                        Ok(PdfDetail {
                            status_label: pdf_status_label(&status).to_string(),
                            status,
                            status_reason: r.get(1)?,
                            title: r.get(2)?,
                            author: r.get(3)?,
                            subject: r.get(4)?,
                            keywords: r.get(5)?,
                            creator: r.get(6)?,
                            producer: r.get(7)?,
                            created: r.get(8)?,
                            modified: r.get(9)?,
                            lang: r.get(10)?,
                            pages: r.get(11)?,
                            pages_read: r.get(12)?,
                            words: r.get(13)?,
                            encrypted: r.get::<_, Option<bool>>(14)?.unwrap_or(false),
                            tagged: r.get::<_, Option<bool>>(15)?.unwrap_or(false),
                            no_text: r.get::<_, Option<bool>>(16)?.unwrap_or(false),
                            links: r.get::<_, Option<u32>>(17)?.unwrap_or(0),
                            text_stored: r.get::<_, Option<bool>>(18)?.unwrap_or(false),
                        })
                    },
                )
                .optional()?
        } else {
            None
        };
        // A PDF's stored text is not page HTML.
        let html_stored = pdf.is_none()
            && self.features.has(Need::RawHtml)
            && self.conn.query_row(
                "SELECT EXISTS (SELECT 1 FROM bodies WHERE url_id = ?1)",
                [id],
                |r| r.get(0),
            )?;
        let md = if content {
            "(SELECT p.meta_description FROM pages p WHERE p.url_id = u.id)"
        } else {
            "NULL"
        };
        let snippet = self
            .conn
            .query_row(
                &format!(
                    "SELECT u.title, {md} FROM urls u WHERE u.id = ?1
                     AND content_kind(u.content_type) = 'html' AND u.status BETWEEN 200 AND 299"
                ),
                [id],
                |r| {
                    Ok(Snippet {
                        title: r.get(0)?,
                        description: r.get(1)?,
                    })
                },
            )
            .optional()?;
        let link_metrics = if internal && self.features.has(Need::Analysis) {
            self.ensure_derived([crate::tabs::Derived::LinkMetrics])?;
            self.conn
                .query_row(
                    "SELECT score, rank, (SELECT COUNT(*) FROM link_metrics), unique_inlinks, followed_inlinks,
                            all_inlinks, unique_outlinks, external_outlinks, pct_linking
                     FROM link_metrics WHERE url_id = ?1",
                    [id],
                    |r| {
                        Ok(LinkMetrics {
                            score: r.get(0)?,
                            rank: r.get(1)?,
                            of: r.get(2)?,
                            unique_inlinks: r.get(3)?,
                            followed_inlinks: r.get(4)?,
                            all_inlinks: r.get(5)?,
                            unique_outlinks: r.get(6)?,
                            external_outlinks: r.get(7)?,
                            pct_linking: r.get(8)?,
                        })
                    },
                )
                .optional()?
        } else {
            None
        };
        let mut unavailable = BTreeMap::new();
        for (field, need) in [
            ("indexability", Need::Extraction),
            ("headers", Need::Extraction),
            ("headings", Need::Extraction),
            ("anchor_text", Need::Extraction),
            ("size", Need::Timing),
            ("response_time", Need::Timing),
            ("attempts", Need::Attempts),
            ("raw_html", Need::RawHtml),
            ("link_metrics", Need::Analysis),
            ("cookies", Need::Cookies),
        ] {
            if !self.features.has(need) {
                unavailable.insert(field.to_string(), need.unavailable_reason().to_string());
            }
        }
        if !self.features.has(Need::Pdf) && sql::content_kind(content_type.as_deref()) == "pdf" {
            unavailable.insert("pdf".into(), Need::Pdf.unavailable_reason().to_string());
        }
        let spelling = if self.features.has(Need::Spelling) {
            self.spelling(id)?
        } else {
            None
        };
        let amp = if self.features.has(Need::Amp) {
            self.amp(id)?
        } else {
            None
        };
        let render = if self.features.has(Need::Rendering) {
            self.render_detail(id)?
        } else {
            None
        };
        let accessibility =
            if self.features.has(Need::AccessibilityStatic) || self.features.has(Need::AccessibilityFull) {
                self.accessibility(id)?
            } else {
                None
            };
        let mobile = if self.features.has(Need::AccessibilityStatic) || self.features.has(Need::MobileLayout)
        {
            self.mobile(id)?
        } else {
            None
        };
        let reason_label = reason.as_deref().and_then(|r| {
            spiderlens_engine::crawl::NotCrawledReason::parse(r).map(|r| r.label().to_string())
        });
        Ok(UrlDetail {
            id,
            url_readable: sql::readable_url(&url),
            url,
            internal,
            final_state,
            status,
            status_text: status
                .and_then(|s| sql::status_text(i64::from(s)))
                .map(str::to_string),
            content_type,
            depth,
            error_label: error_class
                .as_deref()
                .map(|c| sql::error_class_label(c).to_string()),
            error_class,
            error_message,
            not_crawled_reason: reason_label,
            reason_detail,
            robots_rule,
            found_on,
            discovered_via: source,
            in_sitemap,
            fetched_at,
            redirect_over_limit: redirects.len().saturating_sub(1) > limit,
            redirects,
            redirect_loop,
            inlinks,
            outlinks,
            images,
            size,
            response_ms,
            indexability,
            headers,
            headings,
            html_stored,
            link_metrics,
            cookies: self.url_cookies(id)?,
            attempts,
            error_reason,
            manual_retry,
            amp,
            accessibility,
            mobile,
            pdf,
            snippet,
            spelling,
            render,
            unavailable,
        })
    }

    /// The rendering section of one page: its `renders` row and, for a
    /// page with a rendered version, the comparison with both values of
    /// every element that changed. Reads `main` tables only, so it gives
    /// the same answer whichever values the connection shows.
    fn render_detail(&self, id: i64) -> Result<Option<RenderDetail>> {
        let row = self
            .conn
            .query_row(
                "SELECT r.state, r.reason, r.reason_detail, r.attempts, r.render_ms, r.window_w, r.window_h,
                        r.js_redirect, r.js_redirect_id, r.console_errors, r.console_warnings,
                        r.resources_failed, r.resources_blocked, r.bot_challenge,
                        EXISTS (SELECT 1 FROM main.rendered_bodies b WHERE b.url_id = r.url_id),
                        EXISTS (SELECT 1 FROM main.screenshots s WHERE s.url_id = r.url_id)
                 FROM main.renders r WHERE r.url_id = ?1",
                [id],
                |r| {
                    let reason: Option<String> = r.get(1)?;
                    Ok(RenderDetail {
                        state: r.get(0)?,
                        reason_label: reason.as_deref().map(|x| render_reason_label(x).to_string()),
                        reason,
                        reason_detail: r.get(2)?,
                        attempts: r.get::<_, Option<u32>>(3)?.unwrap_or(0),
                        render_ms: r.get(4)?,
                        window_w: r.get(5)?,
                        window_h: r.get(6)?,
                        js_redirect: r.get(7)?,
                        js_redirect_id: r.get(8)?,
                        console_errors: r.get::<_, Option<u32>>(9)?.unwrap_or(0),
                        console_warnings: r.get::<_, Option<u32>>(10)?.unwrap_or(0),
                        resources_failed: r.get::<_, Option<u32>>(11)?.unwrap_or(0),
                        resources_blocked: r.get::<_, Option<u32>>(12)?.unwrap_or(0),
                        bot_challenge: r.get::<_, Option<bool>>(13)?.unwrap_or(false),
                        comparison: None,
                        rendered_html_stored: r.get(14)?,
                        screenshot_stored: r.get(15)?,
                    })
                },
            )
            .optional()?;
        let Some(mut detail) = row else {
            return Ok(None);
        };
        detail.comparison = self.render_comparison(id)?;
        Ok(Some(detail))
    }

    fn render_comparison(&self, id: i64) -> Result<Option<RenderComparison>> {
        type Diffs = [String; 7];
        let row = self
            .conn
            .query_row(
                "SELECT words_raw, words_rendered, words_change, links_raw, links_rendered, links_rendered_only,
                        links_raw_only, title_diff, description_diff, h1_diff, canonical_diff, noindex_diff,
                        nofollow_diff, hreflang_diff, structured_rendered_only, dependence, dep_text,
                        dep_headings, dep_internal_links, dep_structured, dep_images, raw_has_title,
                        raw_has_description, raw_has_canonical, raw_has_directives
                 FROM main.render_comparisons WHERE url_id = ?1",
                [id],
                |r| {
                    let diffs: Diffs = std::array::from_fn(|i| {
                        r.get::<_, Option<String>>(7 + i)
                            .ok()
                            .flatten()
                            .unwrap_or_else(|| "same".to_string())
                    });
                    let n = |i: usize| -> rusqlite::Result<u32> { Ok(r.get::<_, Option<u32>>(i)?.unwrap_or(0)) };
                    let flag = |i: usize| -> rusqlite::Result<bool> {
                        Ok(r.get::<_, Option<bool>>(i)?.unwrap_or(false))
                    };
                    Ok((
                        (n(0)?, n(1)?, r.get::<_, Option<i64>>(2)?.unwrap_or(0)),
                        (n(3)?, n(4)?, n(5)?, n(6)?),
                        diffs,
                        (n(14)?, r.get::<_, Option<f64>>(15)?.unwrap_or(0.0)),
                        [
                            r.get::<_, Option<f64>>(16)?,
                            r.get::<_, Option<f64>>(17)?,
                            r.get::<_, Option<f64>>(18)?,
                            r.get::<_, Option<f64>>(19)?,
                            r.get::<_, Option<f64>>(20)?,
                        ],
                        [flag(21)?, flag(22)?, flag(23)?, flag(24)?],
                    ))
                },
            )
            .optional()?;
        let Some((words, links, diffs, (structured, dependence), dep, has)) = row else {
            return Ok(None);
        };
        let one = |sql: &str| -> Result<Option<String>> {
            Ok(self
                .conn
                .query_row(sql, [id], |r| r.get::<_, Option<String>>(0))
                .optional()?
                .flatten())
        };
        // Both values of an element, read only when it differs.
        let change = |diff: &str, raw_sql: &str, rendered_sql: &str| -> Result<ElementChange> {
            let (raw, rendered) = if diff == "same" {
                (None, None)
            } else {
                (one(raw_sql)?, one(rendered_sql)?)
            };
            Ok(ElementChange {
                diff: diff.to_string(),
                raw,
                rendered,
            })
        };
        // A flag has no text of its own: name it on the side that has it.
        let flag_change = |diff: &str, name: &str| ElementChange {
            diff: diff.to_string(),
            raw: matches!(diff, "removed").then(|| name.to_string()),
            rendered: matches!(diff, "added").then(|| name.to_string()),
        };
        let [title, description, h1, canonical, noindex, nofollow, hreflang] = &diffs;
        let hreflang_list = |extra: &str| {
            format!(
                "SELECT group_concat(x.v, ', ') FROM (SELECT l.lang || ' ' || l.raw AS v FROM main.links l
                 WHERE l.from_id = ?1 AND l.kind = 'hreflang'{extra} ORDER BY l.rowid) x"
            )
        };
        Ok(Some(RenderComparison {
            words_raw: words.0,
            words_rendered: words.1,
            words_change: words.2,
            links_raw: links.0,
            links_rendered: links.1,
            links_rendered_only: links.2,
            links_raw_only: links.3,
            title: change(
                title,
                "SELECT title FROM main.urls WHERE id = ?1",
                "SELECT title FROM main.rendered_pages WHERE url_id = ?1",
            )?,
            description: change(
                description,
                "SELECT meta_description FROM main.pages WHERE url_id = ?1",
                "SELECT meta_description FROM main.rendered_pages WHERE url_id = ?1",
            )?,
            h1: change(
                h1,
                "SELECT h1_1 FROM main.pages WHERE url_id = ?1",
                "SELECT h1_1 FROM main.rendered_pages WHERE url_id = ?1",
            )?,
            canonical: change(
                canonical,
                "SELECT canonical FROM main.urls WHERE id = ?1",
                "SELECT canonical FROM main.rendered_pages WHERE url_id = ?1",
            )?,
            noindex: flag_change(noindex, "noindex"),
            nofollow: flag_change(nofollow, "nofollow"),
            hreflang: change(
                hreflang,
                &hreflang_list(""),
                "SELECT group_concat(v, ', ') FROM (
                       SELECT l.lang || ' ' || l.raw AS v FROM main.links l
                       WHERE l.from_id = ?1 AND l.kind = 'hreflang' AND NOT EXISTS
                         (SELECT 1 FROM main.raw_only_links g WHERE g.from_id = l.from_id AND g.pos = l.pos)
                       UNION ALL
                       SELECT l.lang || ' ' || l.raw FROM main.rendered_links l
                       WHERE l.from_id = ?1 AND l.kind = 'hreflang')",
            )?,
            structured_rendered_only: structured,
            dependence,
            dep_text: dep[0],
            dep_headings: dep[1],
            dep_internal_links: dep[2],
            dep_structured: dep[3],
            dep_images: dep[4],
            raw_has_title: has[0],
            raw_has_description: has[1],
            raw_has_canonical: has[2],
            raw_has_directives: has[3],
        }))
    }

    fn accessibility(&self, id: i64) -> Result<Option<A11yDetail>> {
        let audit = self
            .conn
            .query_row(
                "SELECT static_state, full_state, full_reason FROM a11y_audits WHERE url_id = ?1",
                [id],
                |r| {
                    Ok((
                        r.get::<_, Option<String>>(0)?,
                        r.get::<_, Option<String>>(1)?,
                        r.get::<_, Option<String>>(2)?,
                    ))
                },
            )
            .optional()?;
        let mut stmt = self.conn.prepare_cached(
            "SELECT id, check_id, kind, source, impact, wcag, nodes_total FROM a11y_results WHERE url_id = ?1 \
             ORDER BY CASE impact WHEN 'critical' THEN 0 WHEN 'serious' THEN 1 WHEN 'moderate' THEN 2 \
             WHEN 'minor' THEN 3 ELSE 4 END, check_id, kind",
        )?;
        let rows = stmt
            .query_map([id], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    A11yCheckDetail {
                        check_id: r.get(1)?,
                        title: String::new(),
                        kind: r.get(2)?,
                        source: r.get(3)?,
                        impact: r.get(4)?,
                        wcag: r.get(5)?,
                        nodes_total: r.get::<_, Option<u32>>(6)?.unwrap_or(0),
                        help_url: None,
                        nodes: Vec::new(),
                    },
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        if audit.is_none() && rows.is_empty() {
            return Ok(None);
        }
        let mut nodes = self.conn.prepare_cached(
            "SELECT selector, excerpt, data, x, y, w, h FROM a11y_nodes WHERE result_id = ?1 ORDER BY pos",
        )?;
        let mut checks = Vec::with_capacity(rows.len());
        let mut capped = false;
        for (result_id, mut check) in rows {
            check.nodes = nodes
                .query_map([result_id], |r| {
                    let bbox = match (
                        r.get::<_, Option<f64>>(3)?,
                        r.get::<_, Option<f64>>(4)?,
                        r.get::<_, Option<f64>>(5)?,
                        r.get::<_, Option<f64>>(6)?,
                    ) {
                        (Some(x), Some(y), Some(w), Some(h)) => Some(vec![x, y, w, h]),
                        _ => None,
                    };
                    Ok(A11yNodeDetail {
                        selector: r.get::<_, Option<String>>(0)?.unwrap_or_default(),
                        excerpt: r.get::<_, Option<String>>(1)?.unwrap_or_default(),
                        data: r.get(2)?,
                        bbox,
                    })
                })?
                .collect::<rusqlite::Result<_>>()?;
            capped |= check.nodes_total as usize > check.nodes.len();
            check.title = a11y_title(&check.check_id);
            check.help_url = (!check.check_id.starts_with("static."))
                .then(|| format!("https://dequeuniversity.com/rules/axe/4.14/{}", check.check_id));
            checks.push(check);
        }
        let (static_state, full_state, full_reason) = audit.unwrap_or((None, None, None));
        Ok(Some(A11yDetail {
            static_state,
            full_state,
            full_reason,
            checks,
            capped,
            disclaimer: A11Y_DISCLAIMER.to_string(),
        }))
    }

    fn mobile(&self, id: i64) -> Result<Option<MobileDetail>> {
        Ok(self
            .conn
            .query_row(
                "SELECT viewport, viewport_state, plugins, \"window\", measured, content_px, window_px, widest, \
                 small_text_share, small_targets, examples FROM mobile_measures WHERE url_id = ?1",
                [id],
                |r| {
                    let examples: Option<String> = r.get(10)?;
                    Ok(MobileDetail {
                        viewport: r.get(0)?,
                        viewport_state: r.get(1)?,
                        plugins: r.get::<_, Option<i64>>(2)?.and_then(|n| u32::try_from(n).ok()),
                        window: r.get(3)?,
                        measured: r.get(4)?,
                        content_px: r.get(5)?,
                        window_px: r.get(6)?,
                        widest: r.get(7)?,
                        small_text_share: r.get(8)?,
                        small_targets: r.get::<_, Option<i64>>(9)?.and_then(|n| u32::try_from(n).ok()),
                        examples: examples
                            .and_then(|e| serde_json::from_str(&e).ok())
                            .unwrap_or_default(),
                    })
                },
            )
            .optional()?)
    }

    fn amp(&self, id: i64) -> Result<Option<AmpDetail>> {
        let Some(format) = self
            .conn
            .query_row("SELECT format FROM amp_pages WHERE url_id = ?1", [id], |r| {
                r.get::<_, String>(0)
            })
            .optional()?
        else {
            return Ok(None);
        };
        let failures = self
            .conn
            .prepare_cached("SELECT check_id, detail, pos FROM amp_checks WHERE url_id = ?1 ORDER BY rowid")?
            .query_map([id], |r| {
                Ok(AmpFailure {
                    check: r.get(0)?,
                    detail: r.get(1)?,
                    pos: r.get(2)?,
                })
            })?
            .collect::<rusqlite::Result<_>>()?;
        let paired = self
            .conn
            .query_row(
                "SELECT EXISTS (SELECT 1 FROM urls c JOIN links l ON l.from_id = c.id
                     WHERE c.url = u.canonical AND l.kind = 'amphtml' AND l.to_id = u.id)
                 FROM urls u WHERE u.id = ?1 AND u.canonical IS NOT NULL AND u.canonical <> u.url",
                [id],
                |r| r.get::<_, bool>(0),
            )
            .optional()?;
        let paired = if format == "website" { paired } else { None };
        Ok(Some(AmpDetail {
            format,
            failures,
            paired,
        }))
    }

    fn spelling(&self, id: i64) -> Result<Option<SpellDetail>> {
        let page = self
            .conn
            .query_row(
                "SELECT lang, status, words, errors, lang_mismatch FROM spell_pages WHERE url_id = ?1",
                [id],
                |r| {
                    Ok(SpellDetail {
                        lang: r.get(0)?,
                        status: r.get(1)?,
                        words: r.get::<_, Option<u32>>(2)?.unwrap_or(0),
                        errors: r.get::<_, Option<u32>>(3)?.unwrap_or(0),
                        lang_mismatch: r.get::<_, Option<bool>>(4)?.unwrap_or(false),
                        findings: Vec::new(),
                    })
                },
            )
            .optional()?;
        let Some(mut page) = page else { return Ok(None) };
        page.findings = self
            .conn
            .prepare_cached(
                "SELECT word, source, context, suggestions, site_wide FROM spell_findings WHERE url_id = ?1
                 ORDER BY CASE source WHEN 'title' THEN 0 WHEN 'description' THEN 1 WHEN 'heading' THEN 2 ELSE 3 END, pos
                 LIMIT 200",
            )?
            .query_map([id], |r| {
                let suggestions: Option<String> = r.get(3)?;
                Ok(SpellFinding {
                    word: r.get(0)?,
                    source: r.get(1)?,
                    context: r.get(2)?,
                    suggestions: suggestions
                        .and_then(|s| serde_json::from_str(&s).ok())
                        .unwrap_or_default(),
                    site_wide: r.get::<_, Option<bool>>(4)?.unwrap_or(false),
                })
            })?
            .collect::<rusqlite::Result<_>>()?;
        Ok(Some(page))
    }

    /// The stored HTML of one page, decoded as extraction read it. Errors
    /// when the URL is not in the file or the stored bytes are damaged.
    pub fn page_html(&mut self, id: i64) -> Result<PageHtml> {
        let exists: bool =
            self.conn
                .query_row("SELECT EXISTS (SELECT 1 FROM urls WHERE id = ?1)", [id], |r| {
                    r.get(0)
                })?;
        if !exists {
            return Err(Error::invalid(format!("URL {id} is not in this crawl")));
        }
        let none = |missing| PageHtml {
            html: None,
            missing: Some(missing),
            bytes: 0,
            encoding: None,
            encoding_errors: false,
        };
        if !self.features.has(Need::RawHtml) {
            return Ok(none(HtmlMissing::NotStored));
        }
        let row = self
            .conn
            .query_row(
                "SELECT b.codec, b.size, b.body, p.encoding
                 FROM bodies b LEFT JOIN pages p ON p.url_id = b.url_id WHERE b.url_id = ?1",
                [id],
                |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, i64>(1)?,
                        r.get::<_, Vec<u8>>(2)?,
                        r.get::<_, Option<String>>(3)?,
                    ))
                },
            )
            .optional()?;
        let Some((codec, size, packed, encoding)) = row else {
            return Ok(none(HtmlMissing::NoBody));
        };
        let raw = body::unpack(&codec, size, &packed).map_err(Error::Unavailable)?;
        let (text, used, encoding_errors) = body::to_text(&raw, encoding.as_deref());
        Ok(PageHtml {
            html: Some(text),
            missing: None,
            bytes: raw.len() as u64,
            encoding: Some(used.to_string()),
            encoding_errors,
        })
    }

    /// The stored HTML of one page, the raw version or the rendered one
    /// (spec 014 FR-032). The rendered version is the serialised DOM,
    /// always UTF-8.
    pub fn page_html_version(&mut self, id: i64, version: HtmlVersion) -> Result<PageHtml> {
        if version == HtmlVersion::Raw {
            return self.page_html(id);
        }
        let exists: bool =
            self.conn
                .query_row("SELECT EXISTS (SELECT 1 FROM urls WHERE id = ?1)", [id], |r| {
                    r.get(0)
                })?;
        if !exists {
            return Err(Error::invalid(format!("URL {id} is not in this crawl")));
        }
        let none = |missing| PageHtml {
            html: None,
            missing: Some(missing),
            bytes: 0,
            encoding: None,
            encoding_errors: false,
        };
        if !self.features.has(Need::RenderedHtml) {
            return Ok(none(HtmlMissing::NotStored));
        }
        let row = self
            .conn
            .query_row(
                "SELECT codec, size, body FROM main.rendered_bodies WHERE url_id = ?1",
                [id],
                |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, i64>(1)?,
                        r.get::<_, Vec<u8>>(2)?,
                    ))
                },
            )
            .optional()?;
        let Some((codec, size, packed)) = row else {
            return Ok(none(HtmlMissing::NoBody));
        };
        let bytes = body::unpack(&codec, size, &packed).map_err(Error::Unavailable)?;
        let text = String::from_utf8_lossy(&bytes);
        Ok(PageHtml {
            encoding_errors: matches!(text, std::borrow::Cow::Owned(_)),
            html: Some(text.into_owned()),
            missing: None,
            bytes: bytes.len() as u64,
            encoding: Some("utf-8".to_string()),
        })
    }

    /// The stored screenshot of a rendered page (spec 014 FR-033).
    pub fn page_screenshot(&mut self, id: i64) -> Result<PageScreenshot> {
        let none = |missing| PageScreenshot {
            webp_base64: None,
            missing: Some(missing),
            width: 0,
            height: 0,
        };
        if !self.features.has(Need::Screenshots) {
            return Ok(none(ScreenshotMissing::NotTaken));
        }
        let row = self
            .conn
            .query_row(
                "SELECT image, width, height FROM main.screenshots WHERE url_id = ?1",
                [id],
                |r| Ok((r.get::<_, Vec<u8>>(0)?, r.get::<_, u32>(1)?, r.get::<_, u32>(2)?)),
            )
            .optional()?;
        let Some((image, width, height)) = row else {
            return Ok(none(ScreenshotMissing::NoImage));
        };
        Ok(PageScreenshot {
            webp_base64: Some(base64::Engine::encode(
                &base64::engine::general_purpose::STANDARD,
                image,
            )),
            missing: None,
            width,
            height,
        })
    }

    /// Both stored versions of a page: the raw and the rendered HTML with
    /// their sizes in bytes.
    fn html_pair(&mut self, id: i64) -> Result<(String, String, u64, u64)> {
        let raw = self.page_html_version(id, HtmlVersion::Raw)?;
        let rendered = self.page_html_version(id, HtmlVersion::Rendered)?;
        let (raw_bytes, rendered_bytes) = (raw.bytes, rendered.bytes);
        match (raw.html, rendered.html) {
            (Some(a), Some(b)) => Ok((a, b, raw_bytes, rendered_bytes)),
            (_, None) => Err(Error::Unavailable(
                "The rendered HTML of this page is not stored in this crawl.".into(),
            )),
            (None, _) => Err(Error::Unavailable(
                "The raw HTML of this page is not stored in this crawl.".into(),
            )),
        }
    }

    /// The differences between a page's normalised raw and rendered HTML
    /// (spec 014 FR-028): `max_hunks` hunks from hunk `from_hunk`. Needs
    /// both versions stored.
    pub fn html_diff(
        &mut self,
        id: i64,
        from_hunk: u32,
        max_hunks: u32,
    ) -> Result<crate::html_diff::HtmlDiff> {
        let (a, b, raw_bytes, rendered_bytes) = self.html_pair(id)?;
        let mut d = crate::html_diff::diff(&a, &b, from_hunk, max_hunks.min(1000));
        d.raw_bytes = raw_bytes;
        d.rendered_bytes = rendered_bytes;
        Ok(d)
    }

    /// The same differences as a unified diff of the normalised versions
    /// (`spiderlens html --diff`); empty when there are none.
    pub fn html_unified(&mut self, id: i64) -> Result<String> {
        let (a, b, _, _) = self.html_pair(id)?;
        Ok(crate::html_diff::unified(&a, &b))
    }

    /// Follows stored redirect targets from `id`, stopping at the first URL
    /// that does not redirect, at a loop, or after 100 hops.
    fn redirect_chain(&self, id: i64) -> Result<(Vec<RedirectHop>, bool)> {
        let sql = format!("SELECT x.url, x.status, {FINAL_STATE}, x.redirect_to FROM urls x WHERE x.id = ?1");
        let mut stmt = self.conn.prepare(&sql)?;
        let mut hops: Vec<RedirectHop> = Vec::new();
        let mut cur = Some(id);
        let mut is_loop = false;
        while let Some(c) = cur {
            if hops.iter().any(|h| h.id == c) {
                is_loop = true;
                break;
            }
            if hops.len() > 100 {
                break;
            }
            let Some((url, status, final_state, next)) = stmt
                .query_row([c], |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, Option<u16>>(1)?,
                        r.get::<_, String>(2)?,
                        r.get::<_, Option<i64>>(3)?,
                    ))
                })
                .optional()?
            else {
                break;
            };
            hops.push(RedirectHop {
                id: c,
                url,
                status,
                final_state,
            });
            cur = next;
        }
        if hops.len() == 1 {
            hops.clear();
        }
        Ok((hops, is_loop))
    }

    /// A page of one of the URL's lists (FR-022, FR-023, FR-025).
    pub fn list(&mut self, id: i64, kind: ListKind, from: u32, count: u32) -> Result<ListPage> {
        self.list_at(id, kind, None, from, count)
    }

    /// [`Reader::list`] keeping only the links at one position (spec 023
    /// FR-024): a `Position` name or "unknown". Needs the positions worked
    /// out, and applies to the Inlinks and Outlinks lists only.
    pub fn list_at(
        &mut self,
        id: i64,
        kind: ListKind,
        position: Option<&str>,
        from: u32,
        count: u32,
    ) -> Result<ListPage> {
        self.list_values(id, kind, position, None, from, count)
    }

    /// [`Reader::list_at`] for the raw or the rendered version of the page
    /// (spec 014): `None` shows rendered values when the crawl has them.
    /// In rendered mode the link lists also hold the links JavaScript
    /// removed, and every entry says whether it was found in the raw HTML
    /// only, the rendered page only, or both.
    pub fn list_values(
        &mut self,
        id: i64,
        kind: ListKind,
        position: Option<&str>,
        values: Option<Values>,
        from: u32,
        count: u32,
    ) -> Result<ListPage> {
        let count = count.min(1000);
        let rendered = Values::resolve(values, &self.features) == Values::Rendered;
        if let Some(p) = position {
            if p != "unknown" && !spiderlens_config::Position::ALL.iter().any(|x| x.as_str() == p) {
                return Err(Error::invalid(format!("unknown position {p}")));
            }
            if !matches!(kind, ListKind::Inlinks | ListKind::Outlinks) {
                return Err(Error::invalid("only inlinks and outlinks have positions"));
            }
            if !self.has_derived(Derived::LinkPositions)? {
                return Err(Error::Unavailable(
                    "Link positions are not worked out yet.".into(),
                ));
            }
        }
        let (cond, join) = match kind {
            ListKind::Inlinks => ("l.to_id = ?1", "l.from_id"),
            ListKind::Outlinks => ("l.from_id = ?1", "l.to_id"),
            ListKind::Images => ("l.from_id = ?1 AND l.kind = 'image'", "l.to_id"),
            ListKind::Headings => return self.headings(id, from, count, rendered),
            ListKind::Resources | ListKind::Console => return self.render_list(id, kind, from, count),
        };
        // The links of rendered pages (spec 014): the raw ones, the ones
        // JavaScript removed marked, and the ones found only after rendering.
        let (source, origin, order) = if self.features.has(Need::Rendering) {
            let raw_branch = format!(
                "SELECT l.rowid AS ord, l.from_id, l.to_id, l.raw, l.kind, l.nofollow, l.followed, l.text, l.alt,
                        l.sponsored, l.ugc, l.pos,
                        CASE WHEN rp.url_id IS NULL THEN NULL WHEN g.pos IS NULL THEN 'both' ELSE 'raw' END AS origin
                 FROM links l LEFT JOIN main.rendered_pages rp ON rp.url_id = l.from_id
                 LEFT JOIN main.raw_only_links g ON g.from_id = l.from_id AND g.pos = l.pos
                 WHERE {cond}"
            );
            let src = if rendered {
                format!(
                    "({raw_branch} UNION ALL
                      SELECT 1099511627776 + l.rowid, l.from_id, l.to_id, l.raw, l.kind, l.nofollow, l.followed,
                             l.text, l.alt, l.sponsored, l.ugc, l.pos, 'rendered'
                      FROM main.rendered_links l WHERE {cond})"
                )
            } else {
                format!("({raw_branch})")
            };
            (src, "l.origin", "l.ord")
        } else {
            ("links".to_string(), "NULL", "l.rowid")
        };
        // The conditions are inside the branches already.
        let cond = if source == "links" {
            cond.to_string()
        } else {
            "1".to_string()
        };
        let position_of = "CASE WHEN l.kind = 'anchor' THEN COALESCE((SELECT CASE p.position WHEN 1 THEN 'navigation' \
             WHEN 2 THEN 'header' WHEN 3 THEN 'content' WHEN 4 THEN 'sidebar' WHEN 5 THEN 'footer' END \
             FROM link_positions p WHERE p.from_id = l.from_id AND p.pos = l.pos), 'unknown') END";
        let cond = match position {
            Some(_) => format!("{cond} AND {position_of} = ?4"),
            None => format!("{cond} AND ?4 IS NULL"),
        };
        let total: i64 = self.conn.query_row(
            &format!("SELECT COUNT(*) FROM {source} l WHERE {cond}"),
            rusqlite::params![id, None::<i64>, None::<i64>, position],
            |r| r.get(0),
        )?;
        let text = if !self.features.has(Need::Extraction) {
            "NULL, 0, 0"
        } else if kind == ListKind::Images {
            "l.alt, l.sponsored, l.ugc"
        } else {
            "COALESCE(NULLIF(l.text, ''), l.alt), l.sponsored, l.ugc"
        };
        let position_col = if kind != ListKind::Images && self.has_derived(Derived::LinkPositions)? {
            position_of
        } else {
            "NULL"
        };
        let sql = format!(
            "SELECT x.id, x.url, l.raw, l.kind, l.nofollow, l.followed, x.internal, x.status, {FINAL_STATE}, {text},
                    {position_col}, {origin}
             FROM {source} l JOIN urls x ON x.id = {join}
             WHERE {cond} ORDER BY {order} LIMIT ?2 OFFSET ?3"
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt
            .query_map(rusqlite::params![id, count, from, position], |r| {
                Ok(LinkRow {
                    id: r.get(0)?,
                    url: r.get(1)?,
                    raw: r.get(2)?,
                    kind: r.get(3)?,
                    nofollow: r.get(4)?,
                    followed: r.get(5)?,
                    internal: r.get(6)?,
                    status: r.get(7)?,
                    final_state: r.get(8)?,
                    text: r.get(9)?,
                    sponsored: r.get(10)?,
                    ugc: r.get(11)?,
                    position: r.get(12)?,
                    origin: r.get(13)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(ListPage {
            total: total as u64,
            from,
            rows,
            headings: vec![],
            resources: vec![],
            console: vec![],
            unavailable: None,
        })
    }

    fn headings(&self, id: i64, from: u32, count: u32, rendered: bool) -> Result<ListPage> {
        let empty = |unavailable| ListPage {
            total: 0,
            from,
            rows: vec![],
            headings: vec![],
            resources: vec![],
            console: vec![],
            unavailable,
        };
        if !self.features.has(Need::Extraction) {
            return Ok(empty(Some(Need::Extraction.unavailable_reason().to_string())));
        }
        // A rendered page's headings in rendered order, then the ones
        // JavaScript removed; otherwise the raw ones, marked when they are
        // gone after rendering (spec 014 FR-028).
        let page_rendered = self.features.has(Need::Rendering)
            && self.conn.query_row(
                "SELECT EXISTS (SELECT 1 FROM main.rendered_pages WHERE url_id = ?1)",
                [id],
                |r| r.get::<_, bool>(0),
            )?;
        let (table, order, origin) = if page_rendered && rendered {
            (
                "main.rendered_text",
                "CASE t.origin WHEN 'raw' THEN 1 ELSE 0 END, t.pos",
                "t.origin",
            )
        } else if page_rendered {
            (
                "page_text",
                "t.pos",
                "CASE WHEN EXISTS (SELECT 1 FROM main.rendered_text g WHERE g.url_id = t.url_id \
                 AND g.kind = 'heading' AND g.origin = 'raw' AND g.pos = t.pos) THEN 'raw' ELSE 'both' END",
            )
        } else {
            ("page_text", "t.pos", "NULL")
        };
        let total: i64 = self.conn.query_row(
            &format!("SELECT COUNT(*) FROM {table} t WHERE t.url_id = ?1 AND t.kind = 'heading'"),
            [id],
            |r| r.get(0),
        )?;
        let mut stmt = self.conn.prepare(&format!(
            "SELECT t.level, t.text, {origin} FROM {table} t WHERE t.url_id = ?1 AND t.kind = 'heading'
             ORDER BY {order} LIMIT ?2 OFFSET ?3"
        ))?;
        let headings = stmt
            .query_map(rusqlite::params![id, count, from], |r| {
                Ok(HeadingRow {
                    level: r.get(0)?,
                    text: r.get(1)?,
                    origin: r.get(2)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(ListPage {
            total: total as u64,
            headings,
            ..empty(None)
        })
    }

    /// The Resources or Console list of a rendered page (spec 014 FR-018,
    /// FR-043), in the order the browser reported them.
    fn render_list(&self, id: i64, kind: ListKind, from: u32, count: u32) -> Result<ListPage> {
        let mut page = ListPage {
            total: 0,
            from,
            rows: vec![],
            headings: vec![],
            resources: vec![],
            console: vec![],
            unavailable: None,
        };
        if !self.features.has(Need::Rendering) {
            page.unavailable = Some(Need::Rendering.unavailable_reason().to_string());
            return Ok(page);
        }
        let table = if kind == ListKind::Resources {
            "render_resources"
        } else {
            "console_entries"
        };
        let total: i64 = self.conn.query_row(
            &format!("SELECT COUNT(*) FROM main.{table} WHERE url_id = ?1"),
            [id],
            |r| r.get(0),
        )?;
        page.total = total as u64;
        if kind == ListKind::Resources {
            let mut stmt = self.conn.prepare(
                "SELECT url, type, host, third_party, outcome, status, error, robots_rule, bytes
                 FROM main.render_resources WHERE url_id = ?1 ORDER BY pos LIMIT ?2 OFFSET ?3",
            )?;
            page.resources = stmt
                .query_map(rusqlite::params![id, count, from], |r| {
                    Ok(ResourceRow {
                        url: r.get(0)?,
                        kind: r.get(1)?,
                        host: r.get(2)?,
                        third_party: r.get(3)?,
                        outcome: r.get(4)?,
                        status: r.get(5)?,
                        error: r.get(6)?,
                        robots_rule: r.get(7)?,
                        bytes: r.get(8)?,
                    })
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
        } else {
            let mut stmt = self.conn.prepare(
                "SELECT level, message, source_url, line, col
                 FROM main.console_entries WHERE url_id = ?1 ORDER BY pos LIMIT ?2 OFFSET ?3",
            )?;
            page.console = stmt
                .query_map(rusqlite::params![id, count, from], |r| {
                    Ok(ConsoleRow {
                        level: r.get(0)?,
                        message: r.get(1)?,
                        source_url: r.get(2)?,
                        line: r.get(3)?,
                        col: r.get(4)?,
                    })
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
        }
        Ok(page)
    }
}
