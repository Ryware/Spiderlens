//! The single depth-first walk over the parsed document (plan D1).
//!
//! Iterative, not recursive, so deeply nested malformed pages cannot
//! overflow the stack.

use std::time::{Duration, Instant};

use ego_tree::{NodeId, NodeRef};
use html5ever::driver::ParseOpts;
use html5ever::tendril::TendrilSink;
use html5ever::tree_builder::TreeBuilderOpts;
use html5ever::{local_name, ns};
use scraper::node::Element;
use scraper::{ElementRef, Html, HtmlTreeSink, Node};
use url::Url;

use super::agents::{AgentCtx, Collector};
use super::a11y::A11yCollector;
use super::directives::{meta_crawler, parse_meta};
use super::link_header::{link_kind, rel_has};
use super::main_content::{ARTICLE, BOILER, ContentRules, EXCLUDED, MAIN, MainText, SELECTED, Spans};
use super::text::{collapse, hash, readability_counts, word_count};
use super::{
    Heading, MetaRefresh, PageData, Ref, RefSource, StructuredBlock, StructuredKind, TextItem, microdata,
};
use crate::crawl::LinkKind;

/// The longest a single page may spend in the HTML parser. Pathological
/// nesting (hundreds of thousands of unclosed elements) makes tree
/// construction quadratic; past this budget the rest of the page is not
/// parsed and the page is flagged partial.
pub(super) const PARSE_BUDGET: Duration = Duration::from_secs(10);
const CHUNK: usize = 64 * 1024;

