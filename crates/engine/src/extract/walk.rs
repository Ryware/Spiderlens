//! The single depth-first walk over the parsed document (plan D1).
//!
//! Iterative, not recursive, so deeply nested malformed pages cannot
//! overflow the stack.

use std::time::{Duration, Instant};

use ego_tree::NodeRef;
use html5ever::driver::ParseOpts;
use html5ever::tendril::TendrilSink;
use html5ever::tree_builder::TreeBuilderOpts;
use html5ever::{local_name, ns};
use scraper::node::Element;
use scraper::{Html, HtmlTreeSink, Node};
use url::Url;

use super::a11y::A11yCollector;
use super::directives::{meta_crawler, parse_meta};
use super::link_header::{link_kind, rel_has};
use super::text::{collapse, hash, word_count};
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
    data: PageData,
    base_seen: bool,
    visible: String,
    anchor: Option<Open>,
    heading: Option<Open>,
}

pub(super) fn walk(
    text: &str,
    page: &Url,
    token: &str,
    budget: Duration,
    a11y: bool,
    hook: Option<(&super::PageHook, usize)>,
) -> PageData {
    let (doc, cut) = parse(text, budget);
    // Spec 026: only created when asked for, so "Off" costs this test.
    let mut checks = a11y.then(|| A11yCollector::new(&doc));
    let mut w = Walker {
        page,
        token,
        data: PageData::empty(page.clone(), "", false),
        base_seen: false,
        visible: String::new(),
        anchor: None,
        heading: None,
    };
    w.data.no_content = false;
    w.data.partial = cut;
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
    w.data.text_hash = hash(&visible);
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
    w.data
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
        let name = e.name.local.clone();
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
