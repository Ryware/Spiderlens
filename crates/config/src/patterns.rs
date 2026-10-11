//! Include and exclude patterns (FR-010, FR-011; research R3).
//!
//! Patterns use the `regex` crate, which matches in linear time, so a bad
//! pattern cannot stall a crawl of millions of URLs. Look-around and
//! back-references are not supported; validation explains that.

use regex::{Regex, RegexSet};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::normalize_url;

/// Compiled include and exclude patterns.
#[derive(Debug, Clone)]
pub struct UrlFilter {
    include: Option<RegexSet>,
    exclude: Option<RegexSet>,
}

/// Why a URL is or is not crawled. Indexes refer to the pattern lists.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "decision", rename_all = "snake_case")]
#[ts(export)]
pub enum Decision {
    /// No include patterns, or `include` matched (lowest index).
    Crawl { include: Option<u32> },
    /// An exclude pattern matched; it wins over includes.
    Excluded { exclude: u32 },
    /// Include patterns exist and none matched.
    NotIncluded,
}

impl Decision {
    pub fn crawls(self) -> bool {
        matches!(self, Self::Crawl { .. })
    }
}

/// A pattern that failed to compile, with a plain-language reason.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PatternError {
    pub index: usize,
    pub message: String,
}

pub(crate) fn check_pattern(pattern: &str) -> Result<(), String> {
    if pattern.is_empty() {
        return Err("The pattern is empty.".into());
    }
    if ["(?=", "(?!", "(?<=", "(?<!"].iter().any(|p| pattern.contains(p)) {
        return Err("Look-ahead and look-behind are not supported. Use an exclude pattern instead.".into());
    }
    match Regex::new(pattern) {
        Ok(_) => Ok(()),
        Err(regex::Error::Syntax(detail)) if detail.contains("backreference") => {
            Err("Back-references such as \\1 are not supported.".into())
        }
        Err(regex::Error::Syntax(detail)) => Err(format!(
            "This is not a valid regular expression: {}",
            detail.lines().last().unwrap_or("").trim()
        )),
        Err(e) => Err(format!("This pattern is too large to use: {e}")),
    }
}

fn compile(patterns: &[String]) -> Result<Option<RegexSet>, Vec<PatternError>> {
    let errors: Vec<_> = patterns
        .iter()
        .enumerate()
        .filter_map(|(index, p)| {
            check_pattern(p)
                .err()
                .map(|message| PatternError { index, message })
        })
        .collect();
    if !errors.is_empty() {
        return Err(errors);
    }
    if patterns.is_empty() {
        return Ok(None);
    }
    RegexSet::new(patterns).map(Some).map_err(|e| {
        vec![PatternError {
            index: 0,
            message: e.to_string(),
        }]
    })
}

impl UrlFilter {
    /// Compiles both lists. Errors are returned as (is_include, error).
    pub fn new(include: &[String], exclude: &[String]) -> Result<Self, Vec<(bool, PatternError)>> {
        let inc = compile(include);
        let exc = compile(exclude);
        match (inc, exc) {
            (Ok(include), Ok(exclude)) => Ok(Self { include, exclude }),
            (inc, exc) => {
                let mut errors: Vec<_> = inc
                    .err()
                    .unwrap_or_default()
                    .into_iter()
                    .map(|e| (true, e))
                    .collect();
                errors.extend(exc.err().unwrap_or_default().into_iter().map(|e| (false, e)));
                Err(errors)
            }
        }
    }

    /// Decides for a normalized URL. Exclude always wins (FR-010).
    pub fn decide(&self, url: &str) -> Decision {
        if let Some(index) = self
            .exclude
            .as_ref()
            .and_then(|set| set.matches(url).iter().next())
        {
            return Decision::Excluded {
                exclude: index as u32,
            };
        }
        match &self.include {
            None => Decision::Crawl { include: None },
            Some(set) => match set.matches(url).iter().next() {
                Some(index) => Decision::Crawl {
                    include: Some(index as u32),
                },
                None => Decision::NotIncluded,
            },
        }
    }
}