/// Parses with scripting disabled, so `<noscript>` content is markup whose
/// links and images are extracted (spec edge case). Returns whether the
/// parse was cut short by the time budget.
pub(super) fn parse(text: &str, budget: Duration) -> (Html, bool) {
    let opts = ParseOpts {
        tree_builder: TreeBuilderOpts {
            scripting_enabled: false,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut parser = html5ever::driver::parse_document(HtmlTreeSink::new(Html::new_document()), opts);
    let started = Instant::now();
    let mut rest = text;
    let mut cut = false;
    while !rest.is_empty() {
        if started.elapsed() > budget {
            cut = true;
            break;
        }
        let mut end = rest.len().min(CHUNK);
        while !rest.is_char_boundary(end) {
            end += 1;
        }
        parser.process(rest[..end].into());
        rest = &rest[end..];
    }
    (parser.finish(), cut)
}

#[derive(Clone, Copy, Default)]
struct Ctx {
    in_head: bool,
    in_body: bool,
    noscript: bool,
    in_item: bool,
    /// Main content flags for text here (`main_content::MAIN`, ...).
    main: u8,
    /// Inside `main`, `article` or `section`, where `header` and `footer`
    /// are not site-wide landmarks.
    sectioning: bool,
    agent: AgentCtx,
}

/// Text being collected for an element that is still open.
struct Open {
    index: usize,
    buf: String,
}

enum Frame<'a> {
    Enter(NodeRef<'a, Node>, Ctx),
    /// Closing a non-inline element: separates the words on either side.
    Separate,
    EndAnchor(Option<Open>),
    EndHeading(Option<Open>),
}

struct Walker<'p> {
    page: &'p Url,
    token: &'p str,
    content: Option<&'p ContentRules>,
    data: PageData,
    base_seen: bool,
    visible: String,
    body: Spans,
    anchor: Option<Open>,
    heading: Option<Open>,
    agents: Collector,
    /// Record the element each reference came from (spec 023 link
    /// positions, after the crawl only).
    track: bool,
    at: Option<NodeId>,
    nodes: Vec<NodeId>,
}

#[cfg(test)]
pub(super) fn walk(
    text: &str,
    page: &Url,
    token: &str,
    budget: Duration,
    a11y: bool,
    hook: Option<(&super::PageHook, usize)>,
) -> PageData {
    walk_doc(text, page, token, budget, a11y, hook, false, None, false, None).1
}

/// The page's body text by paragraph with its main content marked, for the
/// comparison view (spec 020, plan D8).
pub(super) fn main_text(text: &str, page: &Url, content: Option<&ContentRules>) -> MainText {
    let (_, _, _, body) = walk_doc(text, page, "", PARSE_BUDGET, false, None, false, None, false, content);
    body.paragraphs(content)
}

/// [`walk`], also returning the document and, when `track` is set, the
/// element of each reference (same index as `PageData::refs`), then the
/// crawl's custom rules on the same parse (spec 013 plan D1) and the body
/// text spans for the main content (spec 020).
#[allow(clippy::too_many_arguments)]
pub(super) fn walk_doc(
    text: &str,
    page: &Url,
    token: &str,
    budget: Duration,
    a11y: bool,
    hook: Option<(&super::PageHook, usize)>,
    track: bool,
    rules: Option<&crate::custom::CompiledRules>,
    keep_text: bool,
    content: Option<&ContentRules>,
) -> (Html, PageData, Vec<NodeId>, Spans) {
    let content = content.filter(|r| !r.is_default());
    let (doc, cut) = parse(text, budget);
    // Spec 026: only created when asked for, so "Off" costs this test.
    let mut checks = a11y.then(|| A11yCollector::new(&doc));
    let mut w = Walker {
        page,
        token,
        content,
        data: PageData::empty(page.clone(), "", false),
        base_seen: false,
        visible: String::new(),
        body: Spans::default(),
        anchor: None,
        heading: None,
        agents: Collector::default(),
        track,
        at: None,
        nodes: Vec::new(),
    };
    w.data.no_content = false;
    w.data.partial = cut;
    w.data.amp = super::amp::check(&doc);
    w.data.lang = doc
        .root_element()
        .value()
        .attr("lang")
        .map(|l| l.trim().to_string());

    let mut stack: Vec<Frame<'_>> = vec![Frame::Enter(doc.tree.root(), Ctx::default())];
    while let Some(frame) = stack.pop() {
        match frame {
            Frame::Enter(node, ctx) => {
                if let Some(c) = checks.as_mut() {
                    c.enter(node, ctx.noscript);
                }
                w.enter(node, ctx, &mut stack);
            }
            Frame::Separate => w.separate(),
            Frame::EndAnchor(prev) => {
                if let Some(open) = std::mem::replace(&mut w.anchor, prev) {
                    w.data.refs[open.index].text = Some(collapse(&open.buf));
                }
            }
            Frame::EndHeading(prev) => {
                if let Some(open) = std::mem::replace(&mut w.heading, prev) {
                    w.data.headings[open.index].text = collapse(&open.buf);
                }
            }
        }
    }

    let visible = collapse(&w.visible);
    w.data.word_count = word_count(&visible);
    // English or undeclared: "en", "en-GB", "EN_us" (spec 009 FR-031).
    let english = w.data.lang.as_deref().is_none_or(|l| {
        let l = l.to_ascii_lowercase();
        l.is_empty() || l == "en" || l.starts_with("en-") || l.starts_with("en_")
    });
    if english {
        let (sentences, syllables) = readability_counts(&visible);
        w.data.sentence_count = Some(sentences);
        w.data.syllable_count = Some(syllables);
    }
    w.data.text_hash = hash(&visible);
    w.data.main = w.body.summary(content);
    if let Some(rules) = rules {
        w.data.custom = Some(crate::custom::apply(
            rules,
            &crate::custom::DocView {
                doc: &doc,
                source: text,
                visible: &visible,
                structured: &w.data.structured,
            },
        ));
    }
    w.data.a11y = checks.map(|c| c.finish(&doc));
    if let Some((hook, body_len)) = hook {
        let input = super::HookInput {
            url: page,
            body_len,
            lang: w.data.lang.as_deref(),
        };
        w.data.facts = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| (hook.0)(&doc, &input)))
            .ok()
            .flatten();
    }
    w.data.agents = w.agents.finish(&doc);
    if keep_text {
        w.data.visible_text = Some(visible);
    }
    (doc, w.data, w.nodes, w.body)
}

