//! Custom extraction and custom search (spec 013).
//!
//! Rules are compiled once per crawl ([`compile`]) and applied to the
//! document spec 005 already parsed for each page ([`apply`], plan D1), so
//! no page is parsed twice (FR-028). Like extraction, this is a pure
//! function of the page: no I/O, no crawl state, deterministic (FR-030).

mod json_ld;
pub mod rerun;
mod search;
pub mod tester;
mod xpath;

use std::collections::HashSet;
use std::time::{Duration, Instant};

use ego_tree::NodeId;
use scraper::{ElementRef, Html, Selector};
use serde::Serialize;

use spiderlens_config::{
    CustomRules, ElementKind, ExtractMode, Extractor, ExtractorKind, Search, SearchCondition, SearchTarget,
};

use crate::extract::StructuredBlock;

/// Values kept per page and extractor (FR-019).
pub const MAX_VALUES: usize = 100;
/// Characters kept per value (FR-019).
pub const MAX_VALUE_CHARS: usize = 32_000;
/// Search matches kept per page with their context (FR-021).
pub const MAX_CONTEXTS: usize = 10;
/// Characters of context on each side of a search match.
const CONTEXT_CHARS: usize = 40;
/// Time all rules together may take on one page (plan D6, research R4).
pub const PAGE_BUDGET: Duration = Duration::from_secs(2);

/// Flag bits stored per page and rule (data-model.md).
pub mod flags {
    pub const MEETS: u16 = 1;
    pub const NO_MATCH: u16 = 2;
    pub const MORE_THAN_STORED: u16 = 4;
    pub const TRUNCATED: u16 = 8;
    pub const TIMED_OUT: u16 = 16;
    pub const ERROR: u16 = 32;
    pub const ELEMENT_NOT_FOUND: u16 = 64;
    pub const NOT_AVAILABLE: u16 = 128;
    pub const BAD_JSON: u16 = 256;
    /// The rule was evaluated on this page.
    pub const RUN: u16 = 512;

    /// Names for the CLI and the tester.
    pub fn names(f: u16) -> Vec<&'static str> {
        [
            (MEETS, "meets"),
            (NO_MATCH, "no_match"),
            (MORE_THAN_STORED, "more_than_stored"),
            (TRUNCATED, "truncated"),
            (TIMED_OUT, "timed_out"),
            (ERROR, "error"),
            (ELEMENT_NOT_FOUND, "element_not_found"),
            (NOT_AVAILABLE, "not_available"),
            (BAD_JSON, "bad_json"),
        ]
        .into_iter()
        .filter(|(b, _)| f & b != 0)
        .map(|(_, n)| n)
        .collect()
    }
}

/// What one rule found on one page.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct RuleOutcome {
    /// True number of matches, also beyond what is stored.
    pub count: u32,
    pub flags: u16,
    /// Extractors: values in document order. Searches: matches in context.
    pub values: Vec<String>,
    /// Set with [`flags::ERROR`].
    pub error: Option<String>,
}

impl RuleOutcome {
    fn flagged(flags: u16) -> Self {
        Self {
            flags: flags | flags::RUN,
            ..Default::default()
        }
    }
}

/// The outcomes of every rule on one page: extractors first, then searches,
/// in rule order.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct PageRules {
    pub extractors: Vec<RuleOutcome>,
    pub searches: Vec<RuleOutcome>,
}

/// The page as rules see it: the one parse plus what the walk built.
pub struct DocView<'a> {
    pub doc: &'a Html,
    /// The decoded HTML source.
    pub source: &'a str,
    /// The page's visible text, as for word count (FR-007).
    pub visible: &'a str,
    pub structured: &'a [StructuredBlock],
}

enum Matcher {
    Css(Selector),
    /// Checked at compile time; built per page, because compiled
    /// `sxd_xpath` expressions cannot be shared between threads.
    Xpath(String),
    Regex(regex::Regex),
    JsonLd(json_ld::Path),
}

struct CExtractor {
    matcher: Matcher,
    mode: ExtractMode,
    attr: String,
}

enum Within {
    Css(Selector),
    Xpath(String),
}

struct CSearch {
    re: regex::Regex,
    not: bool,
    target: SearchTarget,
    within: Option<Within>,
}

/// Rules ready to run. Cheap to share between fetch tasks behind an `Arc`.
pub struct CompiledRules {
    extractors: Vec<CExtractor>,
    searches: Vec<CSearch>,
    needs_xpath: bool,
}

/// A rule that does not compile, with where (`custom.extractors[2].expr`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleError {
    pub path: String,
    pub message: String,
}

impl std::fmt::Display for RuleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.path, self.message)
    }
}

fn regex(pattern: &str, ignore_case: bool) -> Result<regex::Regex, String> {
    regex::RegexBuilder::new(pattern)
        .case_insensitive(ignore_case)
        // Patterns run on whole pages: allow large compiled programs.
        .size_limit(64 << 20)
        .build()
        .map_err(|e| e.to_string())
}

fn xpath(expr: &str) -> Result<String, String> {
    xpath::build(expr).map(|_| expr.to_string())
}

