//! The main content of a page (spec 020, plan D1): the text a reader came
//! for, without navigation, banners, footers and sidebars.
//!
//! The spec 005 walk records every piece of body text once, with flags for
//! where it sits ([`Spans`]). When the walk ends, the first source with
//! words wins: the user's content element, else the first `<main>`, else
//! every `<article>`, else the body without boilerplate (research R5).

use scraper::{ElementRef, Selector};
use serde::Serialize;

use super::shingle::{self, Signature};
use super::text::collapse;

pub(super) const MAIN: u8 = 1;
pub(super) const ARTICLE: u8 = 2;
/// Built-in boilerplate: navigation, sidebars, search, site banner/footer.
pub(super) const BOILER: u8 = 4;
/// Inside the user's content element.
pub(super) const SELECTED: u8 = 8;
/// Inside an element the user excluded.
pub(super) const EXCLUDED: u8 = 16;

/// Where the compared text came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MainSource {
    Main,
    Article,
    Fallback,
    /// The user's content element.
    Selector,
    /// The user's content element is not on the page; fallback rules used.
    SelectorMissing,
}

impl MainSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Main => "main",
            Self::Article => "article",
            Self::Fallback => "fallback",
            Self::Selector => "selector",
            Self::SelectorMissing => "selector_missing",
        }
    }
}

/// The main content summary stored for every HTML page.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MainContent {
    pub words: u32,
    /// Hash of the main text's words in order (case, spacing and
    /// punctuation aside): equal = exact content duplicate.
    pub text_hash: i64,
    pub source: MainSource,
    /// `None` when the main content has no words.
    #[serde(skip)]
    pub signature: Option<Signature>,
}

impl MainContent {
    pub(super) fn empty() -> Self {
        Self {
            words: 0,
            text_hash: shingle::tokens_hash(&[]),
            source: MainSource::Fallback,
            signature: None,
        }
    }
}

/// The user's content and exclusion settings, parsed once per crawl
/// (FR-003).
#[derive(Debug, Default)]
pub struct ContentRules {
    content: Option<Selector>,
    exclude: Vec<Selector>,
}

impl ContentRules {
    /// Parses the selectors; the error names the one that does not parse.
    pub fn new(content: Option<&str>, exclude: &[String]) -> Result<Self, String> {
        let parse = |s: &str| Selector::parse(s).map_err(|_| format!("Not a valid CSS selector: {s}"));
        Ok(Self {
            content: content
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(parse)
                .transpose()?,
            exclude: exclude
                .iter()
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .map(parse)
                .collect::<Result<_, _>>()?,
        })
    }

    pub fn is_default(&self) -> bool {
        self.content.is_none() && self.exclude.is_empty()
    }

    pub(super) fn has_content(&self) -> bool {
        self.content.is_some()
    }

    pub(super) fn is_content(&self, e: ElementRef<'_>) -> bool {
        self.content.as_ref().is_some_and(|s| s.matches(&e))
    }

    pub(super) fn is_excluded(&self, e: ElementRef<'_>) -> bool {
        self.exclude.iter().any(|s| s.matches(&e))
    }
}

/// Body text in document order: one buffer, and spans of it with flags.
/// A span with `sep` set is a block boundary (a paragraph break).
#[derive(Debug, Default)]
pub(super) struct Spans {
    text: String,
    spans: Vec<Span>,
    pub(super) main_seen: bool,
    pub(super) selected_seen: bool,
}

#[derive(Debug, Clone, Copy)]
struct Span {
    start: u32,
    end: u32,
    flags: u8,
    sep: bool,
}

/// One paragraph of a page for the comparison view: runs of text, each
/// marked as part of the compared main content or not.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Paragraph {
    pub runs: Vec<Run>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Run {
    pub text: String,
    pub included: bool,
}

/// A page's body text split into paragraphs, with the main content marked.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MainText {
    pub source: MainSource,
    pub paragraphs: Vec<Paragraph>,
}

impl MainText {
    /// Only the compared text, one string per paragraph.
    pub fn main_paragraphs(&self) -> Vec<String> {
        self.paragraphs
            .iter()
            .map(|p| {
                collapse(
                    &p.runs
                        .iter()
                        .filter(|r| r.included)
                        .map(|r| r.text.as_str())
                        .collect::<Vec<_>>()
                        .join(" "),
                )
            })
            .filter(|p| !p.is_empty())
            .collect()
    }
}