/// The visible text under `node`, as the walk collects it for the page:
/// script, style, template, noscript and comments skipped, a space at the
/// edges of block elements. Not collapsed.
pub(crate) fn visible_text_into(node: NodeRef<'_, Node>, buf: &mut String) {
    enum Step<'a> {
        Enter(NodeRef<'a, Node>),
        Space,
    }
    let mut stack = vec![Step::Enter(node)];
    while let Some(step) = stack.pop() {
        let n = match step {
            Step::Space => {
                if !buf.ends_with(' ') {
                    buf.push(' ');
                }
                continue;
            }
            Step::Enter(n) => n,
        };
        match n.value() {
            Node::Text(t) => {
                buf.push_str(t);
                continue;
            }
            Node::Element(e) => {
                if e.name.ns == ns!(html) {
                    if matches!(
                        e.name.local,
                        local_name!("script")
                            | local_name!("style")
                            | local_name!("template")
                            | local_name!("noscript")
                    ) {
                        continue;
                    }
                    if !is_inline(&e.name.local) {
                        if !buf.ends_with(' ') {
                            buf.push(' ');
                        }
                        stack.push(Step::Space);
                    }
                }
            }
            Node::Document | Node::Fragment => {}
            _ => continue,
        }
        let first = stack.len();
        stack.extend(n.children().map(Step::Enter));
        stack[first..].reverse();
    }
}

/// The first token of a `role` attribute, lowercase.
fn role(e: &Element) -> Option<String> {
    e.attr("role")
        .and_then(|r| r.split_ascii_whitespace().next())
        .map(str::to_ascii_lowercase)
}

/// Elements that do not break words apart (`a<b>b</b>` is one word).
fn is_inline(name: &str) -> bool {
    matches!(
        name,
        "a" | "abbr"
            | "b"
            | "bdi"
            | "bdo"
            | "cite"
            | "code"
            | "data"
            | "dfn"
            | "em"
            | "font"
            | "i"
            | "kbd"
            | "label"
            | "mark"
            | "q"
            | "s"
            | "samp"
            | "small"
            | "span"
            | "strong"
            | "sub"
            | "sup"
            | "time"
            | "u"
            | "var"
            | "wbr"
    )
}

fn children_text(node: NodeRef<'_, Node>) -> String {
    let mut s = String::new();
    for c in node.children() {
        if let Node::Text(t) = c.value() {
            s.push_str(t);
        }
    }
    s
}

fn dimension(e: &Element, name: &str) -> Option<u32> {
    let v = e.attr(name)?.trim();
    let digits: String = v.chars().take_while(char::is_ascii_digit).collect();
    digits.parse().ok()
}

