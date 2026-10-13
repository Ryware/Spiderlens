//! Facts read from a page's stored HTML (spec 015 FR-017, FR-020 to FR-023,
//! FR-026, FR-028; plan D7).
//!
//! One pass over the parsed document builds [`ContentFacts`]; the checks
//! then read only that struct. Pure: HTML in, facts out.

use std::collections::HashMap;
use std::sync::LazyLock;

use ego_tree::NodeRef;
use regex::Regex;
use scraper::{Html, Node};
use serde::Deserialize;
use spiderlens_engine::extract::text::{collapse, word_count};
use url::Url;

/// A block of the main content, in document order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Block {
    Heading { level: u8, text: String },
    Paragraph { text: String },
    List { items: u32, text: String },
    Table,
    Media,
    Quote { text: String },
    Other { text: String },
}

/// A heading phrased as a question, and what followed it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuestionHeading {
    pub level: u8,
    pub text: String,
    /// The first sentence of the next block, when it is a paragraph or list.
    pub answer: Option<String>,
    pub answer_words: u32,
    /// What came next: "paragraph", "list", "heading", "media", "table", "nothing", ...
    pub followed_by: &'static str,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DateFound {
    pub date: String,
    /// Where it was found: `time`, `visible`.
    pub source: &'static str,
    /// Next to an "updated" word.
    pub modified: bool,
}

/// Raw-HTML evidence that a page is built by JavaScript (FR-017).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct JsEvidence {
    pub html_bytes: usize,
    pub script_bytes: usize,
    pub external_scripts: u32,
    pub body_words: u32,
    /// An application container with no text (`#root`, `#app`, `#__next`, ...).
    pub empty_container: Option<String>,
    pub framework_markers: Vec<&'static str>,
}

