//! XPath 1.0 over the page's one parse (plan D3, research R1).
//!
//! html5ever's tree has no XPath, so when a crawl has XPath rules the
//! parsed tree is copied once per page into an `sxd-document` tree (no
//! second parse). Elements go in no namespace, so `//div` matches HTML
//! elements as it does in browsers. A map back to the original nodes lets
//! Text, Inner HTML and Outer HTML come out exactly as for CSS rules.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use ego_tree::NodeId;
use scraper::{Html, Node};
use sxd_document::Package;
use sxd_document::dom::{Document, Element, ParentOfChild};
use sxd_xpath::nodeset::Node as XNode;
use sxd_xpath::{Context, Factory, Value, XPath};

/// Compiles an expression, for validation and per page.
pub(super) fn build(expr: &str) -> Result<XPath, String> {
    match Factory::new().build(expr) {
        Ok(Some(x)) => Ok(x),
        Ok(None) => Err("Enter an XPath expression.".into()),
        Err(e) => Err(e.to_string()),
    }
}

/// Compiled expressions per thread. `XPath` cannot cross threads, and
/// compiling it on every page cost more than evaluating it (T032).
const CACHE_LIMIT: usize = 256;

thread_local! {
    static COMPILED: RefCell<HashMap<String, Rc<XPath>>> = RefCell::new(HashMap::new());
}

fn compiled(expr: &str) -> Result<Rc<XPath>, String> {
    if let Some(x) = COMPILED.with(|c| c.borrow().get(expr).cloned()) {
        return Ok(x);
    }
    let x = Rc::new(build(expr)?);
    COMPILED.with(|c| {
        let mut c = c.borrow_mut();
        if c.len() >= CACHE_LIMIT {
            c.clear();
        }
        c.insert(expr.to_string(), Rc::clone(&x));
    });
    Ok(x)
}

/// One node an expression selected.
pub(super) enum Hit {
    /// An element of the original document.
    Element(NodeId),
    /// An attribute, text or other node: its string value.
    Other(String),
}

pub(super) enum Found {
    Nodes(Vec<Hit>),
    /// A number, string or true/false result (FR-002, function value).
    Value(String),
}

// sxd elements hash and compare by node address, which never changes while
// the package lives, so their interior mutability cannot move a key.
#[allow(clippy::mutable_key_type)]
pub(super) struct Tree<'d> {
    doc: Document<'d>,
    back: HashMap<Element<'d>, NodeId>,
}

impl<'d> Tree<'d> {
    /// Copies `html` into `package`. Linear in the number of nodes.
    #[allow(clippy::mutable_key_type)]
    pub(super) fn new(package: &'d Package, html: &Html) -> Self {
        let doc = package.as_document();
        let mut back = HashMap::new();
        let mut stack: Vec<(ego_tree::NodeRef<'_, Node>, ParentOfChild<'d>)> = html
            .tree
            .root()
            .children()
            .map(|c| (c, ParentOfChild::Root(doc.root())))
            .collect();
        stack.reverse();
        while let Some((node, parent)) = stack.pop() {
            let child = match node.value() {
                Node::Element(e) => {
                    let el = doc.create_element(&*e.name.local);
                    for (name, value) in e.attrs() {
                        el.set_attribute_value(name, value);
                    }
                    back.insert(el, node.id());
                    let first = stack.len();
                    stack.extend(node.children().map(|c| (c, ParentOfChild::Element(el))));
                    stack[first..].reverse();
                    sxd_document::dom::ChildOfElement::Element(el)
                }
                Node::Text(t) => sxd_document::dom::ChildOfElement::Text(doc.create_text(t)),
                Node::Comment(c) => sxd_document::dom::ChildOfElement::Comment(doc.create_comment(c)),
                _ => continue,
            };
            match parent {
                ParentOfChild::Root(r) => {
                    // The root takes elements and comments only.
                    match child {
                        sxd_document::dom::ChildOfElement::Element(e) => r.append_child(e),
                        sxd_document::dom::ChildOfElement::Comment(c) => r.append_child(c),
                        _ => {}
                    }
                }
                ParentOfChild::Element(p) => p.append_child(child),
            }
        }
        Self { doc, back }
    }

