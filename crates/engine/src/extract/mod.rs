//! Page data extraction (spec 005): one parse and one walk per HTML page
//! (plan D1). Everything here is a pure function of the response: bytes and
//! headers in, data out, no I/O and no crawl state (plan D2, FR-032).

pub mod a11y;
pub mod agents;
pub mod amp;
pub mod body;
pub mod challenge;
pub mod compare;
pub mod directives;
mod link_header;
pub mod main_content;
mod microdata;
pub mod shingle;
pub mod text;
mod walk;

pub(crate) use walk::visible_text_into;

use serde::Serialize;
use url::Url;

use crate::crawl::LinkKind;
pub use agents::AgentData;
pub use directives::{Directive, DirectiveSource};
pub use main_content::{ContentRules, MainContent, MainSource, MainText, Paragraph, Run};

/// Where a reference was declared.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RefSource {
    Html,
    Header,
    /// A link annotation in a PDF (spec 027 part C).
    Pdf,
}

impl RefSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Html => "html",
            Self::Header => "header",
            Self::Pdf => "pdf",
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
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
    /// Markdown alternates and service links (spec 016).
    pub agent_links: Vec<agents::AgentLink>,
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
    /// Sentences and syllables for reading ease, on English or undeclared
    /// pages only (spec 009 FR-031).
    pub sentence_count: Option<u32>,
    pub syllable_count: Option<u32>,
    pub text_hash: i64,
    /// The main content summary for near duplicate detection (spec 020).
    pub main: MainContent,
    /// Meta tag directives (header ones are in [`HeaderFacts`]).
    pub directives: Vec<Directive>,
    /// In document order.
    pub refs: Vec<Ref>,
    pub meta_refresh: Option<MetaRefresh>,
    pub structured: Vec<StructuredBlock>,
    /// The collapsed visible text, only when [`PageInput::keep_text`] was
    /// set (spec 014 research R10). `None` in the raw crawl.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub visible_text: Option<String>,
    /// Forms, actions and agent links (spec 016).
    pub agents: AgentData,
    /// AMP checks, for pages marked as AMP (spec 027 part E).
    pub amp: Option<amp::AmpReport>,
    /// Custom rule outcomes; `None` when the crawl has no rules (spec 013).
    pub custom: Option<crate::custom::PageRules>,
    /// Static accessibility checks (spec 026), when asked for.
    pub a11y: Option<a11y::A11yStatic>,
    /// What the crawl's [`PageHook`] read from the parsed page.
    #[serde(skip)]
    pub facts: Option<PageFacts>,
}

/// Facts a [`PageHook`] read from a parsed page, stored in `page_facts`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PageFacts {
    /// Who wrote them, such as `aeo_content` (spec 015).
    pub kind: &'static str,
    /// Bumped when the reader changes, so older rows are read again.
    pub version: i64,
    pub data: Vec<u8>,
}

/// What a [`PageHook`] is given besides the parsed page.
pub struct HookInput<'a> {
    pub url: &'a Url,
    /// Length of the body as received, before decoding.
    pub body_len: usize,
    /// The `lang` of the root element, as stored in `pages.lang`.
    pub lang: Option<&'a str>,
}

/// Reads more from each page while extraction has it parsed, so a later
/// pass need not parse the stored HTML again (spec 015 reads its content
/// facts this way). Runs on the extraction threads; a panic only loses the
/// facts of that page.
#[derive(Clone)]
pub struct PageHook(pub std::sync::Arc<PageHookFn>);

/// The function behind a [`PageHook`].
pub type PageHookFn = dyn Fn(&scraper::Html, &HookInput<'_>) -> Option<PageFacts> + Send + Sync;

impl std::fmt::Debug for PageHook {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PageHook")
    }
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
            sentence_count: None,
            syllable_count: None,
            text_hash: text::hash(""),
            main: MainContent::empty(),
            directives: Vec::new(),
            refs: Vec::new(),
            meta_refresh: None,
            structured: Vec::new(),
            visible_text: None,
            agents: AgentData::default(),
            amp: None,
            custom: None,
            a11y: None,
            facts: None,
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
    /// The user's main content settings (spec 020 FR-003); `None` = defaults.
    pub content: Option<&'a ContentRules>,
    /// Keep the visible text in [`PageData::visible_text`] (spec 014, for
    /// comparing raw and rendered text). The raw crawl passes `false`.
    pub keep_text: bool,
    /// Run the static accessibility checks (spec 026).
    pub a11y: bool,
    pub hook: Option<&'a PageHook>,
}