impl Walker<'_> {
    fn separate(&mut self) {
        self.body.separate();
        for buf in [
            Some(&mut self.visible),
            self.anchor.as_mut().map(|o| &mut o.buf),
            self.heading.as_mut().map(|o| &mut o.buf),
        ]
        .into_iter()
        .flatten()
        {
            if !buf.ends_with(' ') {
                buf.push(' ');
            }
        }
    }

    fn text(&mut self, t: &str, ctx: Ctx) {
        if ctx.in_body && !ctx.noscript {
            self.visible.push_str(t);
            self.body.push(t, ctx.main);
        }
        if let Some(o) = self.anchor.as_mut() {
            o.buf.push_str(t);
        }
        if let Some(o) = self.heading.as_mut() {
            o.buf.push_str(t);
        }
    }

    fn push_ref(&mut self, mut r: Ref, ctx: Ctx) -> usize {
        r.noscript = ctx.noscript;
        if self.track {
            self.nodes.push(self.at.expect("set on entering the element"));
        }
        self.data.refs.push(r);
        self.data.refs.len() - 1
    }

    fn enter<'a>(&mut self, node: NodeRef<'a, Node>, mut ctx: Ctx, stack: &mut Vec<Frame<'a>>) {
        let e = match node.value() {
            Node::Text(t) => {
                self.text(t, ctx);
                return;
            }
            Node::Element(e) => e,
            Node::Document | Node::Fragment => {
                push_children(node, ctx, stack);
                return;
            }
            // Comments, doctype, processing instructions.
            _ => return,
        };
        if e.name.ns != ns!(html) {
            // SVG and MathML: their text is visible, their elements are not
            // HTML links, images or titles.
            push_children(node, ctx, stack);
            return;
        }
        if self.track {
            self.at = Some(node.id());
        }
        let name = e.name.local.clone();
        if ctx.in_body {
            self.main_flags(node, e, &name, &mut ctx);
        }
        if name != local_name!("template") {
            ctx.agent = self.agents.enter(node, e, ctx.agent);
        }
        let inline = is_inline(&name);
        if !inline {
            self.separate();
            stack.push(Frame::Separate);
        }
        match name {
            local_name!("head") => ctx.in_head = true,
            local_name!("body") => ctx.in_body = true,
            local_name!("script") => {
                if e.attr("type")
                    .is_some_and(|t| t.trim().eq_ignore_ascii_case("application/ld+json"))
                {
                    let raw = children_text(node);
                    let parseable = serde_json::from_str::<serde::de::IgnoredAny>(&raw).is_ok();
                    self.data.structured.push(StructuredBlock {
                        kind: StructuredKind::JsonLd,
                        raw,
                        parseable,
                    });
                }
                return;
            }
            local_name!("style") | local_name!("template") => return,
            local_name!("noscript") => ctx.noscript = true,
            local_name!("title") => {
                self.data.titles.push(TextItem {
                    text: collapse(&children_text(node)),
                    in_head: ctx.in_head,
                });
                return;
            }
            local_name!("meta") => self.meta(e, ctx),
            local_name!("base") => {
                if !self.base_seen
                    && let Some(href) = e.attr("href")
                {
                    self.base_seen = true;
                    if let Ok(u) = self.page.join(href.trim())
                        && matches!(u.scheme(), "http" | "https")
                    {
                        self.data.base = u;
                    }
                }
            }
            local_name!("link") => {
                if let Some(href) = e.attr("href")
                    && let Some(kind) = link_kind(e.attr("rel"), e.attr("hreflang").is_some())
                {
                    let mut r = Ref::new(kind, href, RefSource::Html);
                    r.rel = e.attr("rel").map(str::to_string);
                    r.in_head = Some(ctx.in_head);
                    if kind == LinkKind::Hreflang {
                        r.lang = e.attr("hreflang").map(|l| l.trim().to_string());
                    }
                    self.push_ref(r, ctx);
                }
            }
            local_name!("a") | local_name!("area") => {
                if let Some(href) = e.attr("href") {
                    let rel = e.attr("rel");
                    let mut r = Ref::new(LinkKind::Anchor, href, RefSource::Html);
                    r.rel = rel.map(str::to_string);
                    r.nofollow = rel_has(rel, "nofollow");
                    r.sponsored = rel_has(rel, "sponsored");
                    r.ugc = rel_has(rel, "ugc");
                    let is_area = name == local_name!("area");
                    if is_area {
                        r.text = Some(collapse(e.attr("alt").unwrap_or("")));
                    }
                    let index = self.push_ref(r, ctx);
                    if !is_area {
                        let prev = self.anchor.replace(Open {
                            index,
                            buf: String::new(),
                        });
                        stack.push(Frame::EndAnchor(prev));
                    }
                }
            }
            local_name!("img") => self.image(e, ctx),
            local_name!("h1")
            | local_name!("h2")
            | local_name!("h3")
            | local_name!("h4")
            | local_name!("h5")
            | local_name!("h6") => {
                let level = name.as_bytes()[1] - b'0';
                self.data.headings.push(Heading {
                    level,
                    text: String::new(),
                });
                let prev = self.heading.replace(Open {
                    index: self.data.headings.len() - 1,
                    buf: String::new(),
                });
                stack.push(Frame::EndHeading(prev));
            }
            _ => {}
        }
        if !ctx.in_item && e.attr("itemscope").is_some() && e.attr("itemprop").is_none() {
            ctx.in_item = true;
            self.data.structured.push(microdata::item_block(node));
        }
        push_children(node, ctx, stack);
    }

    /// Main content flags for an element and everything inside it
    /// (spec 020 research R5, FR-003).
    fn main_flags(&mut self, node: NodeRef<'_, Node>, e: &Element, name: &str, ctx: &mut Ctx) {
        let role = role(e);
        let role = role.as_deref();
        match name {
            "main" if !self.body.main_seen => {
                self.body.main_seen = true;
                ctx.main |= MAIN;
            }
            "article" => ctx.main |= ARTICLE,
            _ => {}
        }
        let landmark = matches!(name, "nav" | "aside")
            || matches!(role, Some("navigation" | "complementary" | "search"))
            || (!ctx.sectioning
                && (matches!(name, "header" | "footer") || matches!(role, Some("banner" | "contentinfo"))));
        if landmark {
            ctx.main |= BOILER;
        }
        if matches!(name, "main" | "article" | "section") {
            ctx.sectioning = true;
        }
        if let Some(rules) = self.content
            && let Some(el) = ElementRef::wrap(node)
        {
            if rules.is_excluded(el) {
                ctx.main |= EXCLUDED;
            }
            if !self.body.selected_seen && rules.is_content(el) {
                self.body.selected_seen = true;
                ctx.main |= SELECTED;
            }
        }
    }

    fn meta(&mut self, e: &Element, ctx: Ctx) {
        let content = e.attr("content");
        if let Some(name) = e.attr("name") {
            if name.trim().eq_ignore_ascii_case("description") {
                self.data.meta_descriptions.push(TextItem {
                    text: collapse(content.unwrap_or("")),
                    in_head: ctx.in_head,
                });
            } else if let Some(crawler) = meta_crawler(name, self.token) {
                self.data
                    .directives
                    .extend(parse_meta(content.unwrap_or(""), &crawler, ctx.in_head));
            }
        } else if e
            .attr("http-equiv")
            .is_some_and(|h| h.trim().eq_ignore_ascii_case("refresh"))
            && self.data.meta_refresh.is_none()
            && let Some(refresh) = content.and_then(parse_refresh)
        {
            if let Some(url) = &refresh.url {
                let mut r = Ref::new(LinkKind::MetaRefresh, url, RefSource::Html);
                r.in_head = Some(ctx.in_head);
                self.push_ref(r, ctx);
            }
            self.data.meta_refresh = Some(refresh);
        }
    }

    fn image(&mut self, e: &Element, ctx: Ctx) {
        let alt = e.attr("alt").map(collapse);
        let in_link = self.anchor.is_some();
        if let Some(open) = &self.anchor {
            let a = &mut self.data.refs[open.index];
            a.in_link = true;
            if a.alt.is_none() {
                a.alt.clone_from(&alt);
            }
        }
        let make = |raw: &str, srcset: bool| {
            let mut r = Ref::new(LinkKind::Image, raw, RefSource::Html);
            r.alt.clone_from(&alt);
            r.width = dimension(e, "width");
            r.height = dimension(e, "height");
            r.in_link = in_link;
            r.srcset = srcset;
            r
        };
        if let Some(src) = e.attr("src").filter(|s| !s.trim().is_empty()) {
            let r = make(src, false);
            self.push_ref(r, ctx);
        }
        if let Some(set) = e.attr("srcset") {
            for candidate in srcset_urls(set) {
                let r = make(candidate, true);
                self.push_ref(r, ctx);
            }
        }
    }
}