impl Spans {
    pub(super) fn push(&mut self, t: &str, flags: u8) {
        if t.is_empty() {
            return;
        }
        let start = self.text.len() as u32;
        self.text.push_str(t);
        self.spans.push(Span {
            start,
            end: self.text.len() as u32,
            flags,
            sep: false,
        });
    }

    pub(super) fn separate(&mut self) {
        if self.spans.last().is_some_and(|s| !s.sep) {
            let at = self.text.len() as u32;
            self.spans.push(Span {
                start: at,
                end: at,
                flags: 0,
                sep: true,
            });
        }
    }

    fn keeps(source: MainSource, flags: u8) -> bool {
        if flags & EXCLUDED != 0 {
            return false;
        }
        match source {
            MainSource::Selector => flags & SELECTED != 0,
            MainSource::Main => flags & MAIN != 0 && flags & BOILER == 0,
            MainSource::Article => flags & ARTICLE != 0 && flags & BOILER == 0,
            MainSource::Fallback | MainSource::SelectorMissing => flags & BOILER == 0,
        }
    }

    /// The source's text, spans joined with a space where they were apart
    /// (not collapsed: tokenising ignores spacing).
    fn collect(&self, source: MainSource) -> String {
        let mut out = String::with_capacity(self.text.len());
        for s in &self.spans {
            if !s.sep && Self::keeps(source, s.flags) {
                out.push_str(&self.text[s.start as usize..s.end as usize]);
            } else {
                // Left-out inline content still separates its neighbours.
                out.push(' ');
            }
        }
        out
    }

    /// Whether the source holds at least one word.
    fn has_words(&self, source: MainSource) -> bool {
        self.spans.iter().any(|s| {
            !s.sep
                && Self::keeps(source, s.flags)
                && self.text[s.start as usize..s.end as usize]
                    .chars()
                    .any(|c| c.is_alphanumeric())
        })
    }

    /// The winning source.
    fn source(&self, rules: Option<&ContentRules>) -> MainSource {
        if rules.is_some_and(ContentRules::has_content) {
            return if self.selected_seen {
                MainSource::Selector
            } else {
                MainSource::SelectorMissing
            };
        }
        [MainSource::Main, MainSource::Article]
            .into_iter()
            .find(|&s| self.has_words(s))
            .unwrap_or(MainSource::Fallback)
    }

    pub(super) fn summary(&self, rules: Option<&ContentRules>) -> MainContent {
        let source = self.source(rules);
        let (tokens, words) = shingle::scan(&self.collect(source));
        MainContent {
            words,
            text_hash: shingle::tokens_hash(&tokens),
            source,
            signature: shingle::signature_of_tokens(&tokens),
        }
    }