    pub(super) fn evaluate(&self, expr: &str) -> Result<Found, String> {
        let x = compiled(expr)?;
        let ctx = Context::new();
        let value = x.evaluate(&ctx, self.doc.root()).map_err(|e| e.to_string())?;
        Ok(match value {
            Value::Nodeset(set) => Found::Nodes(
                set.document_order()
                    .into_iter()
                    .map(|n| match n {
                        XNode::Element(e) => match self.back.get(&e) {
                            Some(id) => Hit::Element(*id),
                            None => Hit::Other(n.string_value()),
                        },
                        XNode::Attribute(a) => Hit::Other(a.value().to_string()),
                        XNode::Text(t) => Hit::Other(t.text().to_string()),
                        other => Hit::Other(other.string_value()),
                    })
                    .collect(),
            ),
            Value::Number(n) => Found::Value(number(n)),
            Value::String(s) => Found::Value(s),
            Value::Boolean(b) => Found::Value(b.to_string()),
        })
    }

    /// The elements an expression selects, for search limits.
    pub(super) fn elements(&self, expr: &str) -> Result<Vec<NodeId>, String> {
        Ok(match self.evaluate(expr)? {
            Found::Nodes(hits) => hits
                .into_iter()
                .filter_map(|h| match h {
                    Hit::Element(id) => Some(id),
                    Hit::Other(_) => None,
                })
                .collect(),
            Found::Value(_) => return Err("The element limit must select elements, not a value.".into()),
        })
    }
}

/// XPath numbers as people write them: `5`, not `5.0`.
fn number(n: f64) -> String {
    if n.is_nan() {
        "NaN".into()
    } else if n.is_infinite() {
        if n > 0.0 {
            "Infinity".into()
        } else {
            "-Infinity".into()
        }
    } else if n.fract() == 0.0 && n.abs() < 1e15 {
        format!("{}", n as i64)
    } else {
        format!("{n}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn eval(html: &str, expr: &str) -> Vec<String> {
        let doc = Html::parse_document(html);
        let p = Package::new();
        let t = Tree::new(&p, &doc);
        match t.evaluate(expr).unwrap() {
            Found::Nodes(n) => n
                .into_iter()
                .map(|h| match h {
                    Hit::Element(id) => {
                        let e = doc.tree.get(id).unwrap();
                        format!("<{}>", e.value().as_element().unwrap().name())
                    }
                    Hit::Other(s) => s,
                })
                .collect(),
            Found::Value(v) => vec![v],
        }
    }

    #[test]
    fn html_elements_match_without_a_namespace() {
        assert_eq!(eval("<p>a</p><div><p>b</p></div>", "//p"), vec!["<p>", "<p>"]);
        assert_eq!(eval("<p>a</p>", "count(//p)"), vec!["1"]);
    }

    #[test]
    fn attributes_and_text() {
        let html = r#"<meta itemprop="sku" content="AB-123"><p>Hi <b>there</b></p>"#;
        assert_eq!(eval(html, "//meta[@itemprop='sku']/@content"), vec!["AB-123"]);
        assert_eq!(eval(html, "//p/text()"), vec!["Hi "]);
        assert_eq!(eval(html, "string(//p)"), vec!["Hi there"]);
        assert_eq!(eval(html, "boolean(//b)"), vec!["true"]);
    }

    #[test]
    fn malformed_markup_follows_the_browser_tree() {
        // The parser closes <p> before <div>, as browsers do.
        assert_eq!(eval("<p>one<div>two</div>", "count(//p/div)"), vec!["0"]);
        assert_eq!(eval("<table><td>x</table>", "count(//tbody/tr/td)"), vec!["1"]);
    }

    #[test]
    fn numbers_print_plainly() {
        assert_eq!(number(5.0), "5");
        assert_eq!(number(2.5), "2.5");
        assert_eq!(number(f64::NAN), "NaN");
    }
}

#[cfg(test)]
mod cost {
    use super::*;

    #[test]
    #[ignore]
    fn tree_copy_cost() {
        let Ok(page) = std::env::var("BENCH_PAGE") else {
            return;
        };
        let html = Html::parse_document(&std::fs::read_to_string(page).unwrap());
        let n = 5_000;
        let t = std::time::Instant::now();
        for _ in 0..n {
            let p = Package::new();
            std::hint::black_box(Tree::new(&p, &html));
        }
        println!("copy: {:.1} us", t.elapsed().as_secs_f64() * 1e6 / n as f64);
        let p = Package::new();
        let tree = Tree::new(&p, &html);
        for e in [
            "/descendant::h1",
            "count(/descendant::a)",
            "//h1",
            "count(//a)",
            "//meta[@name='description']/@content",
            "//ul/li[2]/a/@href",
            "string(//title)",
        ] {
            let t = std::time::Instant::now();
            for _ in 0..n {
                std::hint::black_box(tree.evaluate(e).ok());
            }
            println!("{e}: {:.1} us", t.elapsed().as_secs_f64() * 1e6 / n as f64);
        }
    }
}