/// One row of the pattern tester (FR-011).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PatternTest {
    pub input: String,
    /// Normalized URL that was tested, or null if the input is not a URL.
    pub url: Option<String>,
    pub decision: Option<Decision>,
    /// The pattern that decided, for display.
    pub pattern: Option<String>,
    pub error: Option<String>,
}

/// Tests sample URLs against the patterns. Pattern errors are reported by
/// `validate`; here they make every row carry the error.
pub fn test_patterns(include: &[String], exclude: &[String], samples: &[String]) -> Vec<PatternTest> {
    let filter = UrlFilter::new(include, exclude);
    samples
        .iter()
        .filter(|s| !s.trim().is_empty())
        .map(|input| {
            let mut row = PatternTest {
                input: input.clone(),
                url: None,
                decision: None,
                pattern: None,
                error: None,
            };
            let filter = match &filter {
                Ok(f) => f,
                Err(_) => {
                    row.error = Some("Fix the invalid patterns first.".into());
                    return row;
                }
            };
            match normalize_url(input) {
                Ok(url) => {
                    let decision = filter.decide(url.as_str());
                    row.pattern = match decision {
                        Decision::Crawl { include: Some(i) } => include.get(i as usize).cloned(),
                        Decision::Excluded { exclude: i } => exclude.get(i as usize).cloned(),
                        _ => None,
                    };
                    row.url = Some(url.into());
                    row.decision = Some(decision);
                }
                Err(e) => row.error = Some(e.0),
            }
            row
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn exclude_beats_include() {
        let f = UrlFilter::new(&s(&["/blog/"]), &s(&["/blog/tag/"])).unwrap();
        assert_eq!(
            f.decide("https://a.test/blog/post-1"),
            Decision::Crawl { include: Some(0) }
        );
        assert_eq!(
            f.decide("https://a.test/blog/tag/news"),
            Decision::Excluded { exclude: 0 }
        );
        assert_eq!(f.decide("https://a.test/about"), Decision::NotIncluded);
    }

    #[test]
    fn no_patterns_crawls_everything() {
        let f = UrlFilter::new(&[], &[]).unwrap();
        assert!(f.decide("https://a.test/anything").crawls());
    }

    #[test]
    fn query_patterns_work() {
        let f = UrlFilter::new(&[], &s(&[r"\?sort="])).unwrap();
        assert!(!f.decide("https://a.test/list?sort=asc").crawls());
        assert!(f.decide("https://a.test/list?page=2").crawls());
    }

    #[test]
    fn errors_name_the_pattern() {
        let errs = UrlFilter::new(&s(&["ok", "(unclosed"]), &s(&["(?!x)"])).unwrap_err();
        assert_eq!(errs.len(), 2);
        assert_eq!((errs[0].0, errs[0].1.index), (true, 1));
        assert_eq!((errs[1].0, errs[1].1.index), (false, 0));
        assert!(errs[1].1.message.contains("Look-ahead"));
    }

    #[test]
    fn backreference_is_explained() {
        assert!(check_pattern(r"(a)\1").unwrap_err().contains("Back-references"));
    }

    #[test]
    fn tester_names_deciding_pattern() {
        let rows = test_patterns(
            &s(&["/blog/"]),
            &s(&["/blog/tag/"]),
            &s(&[
                "a.test/blog/post-1",
                "https://a.test/blog/tag/news",
                "not a url",
                "",
            ]),
        );
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].url.as_deref(), Some("https://a.test/blog/post-1"));
        assert_eq!(rows[0].pattern.as_deref(), Some("/blog/"));
        assert_eq!(rows[1].decision, Some(Decision::Excluded { exclude: 0 }));
        assert_eq!(rows[1].pattern.as_deref(), Some("/blog/tag/"));
        assert!(rows[2].error.is_some());
    }

    #[test]
    fn catastrophic_pattern_is_fast() {
        // Exponential in backtracking engines; linear here.
        let f = UrlFilter::new(&s(&["(a+)+$"]), &[]).unwrap();
        let url = format!("https://a.test/{}b", "a".repeat(10_000));
        let start = std::time::Instant::now();
        let _ = f.decide(&url);
        assert!(start.elapsed() < std::time::Duration::from_millis(500));
    }
}
