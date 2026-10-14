//! Page data extraction (spec 005): one parse and one walk per HTML page
//! (plan D1). Everything here is a pure function of the response: bytes and
//! headers in, data out, no I/O and no crawl state (plan D2, FR-032).

pub mod body;
pub mod directives;
mod link_header;
mod microdata;
pub mod text;
mod walk;

pub(crate) use walk::visible_text_into;

use serde::Serialize;
use url::Url;

use crate::crawl::LinkKind;
pub use directives::{Directive, DirectiveSource};

/// Where a reference was declared.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RefSource {
    Html,
    Header,
}

impl RefSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Html => "html",
            Self::Header => "header",
        }
    }
}

/// One reference from the page to a URL: a link, image, canonical,
/// hreflang, next/prev or meta refresh (data-model.md, `links`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Ref {
    pub kind: LinkKind,
    /// The reference exactly as written.
    pub raw: String,
    /// Anchor text (anchors), alt text (areas).
    pub text: Option<String>,
    /// Anchors: alt of an image inside. Images: their alt. None = missing.
    pub alt: Option<String>,
    pub rel: Option<String>,
    pub nofollow: bool,
    pub sponsored: bool,
    pub ugc: bool,
    pub source: RefSource,
    /// Link elements and meta refresh: inside `<head>`.
    pub in_head: Option<bool>,
    pub noscript: bool,
    /// hreflang language code.
    pub lang: Option<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    /// Image URL taken from `srcset`.
    pub srcset: bool,
    /// Images: inside a link. Anchors: wrap an image.
    pub in_link: bool,
}

impl Ref {
    pub(crate) fn new(kind: LinkKind, raw: &str, source: RefSource) -> Self {
        Self {
            kind,
            raw: raw.to_string(),
            text: None,
            alt: None,
            rel: None,
            nofollow: false,
            sponsored: false,
            ugc: false,
            source,
            in_head: None,
            noscript: false,
            lang: None,
            width: None,
            height: None,
            srcset: false,
            in_link: false,
        }
    }
}

/// A title or meta description with where it was found (FR-010).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TextItem {
    pub text: String,
    pub in_head: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Heading {
    pub level: u8,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MetaRefresh {
    pub delay: f64,
    /// The target as written, if any.
    pub url: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StructuredKind {
    JsonLd,
    Microdata,
}

impl StructuredKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::JsonLd => "json_ld",
            Self::Microdata => "microdata",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StructuredBlock {
    pub kind: StructuredKind,
    /// JSON-LD as written; microdata as a JSON item tree.
    pub raw: String,
    pub parseable: bool,
}

/// Facts from response headers, for every resource (FR-003).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct HeaderFacts {
    pub directives: Vec<Directive>,
    pub refs: Vec<Ref>,
}

/// Everything extracted from one HTML page.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PageData {
    /// What relative references resolve against: the first `<base href>`
    /// or the page URL (FR-021).
    pub base: Url,
    /// The encoding the body was decoded with.
    pub encoding: &'static str,
    pub encoding_errors: bool,
    /// The body was cut off; data comes from the part received.
    pub partial: bool,
    /// Labelled HTML but empty or binary.
    pub no_content: bool,
    pub titles: Vec<TextItem>,
    pub meta_descriptions: Vec<TextItem>,
    pub headings: Vec<Heading>,
    pub lang: Option<String>,
    pub word_count: u32,
    pub text_hash: i64,
    /// Meta tag directives (header ones are in [`HeaderFacts`]).
    pub directives: Vec<Directive>,
    /// In document order.
    pub refs: Vec<Ref>,
    pub meta_refresh: Option<MetaRefresh>,
    pub structured: Vec<StructuredBlock>,
    /// Custom rule outcomes; `None` when the crawl has no rules (spec 013).
    pub custom: Option<crate::custom::PageRules>,
}

impl PageData {
    pub fn title(&self) -> Option<&str> {
        self.titles.first().map(|t| t.text.as_str())
    }

    pub fn meta_description(&self) -> Option<&str> {
        self.meta_descriptions.first().map(|t| t.text.as_str())
    }

    /// The placeholder stored when extraction failed (FR-033).
    pub(crate) fn failed() -> Self {
        let mut d = Self::empty(Url::parse("about:blank").expect("valid"), "", false);
        d.no_content = false;
        d
    }