impl JsEvidence {
    /// Little visible text for the page's size, plus a container or heavy scripts.
    pub fn suspected(&self) -> bool {
        self.body_words < 50
            && (self.empty_container.is_some()
                || self.script_bytes * 2 > self.html_bytes
                || (self.external_scripts >= 3 && self.html_bytes > 2_000))
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ContentFacts {
    /// Which element was taken as the main content.
    pub main_source: &'static str,
    pub main_words: u32,
    pub blocks: Vec<Block>,
    pub questions: Vec<QuestionHeading>,
    pub subheadings: u32,
    pub paragraphs: u32,
    pub longest_paragraph: u32,
    pub long_paragraphs: u32,
    pub lists: u32,
    pub tables: u32,
    pub figures: u32,
    pub quotations: u32,
    /// Links from the main content to other sites.
    pub citations: u32,
    /// `details`/`summary` and `dt` questions shown on the page.
    pub visible_qa: u32,
    /// Normalised visible text of the whole body, for FAQ comparison.
    pub visible_text: String,
    pub byline: Option<String>,
    pub author_link: Option<String>,
    pub dates: Vec<DateFound>,
    pub data_nosnippet: u32,
    pub data_snippet: u32,
    pub js: JsEvidence,
}

/// Thresholds used while reading content (scoring profile, FR-040).
#[derive(Debug, Clone, Copy)]
pub struct ContentLimits {
    pub long_paragraph_words: u32,
}

impl Default for ContentLimits {
    fn default() -> Self {
        Self {
            long_paragraph_words: 150,
        }
    }
}

#[derive(Deserialize)]
struct DatesData {
    months: HashMap<String, Vec<Vec<String>>>,
    updated_words: HashMap<String, Vec<String>>,
}

static QUESTIONS: LazyLock<HashMap<String, Vec<String>>> =
    LazyLock::new(|| serde_json::from_str(include_str!("../data/questions.json")).expect("questions.json"));
static DATES: LazyLock<DatesData> =
    LazyLock::new(|| serde_json::from_str(include_str!("../data/dates.json")).expect("dates.json"));

/// Languages with question words and month names (Assumptions in spec.md).
pub fn supported_language(lang: &str) -> bool {
    QUESTIONS.contains_key(primary(lang).as_str())
}

fn primary(lang: &str) -> String {
    let p = lang.split(['-', '_']).next().unwrap_or("").to_ascii_lowercase();
    // Old ISO code for Hebrew.
    if p == "iw" { "he".into() } else { p }
}

const SKIP: &[&str] = &["script", "style", "template", "noscript", "svg", "head"];
const CHROME: &[&str] = &["nav", "header", "footer", "aside", "form"];
const CHROME_ROLES: &[&str] = &["navigation", "banner", "contentinfo", "complementary", "search"];

fn is_chrome(el: &scraper::node::Element) -> bool {
    CHROME.contains(&el.name()) || el.attr("role").is_some_and(|r| CHROME_ROLES.contains(&r))
}

/// Text of a node, skipping scripts and similar.
fn text_of(node: NodeRef<'_, Node>) -> String {
    let mut out = String::new();
    let mut stack = vec![node];
    while let Some(n) = stack.pop() {
        match n.value() {
            Node::Text(t) => {
                out.push_str(t);
            }
            Node::Element(el) if SKIP.contains(&el.name()) => continue,
            Node::Element(el)
                if !matches!(
                    el.name(),
                    "a" | "span"
                        | "em"
                        | "strong"
                        | "b"
                        | "i"
                        | "code"
                        | "small"
                        | "sup"
                        | "sub"
                        | "abbr"
                        | "time"
                        | "mark"
                        | "q"
                        | "cite"
                ) =>
            {
                out.push(' ');
            }
            _ => {}
        }
        let children: Vec<_> = n.children().collect();
        stack.extend(children.into_iter().rev());
    }
    collapse(&out)
}

/// The main content root and its name (plan D7).
fn main_root(doc: &Html) -> (NodeRef<'_, Node>, &'static str) {
    let find = |name: &str| -> Vec<NodeRef<'_, Node>> {
        doc.tree
            .root()
            .descendants()
            .filter(|n| n.value().as_element().is_some_and(|e| e.name() == name))
            .collect()
    };
    if let Some(m) = find("main").into_iter().next() {
        return (m, "main");
    }
    let articles = find("article");
    if articles.len() == 1 {
        return (articles[0], "article");
    }
    match find("body").into_iter().next() {
        Some(b) => (b, "body"),
        None => (doc.tree.root(), "document"),
    }
}

/// Splits the main content into blocks, skipping page chrome.
fn blocks(root: NodeRef<'_, Node>) -> Vec<Block> {
    let mut out = Vec::new();
    let mut stack = vec![root];
    while let Some(n) = stack.pop() {
        let Some(el) = n.value().as_element() else {
            continue;
        };
        if SKIP.contains(&el.name()) || (n.id() != root.id() && is_chrome(el)) {
            continue;
        }
        let block = match el.name() {
            "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                let level = el.name().as_bytes()[1] - b'0';
                Some(Block::Heading {
                    level,
                    text: text_of(n),
                })
            }
            "p" => Some(Block::Paragraph { text: text_of(n) }),
            "ul" | "ol" | "dl" => {
                let items = n
                    .children()
                    .filter(|c| {
                        c.value()
                            .as_element()
                            .is_some_and(|e| matches!(e.name(), "li" | "dt"))
                    })
                    .count() as u32;
                Some(Block::List {
                    items,
                    text: text_of(n),
                })
            }
            "table" => Some(Block::Table),
            "img" | "picture" | "figure" | "video" | "iframe" => Some(Block::Media),
            "blockquote" => Some(Block::Quote { text: text_of(n) }),
            "pre" => Some(Block::Other { text: text_of(n) }),
            _ => None,
        };
        match block {
            Some(Block::Paragraph { text }) if text.is_empty() => {}
            Some(b) => out.push(b),
            None => {
                // Containers: text sitting directly in them counts as a paragraph.
                let direct: String = n
                    .children()
                    .filter_map(|c| c.value().as_text().map(|t| t.to_string()))
                    .collect();
                let direct = collapse(&direct);
                if word_count(&direct) >= 5 && n.id() != root.id() {
                    out.push(Block::Other { text: direct });
                }
                let children: Vec<_> = n.children().collect();
                stack.extend(children.into_iter().rev());
            }
        }
    }
    out
}