/// Extracts everything spec 005 lists from one HTML body. Never fails: a
/// page that cannot be read comes back flagged (FR-008, FR-033).
pub fn extract_page(input: &PageInput<'_>) -> PageData {
    extract(input, false, None).1
}

/// [`extract_page`] plus the crawl's custom rules on the same parse
/// (spec 013). Pages with no readable content get every rule as no match.
pub fn extract_page_with(input: &PageInput<'_>, rules: Option<&crate::custom::CompiledRules>) -> PageData {
    extract(input, false, rules).1
}

/// A stored page parsed again after the crawl, with the element each
/// reference came from (spec 023 link positions). The same decoding and
/// walk as [`extract_page`], so `refs[i]` is the link stored with `pos = i`.
pub struct ParsedPage {
    pub doc: scraper::Html,
    pub refs: Vec<Ref>,
    /// The element of `refs[i]`, in `doc`.
    pub nodes: Vec<ego_tree::NodeId>,
}

/// Parses a stored body for [`ParsedPage`]. None when the body is not a
/// document.
pub fn parse_page(input: &PageInput<'_>) -> Option<ParsedPage> {
    let (doc, data, nodes) = extract(input, true, None);
    let doc = doc?;
    Some(ParsedPage {
        doc,
        refs: data.refs,
        nodes,
    })
}

fn extract(
    input: &PageInput<'_>,
    track: bool,
    rules: Option<&crate::custom::CompiledRules>,
) -> (Option<scraper::Html>, PageData, Vec<ego_tree::NodeId>) {
    let encoding = page_encoding(input.charset, input.body, input.truncated);
    if looks_binary(input.body) {
        let mut data = PageData::empty(input.url.clone(), encoding.name(), input.truncated);
        data.custom = rules.map(crate::custom::CompiledRules::on_empty_page);
        if input.keep_text {
            data.visible_text = Some(String::new());
        }
        return (None, data, Vec::new());
    }
    let (text, used, had_errors) = encoding.decode(input.body);
    let hook = input.hook.map(|h| (h, input.body.len()));
    let (doc, mut data, nodes, _) = walk::walk_doc(
        &text,
        input.url,
        input.robots_token,
        walk::PARSE_BUDGET,
        input.a11y,
        hook,
        track,
        rules,
        input.keep_text,
        input.content,
    );
    data.encoding = used.name();
    data.encoding_errors = had_errors;
    data.partial |= input.truncated;
    (Some(doc), data, nodes)
}

/// The encoding extraction decodes a body with.
fn page_encoding(charset: Option<&str>, body: &[u8], truncated: bool) -> &'static encoding_rs::Encoding {
    match crate::title::declared_encoding(charset, body) {
        Some(enc) => enc,
        // Undeclared and not valid UTF-8: legacy pages of this kind are
        // almost always windows-1252, which is what browsers in Western
        // locales fall back to. A sequence cut by truncation does not count.
        None if !utf8_or_cut(body, truncated) => encoding_rs::WINDOWS_1252,
        None => encoding_rs::UTF_8,
    }
}

/// Decodes and parses a stored body exactly as [`extract_page`] does
/// (same encoding rules, scripting off, same time budget), for analyses
/// that need the DOM after the crawl (spec 015). `None` for binary bodies.
pub fn parse_body(charset: Option<&str>, body: &[u8], truncated: bool) -> Option<scraper::Html> {
    if looks_binary(body) {
        return None;
    }
    let (text, _, _) = page_encoding(charset, body, truncated).decode(body);
    Some(walk::parse(&text, walk::PARSE_BUDGET).0)
}

/// Whether `body` is valid UTF-8, allowing a sequence cut off at the end
/// when the body was truncated.
fn utf8_or_cut(body: &[u8], truncated: bool) -> bool {
    match std::str::from_utf8(body) {
        Ok(_) => true,
        Err(e) => truncated && e.error_len().is_none(),
    }
}

/// A page's body text by paragraph with the compared main content marked
/// (spec 020 comparison view). Decodes like [`extract_page`].
pub fn main_text(url: &Url, charset: Option<&str>, body: &[u8], content: Option<&ContentRules>) -> MainText {
    let encoding = match crate::title::declared_encoding(charset, body) {
        Some(enc) => enc,
        None if std::str::from_utf8(body).is_err() => encoding_rs::WINDOWS_1252,
        None => encoding_rs::UTF_8,
    };
    let (text, _, _) = encoding.decode(body);
    walk::main_text(&text, url, content)
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
        agent_links: link.iter().flat_map(|v| link_header::parse_agent(v)).collect(),
    }
}

#[cfg(test)]
mod tests;
