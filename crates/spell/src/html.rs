//! The visible body text of a stored page, in blocks, for spelling.
//!
//! Headings are left out (they are checked from the crawl's heading list),
//! and so is text that is not prose: scripts, styles, code, preformatted
//! text, form controls, SVG and MathML.

use scraper::{ElementRef, Html, Node};

const SKIP: &[&str] = &[
    "script", "style", "template", "noscript", "code", "pre", "kbd", "samp", "var", "svg", "math",
    "textarea", "select", "option", "button", "h1", "h2", "h3", "h4", "h5", "h6", "head", "title", "iframe",
    "object",
];

const BLOCK: &[&str] = &[
    "p",
    "div",
    "li",
    "td",
    "th",
    "dd",
    "dt",
    "section",
    "article",
    "aside",
    "header",
    "footer",
    "nav",
    "main",
    "blockquote",
    "figcaption",
    "caption",
    "summary",
    "details",
    "form",
    "fieldset",
    "legend",
    "label",
    "ul",
    "ol",
    "dl",
    "table",
    "tr",
    "br",
    "hr",
    "address",
];

/// Body text split into blocks, whitespace collapsed, empty blocks dropped.
pub fn body_blocks(html: &str) -> Vec<String> {
    let doc = Html::parse_document(html);
    let mut blocks = vec![String::new()];
    let root = doc
        .root_element()
        .children()
        .filter_map(ElementRef::wrap)
        .find(|e| e.value().name() == "body")
        .unwrap_or_else(|| doc.root_element());
    walk(root, &mut blocks);
    blocks
        .into_iter()
        .map(|b| b.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|b| !b.is_empty())
        .collect()
}

fn walk(el: ElementRef, blocks: &mut Vec<String>) {
    for child in el.children() {
        match child.value() {
            Node::Text(t) => {
                let last = blocks.last_mut().expect("never empty");
                last.push_str(t);
            }
            Node::Element(e) => {
                let name = e.name();
                if SKIP.contains(&name) {
                    // A skipped element still ends a word.
                    blocks.last_mut().expect("never empty").push(' ');
                    continue;
                }
                let block = BLOCK.contains(&name);
                if block {
                    blocks.push(String::new());
                }
                if let Some(c) = ElementRef::wrap(child) {
                    walk(c, blocks);
                }
                if block {
                    blocks.push(String::new());
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_skip_code_scripts_and_headings() {
        let html = "<html><head><title>T</title></head><body><h1>Head</h1><p>First <b>bold</b> para</p>\
                    <script>var x</script><pre>let y</pre><p>Use <code>fn main</code> here</p>\
                    <ul><li>One</li><li>Two</li></ul><footer>Copyright notise</footer></body></html>";
        assert_eq!(
            body_blocks(html),
            ["First bold para", "Use here", "One", "Two", "Copyright notise"]
        );
    }
}
