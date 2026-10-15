//! A fast path for the XPath most people write (T032).
//!
//! Copying the page into an XPath tree and evaluating there costs about
//! 20 µs per rule on a 3 KB page, several times a CSS rule. Simple location
//! paths have an exact CSS equivalent, so they run on the page's own tree:
//! steps with `/` or `//` and a lowercase element name or `*`, attribute
//! predicates (`[@a]`, `[@a='v']`) or one position (`[2]`), and an optional
//! last `/@attr` or `/text()`, alone or inside `count()` or `string()`.
//! Anything else goes to the full engine. The tests check both give the
//! same answer.

use scraper::{ElementRef, Html, Node, Selector};

/// What the path selects from each matched element.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Tail {
    Element,
    Attr(String),
    Text,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Wrap {
    None,
    Count,
    String,
}

pub(super) struct Fast {
    selector: Selector,
    tail: Tail,
    wrap: Wrap,
}

/// One node the path selected, as [`super::xpath::Hit`].
pub(super) enum Hit<'a> {
    Element(ElementRef<'a>),
    Other(String),
}

pub(super) enum Found<'a> {
    Nodes(Vec<Hit<'a>>),
    Value(String),
}

impl Fast {
    /// The CSS form of `expr`, or `None` when it needs the full engine.
    pub(super) fn new(expr: &str) -> Option<Self> {
        let e = expr.trim();
        let (wrap, inner) = if let Some(r) = e.strip_prefix("count(").and_then(|r| r.strip_suffix(')')) {
            (Wrap::Count, r)
        } else if let Some(r) = e.strip_prefix("string(").and_then(|r| r.strip_suffix(')')) {
            (Wrap::String, r)
        } else {
            (Wrap::None, e)
        };
        let (css, tail) = translate(inner.trim())?;
        let selector = Selector::parse(&css).ok()?;
        Some(Self { selector, tail, wrap })
    }

    /// Only element results, for search limits.
    pub(super) fn selects_elements(&self) -> bool {
        self.wrap == Wrap::None && self.tail == Tail::Element
    }

    pub(super) fn selector(&self) -> &Selector {
        &self.selector
    }

    pub(super) fn evaluate<'a>(&self, doc: &'a Html) -> Found<'a> {
        let mut hits = Vec::new();
        for el in doc.select(&self.selector) {
            match &self.tail {
                Tail::Element => hits.push(Hit::Element(el)),
                Tail::Attr(a) => {
                    if let Some(v) = el.value().attr(a) {
                        hits.push(Hit::Other(v.to_string()));
                    }
                }
                Tail::Text => {
                    for c in el.children() {
                        if let Node::Text(t) = c.value() {
                            hits.push(Hit::Other(t.to_string()));
                        }
                    }
                }
            }
            // string() needs only the first node.
            if self.wrap == Wrap::String && !hits.is_empty() {
                break;
            }
        }
        match self.wrap {
            Wrap::None => Found::Nodes(hits),
            Wrap::Count => Found::Value(hits.len().to_string()),
            Wrap::String => Found::Value(match hits.into_iter().next() {
                None => String::new(),
                Some(Hit::Element(el)) => el.text().collect(),
                Some(Hit::Other(s)) => s,
            }),
        }
    }
}

fn name_char(c: char) -> bool {
    c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_'
}

/// A lowercase name at the start of `s`, and the rest.
fn name(s: &str) -> Option<(&str, &str)> {
    let end = s.find(|c: char| !name_char(c)).unwrap_or(s.len());
    let n = &s[..end];
    n.starts_with(|c: char| c.is_ascii_lowercase())
        .then_some((n, &s[end..]))
}

/// A quoted XPath string literal at the start of `s`, and the rest.
fn literal(s: &str) -> Option<(&str, &str)> {
    let q = s.chars().next().filter(|c| *c == '\'' || *c == '"')?;
    let body = &s[1..];
    let end = body.find(q)?;
    Some((&body[..end], &body[end + 1..]))
}