fn first_sentence(text: &str) -> &str {
    let end = text
        .char_indices()
        .find(|(i, c)| {
            matches!(c, '.' | '?' | '!' | '。' | '؟' | '׃')
                && text[i + c.len_utf8()..]
                    .chars()
                    .next()
                    .is_none_or(char::is_whitespace)
        })
        .map(|(i, c)| i + c.len_utf8())
        .unwrap_or(text.len());
    &text[..end]
}

/// Whether a heading asks a question in `lang` (or any supported language
/// when the page's language is unknown).
pub fn is_question(text: &str, lang: Option<&str>) -> bool {
    let t = text.trim();
    if t.ends_with('?') || t.ends_with('؟') || t.starts_with('¿') {
        return true;
    }
    let lower = t.to_lowercase();
    let starts = |words: &Vec<String>| {
        words.iter().any(|w| {
            lower.starts_with(w.as_str())
                && lower[w.len()..]
                    .chars()
                    .next()
                    .is_some_and(|c| c.is_whitespace() || c == ',')
        })
    };
    match lang.map(primary).filter(|l| QUESTIONS.contains_key(l)) {
        // Words such as "is" or "do" start many statements; without a
        // question mark only the wh-words of the page's language count.
        Some(l) => starts(&QUESTIONS[&l].iter().take(9).cloned().collect()),
        None => false,
    }
}

static FIGURE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?x)
        \d+(?:[.,]\d+)?\s?%                                  # percentages
      | [$€£₪¥]\s?\d                                          # amounts
      | \d+(?:[.,]\d+)?\s?(?:kg|g|km|m|cm|mm|mb|gb|tb|ms|s|h|mph|km/h|°c|°f|usd|eur|ils)\b
      | \b(?:19|20)\d{2}\b                                    # years
      | \b\d{1,3}(?:,\d{3})+\b                                # 1,234,567
    ",
    )
    .expect("figure regex")
});