fn css(expr: &str) -> Result<Selector, String> {
    Selector::parse(expr).map_err(|e| e.to_string())
}

/// Compiles a rule set. Returns `None` for an empty set, so a crawl with no
/// rules carries nothing (FR-027).
pub fn compile(rules: &CustomRules) -> Result<Option<CompiledRules>, Vec<RuleError>> {
    if rules.is_empty() {
        return Ok(None);
    }
    let mut errors = Vec::new();
    let mut extractors = Vec::new();
    let mut searches = Vec::new();
    let mut needs_xpath = false;
    for (i, e) in rules.extractors.iter().enumerate() {
        match compile_extractor(e) {
            Ok(c) => {
                needs_xpath |= matches!(c.matcher, Matcher::Xpath(_));
                extractors.push(c);
            }
            Err(message) => errors.push(RuleError {
                path: format!("custom.extractors[{i}].expr"),
                message,
            }),
        }
    }
    for (i, s) in rules.searches.iter().enumerate() {
        match compile_search(s) {
            Ok(c) => {
                needs_xpath |= matches!(c.within, Some(Within::Xpath(_)));
                searches.push(c);
            }
            Err(message) => errors.push(RuleError {
                path: format!("custom.searches[{i}]"),
                message,
            }),
        }
    }
    if errors.is_empty() {
        Ok(Some(CompiledRules {
            extractors,
            searches,
            needs_xpath,
        }))
    } else {
        Err(errors)
    }
}

fn compile_extractor(e: &Extractor) -> Result<CExtractor, String> {
    let matcher = match e.kind {
        ExtractorKind::Css => Matcher::Css(css(&e.expr)?),
        ExtractorKind::Xpath => Matcher::Xpath(xpath(&e.expr)?),
        ExtractorKind::Regex => Matcher::Regex(regex(&e.expr, false)?),
        ExtractorKind::JsonLd => Matcher::JsonLd(json_ld::Path::new(&e.expr, e.json_type.as_deref())?),
    };
    Ok(CExtractor {
        matcher,
        mode: e.mode,
        attr: e.attr.clone().unwrap_or_default().trim().to_ascii_lowercase(),
    })
}

fn compile_search(s: &Search) -> Result<CSearch, String> {
    if s.term.is_empty() {
        return Err("empty search term".into());
    }
    let pattern = if s.regex {
        s.term.clone()
    } else {
        regex::escape(&s.term)
    };
    let within = match &s.within {
        None => None,
        Some(w) => Some(match w.kind {
            ElementKind::Css => Within::Css(css(&w.expr)?),
            ElementKind::Xpath => Within::Xpath(xpath(&w.expr)?),
        }),
    };
    Ok(CSearch {
        re: regex(&pattern, s.ignore_case)?,
        not: s.condition == SearchCondition::NotContains,
        target: s.target,
        within,
    })
}

impl CompiledRules {
    pub fn extractor_count(&self) -> usize {
        self.extractors.len()
    }

    pub fn search_count(&self) -> usize {
        self.searches.len()
    }

    /// A page labelled HTML with no content: nothing matches; "does not
    /// contain" searches are met.
    pub fn on_empty_page(&self) -> PageRules {
        let none = RuleOutcome::flagged(flags::NO_MATCH);
        PageRules {
            extractors: vec![none.clone(); self.extractors.len()],
            searches: self
                .searches
                .iter()
                .map(|s| {
                    let mut o = none.clone();
                    if s.not {
                        o.flags |= flags::MEETS;
                    }
                    o
                })
                .collect(),
        }
    }

    /// The outcome every rule gets on a page that has no stored HTML.
    pub fn not_available(&self) -> PageRules {
        PageRules {
            extractors: vec![RuleOutcome::flagged(flags::NOT_AVAILABLE); self.extractors.len()],
            searches: vec![RuleOutcome::flagged(flags::NOT_AVAILABLE); self.searches.len()],
        }
    }
}

/// Applies every rule to one page within [`PAGE_BUDGET`].
pub fn apply(rules: &CompiledRules, page: &DocView<'_>) -> PageRules {
    apply_within(rules, page, PAGE_BUDGET)
}

pub(crate) fn apply_within(rules: &CompiledRules, page: &DocView<'_>, budget: Duration) -> PageRules {
    let started = Instant::now();
    let package = rules.needs_xpath.then(sxd_document::Package::new);
    let tree = package.as_ref().map(|p| xpath::Tree::new(p, page.doc));
    let mut out = PageRules::default();
    let mut timed_out = false;
    for e in &rules.extractors {
        if timed_out || started.elapsed() > budget {
            timed_out = true;
            out.extractors.push(RuleOutcome::flagged(flags::TIMED_OUT));
            continue;
        }
        out.extractors.push(extract(e, page, tree.as_ref()));
    }
    for s in &rules.searches {
        if timed_out || started.elapsed() > budget {
            timed_out = true;
            out.searches.push(RuleOutcome::flagged(flags::TIMED_OUT));
            continue;
        }
        out.searches.push(search::run(s, page, tree.as_ref()));
    }
    out
}