fn push_children<'a>(node: NodeRef<'a, Node>, ctx: Ctx, stack: &mut Vec<Frame<'a>>) {
    // Reversed, so the first child is popped first.
    let mut children: Vec<_> = node.children().collect();
    children.reverse();
    stack.extend(children.into_iter().map(|c| Frame::Enter(c, ctx)));
}

/// `5; url=/new`, `0;URL='/x'`, `3`, `0; /new`.
pub(super) fn parse_refresh(content: &str) -> Option<MetaRefresh> {
    let s = content.trim_start();
    let num_len = s
        .find(|c: char| !(c.is_ascii_digit() || c == '.'))
        .unwrap_or(s.len());
    if num_len == 0 {
        return None;
    }
    let delay: f64 = s[..num_len].trim_end_matches('.').parse().unwrap_or(0.0);
    let mut rest = s[num_len..].trim_start();
    rest = rest.strip_prefix([';', ',']).unwrap_or(rest).trim_start();
    if rest.len() >= 3 && rest[..3].eq_ignore_ascii_case("url") {
        let after = rest[3..].trim_start();
        if let Some(v) = after.strip_prefix('=') {
            rest = v.trim_start();
        }
    }
    let url = if let Some(q) = rest.chars().next().filter(|c| *c == '"' || *c == '\'') {
        let inner = &rest[1..];
        inner.split(q).next().unwrap_or("")
    } else {
        rest
    }
    .trim();
    Some(MetaRefresh {
        delay,
        url: (!url.is_empty()).then(|| url.to_string()),
    })
}