fn css_string(v: &str) -> String {
    let mut out = String::from("\"");
    for c in v.chars() {
        match c {
            '"' | '\\' => {
                out.push('\\');
                out.push(c);
            }
            '\n' => out.push_str("\\a "),
            c if c.is_control() => return String::new(),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn translate(path: &str) -> Option<(String, Tail)> {
    let mut css = String::new();
    let mut rest = path;
    let mut first = true;
    loop {
        let (descendant, after) = if let Some(r) = rest.strip_prefix("//") {
            (true, r)
        } else {
            (false, rest.strip_prefix('/')?)
        };
        rest = after;
        // The last step may pick an attribute or the text children.
        if !first && !descendant {
            if let Some(r) = rest.strip_prefix('@') {
                let (a, r) = name(r)?;
                return r.is_empty().then(|| (css, Tail::Attr(a.to_string())));
            }
            if rest == "text()" {
                return Some((css, Tail::Text));
            }
        }
        let (step, r) = if let Some(r) = rest.strip_prefix('*') {
            ("*", r)
        } else {
            name(rest)?
        };
        rest = r;
        if first {
            // An absolute first step is the root element itself.
            css.push_str(if descendant { "" } else { ":root" });
        } else {
            css.push_str(if descendant { " " } else { " > " });
        }
        if first && !descendant {
            // `:root` then the name test on the same element.
            if step != "*" {
                css.insert_str(0, step);
            }
        } else {
            css.push_str(step);
        }
        let mut position = None;
        let mut attrs = 0;
        while let Some(r) = rest.strip_prefix('[') {
            if let Some(r) = r.strip_prefix('@') {
                let (a, r) = name(r)?;
                let (sel, r) = if let Some(r) = r.strip_prefix('=') {
                    let (v, r) = literal(r)?;
                    let q = css_string(v);
                    if q.is_empty() {
                        return None;
                    }
                    // `s`: values compare exactly, as in XPath, also for
                    // the attributes HTML matches without case in CSS.
                    (format!("[{a}={q} s]"), r)
                } else {
                    (format!("[{a}]"), r)
                };
                rest = r.strip_prefix(']')?;
                css.push_str(&sel);
                attrs += 1;
            } else {
                let end = r.find(']')?;
                let n: u32 = r[..end].parse().ok().filter(|n| *n > 0)?;
                position = Some(n);
                rest = &r[end + 1..];
            }
        }
        if let Some(n) = position {
            // `[2]` counts among the siblings the step names; with other
            // predicates it would count among those that pass them.
            if attrs > 0 {
                return None;
            }
            css.push_str(&if step == "*" {
                format!(":nth-child({n})")
            } else {
                format!(":nth-of-type({n})")
            });
        }
        first = false;
        if rest.is_empty() {
            return Some((css, Tail::Element));
        }
    }
}

#[cfg(test)]
mod tests {
    use sxd_document::Package;

    use super::super::xpath::{self, Tree};
    use super::*;

    const PAGES: &[&str] = &[
        r#"<!doctype html><html lang="en"><head><title>Shop &amp; more</title>
        <meta name="description" content="Shoes"><meta itemprop="sku" content="AB-1">
        <script>var x = "<p>";</script></head><body><h1>Big <b>shoe</b> sale</h1>
        <ul><li><a href="/a">A</a></li><li><a href="/b" rel="nofollow">B</a> tail</li><li>C</li></ul>
        <div class="x"><p>one</p><div><p>two <a href="/c">c</a></p></div><p></p></div>
        <table><td>cell</td></table><!-- note --><p>after</p></body></html>"#,
        "<p>plain",
        r#"<a rel="NoFollow" href="/x">x</a><a rel="nofollow">y</a><input type="TEXT">"#,
        r#"<div><span id="q">it's "quoted"</span><span id='q2'>x</span></div>"#,
    ];

    const EXPRS: &[&str] = &[
        "//h1",
        "//a",
        "//a/@href",
        "//ul/li[2]/a/@href",
        "//li[2]",
        "//li[3]/text()",
        "//*[2]",
        "//meta[@name='description']/@content",
        "//meta[@itemprop=\"sku\"]/@content",
        "//a[@rel]",
        "//a[@rel='nofollow'][@href='/b']",
        "/html/body/h1",
        "/html//p",
        "/*/head/title",
        "//div//p",
        "//div/p",
        "//div[@class='x']/div/p/a/@href",
        "//p/text()",
        "//h1/text()",
        "//tbody/tr/td",
        "//span[@id='q']",
        "count(//a)",
        "count(//p)",
        "count(//nothing)",
        "count(//a/@href)",
        "string(//title)",
        "string(//h1)",
        "string(//nothing)",
        "string(//meta/@content)",
        "string(//script)",
        "//body/p",
        "//a[@rel='nofollow']",
        "//input[@type='text']",
    ];

    /// Result rendering that both engines share.
    fn fast(doc: &Html, expr: &str) -> Vec<String> {
        match Fast::new(expr)
            .unwrap_or_else(|| panic!("{expr} has a fast path"))
            .evaluate(doc)
        {
            Found::Nodes(n) => n
                .into_iter()
                .map(|h| match h {
                    Hit::Element(e) => format!("<{:?}>", e.id()),
                    Hit::Other(s) => s,
                })
                .collect(),
            Found::Value(v) => vec![v],
        }
    }

    fn full(doc: &Html, expr: &str) -> Vec<String> {
        let p = Package::new();
        let t = Tree::new(&p, doc);
        match t.evaluate(expr).unwrap() {
            xpath::Found::Nodes(n) => n
                .into_iter()
                .map(|h| match h {
                    xpath::Hit::Element(id) => format!("<{id:?}>"),
                    xpath::Hit::Other(s) => s,
                })
                .collect(),
            xpath::Found::Value(v) => vec![v],
        }
    }

    #[test]
    fn fast_path_agrees_with_the_full_engine() {
        for page in PAGES {
            let doc = Html::parse_document(page);
            for expr in EXPRS {
                assert_eq!(fast(&doc, expr), full(&doc, expr), "{expr} on {page}");
            }
        }
    }

    #[test]
    fn other_expressions_use_the_full_engine() {
        for expr in [
            "//H1",
            "//a[@rel='x'][2]",
            "//a[contains(@href, 'x')]",
            "//a/..",
            "//a[last()]",
            "//svg:path",
            "//a/@Href",
            "//p[0]",
            "h1",
            "//a | //p",
            "//p//text()",
            "normalize-space(//h1)",
            "//a[@href!='x']",
            "(//a)[1]",
        ] {
            assert!(Fast::new(expr).is_none(), "{expr}");
        }
    }
}
