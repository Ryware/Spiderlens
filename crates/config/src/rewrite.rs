//! URL rewriting (spec 002 follow-up): regex replacements applied in order
//! to every discovered link after normalization.
//!
//! Patterns use the `regex` crate, like include and exclude patterns, so
//! matching is linear in the URL's length. `$1` and `${name}` in the
//! replacement refer to groups.

use regex::Regex;

use crate::RewriteRule;

/// Rewrite rules compiled once per crawl.
#[derive(Debug, Clone, Default)]
pub struct CompiledRewrites {
    rules: Vec<(Regex, String)>,
}

impl CompiledRewrites {
    /// Fails on the first empty or invalid pattern, with the rule's number.
    pub fn new(rules: &[RewriteRule]) -> Result<Self, String> {
        let mut out = Vec::with_capacity(rules.len());
        for (i, r) in rules.iter().enumerate() {
            if r.pattern.is_empty() {
                return Err(format!("Rewrite rule {} has an empty pattern.", i + 1));
            }
            let re = Regex::new(&r.pattern).map_err(|e| {
                format!(
                    "Rewrite rule {}: {}",
                    i + 1,
                    e.to_string().lines().last().unwrap_or("").trim()
                )
            })?;
            out.push((re, r.replace.clone()));
        }
        Ok(Self { rules: out })
    }

    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }

    /// Applies every rule in order. The text is returned as it is when no
    /// rule matches.
    pub fn apply(&self, url: &str) -> String {
        let mut text = url.to_string();
        for (re, replace) in &self.rules {
            // `replace_all` borrows when nothing matches, so a rule that
            // does not match costs no copy.
            let changed = match re.replace_all(&text, replace.as_str()) {
                std::borrow::Cow::Owned(s) => Some(s),
                std::borrow::Cow::Borrowed(_) => None,
            };
            if let Some(s) = changed {
                text = s;
            }
        }
        text
    }
}

/// Applies `rules` to `url` once, without keeping them compiled. For tests
/// and one-off use; the crawler uses `CompiledRewrites`.
pub fn rewrite_url(rules: &[RewriteRule], url: &str) -> Result<String, String> {
    Ok(CompiledRewrites::new(rules)?.apply(url))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule(pattern: &str, replace: &str) -> RewriteRule {
        RewriteRule {
            pattern: pattern.into(),
            replace: replace.into(),
        }
    }

    #[test]
    fn rules_apply_in_order_with_groups() {
        let rules = [
            rule(r"^http://", "https://"),
            rule(r"/old/(\w+)", "/new/$1"),
            rule(r"/new/", "/newer/"),
        ];
        assert_eq!(
            rewrite_url(&rules, "http://a.test/old/page").unwrap(),
            "https://a.test/newer/page"
        );
    }

    #[test]
    fn no_rules_or_no_match_leave_the_url_alone() {
        assert_eq!(rewrite_url(&[], "https://a.test/x").unwrap(), "https://a.test/x");
        let rules = [rule("zzz", "y")];
        assert_eq!(
            rewrite_url(&rules, "https://a.test/x").unwrap(),
            "https://a.test/x"
        );
        assert!(CompiledRewrites::new(&[]).unwrap().is_empty());
    }

    #[test]
    fn replaces_every_match_and_named_groups() {
        let rules = [rule(r"(?P<n>\d+)", "<${n}>")];
        assert_eq!(
            rewrite_url(&rules, "https://a.test/1/22").unwrap(),
            "https://a.test/<1>/<22>"
        );
    }

    #[test]
    fn empty_and_invalid_patterns_fail() {
        assert!(rewrite_url(&[rule("", "x")], "https://a.test/").is_err());
        let e = rewrite_url(&[rule("(", "x")], "https://a.test/").unwrap_err();
        assert!(e.starts_with("Rewrite rule 1"));
    }
}
