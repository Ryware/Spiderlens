//! JSON-LD paths (plan D4, research R2).
//!
//! Two forms: an item path (`offers.price`, optionally only for items of
//! one `@type`) over the items of every block, `@graph` members included;
//! or a JSONPath (RFC 9535) starting with `$`, applied to each block.

use serde_json::Value;
use serde_json_path::JsonPath;

use crate::extract::{StructuredBlock, StructuredKind};

pub(super) enum Path {
    Item {
        segments: Vec<String>,
        type_: Option<String>,
    },
    Json(JsonPath),
}

pub(super) struct Found {
    pub values: Vec<String>,
    pub bad_json: bool,
}

impl Path {
    pub(super) fn new(expr: &str, type_: Option<&str>) -> Result<Self, String> {
        let e = expr.trim();
        if e.starts_with('$') {
            return JsonPath::parse(e).map(Self::Json).map_err(|err| err.to_string());
        }
        let segments: Vec<String> = e.split('.').map(|s| s.trim().to_string()).collect();
        if segments.iter().any(String::is_empty) {
            return Err("property names separated by dots, such as offers.price".into());
        }
        Ok(Self::Item {
            segments,
            type_: type_.map(str::trim).filter(|t| !t.is_empty()).map(str::to_string),
        })
    }

    pub(super) fn find(&self, blocks: &[Option<Value>]) -> Found {
        let mut found = Found {
            values: Vec::new(),
            bad_json: false,
        };
        for b in blocks {
            let Some(json) = b else {
                found.bad_json = true;
                continue;
            };
            match self {
                Self::Json(p) => {
                    for v in p.query(json).all() {
                        push_value(v, &mut found.values);
                    }
                }
                Self::Item { segments, type_ } => {
                    let mut items = Vec::new();
                    collect_items(json, &mut items);
                    for item in items {
                        if type_.as_deref().is_some_and(|t| !has_type(item, t)) {
                            continue;
                        }
                        let mut current = vec![item];
                        for seg in segments {
                            current = current.into_iter().flat_map(|v| step(v, seg)).collect();
                        }
                        for v in current {
                            push_value(v, &mut found.values);
                        }
                    }
                }
            }
        }
        found
    }
}

/// The page's JSON-LD blocks, parsed once for every rule; `None` for a
/// block that is not valid JSON.
pub(super) fn parse(blocks: &[StructuredBlock]) -> Vec<Option<Value>> {
    blocks
        .iter()
        .filter(|b| b.kind == StructuredKind::JsonLd)
        .map(|b| serde_json::from_str(&b.raw).ok())
        .collect()
}

/// Top-level objects, array members and `@graph` members, in order.
fn collect_items<'a>(v: &'a Value, out: &mut Vec<&'a Value>) {
    match v {
        Value::Array(a) => a.iter().for_each(|x| collect_items(x, out)),
        Value::Object(o) => {
            out.push(v);
            if let Some(g) = o.get("@graph") {
                collect_items(g, out);
            }
        }
        _ => {}
    }
}

fn has_type(item: &Value, t: &str) -> bool {
    let matches = |s: &str| s == t || s.rsplit(['/', '#', ':']).next() == Some(t);
    match item.get("@type") {
        Some(Value::String(s)) => matches(s),
        Some(Value::Array(a)) => a.iter().any(|x| x.as_str().is_some_and(matches)),
        _ => false,
    }
}

/// One path step; arrays are walked through, as in `offers.price` when
/// there are several offers.
fn step<'a>(v: &'a Value, seg: &str) -> Vec<&'a Value> {
    match v {
        Value::Object(o) => o.get(seg).into_iter().flat_map(flatten).collect(),
        Value::Array(a) => a.iter().flat_map(|x| step(x, seg)).collect(),
        _ => Vec::new(),
    }
}

fn flatten(v: &Value) -> Vec<&Value> {
    match v {
        Value::Array(a) => a.iter().flat_map(flatten).collect(),
        other => vec![other],
    }
}

fn push_value(v: &Value, out: &mut Vec<String>) {
    match v {
        Value::Null => {}
        Value::String(s) => out.push(s.clone()),
        Value::Number(n) => out.push(n.to_string()),
        Value::Bool(b) => out.push(b.to_string()),
        other => out.push(other.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn block(raw: &str) -> StructuredBlock {
        StructuredBlock {
            kind: StructuredKind::JsonLd,
            raw: raw.into(),
            parseable: serde_json::from_str::<Value>(raw).is_ok(),
        }
    }

    fn find(expr: &str, t: Option<&str>, raw: &[&str]) -> (Vec<String>, bool) {
        let blocks: Vec<_> = raw.iter().map(|r| block(r)).collect();
        let f = Path::new(expr, t).unwrap().find(&parse(&blocks));
        (f.values, f.bad_json)
    }

    const PRODUCT: &str = r#"{"@context":"https://schema.org","@type":"Product","name":"Shoe",
        "offers":{"@type":"Offer","price":49,"priceCurrency":"USD"},
        "aggregateRating":{"ratingValue":4.5,"reviewCount":12}}"#;

    #[test]
    fn item_path_with_type() {
        assert_eq!(find("offers.price", Some("Product"), &[PRODUCT]).0, vec!["49"]);
        assert_eq!(
            find("aggregateRating.reviewCount", Some("Product"), &[PRODUCT]).0,
            vec!["12"]
        );
        assert!(find("offers.price", Some("Article"), &[PRODUCT]).0.is_empty());
        let full = r#"{"@type":"https://schema.org/Product","name":"Shoe"}"#;
        assert_eq!(find("name", Some("Product"), &[full]).0, vec!["Shoe"]);
    }

    #[test]
    fn graph_items_are_found() {
        let g = format!(r#"{{"@context":"https://schema.org","@graph":[{{"@type":"WebSite"}},{PRODUCT}]}}"#);
        assert_eq!(find("offers.price", Some("Product"), &[&g]).0, vec!["49"]);
    }

    #[test]
    fn arrays_are_walked() {
        let raw = r#"{"@type":["Product","Thing"],"offers":[{"price":"10"},{"price":"12"}]}"#;
        assert_eq!(find("offers.price", Some("Product"), &[raw]).0, vec!["10", "12"]);
    }

    #[test]
    fn jsonpath_form() {
        assert_eq!(find("$..reviewCount", None, &[PRODUCT]).0, vec!["12"]);
        assert_eq!(find("$.offers", None, &[PRODUCT]).0.len(), 1);
    }

    #[test]
    fn bad_blocks_are_flagged_and_skipped() {
        let (v, bad) = find("offers.price", None, &["{oops", PRODUCT]);
        assert_eq!(v, vec!["49"]);
        assert!(bad);
    }
}
