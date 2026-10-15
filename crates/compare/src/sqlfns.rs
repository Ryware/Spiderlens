//! SQL functions the comparison registers on its connections.

use rusqlite::Connection;
use rusqlite::functions::FunctionFlags;
use serde_json::Value;
use spiderlens_config::MapRule;

use crate::{Error, Result};

const FLAGS: FunctionFlags = FunctionFlags::SQLITE_UTF8.union(FunctionFlags::SQLITE_DETERMINISTIC);

/// Mapping rules compiled once (plan D7). The first rule that matches a URL
/// rewrites it; later rules are not tried.
#[derive(Clone)]
pub(crate) struct Mapping {
    rules: Vec<Compiled>,
}

#[derive(Clone)]
enum Compiled {
    Text { find: String, replace: String },
    Regex { re: regex::Regex, replace: String },
}

impl Mapping {
    pub fn new(rules: &[MapRule]) -> Result<Self> {
        let rules = rules
            .iter()
            .enumerate()
            .map(|(i, r)| {
                if r.find.is_empty() {
                    return Err(Error::invalid(format!(
                        "mapping rule {}: the text to find is empty",
                        i + 1
                    )));
                }
                Ok(if r.regex {
                    Compiled::Regex {
                        re: regex::Regex::new(&r.find)
                            .map_err(|e| Error::invalid(format!("mapping rule {}: {e}", i + 1)))?,
                        replace: r.replace.clone(),
                    }
                } else {
                    Compiled::Text {
                        find: r.find.clone(),
                        replace: r.replace.clone(),
                    }
                })
            })
            .collect::<Result<_>>()?;
        Ok(Self { rules })
    }

    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }

    pub fn apply(&self, url: &str) -> String {
        for r in &self.rules {
            match r {
                Compiled::Text { find, replace } if url.contains(find.as_str()) => {
                    return url.replacen(find.as_str(), replace, 1);
                }
                Compiled::Regex { re, replace } if re.is_match(url) => {
                    return re.replace(url, replace.as_str()).into_owned();
                }
                _ => {}
            }
        }
        url.to_string()
    }
}

/// Structured data types of one stored block, as a JSON array: `@type` of
/// JSON-LD (top level, arrays and `@graph`), the last path segment of
/// microdata `itemtype` URLs.
pub(crate) fn sd_types(kind: &str, raw: &str) -> String {
    let mut out = Vec::new();
    if let Ok(v) = serde_json::from_str::<Value>(raw) {
        match kind {
            "microdata" => collect_microdata(&v, &mut out),
            _ => collect_json_ld(&v, &mut out),
        }
    }
    out.sort();
    out.dedup();
    serde_json::to_string(&out).expect("types serialise")
}

fn collect_json_ld(v: &Value, out: &mut Vec<String>) {
    match v {
        Value::Array(items) => items.iter().for_each(|i| collect_json_ld(i, out)),
        Value::Object(m) => {
            match m.get("@type") {
                Some(Value::String(s)) => out.push(short_type(s)),
                Some(Value::Array(a)) => out.extend(a.iter().filter_map(Value::as_str).map(short_type)),
                _ => {}
            }
            if let Some(g) = m.get("@graph") {
                collect_json_ld(g, out);
            }
        }
        _ => {}
    }
}

fn collect_microdata(v: &Value, out: &mut Vec<String>) {
    if let Some(Value::Array(types)) = v.get("type") {
        out.extend(types.iter().filter_map(Value::as_str).map(short_type));
    }
}

fn short_type(s: &str) -> String {
    s.trim_end_matches('/')
        .rsplit(['/', '#'])
        .next()
        .unwrap_or(s)
        .to_string()
}

pub(crate) fn hash_text(s: &str) -> i64 {
    xxhash_rust::xxh3::xxh3_64(s.as_bytes()) as i64
}

/// Registers what both kinds of connection need.
pub(crate) fn register_common(conn: &Connection) -> rusqlite::Result<()> {
    conn.create_scalar_function("sd_types", 2, FLAGS, |ctx| {
        let kind: String = ctx.get(0)?;
        let raw: String = ctx.get(1)?;
        Ok(sd_types(&kind, &raw))
    })?;
    conn.create_scalar_function("sd_hash", 1, FLAGS, |ctx| {
        let raw: Option<String> = ctx.get(0)?;
        Ok(raw.map(|r| hash_text(&r)))
    })?;
    conn.create_scalar_function("cmp_norm", 1, FLAGS, |ctx| {
        let s: Option<String> = ctx.get(0)?;
        Ok(s.map(|s| spiderlens_results::norm_ws(&s)))
    })?;
    conn.create_scalar_function("cmp_contains", 2, FLAGS, |ctx| {
        let hay: Option<String> = ctx.get(0)?;
        let needle: String = ctx.get(1)?;
        Ok(hay.is_some_and(|h| spiderlens_results::ci_contains(&h, &needle)))
    })?;
    Ok(())
}

/// `map_url(url)` with this comparison's rules (previous side only).
pub(crate) fn register_mapping(conn: &Connection, mapping: Mapping) -> rusqlite::Result<()> {
    conn.create_scalar_function("map_url", 1, FLAGS, move |ctx| {
        let url: String = ctx.get(0)?;
        Ok(mapping.apply(&url))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule(find: &str, replace: &str, regex: bool) -> MapRule {
        MapRule {
            find: find.into(),
            replace: replace.into(),
            regex,
        }
    }

    #[test]
    fn first_matching_rule_wins() {
        let m = Mapping::new(&[
            rule("https://old.test/products/", "https://www.test/shop/p/", false),
            rule(r"^https://old\.test/(.*)$", "https://www.test/$1", true),
        ])
        .unwrap();
        assert_eq!(
            m.apply("https://old.test/products/a"),
            "https://www.test/shop/p/a"
        );
        assert_eq!(m.apply("https://old.test/about"), "https://www.test/about");
        assert_eq!(m.apply("https://other.test/"), "https://other.test/");
        assert!(Mapping::new(&[rule("(", "", true)]).is_err());
    }

    #[test]
    fn structured_data_types() {
        let ld = r#"[{"@context":"https://schema.org","@type":"Product"},{"@graph":[{"@type":["BreadcrumbList","Thing"]}]}]"#;
        assert_eq!(sd_types("json_ld", ld), r#"["BreadcrumbList","Product","Thing"]"#);
        let md = r#"{"type":["https://schema.org/Recipe"],"properties":{}}"#;
        assert_eq!(sd_types("microdata", md), r#"["Recipe"]"#);
        assert_eq!(sd_types("json_ld", "not json"), "[]");
    }
}