    fn empty(base: Url, encoding: &'static str, partial: bool) -> Self {
        Self {
            base,
            encoding,
            encoding_errors: false,
            partial,
            no_content: true,
            titles: Vec::new(),
            meta_descriptions: Vec::new(),
            headings: Vec::new(),
            lang: None,
            word_count: 0,
            text_hash: text::hash(""),
            directives: Vec::new(),
            refs: Vec::new(),
            meta_refresh: None,
            structured: Vec::new(),
            custom: None,
        }
    }
}

/// The input to [`extract_page`].
#[derive(Debug, Clone, Copy)]
pub struct PageInput<'a> {
    pub url: &'a Url,
    /// `charset` from the Content-Type header.
    pub charset: Option<&'a str>,
    pub body: &'a [u8],
    /// The body was cut off by the size limit or a dropped connection.
    pub truncated: bool,
    /// The crawl's robots.txt token, for crawler-specific meta tags.
    pub robots_token: &'a str,
}

/// Extracts everything spec 005 lists from one HTML body. Never fails: a
/// page that cannot be read comes back flagged (FR-008, FR-033).
pub fn extract_page(input: &PageInput<'_>) -> PageData {
    extract_page_with(input, None)
}

/// [`extract_page`] plus the crawl's custom rules on the same parse
/// (spec 013). Pages with no readable content get every rule as no match.
pub fn extract_page_with(input: &PageInput<'_>, rules: Option<&crate::custom::CompiledRules>) -> PageData {
    let declared = crate::title::declared_encoding(input.charset, input.body);
    let encoding = match declared {
        Some(enc) => enc,
        // Undeclared and not valid UTF-8: legacy pages of this kind are
        // almost always windows-1252, which is what browsers in Western
        // locales fall back to. A sequence cut by truncation does not count.
        None if !utf8_or_cut(input.body, input.truncated) => encoding_rs::WINDOWS_1252,
        None => encoding_rs::UTF_8,
    };
    if looks_binary(input.body) {
        let mut d = PageData::empty(input.url.clone(), encoding.name(), input.truncated);
        d.custom = rules.map(crate::custom::CompiledRules::on_empty_page);
        return d;
    }
    let (text, used, had_errors) = encoding.decode(input.body);
    let mut data = walk::walk_with(&text, input.url, input.robots_token, walk::PARSE_BUDGET, rules);
    data.encoding = used.name();
    data.encoding_errors = had_errors;
    data.partial |= input.truncated;
    data
}

/// Whether `body` is valid UTF-8, allowing a sequence cut off at the end
/// when the body was truncated.
fn utf8_or_cut(body: &[u8], truncated: bool) -> bool {
    match std::str::from_utf8(body) {
        Ok(_) => true,
        Err(e) => truncated && e.error_len().is_none(),
    }
}

/// Like [`extract_page`], but a panic inside the parser becomes an error
/// message instead of taking the crawl down (FR-033).
pub fn extract_page_caught(
    input: &PageInput<'_>,
    rules: Option<&crate::custom::CompiledRules>,
) -> Result<PageData, String> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| extract_page_with(input, rules))).map_err(|p| {
        let msg = p
            .downcast_ref::<&str>()
            .map(|s| s.to_string())
            .or_else(|| p.downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "unknown error".into());
        format!("Page extraction failed: {msg}")
    })
}

/// Empty, or several NUL bytes near the start (and not UTF-16): not a
/// document. A stray NUL or two is a broken page, which browsers still show.
fn looks_binary(body: &[u8]) -> bool {
    if body.iter().all(u8::is_ascii_whitespace) {
        return true;
    }
    let utf16 = body.starts_with(&[0xFF, 0xFE]) || body.starts_with(&[0xFE, 0xFF]);
    !utf16 && body[..body.len().min(1024)].iter().filter(|&&b| b == 0).count() > 2
}

/// Robots directives, canonicals and hreflang from `X-Robots-Tag` and
/// `Link` headers. Applies to every resource, HTML or not (FR-003).
pub fn header_facts(x_robots_tag: &[String], link: &[String]) -> HeaderFacts {
    HeaderFacts {
        directives: x_robots_tag
            .iter()
            .flat_map(|v| directives::parse_header(v))
            .collect(),
        refs: link.iter().flat_map(|v| link_header::parse(v)).collect(),
    }
}

#[cfg(test)]
mod tests;