/// Collects values with the per-page limits (FR-019).
#[derive(Default)]
struct Values {
    out: RuleOutcome,
}

impl Values {
    fn push(&mut self, v: String) {
        self.out.count += 1;
        if self.out.values.len() < MAX_VALUES {
            let v = if v.chars().count() > MAX_VALUE_CHARS {
                self.out.flags |= flags::TRUNCATED;
                v.chars().take(MAX_VALUE_CHARS).collect()
            } else {
                v
            };
            self.out.values.push(v);
        } else {
            self.out.flags |= flags::MORE_THAN_STORED;
        }
    }

    fn finish(mut self) -> RuleOutcome {
        self.out.flags |= flags::RUN;
        if self.out.count == 0 && self.out.flags & flags::ERROR == 0 {
            self.out.flags |= flags::NO_MATCH;
        }
        self.out
    }
}

fn extract(e: &CExtractor, page: &DocView<'_>, tree: Option<&xpath::Tree<'_>>) -> RuleOutcome {
    let mut v = Values::default();
    match &e.matcher {
        Matcher::Css(sel) => {
            for el in page.doc.select(sel) {
                if let Some(s) = node_value(el, e.mode, &e.attr) {
                    v.push(s);
                }
            }
        }
        Matcher::Xpath(x) => {
            let tree = tree.expect("built when XPath rules exist");
            match tree.evaluate(x) {
                Ok(xpath::Found::Nodes(nodes)) => {
                    for n in nodes {
                        let s = match n {
                            xpath::Hit::Element(id) => {
                                let el = ElementRef::wrap(page.doc.tree.get(id).expect("mapped node"))
                                    .expect("mapped node is an element");
                                node_value(el, e.mode, &e.attr)
                            }
                            xpath::Hit::Other(s) => Some(collapse_if_text(e.mode, s)),
                        };
                        if let Some(s) = s {
                            v.push(s);
                        }
                    }
                }
                Ok(xpath::Found::Value(s)) => v.push(s),
                Err(msg) => {
                    v.out.flags |= flags::ERROR;
                    v.out.error = Some(msg);
                }
            }
        }
        Matcher::Regex(re) => {
            for caps in re.captures_iter(page.source) {
                let m = caps
                    .get(1)
                    .or_else(|| caps.get(0))
                    .expect("group 0 always matches");
                v.push(m.as_str().to_string());
            }
        }
        Matcher::JsonLd(path) => {
            let found = path.find(page.structured);
            if found.bad_json {
                v.out.flags |= flags::BAD_JSON;
            }
            for s in found.values {
                v.push(s);
            }
        }
    }
    v.finish()
}

fn collapse_if_text(mode: ExtractMode, s: String) -> String {
    if mode == ExtractMode::Text || mode == ExtractMode::Value {
        crate::extract::text::collapse(&s)
    } else {
        s
    }
}

/// An element's value in an extractor mode. `None` when an attribute is
/// asked for and absent: that node is not a match.
fn node_value(el: ElementRef<'_>, mode: ExtractMode, attr: &str) -> Option<String> {
    Some(match mode {
        ExtractMode::Text | ExtractMode::Value => visible_text(el),
        ExtractMode::InnerHtml => el.inner_html(),
        ExtractMode::OuterHtml => el.html(),
        ExtractMode::Attribute => el.value().attr(attr)?.to_string(),
    })
}

/// The text a user sees inside an element: script, style, template and
/// noscript content and comments left out, words kept apart at block
/// boundaries, whitespace collapsed (spec 005 FR-009, FR-014).
pub fn visible_text(el: ElementRef<'_>) -> String {
    let mut buf = String::new();
    crate::extract::visible_text_into(*el, &mut buf);
    crate::extract::text::collapse(&buf)
}

/// Keeps only the outermost of the selected elements, so nested matches
/// (`div` inside `div`) are not searched twice.
fn outermost(doc: &Html, ids: Vec<NodeId>) -> Vec<NodeId> {
    let set: HashSet<NodeId> = ids.iter().copied().collect();
    ids.into_iter()
        .filter(|id| {
            let node = doc.tree.get(*id).expect("node of this document");
            !node.ancestors().any(|a| set.contains(&a.id()))
        })
        .collect()
}

/// Up to [`CONTEXT_CHARS`] on each side of a match, on character boundaries.
fn context(hay: &str, start: usize, end: usize) -> String {
    let before = hay[..start]
        .char_indices()
        .rev()
        .nth(CONTEXT_CHARS.saturating_sub(1))
        .map_or(0, |(i, _)| i);
    let after = hay[end..]
        .char_indices()
        .nth(CONTEXT_CHARS)
        .map_or(hay.len(), |(i, _)| end + i);
    let mut s = String::new();
    if before > 0 {
        s.push('…');
    }
    s.push_str(&hay[before..after]);
    if after < hay.len() {
        s.push('…');
    }
    crate::extract::text::collapse(&s)
}

#[cfg(test)]
mod tests;