    pub(super) fn paragraphs(&self, rules: Option<&ContentRules>) -> MainText {
        let source = self.source(rules);
        let mut paragraphs = Vec::new();
        let mut runs: Vec<Run> = Vec::new();
        let mut flush = |runs: &mut Vec<Run>| {
            let mut merged: Vec<Run> = Vec::new();
            for r in runs.drain(..) {
                match merged.last_mut() {
                    Some(last) if last.included == r.included => last.text.push_str(&r.text),
                    _ => merged.push(r),
                }
            }
            for r in &mut merged {
                r.text = collapse(&r.text);
            }
            merged.retain(|r| !r.text.is_empty());
            if !merged.is_empty() {
                paragraphs.push(Paragraph { runs: merged });
            }
        };
        for s in &self.spans {
            if s.sep {
                flush(&mut runs);
            } else {
                runs.push(Run {
                    text: self.text[s.start as usize..s.end as usize].to_string(),
                    included: Self::keeps(source, s.flags),
                });
            }
        }
        flush(&mut runs);
        MainText { source, paragraphs }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extract::{PageInput, extract_page, main_text};
    use url::Url;

    fn main_of(html: &str, rules: Option<&ContentRules>) -> MainContent {
        let url = Url::parse("https://example.com/p").unwrap();
        extract_page(&PageInput {
            url: &url,
            charset: Some("utf-8"),
            body: html.as_bytes(),
            truncated: false,
            robots_token: "SpiderLens",
            content: rules,
        })
        .main
    }

    fn words(n: usize, seed: &str) -> String {
        (0..n).map(|i| format!("{seed}{i}")).collect::<Vec<_>>().join(" ")
    }

    fn page(chrome: &str, content: &str) -> String {
        format!(
            "<html><body><header><a href=/>Logo</a> {chrome}</header><nav>{chrome} menu</nav>\
             <div class=wrap><p>{content}</p><aside>{chrome} side</aside></div>\
             <footer>{chrome} footer</footer></body></html>"
        )
    }

    #[test]
    fn fallback_drops_site_chrome() {
        let content = words(200, "c");
        let a = main_of(&page(&words(300, "x"), &content), None);
        let b = main_of(&page(&words(300, "y"), &content), None);
        assert_eq!(a.source, MainSource::Fallback);
        assert_eq!(a.words, 200);
        // US1.2: different chrome, identical content.
        assert_eq!(a.text_hash, b.text_hash);
        assert_eq!(a.signature.unwrap().similarity(&b.signature.unwrap()), 100);
    }

    #[test]
    fn shared_chrome_does_not_make_pages_similar() {
        // US1.3: same 300-word chrome, unrelated content.
        let chrome = words(300, "x");
        let a = main_of(&page(&chrome, &words(200, "a")), None);
        let b = main_of(&page(&chrome, &words(200, "b")), None);
        assert!(a.signature.unwrap().similarity(&b.signature.unwrap()) < 5);
    }

    #[test]
    fn main_then_article() {
        let m = main_of(
            "<body><p>outside words</p><main><h1>Title</h1><p>inside text</p><nav>skip</nav></main></body>",
            None,
        );
        assert_eq!((m.source, m.words), (MainSource::Main, 3));
        let a = main_of(
            "<body><p>outside</p><article><header>By Ann</header><p>one</p></article>\
             <article><p>two three</p><footer>Jan 1</footer></article></body>",
            None,
        );
        // Header and footer inside an article are content (research R5).
        assert_eq!((a.source, a.words), (MainSource::Article, 7));
        let empty_main = main_of("<body><main> </main><p>body text</p></body>", None);
        assert_eq!(empty_main.source, MainSource::Fallback);
    }

    #[test]
    fn roles_count_as_landmarks() {
        let m = main_of(
            "<body><div role=navigation>a b</div><div role=banner>c</div><div role=search>d</div>\
             <section><div role=contentinfo>kept</div></section><p>text</p></body>",
            None,
        );
        assert_eq!(m.words, 2);
    }

    #[test]
    fn user_selectors() {
        let html = "<body><div id=content><p>alpha beta</p><div class=related-products>gamma delta</div></div>\
                    <p>outside</p></body>";
        let rules = ContentRules::new(Some("#content"), &[".related-products".into()]).unwrap();
        let m = main_of(html, Some(&rules));
        assert_eq!((m.source, m.words), (MainSource::Selector, 2));
        let missing = ContentRules::new(Some("#nope"), &[]).unwrap();
        let m = main_of(html, Some(&missing));
        assert_eq!((m.source, m.words), (MainSource::SelectorMissing, 5));
        assert!(ContentRules::new(Some("div[["), &[]).is_err());
        assert!(ContentRules::new(None, &["".into()]).unwrap().is_default());
    }

    #[test]
    fn excluded_inline_content_separates_words() {
        let rules = ContentRules::new(None, &[".ad".into()]).unwrap();
        let m = main_of("<body><p>one<span class=ad>x</span>two</p></body>", Some(&rules));
        assert_eq!(m.words, 2);
    }

    #[test]
    fn paragraphs_for_comparison() {
        let url = Url::parse("https://example.com/").unwrap();
        let t = main_text(
            &url,
            None,
            b"<body><nav>Home About</nav><main><p>First para.</p><p>Second <b>bold</b> para.</p></main></body>",
            None,
        );
        assert_eq!(t.source, MainSource::Main);
        assert_eq!(t.main_paragraphs(), ["First para.", "Second bold para."]);
        assert!(
            t.paragraphs
                .iter()
                .any(|p| p.runs.iter().any(|r| !r.included && r.text == "Home About"))
        );
    }
}
