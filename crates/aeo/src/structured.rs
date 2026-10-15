//! Structured data: reading JSON-LD items, validating them against bundled
//! type rules, and pulling out the facts other checks use (spec 015 FR-026,
//! FR-028, FR-029, FR-031, FR-033; plan D8).
//!
//! The type rules (`data/type_rules.json`) ship with the app so validation
//! works offline. Unknown-type checks against the full Schema.org vocabulary
//! are a later step (tasks.md T019).

use std::collections::{BTreeMap, HashMap};
use std::sync::LazyLock;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;
use url::Url;

#[derive(Deserialize)]
struct TypeRule {
    #[serde(default)]
    parent: Option<String>,
    #[serde(default)]
    required: Vec<String>,
    #[serde(default)]
    one_of: Vec<Vec<String>>,
    #[serde(default)]
    recommended: Vec<String>,
}

#[derive(Deserialize)]
struct Rules {
    version: String,
    types: HashMap<String, TypeRule>,
    kinds: HashMap<String, Vec<String>>,
}

static RULES: LazyLock<Rules> =
    LazyLock::new(|| serde_json::from_str(include_str!("../data/type_rules.json")).expect("type_rules.json"));

/// The version of the bundled type rules, stored with results.
pub fn rules_version() -> &'static str {
    &RULES.version
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum Severity {
    Error,
    Warning,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Message {
    pub severity: Severity,
    /// Block number on the page (document order) and JSON pointer.
    pub block: u32,
    pub pointer: String,
    pub message: String,
}

/// One structured item, flattened out of `@graph` and nesting.
#[derive(Debug, Clone, PartialEq)]
pub struct Item {
    pub block: u32,
    pub pointer: String,
    pub types: Vec<String>,
    pub value: Value,
}

impl Item {
    pub fn is(&self, ty: &str) -> bool {
        self.types.iter().any(|t| t == ty || is_subtype(t, ty))
    }

    pub fn str(&self, prop: &str) -> Option<&str> {
        text(self.value.get(prop)?)
    }
}

fn is_subtype(t: &str, ancestor: &str) -> bool {
    let mut cur = RULES.types.get(t).and_then(|r| r.parent.as_deref());
    while let Some(p) = cur {
        if p == ancestor {
            return true;
        }
        cur = RULES.types.get(p).and_then(|r| r.parent.as_deref());
    }
    false
}

/// A value as text: a string, or an object's `name` or `@id`, or the first
/// element of an array.
fn text(v: &Value) -> Option<&str> {
    match v {
        Value::String(s) => Some(s.as_str()),
        Value::Array(a) => a.first().and_then(text),
        Value::Object(o) => o.get("name").or_else(|| o.get("@id")).and_then(text),
        _ => None,
    }
}

fn types_of(v: &Value) -> Vec<String> {
    match v.get("@type") {
        Some(Value::String(s)) => vec![strip_ns(s)],
        Some(Value::Array(a)) => a.iter().filter_map(Value::as_str).map(strip_ns).collect(),
        _ => vec![],
    }
}

fn strip_ns(t: &str) -> String {
    t.trim_start_matches("https://schema.org/")
        .trim_start_matches("http://schema.org/")
        .trim_start_matches("schema:")
        .to_string()
}

/// Parses stored JSON-LD blocks (raw text, document order) into items.
/// Unparseable blocks become error messages.
pub fn read_json_ld(blocks: &[&str]) -> (Vec<Item>, Vec<Message>) {
    let mut items = Vec::new();
    let mut messages = Vec::new();
    for (i, raw) in blocks.iter().enumerate() {
        let block = u32::try_from(i).unwrap_or(u32::MAX);
        match serde_json::from_str::<Value>(raw) {
            Ok(v) => collect(&v, block, String::new(), &mut items),
            Err(e) => messages.push(Message {
                severity: Severity::Error,
                block,
                pointer: String::new(),
                message: format!("Cannot be parsed as JSON: {e}"),
            }),
        }
    }
    (items, messages)
}

fn collect(v: &Value, block: u32, pointer: String, out: &mut Vec<Item>) {
    match v {
        Value::Array(a) => {
            for (i, x) in a.iter().enumerate() {
                collect(x, block, format!("{pointer}/{i}"), out);
            }
        }
        Value::Object(o) => {
            let types = types_of(v);
            if !types.is_empty() {
                out.push(Item {
                    block,
                    pointer: pointer.clone(),
                    types,
                    value: v.clone(),
                });
            }
            for (k, x) in o {
                if k == "@context" {
                    continue;
                }
                collect(
                    x,
                    block,
                    format!("{pointer}/{}", k.replace('~', "~0").replace('/', "~1")),
                    out,
                );
            }
        }
        _ => {}
    }
}

/// Validates items against the bundled type rules (FR-031).
pub fn validate(items: &[Item]) -> Vec<Message> {
    let mut out = Vec::new();
    let date_props = &RULES.kinds["date"];
    let url_props = &RULES.kinds["url"];
    for item in items {
        for ty in &item.types {
            let mut chain = vec![ty.as_str()];
            while let Some(p) = RULES
                .types
                .get(*chain.last().unwrap())
                .and_then(|r| r.parent.as_deref())
            {
                chain.push(p);
            }
            for t in chain.iter().filter_map(|t| RULES.types.get(*t)) {
                for prop in &t.required {
                    if item.value.get(prop).is_none_or(Value::is_null) {
                        out.push(msg(
                            Severity::Error,
                            item,
                            prop,
                            format!("{ty} is missing the required property {prop}"),
                        ));
                    }
                }
                for group in &t.one_of {
                    if !group.iter().any(|p| item.value.get(p).is_some()) {
                        out.push(msg(
                            Severity::Error,
                            item,
                            "",
                            format!("{ty} needs one of {}", group.join(" or ")),
                        ));
                    }
                }
                for prop in &t.recommended {
                    if item.value.get(prop).is_none() {
                        out.push(msg(
                            Severity::Warning,
                            item,
                            prop,
                            format!("{ty} lacks the recommended property {prop}"),
                        ));
                    }
                }
            }
        }
        let Some(obj) = item.value.as_object() else {
            continue;
        };
        for (k, v) in obj {
            if date_props.contains(k)
                && let Some(s) = v.as_str()
                && parse_date(s).is_none()
            {
                out.push(msg(
                    Severity::Warning,
                    item,
                    k,
                    format!("{k} {s:?} is not a date (expected YYYY-MM-DD or ISO 8601)"),
                ));
            }

            if url_props.contains(k) {
                let strings: Vec<&str> = match v {
                    Value::String(s) => vec![s.as_str()],
                    Value::Array(a) => a.iter().filter_map(Value::as_str).collect(),
                    _ => vec![],
                };
                for s in strings {
                    if Url::parse(s).is_err() && !s.starts_with('/') {
                        out.push(msg(
                            Severity::Warning,
                            item,
                            k,
                            format!("{k} {s:?} is not an absolute URL"),
                        ));
                    }
                }
            }
        }
    }
    out.sort_by(|a, b| {
        (a.block, &a.pointer, a.severity, &a.message).cmp(&(b.block, &b.pointer, b.severity, &b.message))
    });
    out.dedup();
    out
}

fn msg(severity: Severity, item: &Item, prop: &str, message: String) -> Message {
    let pointer = if prop.is_empty() {
        item.pointer.clone()
    } else {
        format!("{}/{prop}", item.pointer)
    };
    Message {
        severity,
        block: item.block,
        pointer,
        message,
    }
}

/// A date or date-time in ISO 8601 form, as `YYYY-MM-DD`.
pub fn parse_date(s: &str) -> Option<String> {
    let s = s.trim();
    let day = s.get(..10)?;
    let d: jiff::civil::Date = day.parse().ok()?;
    let rest = &s[10..];
    if !(rest.is_empty() || rest.starts_with('T') || rest.starts_with(' ')) {
        return None;
    }
    Some(d.to_string())
}

/// Facts from structured data that other checks use.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StructuredFacts {
    pub types: Vec<String>,
    pub published: Option<String>,
    pub modified: Option<String>,
    pub headline: Option<String>,
    pub author: Option<String>,
    pub author_url: Option<String>,
    /// (question, answer) pairs from FAQPage and QAPage.
    pub qa: Vec<(String, String)>,
    pub org: Option<Entity>,
    pub website_name: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Entity {
    pub kind: String,
    pub name: Option<String>,
    pub url: Option<String>,
    pub logo: Option<String>,
    pub same_as: Vec<String>,
    pub contact: bool,
}

const ARTICLES: &[&str] = &[
    "Article",
    "NewsArticle",
    "BlogPosting",
    "TechArticle",
    "Report",
    "ScholarlyArticle",
];

pub fn facts(items: &[Item]) -> StructuredFacts {
    let mut f = StructuredFacts::default();
    let mut types: Vec<String> = items.iter().flat_map(|i| i.types.iter().cloned()).collect();
    types.sort();
    types.dedup();
    f.types = types;
    if let Some(a) = items.iter().find(|i| {
        ARTICLES.iter().any(|t| i.is(t)) || i.is("WebPage") && i.value.get("datePublished").is_some()
    }) {
        f.published = a.str("datePublished").and_then(parse_date);
        f.modified = a.str("dateModified").and_then(parse_date);
        f.headline = a.str("headline").or_else(|| a.str("name")).map(str::to_string);
        if let Some(author) = a.value.get("author") {
            let first = match author {
                Value::Array(v) => v.first(),
                other => Some(other),
            };
            f.author = first.and_then(text).map(str::to_string);
            f.author_url = first
                .and_then(|v| v.get("url"))
                .and_then(text)
                .map(str::to_string);
        }
    }
    for page in items.iter().filter(|i| i.is("FAQPage") || i.is("QAPage")) {
        let entities = match page.value.get("mainEntity") {
            Some(Value::Array(a)) => a.iter().collect::<Vec<_>>(),
            Some(v) => vec![v],
            None => vec![],
        };
        for q in entities {
            let name = q.get("name").and_then(Value::as_str).unwrap_or("").to_string();
            let answer = q
                .get("acceptedAnswer")
                .or_else(|| q.get("suggestedAnswer"))
                .map(|a| match a {
                    Value::Array(v) => v.first().cloned().unwrap_or(Value::Null),
                    v => v.clone(),
                })
                .and_then(|a| a.get("text").and_then(Value::as_str).map(str::to_string))
                .unwrap_or_default();
            if !name.is_empty() {
                f.qa.push((name, answer));
            }
        }
    }
    if let Some(o) = items
        .iter()
        .find(|i| i.is("Organization") || i.is("LocalBusiness"))
        .or_else(|| items.iter().find(|i| i.is("Person") && i.pointer.is_empty()))
    {
        let same_as = match o.value.get("sameAs") {
            Some(Value::String(s)) => vec![s.clone()],
            Some(Value::Array(a)) => a.iter().filter_map(Value::as_str).map(str::to_string).collect(),
            _ => vec![],
        };
        f.org = Some(Entity {
            kind: o.types.first().cloned().unwrap_or_default(),
            name: o.str("name").map(str::to_string),
            url: o.str("url").map(str::to_string),
            logo: o
                .value
                .get("logo")
                .and_then(|l| text(l).or_else(|| l.get("url").and_then(text)))
                .map(str::to_string),
            same_as,
            contact: o.value.get("contactPoint").is_some()
                || o.value.get("telephone").is_some()
                || o.value.get("email").is_some(),
        });
    }
    f.website_name = items
        .iter()
        .find(|i| i.is("WebSite"))
        .and_then(|i| i.str("name"))
        .map(str::to_string);
    f
}

/// Item counts per type, for coverage (FR-032).
pub fn type_counts(items: &[Item]) -> BTreeMap<String, u32> {
    let mut m = BTreeMap::new();
    for i in items {
        for t in &i.types {
            *m.entry(t.clone()).or_default() += 1;
        }
    }
    m
}

/// Normalises text for "is it on the page" comparisons.
pub fn normalise(s: &str) -> String {
    s.to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { ' ' })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    const ARTICLE: &str = r#"{"@context":"https://schema.org","@graph":[
        {"@type":"NewsArticle","headline":"Red shoes","datePublished":"2026-08-01","dateModified":"2026-09-01T10:00:00+02:00",
         "author":{"@type":"Person","name":"Ada","url":"https://a.test/authors/ada"},"image":"https://a.test/i.png"},
        {"@type":"Organization","name":"Acme","logo":{"@type":"ImageObject","url":"https://a.test/l.png"},"sameAs":["https://x.test/acme","acme"]},
        {"@type":"WebSite","name":"Acme shop","url":"https://a.test/"}]}"#;

    #[test]
    fn reads_graph_items_and_facts() {
        let (items, errs) = read_json_ld(&[ARTICLE]);
        assert!(errs.is_empty());
        let f = facts(&items);
        assert_eq!(f.published.as_deref(), Some("2026-08-01"));
        assert_eq!(f.modified.as_deref(), Some("2026-09-01"));
        assert_eq!(f.headline.as_deref(), Some("Red shoes"));
        assert_eq!(f.author.as_deref(), Some("Ada"));
        assert_eq!(f.author_url.as_deref(), Some("https://a.test/authors/ada"));
        let org = f.org.unwrap();
        assert_eq!(org.name.as_deref(), Some("Acme"));
        assert_eq!(org.logo.as_deref(), Some("https://a.test/l.png"));
        assert_eq!(f.website_name.as_deref(), Some("Acme shop"));
    }

    #[test]
    fn validation_messages() {
        let (items, _) = read_json_ld(&[ARTICLE]);
        let m = validate(&items);
        let errors: Vec<_> = m.iter().filter(|x| x.severity == Severity::Error).collect();
        assert!(errors.is_empty(), "{errors:?}");
        // "acme" in sameAs is not a URL.
        assert!(
            m.iter()
                .any(|x| x.pointer == "/@graph/1/sameAs" && x.message.contains("absolute URL"))
        );

        let (items, parse) = read_json_ld(&[
            r#"{"@type":"Product","offers":{"@type":"Offer","price":"5"},"releaseDate":"x"}"#,
            "{oops",
        ]);
        assert_eq!(parse.len(), 1);
        let m = validate(&items);
        assert!(
            m.iter().any(|x| x.severity == Severity::Error
                && x.message == "Product is missing the required property name")
        );
        assert!(m.iter().any(|x| x.pointer == "/offers/priceCurrency"));
    }

    #[test]
    fn faq_pairs_and_dates() {
        let (items, _) = read_json_ld(&[
            r#"{"@type":"FAQPage","mainEntity":[{"@type":"Question","name":"Can I return shoes?","acceptedAnswer":{"@type":"Answer","text":"Yes, within 30 days."}}]}"#,
        ]);
        assert_eq!(
            facts(&items).qa,
            [(
                "Can I return shoes?".to_string(),
                "Yes, within 30 days.".to_string()
            )]
        );
        assert_eq!(parse_date("2026-02-30"), None);
        assert_eq!(parse_date("2026-02-03T00:00Z").as_deref(), Some("2026-02-03"));
        assert_eq!(normalise("Can I  return shoes?"), "can i return shoes");
    }
}