static QUOTE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"[“"„«]([^”"“»]{25,400})[”"“»]"#).expect("quote regex"));

fn same_site(a: &Url, b: &Url) -> bool {
    let strip = |h: &str| h.trim_start_matches("www.").to_ascii_lowercase();
    match (a.host_str(), b.host_str()) {
        (Some(x), Some(y)) => strip(x) == strip(y),
        _ => true,
    }
}

static ISO_DATE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\b(\d{4})-(\d{2})-(\d{2})\b").expect("iso date regex"));

/// Parses a visible date in the page's language (or any supported language).
pub fn parse_visible_date(text: &str, lang: Option<&str>) -> Option<String> {
    let lower = text.to_lowercase();
    if let Some(c) = ISO_DATE.captures(&lower) {
        return valid(c[1].parse().ok()?, c[2].parse().ok()?, c[3].parse().ok()?);
    }
    let langs: Vec<String> = match lang.map(primary).filter(|l| DATES.months.contains_key(l)) {
        Some(l) => vec![l],
        None => {
            let mut all: Vec<_> = DATES.months.keys().cloned().collect();
            all.sort();
            all
        }
    };
    let tokens: Vec<&str> = lower
        .split(|c: char| c.is_whitespace() || matches!(c, ',' | '.' | '/' | '|' | '(' | ')'))
        .filter(|t| !t.is_empty())
        .collect();
    let month_of = |tok: &str| -> Option<i8> {
        for l in &langs {
            for (i, names) in DATES.months[l].iter().enumerate() {
                // Hebrew dates write the month with the prefix ב ("בספטמבר").
                let t = if l == "he" {
                    tok.trim_start_matches('ב')
                } else {
                    tok
                };
                if names.iter().any(|n| n == t || n == tok) {
                    return Some(i as i8 + 1);
                }
            }
        }
        None
    };
    let num = |t: &str| t.parse::<i32>().ok();
    for i in 0..tokens.len() {
        let Some(m) = month_of(tokens[i]) else { continue };
        // "1 September 2026", "1. September 2026", "1 de septiembre de 2026", "1 בספטמבר 2026"
        let before = i.checked_sub(1).and_then(|j| num(tokens[j])).or_else(|| {
            i.checked_sub(2)
                .filter(|&j| tokens[j + 1] == "de")
                .and_then(|j| num(tokens[j]))
        });
        let after_year = tokens[i + 1..]
            .iter()
            .take(2)
            .filter(|t| **t != "de")
            .find_map(|t| num(t).filter(|y| *y > 1900));
        if let (Some(d), Some(y)) = (before, after_year)
            && let Some(v) = valid(y, m, d)
        {
            return Some(v);
        }
        // "September 1, 2026"
        if let (Some(d), Some(y)) = (
            tokens.get(i + 1).and_then(|t| num(t)),
            tokens.get(i + 2).and_then(|t| num(t)),
        ) && let Some(v) = valid(y, m, d)
        {
            return Some(v);
        }
    }
    None
}

fn valid(y: i32, m: i8, d: i32) -> Option<String> {
    let d = i8::try_from(d).ok()?;
    let y = i16::try_from(y).ok()?;
    jiff::civil::Date::new(y, m, d).ok().map(|date| date.to_string())
}

fn is_updated_label(text: &str, lang: Option<&str>) -> bool {
    let lower = text.to_lowercase();
    let check = |l: &str| {
        DATES
            .updated_words
            .get(l)
            .is_some_and(|w| w.iter().any(|w| lower.contains(w.as_str())))
    };
    match lang.map(primary) {
        Some(l) if DATES.updated_words.contains_key(&l) => check(&l) || check("en"),
        _ => DATES.updated_words.keys().any(|l| check(l)),
    }
}

/// Reads every content fact from a parsed page.
pub fn read(
    doc: &Html,
    page_url: &Url,
    html_bytes: usize,
    lang: Option<&str>,
    limits: ContentLimits,
) -> ContentFacts {
    let (root, main_source) = main_root(doc);
    let blocks = blocks(root);
    let mut f = ContentFacts {
        main_source,
        ..Default::default()
    };

    // Structure and questions.
    let mut main_text = String::new();
    for (i, b) in blocks.iter().enumerate() {
        match b {
            Block::Heading { level, text } => {
                if *level >= 2 {
                    f.subheadings += 1;
                }
                if (2..=4).contains(level) && is_question(text, lang) {
                    let next = blocks[i + 1..].first();
                    let (answer, followed_by) = match next {
                        Some(Block::Paragraph { text }) | Some(Block::Other { text }) => {
                            (Some(first_sentence(text).to_string()), "paragraph")
                        }
                        Some(Block::List { text, .. }) => (Some(first_sentence(text).to_string()), "list"),
                        Some(Block::Heading { .. }) => (None, "heading"),
                        Some(Block::Media) => (None, "media"),
                        Some(Block::Table) => (None, "table"),
                        Some(Block::Quote { .. }) => (None, "quote"),
                        None => (None, "nothing"),
                    };
                    let answer_words = answer.as_deref().map(word_count).unwrap_or(0);
                    f.questions.push(QuestionHeading {
                        level: *level,
                        text: text.clone(),
                        answer,
                        answer_words,
                        followed_by,
                    });
                }
                main_text.push_str(text);
            }
            Block::Paragraph { text } | Block::Other { text } => {
                let w = word_count(text);
                f.paragraphs += 1;
                f.longest_paragraph = f.longest_paragraph.max(w);
                if w > limits.long_paragraph_words {
                    f.long_paragraphs += 1;
                }
                main_text.push_str(text);
            }
            Block::List { text, .. } => {
                f.lists += 1;
                main_text.push_str(text);
            }
            Block::Table => f.tables += 1,
            Block::Quote { text } => {
                f.quotations += 1;
                main_text.push_str(text);
            }
            Block::Media => {}
        }
        main_text.push(' ');
    }
    f.main_words = word_count(&main_text);
    f.figures = FIGURE.find_iter(&main_text).count() as u32;
    f.quotations += QUOTE.find_iter(&main_text).count() as u32;
    f.blocks = blocks;

    // Links out of the main content, Q&A, snippet markers, bylines, dates.
    let mut seen_hosts = std::collections::BTreeSet::new();
    let in_chrome = |n: NodeRef<'_, Node>| {
        n.ancestors()
            .take_while(|a| a.id() != root.id())
            .any(|a| a.value().as_element().is_some_and(is_chrome))
    };
    for n in root.descendants() {
        let Some(el) = n.value().as_element() else {
            continue;
        };
        match el.name() {
            "a" if !in_chrome(n) => {
                if let Some(u) = el.attr("href").and_then(|h| page_url.join(h).ok())
                    && matches!(u.scheme(), "http" | "https")
                    && !same_site(&u, page_url)
                    && !el
                        .attr("rel")
                        .is_some_and(|r| r.split_whitespace().any(|v| v.eq_ignore_ascii_case("sponsored")))
                {
                    // One citation per cited site.
                    if seen_hosts.insert(u.host_str().unwrap_or("").to_string()) {
                        f.citations += 1;
                    }
                }
            }
            "q" => f.quotations += 1,
            "summary" | "dt" => f.visible_qa += 1,
            _ => {}
        }
    }
    for n in doc.tree.root().descendants() {
        let Some(el) = n.value().as_element() else {
            continue;
        };
        if el.attr("data-nosnippet").is_some() {
            f.data_nosnippet += 1;
        }
        if el.attr("data-snippet").is_some() {
            f.data_snippet += 1;
        }
        let class_id = format!(
            "{} {}",
            el.attr("class").unwrap_or(""),
            el.attr("id").unwrap_or("")
        )
        .to_ascii_lowercase();
        let is_author = el
            .attr("rel")
            .is_some_and(|r| r.split_whitespace().any(|v| v == "author"))
            || el.attr("itemprop") == Some("author")
            || class_id.contains("author")
            || class_id.contains("byline");
        if is_author && f.byline.is_none() {
            let t = text_of(n);
            let t = t.trim_start_matches(|c: char| !c.is_alphanumeric()).to_string();
            if !t.is_empty() && t.chars().count() <= 120 {
                f.byline = Some(t);
                if el.name() == "a" {
                    f.author_link = el
                        .attr("href")
                        .and_then(|h| page_url.join(h).ok())
                        .map(String::from);
                } else if let Some(a) = n
                    .descendants()
                    .find_map(|d| d.value().as_element().filter(|e| e.name() == "a"))
                {
                    f.author_link = a
                        .attr("href")
                        .and_then(|h| page_url.join(h).ok())
                        .map(String::from);
                }
            }
        }
        if el.name() == "time" {
            // The label is the text just before the element ("Updated <time>").
            let label: String = n
                .prev_siblings()
                .take(2)
                .filter_map(|p| match p.value() {
                    Node::Text(t) => Some(t.to_string()),
                    Node::Element(e)
                        if matches!(e.name(), "span" | "strong" | "b" | "em" | "i" | "small") =>
                    {
                        Some(text_of(p))
                    }
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join(" ");
            if let Some(d) = el
                .attr("datetime")
                .and_then(|v| parse_visible_date(v, None))
                .or_else(|| parse_visible_date(&text_of(n), lang))
            {
                f.dates.push(DateFound {
                    date: d,
                    source: "time",
                    modified: is_updated_label(&label, lang) || el.attr("itemprop") == Some("dateModified"),
                });
            }
        } else if (class_id.contains("date")
            || class_id.contains("posted")
            || class_id.contains("updated")
            || class_id.contains("byline")
            || class_id.contains("meta"))
            && !n
                .descendants()
                .any(|d| d.value().as_element().is_some_and(|e| e.name() == "time"))
        {
            let t = text_of(n);
            if t.chars().count() <= 160
                && let Some(d) = parse_visible_date(&t, lang)
            {
                f.dates.push(DateFound {
                    date: d,
                    source: "visible",
                    modified: is_updated_label(&t, lang) || class_id.contains("updated"),
                });
            }
        }
    }
    f.dates.dedup();

    // Visible text and raw-HTML JavaScript evidence.
    let body = doc
        .tree
        .root()
        .descendants()
        .find(|n| n.value().as_element().is_some_and(|e| e.name() == "body"))
        .unwrap_or(doc.tree.root());
    f.visible_text = text_of(body).to_lowercase();
    f.js = js_evidence(doc, body, html_bytes, word_count(&f.visible_text));
    f
}

fn js_evidence(doc: &Html, body: NodeRef<'_, Node>, html_bytes: usize, body_words: u32) -> JsEvidence {
    let mut e = JsEvidence {
        html_bytes,
        body_words,
        ..Default::default()
    };
    for n in doc.tree.root().descendants() {
        let Some(el) = n.value().as_element() else {
            continue;
        };
        if el.name() == "script" {
            if el.attr("src").is_some() {
                e.external_scripts += 1;
            } else {
                let t: usize = n
                    .children()
                    .filter_map(|c| c.value().as_text().map(|t| t.len()))
                    .sum();
                e.script_bytes += t;
                if el.attr("id") == Some("__NEXT_DATA__") {
                    e.framework_markers.push("Next.js data");
                }
                if n.children()
                    .any(|c| c.value().as_text().is_some_and(|t| t.contains("window.__NUXT__")))
                {
                    e.framework_markers.push("Nuxt state");
                }
            }
        }
        if el.attr("ng-version").is_some() {
            e.framework_markers.push("Angular");
        }
        if el.attr("data-reactroot").is_some() {
            e.framework_markers.push("React root");
        }
    }
    for n in body.descendants() {
        let Some(el) = n.value().as_element() else {
            continue;
        };
        let id = el.attr("id").unwrap_or("");
        let container = matches!(id, "root" | "app" | "__next" | "__nuxt" | "svelte" | "main-app")
            || el.name() == "app-root"
            || el.attr("data-reactroot").is_some();
        if container && word_count(&text_of(n)) < 5 {
            e.empty_container = Some(if id.is_empty() {
                el.name().to_string()
            } else {
                format!("#{id}")
            });
            break;
        }
    }
    e.framework_markers.sort_unstable();
    e.framework_markers.dedup();
    e
}

#[cfg(test)]
mod tests {
    use super::*;

    fn facts(html: &str, lang: Option<&str>) -> ContentFacts {
        let doc = Html::parse_document(html);
        read(
            &doc,
            &Url::parse("https://shop.test/articles/x").unwrap(),
            html.len(),
            lang,
            ContentLimits::default(),
        )
    }

    #[test]
    fn question_with_direct_answer() {
        // US3.1
        let f = facts(
            "<main><h1>Shipping</h1><h2>How long does shipping take?</h2><p>Shipping takes 3 to 5 working days in the EU. Express is faster.</p></main>",
            Some("en"),
        );
        assert_eq!(f.questions.len(), 1);
        let q = &f.questions[0];
        assert_eq!(q.followed_by, "paragraph");
        assert_eq!(
            q.answer.as_deref(),
            Some("Shipping takes 3 to 5 working days in the EU.")
        );
        assert_eq!(q.answer_words, 10);
    }

    #[test]
    fn question_without_answer() {
        // US3.2
        let f = facts(
            "<main><h2>What sizes are there?</h2><img src=a.png><p>Text</p><h2>Why us?</h2><h2>Next</h2></main>",
            Some("en"),
        );
        assert_eq!(
            f.questions.iter().map(|q| q.followed_by).collect::<Vec<_>>(),
            ["media", "heading"]
        );
    }

    #[test]
    fn question_words_by_language() {
        assert!(is_question("How to choose a bike", Some("en")));
        assert!(
            !is_question("Is it here", Some("en")),
            "auxiliaries need a question mark"
        );
        assert!(is_question("Is it here?", None));
        assert!(is_question("איך בוחרים אופניים", Some("he")));
        assert!(is_question("¿Cuánto cuesta", Some("es")));
        assert!(is_question("Wie lange dauert der Versand", Some("de-AT")));
        assert!(!is_question("Shipping times", Some("en")));
        assert!(!is_question("Whatever happens", Some("en")));
    }

    #[test]
    fn structure_facts_and_sources() {
        // US3.3, US3.4
        let long = "word ".repeat(200);
        let f = facts(
            &format!(
                "<body><nav><a href='https://ads.test/'>Ad</a></nav><article><h1>T</h1><p>{long}</p><p>Sales grew 35% in 2025 to $4,200,000, says the report.</p>\
                 <p>As Ada said, “the best way to predict the future is to invent it, really”.</p>\
                 <ul><li>a</li><li>b</li></ul><table><tr><td>1</td></tr></table>\
                 <p>See <a href='https://stats.test/a'>stats</a> and <a href='https://stats.test/b'>more</a> and <a href='/local'>ours</a>.</p></article></body>"
            ),
            Some("en"),
        );
        assert_eq!(f.main_source, "article");
        assert_eq!(f.longest_paragraph, 200);
        assert_eq!(f.long_paragraphs, 1);
        assert_eq!((f.lists, f.tables), (1, 1));
        assert!(f.figures >= 3, "{}", f.figures);
        assert_eq!(f.quotations, 1);
        assert_eq!(
            f.citations, 1,
            "one cited site; nav links and own links do not count"
        );
    }

    #[test]
    fn dates_bylines_and_snippet_markers() {
        // US4.1
        let f = facts(
            "<body><article><h1>T</h1><p class='byline'>By <a href='/authors/ada'>Ada Lovelace</a></p>\
             <p class='post-meta'>Updated 1 September 2026</p><time datetime='2026-08-01'>1 Aug</time>\
             <p data-nosnippet>secret</p><p>Body text here.</p></article></body>",
            Some("en"),
        );
        assert_eq!(f.byline.as_deref(), Some("By Ada Lovelace"));
        assert_eq!(f.author_link.as_deref(), Some("https://shop.test/authors/ada"));
        assert!(f.dates.contains(&DateFound {
            date: "2026-09-01".into(),
            source: "visible",
            modified: true
        }));
        assert!(f.dates.contains(&DateFound {
            date: "2026-08-01".into(),
            source: "time",
            modified: false
        }));
        assert_eq!(f.data_nosnippet, 1);
    }

    #[test]
    fn visible_dates_in_several_languages() {
        assert_eq!(
            parse_visible_date("Publié le 3 mars 2026", Some("fr")).as_deref(),
            Some("2026-03-03")
        );
        assert_eq!(
            parse_visible_date("עודכן 1 בספטמבר 2026", Some("he")).as_deref(),
            Some("2026-09-01")
        );
        assert_eq!(
            parse_visible_date("1 de septiembre de 2026", Some("es")).as_deref(),
            Some("2026-09-01")
        );
        assert_eq!(
            parse_visible_date("September 1, 2026", Some("en")).as_deref(),
            Some("2026-09-01")
        );
        assert_eq!(
            parse_visible_date("12. Oktober 2026", Some("de")).as_deref(),
            Some("2026-10-12")
        );
        assert_eq!(parse_visible_date("31 February 2026", Some("en")), None);
        assert_eq!(parse_visible_date("no date here", Some("en")), None);
    }

    #[test]
    fn javascript_shell_is_suspected() {
        // US2.3
        let shell = format!(
            "<html><head><script src=a.js></script><script src=b.js></script><script src=c.js></script></head><body><div id='root'></div><script>{}</script></body></html>",
            "x".repeat(5000)
        );
        let f = facts(&shell, None);
        assert!(f.js.suspected());
        assert_eq!(f.js.empty_container.as_deref(), Some("#root"));
        let server = facts(
            &format!("<body><main><p>{}</p></main></body>", "word ".repeat(300)),
            None,
        );
        assert!(!server.js.suspected());
    }

    #[test]
    fn faq_text_is_visible() {
        let f = facts(
            "<main><details><summary>Can I return shoes?</summary><p>Yes, within 30 days.</p></details></main>",
            Some("en"),
        );
        assert_eq!(f.visible_qa, 1);
        assert!(f.visible_text.contains("can i return shoes?"));
    }
}
