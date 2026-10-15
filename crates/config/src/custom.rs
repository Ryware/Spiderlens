//! Custom extraction and custom search rules (spec 013, data-model.md).
//!
//! The rules are plain configuration: they live in profiles and in every
//! crawl file. Syntax is checked here with the same parsers the engine
//! uses, so the settings sheet, `config validate` and crawl start report
//! the same errors (FR-008).

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// At most this many extractors and this many searches (FR-009).
pub const MAX_RULES: usize = 100;

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export)]
pub struct CustomRules {
    pub extractors: Vec<Extractor>,
    pub searches: Vec<Search>,
}

impl CustomRules {
    pub fn is_empty(&self) -> bool {
        self.extractors.is_empty() && self.searches.is_empty()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ExtractorKind {
    #[default]
    Css,
    Xpath,
    Regex,
    JsonLd,
}

/// What a CSS or XPath extractor returns for each matched node (FR-002).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ExtractMode {
    #[default]
    Text,
    InnerHtml,
    OuterHtml,
    Attribute,
    /// The expression's own result: XPath functions, regex matches, JSON-LD values.
    Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export)]
pub struct Extractor {
    pub name: String,
    pub kind: ExtractorKind,
    /// Selector, XPath, pattern, item path or JSONPath (starting with `$`).
    pub expr: String,
    pub mode: ExtractMode,
    /// Attribute name, for [`ExtractMode::Attribute`].
    #[ts(optional)]
    pub attr: Option<String>,
    /// JSON-LD item path: only items whose `@type` is this.
    #[ts(optional)]
    pub json_type: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum SearchCondition {
    #[default]
    Contains,
    NotContains,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum SearchTarget {
    #[default]
    Html,
    Text,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ElementKind {
    #[default]
    Css,
    Xpath,
}

/// Limits a search to the content of the elements an expression selects (US6).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export)]
pub struct ElementLimit {
    pub kind: ElementKind,
    pub expr: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export)]
pub struct Search {
    pub name: String,
    pub term: String,
    pub regex: bool,
    pub condition: SearchCondition,
    pub target: SearchTarget,
    pub ignore_case: bool,
    #[ts(optional)]
    pub within: Option<ElementLimit>,
}

/// The `format` of a rule file.
pub const RULE_FILE_FORMAT: &str = "spiderlens-rules";
pub const RULE_FILE_VERSION: u32 = 1;

#[derive(Serialize, Deserialize)]
struct RuleFile {
    format: String,
    version: u32,
    #[serde(default)]
    extractors: Vec<Extractor>,
    #[serde(default)]
    searches: Vec<Search>,
}

/// A rule file to share rules without a whole profile (US5.4).
pub fn export_rules(r: &CustomRules) -> String {
    serde_json::to_string_pretty(&RuleFile {
        format: RULE_FILE_FORMAT.into(),
        version: RULE_FILE_VERSION,
        extractors: r.extractors.clone(),
        searches: r.searches.clone(),
    })
    .expect("rules serialize")
}

/// Reads a rule file. A bare `{ "extractors": [...], "searches": [...] }`
/// (the `custom` section of a profile) is accepted too.
pub fn import_rules(text: &str) -> Result<CustomRules, String> {
    let v: serde_json::Value = serde_json::from_str(text).map_err(|e| format!("not a rule file: {e}"))?;
    if let Some(f) = v.get("format").and_then(|f| f.as_str()) {
        if f != RULE_FILE_FORMAT {
            return Err(format!("not a rule file (format \"{f}\")"));
        }
        let version = v.get("version").and_then(serde_json::Value::as_u64).unwrap_or(0);
        if version > u64::from(RULE_FILE_VERSION) {
            return Err(format!(
                "this rule file is from a newer SpiderLens (version {version}); update SpiderLens to read it"
            ));
        }
    }
    serde_json::from_value(v).map_err(|e| format!("not a rule file: {e}"))
}

/// Adds `more` to `base`; a rule with the same name replaces the old one.
pub fn merge_rules(base: &mut CustomRules, more: CustomRules) {
    for e in more.extractors {
        match base
            .extractors
            .iter_mut()
            .find(|x| x.name.trim().eq_ignore_ascii_case(e.name.trim()))
        {
            Some(x) => *x = e,
            None => base.extractors.push(e),
        }
    }
    for s in more.searches {
        match base
            .searches
            .iter_mut()
            .find(|x| x.name.trim().eq_ignore_ascii_case(s.name.trim()))
        {
            Some(x) => *x = s,
            None => base.searches.push(s),
        }
    }
}

/// One problem with a rule, for `Issue.path` `custom.extractors[3].expr`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleProblem {
    pub path: String,
    pub code: &'static str,
    pub message: String,
    pub warning: bool,
}

/// Every problem in a rule set (FR-008, FR-009).
pub fn check_rules(r: &CustomRules) -> Vec<RuleProblem> {
    let mut out = Vec::new();
    for (group, n) in [("extractors", r.extractors.len()), ("searches", r.searches.len())] {
        if n > MAX_RULES {
            err(
                &mut out,
                format!("custom.{group}"),
                "too_many_rules",
                format!("At most {MAX_RULES} {group} are allowed; this profile has {n}."),
            );
        }
    }
    let names = |it: &mut dyn Iterator<Item = &str>, group: &str, out: &mut Vec<RuleProblem>| {
        let mut seen = std::collections::HashSet::new();
        for (i, name) in it.enumerate() {
            let path = format!("custom.{group}[{i}].name");
            if name.trim().is_empty() {
                err(
                    out,
                    path,
                    "rule_name_empty",
                    "Give the rule a name; it becomes the column title.".into(),
                );
            } else if !seen.insert(name.trim().to_lowercase()) {
                err(
                    out,
                    path,
                    "rule_name_duplicate",
                    format!("Another rule is already named \"{}\".", name.trim()),
                );
            }
        }
    };
    names(
        &mut r.extractors.iter().map(|e| e.name.as_str()),
        "extractors",
        &mut out,
    );
    names(
        &mut r.searches.iter().map(|s| s.name.as_str()),
        "searches",
        &mut out,
    );

    for (i, e) in r.extractors.iter().enumerate() {
        let path = format!("custom.extractors[{i}].expr");
        if e.expr.trim().is_empty() {
            err(&mut out, path, "rule_expr_empty", "Enter what to extract.".into());
            continue;
        }
        let checked = match e.kind {
            ExtractorKind::Css => check_css(&e.expr),
            ExtractorKind::Xpath => check_xpath(&e.expr),
            ExtractorKind::Regex => check_regex(&e.expr, false),
            ExtractorKind::JsonLd => check_json_path(&e.expr),
        };
        if let Err(m) = checked {
            err(&mut out, path, "rule_syntax", m);
        }
        let node_kind = matches!(e.kind, ExtractorKind::Css | ExtractorKind::Xpath);
        if e.mode == ExtractMode::Attribute {
            if !node_kind {
                err(
                    &mut out,
                    format!("custom.extractors[{i}].mode"),
                    "rule_mode",
                    "Attribute mode is for CSS selector and XPath rules.".into(),
                );
            } else if e.attr.as_deref().is_none_or(|a| a.trim().is_empty()) {
                err(
                    &mut out,
                    format!("custom.extractors[{i}].attr"),
                    "rule_attr_empty",
                    "Enter the attribute to read, such as href or content.".into(),
                );
            }
        }
        if !node_kind && matches!(e.mode, ExtractMode::InnerHtml | ExtractMode::OuterHtml) {
            err(
                &mut out,
                format!("custom.extractors[{i}].mode"),
                "rule_mode",
                "Inner and outer HTML are for CSS selector and XPath rules.".into(),
            );
        }
    }
    for (i, s) in r.searches.iter().enumerate() {
        let path = format!("custom.searches[{i}].term");
        if s.term.is_empty() {
            err(
                &mut out,
                path.clone(),
                "rule_expr_empty",
                "Enter the text to search for.".into(),
            );
        } else if s.regex
            && let Err(m) = check_regex(&s.term, s.ignore_case)
        {
            err(&mut out, path.clone(), "rule_syntax", m);
        } else if s.regex && regex::Regex::new(&s.term).is_ok_and(|re| re.is_match("")) {
            out.push(RuleProblem {
                path,
                code: "rule_matches_empty",
                message: "This pattern also matches empty text, so it matches everywhere. \
                          Use + instead of * if a character is required."
                    .into(),
                warning: true,
            });
        }
        if let Some(w) = &s.within {
            let path = format!("custom.searches[{i}].within");
            let checked = if w.expr.trim().is_empty() {
                Err("Enter the element to search in, or remove the limit.".to_string())
            } else {
                match w.kind {
                    ElementKind::Css => check_css(&w.expr),
                    ElementKind::Xpath => check_xpath(&w.expr),
                }
            };
            if let Err(m) = checked {
                out.push(RuleProblem {
                    path,
                    code: "rule_syntax",
                    message: m,
                    warning: false,
                });
            }
        }
    }
    out
}

fn err(out: &mut Vec<RuleProblem>, path: String, code: &'static str, message: String) {
    out.push(RuleProblem {
        path,
        code,
        message,
        warning: false,
    });
}

/// A CSS selector as browsers accept it for static documents.
pub fn check_css(expr: &str) -> Result<(), String> {
    scraper::Selector::parse(expr)
        .map(|_| ())
        .map_err(|e| format!("Not a valid CSS selector: {}", css_error(&e)))
}

fn css_error(e: &scraper::error::SelectorErrorKind<'_>) -> String {
    let s = e.to_string();
    if s.is_empty() {
        "check the syntax".into()
    } else {
        s
    }
}

/// An XPath 1.0 expression.
pub fn check_xpath(expr: &str) -> Result<(), String> {
    match sxd_xpath::Factory::new().build(expr) {
        Ok(Some(_)) => Ok(()),
        Ok(None) => Err("Enter an XPath expression.".into()),
        Err(e) => Err(format!("Not a valid XPath 1.0 expression: {e}")),
    }
}

/// A pattern in the `regex` crate's syntax (research R3).
pub fn check_regex(expr: &str, ignore_case: bool) -> Result<(), String> {
    regex::RegexBuilder::new(expr)
        .case_insensitive(ignore_case)
        .build()
        .map(|_| ())
        .map_err(|e| match e {
            regex::Error::Syntax(s) => {
                let hint = if s.contains("look-around") || s.contains("backreference") {
                    " Look-around and backreferences are not supported."
                } else {
                    ""
                };
                format!("Not a valid regular expression:\n{s}{hint}")
            }
            other => format!("Not a valid regular expression: {other}"),
        })
}

/// A JSON-LD item path (`offers.price`) or a JSONPath (`$..price`).
pub fn check_json_path(expr: &str) -> Result<(), String> {
    let e = expr.trim();
    if e.starts_with('$') {
        serde_json_path::JsonPath::parse(e)
            .map(|_| ())
            .map_err(|err| format!("Not a valid JSONPath: {err}"))
    } else if e.split('.').any(|seg| seg.trim().is_empty()) {
        Err("Write the path as property names separated by dots, such as offers.price.".into())
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ex(kind: ExtractorKind, expr: &str) -> Extractor {
        Extractor {
            name: "A".into(),
            kind,
            expr: expr.into(),
            ..Default::default()
        }
    }

    fn codes(r: &CustomRules) -> Vec<&'static str> {
        check_rules(r).iter().map(|p| p.code).collect()
    }

    #[test]
    fn valid_rules_have_no_problems() {
        let r = CustomRules {
            extractors: vec![
                ex(ExtractorKind::Css, ".price"),
                Extractor {
                    name: "B".into(),
                    ..ex(ExtractorKind::Xpath, "//meta[@itemprop='sku']/@content")
                },
                Extractor {
                    name: "C".into(),
                    ..ex(ExtractorKind::Regex, r"gtm\.js\?id=(GTM-[A-Z0-9]+)")
                },
                Extractor {
                    name: "D".into(),
                    ..ex(ExtractorKind::JsonLd, "offers.price")
                },
                Extractor {
                    name: "E".into(),
                    ..ex(ExtractorKind::JsonLd, "$..price")
                },
            ],
            searches: vec![Search {
                name: "S".into(),
                term: "consent".into(),
                ..Default::default()
            }],
        };
        assert!(check_rules(&r).is_empty(), "{:?}", check_rules(&r));
    }

    #[test]
    fn syntax_errors_name_the_rule() {
        let r = CustomRules {
            extractors: vec![ex(ExtractorKind::Regex, "(unclosed")],
            searches: vec![],
        };
        let p = check_rules(&r);
        assert_eq!(p[0].path, "custom.extractors[0].expr");
        assert_eq!(p[0].code, "rule_syntax");
        for (kind, bad) in [
            (ExtractorKind::Css, "div[["),
            (ExtractorKind::Xpath, "//div[@"),
            (ExtractorKind::JsonLd, "$[?"),
            (ExtractorKind::JsonLd, "offers..price"),
        ] {
            let r = CustomRules {
                extractors: vec![ex(kind, bad)],
                searches: vec![],
            };
            assert_eq!(codes(&r), vec!["rule_syntax"], "{bad}");
        }
    }

    #[test]
    fn lookaround_is_explained() {
        let m = check_regex("a(?=b)", false).unwrap_err();
        assert!(m.contains("not supported"), "{m}");
    }

    #[test]
    fn names_are_required_and_unique() {
        let r = CustomRules {
            extractors: vec![
                ex(ExtractorKind::Css, "a"),
                ex(ExtractorKind::Css, "b"),
                Extractor {
                    name: " ".into(),
                    ..ex(ExtractorKind::Css, "c")
                },
            ],
            searches: vec![],
        };
        assert_eq!(codes(&r), vec!["rule_name_duplicate", "rule_name_empty"]);
    }

    #[test]
    fn attribute_mode_needs_an_attribute() {
        let r = CustomRules {
            extractors: vec![Extractor {
                mode: ExtractMode::Attribute,
                ..ex(ExtractorKind::Css, "a")
            }],
            searches: vec![],
        };
        assert_eq!(codes(&r), vec!["rule_attr_empty"]);
    }

    #[test]
    fn empty_matching_search_warns() {
        let r = CustomRules {
            extractors: vec![],
            searches: vec![Search {
                name: "S".into(),
                term: "a*".into(),
                regex: true,
                ..Default::default()
            }],
        };
        let p = check_rules(&r);
        assert_eq!(p.len(), 1);
        assert!(p[0].warning);
    }

    #[test]
    fn too_many_rules() {
        let r = CustomRules {
            extractors: (0..=MAX_RULES)
                .map(|i| Extractor {
                    name: format!("r{i}"),
                    ..ex(ExtractorKind::Css, "a")
                })
                .collect(),
            searches: vec![],
        };
        assert_eq!(codes(&r), vec!["too_many_rules"]);
    }

    #[test]
    fn rule_files_round_trip_and_merge() {
        let r = CustomRules {
            extractors: vec![ex(ExtractorKind::Css, ".price")],
            searches: vec![Search {
                name: "S".into(),
                term: "x".into(),
                ..Default::default()
            }],
        };
        let text = export_rules(&r);
        assert!(text.contains("spiderlens-rules"));
        assert_eq!(import_rules(&text).unwrap(), r);
        assert_eq!(
            import_rules(r#"{"extractors":[]}"#).unwrap(),
            CustomRules::default()
        );
        assert!(
            import_rules(r#"{"format":"spiderlens-rules","version":99}"#)
                .unwrap_err()
                .contains("newer")
        );
        let mut base = r.clone();
        merge_rules(
            &mut base,
            CustomRules {
                extractors: vec![
                    ex(ExtractorKind::Css, "h1"),
                    Extractor {
                        name: "B".into(),
                        ..ex(ExtractorKind::Css, "p")
                    },
                ],
                searches: vec![],
            },
        );
        assert_eq!(base.extractors.len(), 2);
        assert_eq!(base.extractors[0].expr, "h1");
    }

    #[test]
    fn old_configs_have_no_rules() {
        let r: CustomRules = serde_json::from_str("{}").unwrap();
        assert!(r.is_empty());
    }
}