/// Candidate URLs of a `srcset`: each is a run of non-space characters,
/// candidates are separated by commas, descriptors follow the URL.
fn srcset_urls(set: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut rest = set;
    loop {
        rest = rest.trim_start_matches(|c: char| c.is_ascii_whitespace() || c == ',');
        if rest.is_empty() {
            break;
        }
        let end = rest.find(|c: char| c.is_ascii_whitespace()).unwrap_or(rest.len());
        let url = &rest[..end];
        rest = &rest[end..];
        if let Some(u) = url.strip_suffix(',') {
            out.push(u.trim_end_matches(','));
            continue;
        }
        out.push(url);
        // Skip descriptors up to the next comma outside parentheses.
        let mut depth = 0;
        let mut cut = rest.len();
        for (i, c) in rest.char_indices() {
            match c {
                '(' => depth += 1,
                ')' => depth -= 1,
                ',' if depth <= 0 => {
                    cut = i + 1;
                    break;
                }
                _ => {}
            }
        }
        rest = &rest[cut..];
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refresh_forms() {
        let r = parse_refresh("0;url=/new").unwrap();
        assert_eq!((r.delay, r.url.as_deref()), (0.0, Some("/new")));
        let r = parse_refresh(" 5 ; URL = '/a b' ").unwrap();
        assert_eq!((r.delay, r.url.as_deref()), (5.0, Some("/a b")));
        let r = parse_refresh("3").unwrap();
        assert_eq!((r.delay, r.url), (3.0, None));
        let r = parse_refresh("1.5, /x").unwrap();
        assert_eq!((r.delay, r.url.as_deref()), (1.5, Some("/x")));
        assert!(parse_refresh("soon").is_none());
    }

    #[test]
    fn srcset_candidates() {
        assert_eq!(
            srcset_urls("a.jpg 1x, b.jpg 2x,c.jpg 3x"),
            ["a.jpg", "b.jpg", "c.jpg"]
        );
        assert_eq!(srcset_urls("/i?w=1,2 100w, /j 200w"), ["/i?w=1,2", "/j"]);
        assert_eq!(srcset_urls("x.png"), ["x.png"]);
        assert_eq!(srcset_urls(" , "), Vec::<&str>::new());
    }
}
