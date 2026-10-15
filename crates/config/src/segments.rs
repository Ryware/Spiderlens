//! URL segments (spec 027 part B, plan D3).
//!
//! A segment is a named group of URLs defined by conditions on the URL and a
//! few stored page facts. Segments are part of the crawl profile, so they are
//! saved and exported with it and stored with every crawl. Membership is
//! decided here, in one place, for the results reader, the CLI and the scope
//! tester: each URL belongs to the first enabled segment that matches, or to
//! none (FR-B03).

use regex::Regex;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::validate::Issue;

/// At most this many segments per profile (data model).
pub const MAX_SEGMENTS: usize = 100;
/// At most this many conditions per segment.
pub const MAX_CLAUSES: usize = 10;
pub const MAX_NAME_CHARS: usize = 60;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export)]
pub struct Segment {
    /// Stable within the profile; never 0 (0 means "No segment" in filters).
    pub id: u32,
    pub name: String,
    pub color: SegmentColor,
    pub enabled: bool,
    pub mode: Match,
    pub clauses: Vec<Clause>,
}

impl Default for Segment {
    fn default() -> Self {
        Self {
            id: 1,
            name: String::new(),
            color: SegmentColor::default(),
            enabled: true,
            mode: Match::All,
            clauses: vec![Clause::default()],
        }
    }
}

/// One of the design system's segment colours (spec 017 tokens).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum SegmentColor {
    #[default]
    Teal,
    Blue,
    Violet,
    Pink,
    Orange,
    Amber,
    Green,
    Gray,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, rename = "SegmentMatch")]
pub enum Match {
    /// Every condition must hold.
    #[default]
    All,
    /// At least one condition must hold.
    Any,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export, rename = "SegmentClause")]
pub struct Clause {
    pub field: Field,
    pub op: Op,
    pub value: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, rename = "SegmentField")]
pub enum Field {
    /// The whole URL.
    #[default]
    Url,
    Host,
    Path,
    /// The query string without the `?`.
    Query,
    /// `2xx`, `3xx`, `4xx`, `5xx` or `none` (no response).
    StatusClass,
    /// `html`, `image`, `pdf`, `css`, `javascript` or `other`.
    ContentKind,
    /// `indexable` or `non-indexable`.
    Indexability,
    /// The page's declared language, for example `he` or `en-gb`.
    Language,
    Depth,
    WordCount,
}

impl Field {
    pub fn is_numeric(self) -> bool {
        matches!(self, Self::Depth | Self::WordCount)
    }

    /// Fields that need a response or page content; the scope tester cannot
    /// know them before the crawl.
    pub fn needs_crawl(self) -> bool {
        !matches!(self, Self::Url | Self::Host | Self::Path | Self::Query)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, rename = "SegmentOp")]
pub enum Op {
    #[default]
    Contains,
    NotContains,
    StartsWith,
    EndsWith,
    Equals,
    NotEquals,
    Matches,
    Lt,
    Le,
    Gt,
    Ge,
}

impl Op {
    pub fn is_numeric(self) -> bool {
        matches!(self, Self::Lt | Self::Le | Self::Gt | Self::Ge)
    }
}

/// What a segment condition can test for one URL. Facts that the crawl file
/// does not have (yet) are `None`, and a condition on them does not match.
#[derive(Debug, Clone, Default)]
pub struct UrlFacts<'a> {
    pub url: &'a str,
    pub status: Option<i64>,
    pub content_kind: Option<&'a str>,
    pub indexable: Option<bool>,
    pub lang: Option<&'a str>,
    pub depth: Option<u32>,
    pub word_count: Option<i64>,
}

enum Test {
    Text { op: Op, value: String },
    Regex(Regex),
    Number { op: Op, value: i64 },
}

struct CompiledClause {
    field: Field,
    test: Test,
}

struct CompiledSegment {
    id: u32,
    mode: Match,
    clauses: Vec<CompiledClause>,
}

/// Segments ready to classify URLs. Disabled segments are left out.
pub struct SegmentMatcher {
    segments: Vec<CompiledSegment>,
    needs_facts: bool,
}

impl SegmentMatcher {
    /// Compiles valid segments. Use [`validate_segments`] first to report
    /// problems; an invalid clause here never matches.
    pub fn new(segments: &[Segment]) -> Self {
        let compiled: Vec<CompiledSegment> = segments
            .iter()
            .filter(|s| s.enabled)
            .map(|s| CompiledSegment {
                id: s.id,
                mode: s.mode,
                clauses: s.clauses.iter().filter_map(compile_clause).collect(),
            })
            .filter(|s| !s.clauses.is_empty())
            .collect();
        let needs_facts = compiled
            .iter()
            .any(|s| s.clauses.iter().any(|c| c.field.needs_crawl()));
        Self {
            segments: compiled,
            needs_facts,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.segments.is_empty()
    }

    /// Whether any condition reads facts other than the URL.
    pub fn needs_facts(&self) -> bool {
        self.needs_facts
    }

    /// The id of the first enabled segment `facts` belongs to.
    pub fn classify(&self, facts: &UrlFacts) -> Option<u32> {
        let parsed = url::Url::parse(facts.url).ok();
        self.segments
            .iter()
            .find(|s| {
                let hit = |c: &CompiledClause| clause_matches(c, facts, parsed.as_ref());
                match s.mode {
                    Match::All => s.clauses.iter().all(hit),
                    Match::Any => s.clauses.iter().any(hit),
                }
            })
            .map(|s| s.id)
    }
}

fn compile_clause(c: &Clause) -> Option<CompiledClause> {
    let test = if c.op == Op::Matches {
        Test::Regex(Regex::new(c.value.trim()).ok()?)
    } else if c.op.is_numeric() || c.field.is_numeric() {
        Test::Number {
            op: c.op,
            value: c.value.trim().parse().ok()?,
        }
    } else {
        Test::Text {
            op: c.op,
            value: c.value.trim().to_lowercase(),
        }
    };
    Some(CompiledClause { field: c.field, test })
}

fn status_class(status: Option<i64>) -> &'static str {
    match status {
        Some(200..=299) => "2xx",
        Some(300..=399) => "3xx",
        Some(400..=499) => "4xx",
        Some(500..=599) => "5xx",
        Some(_) => "other",
        None => "none",
    }
}

fn clause_matches(c: &CompiledClause, f: &UrlFacts, url: Option<&url::Url>) -> bool {
    let text: Option<String> = match c.field {
        Field::Url => Some(f.url.to_string()),
        Field::Host => url.and_then(|u| u.host_str()).map(str::to_string),
        Field::Path => url.map(|u| u.path().to_string()),
        Field::Query => url.map(|u| u.query().unwrap_or("").to_string()),
        Field::StatusClass => Some(status_class(f.status).to_string()),
        Field::ContentKind => f.content_kind.map(str::to_string),
        Field::Indexability => f
            .indexable
            .map(|i| if i { "indexable" } else { "non-indexable" }.to_string()),
        Field::Language => f.lang.map(str::to_string),
        Field::Depth => f.depth.map(|d| d.to_string()),
        Field::WordCount => f.word_count.map(|w| w.to_string()),
    };
    let Some(text) = text else { return false };
    match &c.test {
        Test::Regex(re) => re.is_match(&text),
        Test::Number { op, value } => {
            let Ok(n) = text.parse::<i64>() else { return false };
            match op {
                Op::Lt => n < *value,
                Op::Le => n <= *value,
                Op::Gt => n > *value,
                Op::Ge => n >= *value,
                Op::Equals => n == *value,
                Op::NotEquals => n != *value,
                _ => false,
            }
        }
        Test::Text { op, value } => {
            let hay = text.to_lowercase();
            match op {
                Op::Contains => hay.contains(value.as_str()),
                Op::NotContains => !hay.contains(value.as_str()),
                Op::StartsWith => hay.starts_with(value.as_str()),
                Op::EndsWith => hay.ends_with(value.as_str()),
                Op::Equals => hay == *value,
                Op::NotEquals => hay != *value,
                _ => false,
            }
        }
    }
}

const STATUS_CLASSES: [&str; 6] = ["2xx", "3xx", "4xx", "5xx", "other", "none"];
const CONTENT_KINDS: [&str; 6] = ["html", "image", "pdf", "css", "javascript", "other"];
const INDEXABILITY: [&str; 2] = ["indexable", "non-indexable"];

/// Problems with a profile's segments, with paths such as
/// `segments[2].clauses[0]` so the settings screen can show each one next
/// to its condition (FR-B07).
pub fn validate_segments(segments: &[Segment]) -> Vec<Issue> {
    let mut out = Vec::new();
    let mut err = |path: String, code: &str, message: String| out.push(Issue::error(path, code, message));
    if segments.len() > MAX_SEGMENTS {
        err(
            "segments".into(),
            "too_many_segments",
            format!("Use at most {MAX_SEGMENTS} segments."),
        );
    }
    let mut names: Vec<String> = Vec::new();
    let mut ids: Vec<u32> = Vec::new();
    for (i, s) in segments.iter().enumerate() {
        let at = format!("segments[{i}]");
        let name = s.name.trim();
        if name.is_empty() {
            err(
                format!("{at}.name"),
                "segment_name_empty",
                "Give the segment a name.".into(),
            );
        } else if name.chars().count() > MAX_NAME_CHARS {
            err(
                format!("{at}.name"),
                "segment_name_long",
                format!("Use at most {MAX_NAME_CHARS} characters."),
            );
        } else if names.contains(&name.to_lowercase()) {
            err(
                format!("{at}.name"),
                "segment_name_taken",
                format!("Another segment is already called {name}."),
            );
        }
        names.push(name.to_lowercase());
        if s.id == 0 || ids.contains(&s.id) {
            err(
                format!("{at}.id"),
                "segment_id",
                "Each segment needs its own id above 0.".into(),
            );
        }
        ids.push(s.id);
        if s.clauses.is_empty() {
            err(
                format!("{at}.clauses"),
                "segment_no_clauses",
                "Add at least one condition.".into(),
            );
        }
        if s.clauses.len() > MAX_CLAUSES {
            err(
                format!("{at}.clauses"),
                "segment_too_many_clauses",
                format!("Use at most {MAX_CLAUSES} conditions."),
            );
        }
        for (j, c) in s.clauses.iter().enumerate() {
            if let Err((code, message)) = check_clause(c) {
                err(format!("{at}.clauses[{j}]"), code, message);
            }
        }
    }
    out
}

fn check_clause(c: &Clause) -> Result<(), (&'static str, String)> {
    let value = c.value.trim();
    if value.is_empty() {
        return Err(("segment_value_empty", "Enter a value to compare with.".into()));
    }
    if c.field.is_numeric() {
        if !(c.op.is_numeric() || matches!(c.op, Op::Equals | Op::NotEquals)) {
            return Err(("segment_op", "Compare numbers with =, ≠, <, ≤, > or ≥.".into()));
        }
        if value.parse::<i64>().is_err() {
            return Err(("segment_number", format!("{value} is not a whole number.")));
        }
        return Ok(());
    }
    if c.op.is_numeric() {
        return Err((
            "segment_op",
            "Less than and greater than only apply to numbers.".into(),
        ));
    }
    let allowed: Option<&[&str]> = match c.field {
        Field::StatusClass => Some(&STATUS_CLASSES),
        Field::ContentKind => Some(&CONTENT_KINDS),
        Field::Indexability => Some(&INDEXABILITY),
        _ => None,
    };
    if let Some(allowed) = allowed {
        if !matches!(c.op, Op::Equals | Op::NotEquals) {
            return Err(("segment_op", "Use is or is not for this field.".into()));
        }
        if !allowed.contains(&value.to_lowercase().as_str()) {
            return Err(("segment_value", format!("Use one of: {}.", allowed.join(", "))));
        }
    }
    if c.op == Op::Matches {
        crate::patterns::check_pattern(value).map_err(|m| ("pattern_syntax", m))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seg(id: u32, name: &str, clauses: &[(Field, Op, &str)]) -> Segment {
        Segment {
            id,
            name: name.into(),
            clauses: clauses
                .iter()
                .map(|&(field, op, value)| Clause {
                    field,
                    op,
                    value: value.into(),
                })
                .collect(),
            ..Segment::default()
        }
    }

    fn url(u: &str) -> UrlFacts<'_> {
        UrlFacts {
            url: u,
            ..UrlFacts::default()
        }
    }

    #[test]
    fn first_matching_segment_wins() {
        let blog = seg(1, "Blog", &[(Field::Path, Op::Contains, "/blog/")]);
        let shop = seg(2, "Shop", &[(Field::Path, Op::Contains, "/shop/")]);
        let m = SegmentMatcher::new(&[blog.clone(), shop.clone()]);
        assert_eq!(m.classify(&url("https://a.test/shop/blog/x")), Some(1));
        assert_eq!(m.classify(&url("https://a.test/shop/x")), Some(2));
        assert_eq!(m.classify(&url("https://a.test/about")), None);
        // Order decides (B1.2).
        let m = SegmentMatcher::new(&[shop, blog]);
        assert_eq!(m.classify(&url("https://a.test/shop/blog/x")), Some(2));
    }

    #[test]
    fn disabled_segments_are_skipped() {
        let mut blog = seg(1, "Blog", &[(Field::Url, Op::Contains, "blog")]);
        blog.enabled = false;
        let m = SegmentMatcher::new(&[blog]);
        assert!(m.is_empty());
        assert_eq!(m.classify(&url("https://a.test/blog")), None);
    }

    #[test]
    fn url_parts_and_text_ops() {
        let m = |c: (Field, Op, &str), u: &str| {
            SegmentMatcher::new(&[seg(1, "S", &[c])]).classify(&url(u)) == Some(1)
        };
        assert!(m(
            (Field::Host, Op::Equals, "help.a.test"),
            "https://help.a.test/x"
        ));
        assert!(!m(
            (Field::Host, Op::Equals, "help.a.test"),
            "https://a.test/help"
        ));
        assert!(m((Field::Path, Op::StartsWith, "/p/"), "https://a.test/p/1"));
        assert!(m((Field::Path, Op::EndsWith, ".html"), "https://a.test/x.HTML"));
        assert!(m(
            (Field::Query, Op::Contains, "page="),
            "https://a.test/x?page=2"
        ));
        assert!(m((Field::Url, Op::NotContains, "/tag/"), "https://a.test/x"));
        assert!(m(
            (Field::Path, Op::Matches, r"^/p/[0-9]+$"),
            "https://a.test/p/42"
        ));
        assert!(!m(
            (Field::Path, Op::Matches, r"^/p/[0-9]+$"),
            "https://a.test/p/x"
        ));
    }

    #[test]
    fn page_facts_and_all_or_any() {
        // B2.1: "Thin: word count under 300 AND status is 200".
        let thin = seg(
            1,
            "Thin",
            &[
                (Field::WordCount, Op::Lt, "300"),
                (Field::StatusClass, Op::Equals, "2xx"),
            ],
        );
        let m = SegmentMatcher::new(std::slice::from_ref(&thin));
        assert!(m.needs_facts());
        let page = |status, words| UrlFacts {
            url: "https://a.test/x",
            status: Some(status),
            word_count: Some(words),
            ..UrlFacts::default()
        };
        assert_eq!(m.classify(&page(200, 120)), Some(1));
        assert_eq!(m.classify(&page(200, 900)), None);
        assert_eq!(m.classify(&page(404, 120)), None);
        // Unknown facts never match.
        assert_eq!(m.classify(&url("https://a.test/x")), None);
        let mut any = thin;
        any.mode = Match::Any;
        assert_eq!(SegmentMatcher::new(&[any]).classify(&page(404, 120)), Some(1));
    }

    #[test]
    fn indexability_language_kind_depth() {
        let facts = UrlFacts {
            url: "https://a.test/he/x",
            indexable: Some(false),
            lang: Some("he"),
            content_kind: Some("html"),
            depth: Some(3),
            ..UrlFacts::default()
        };
        for c in [
            (Field::Indexability, Op::Equals, "non-indexable"),
            (Field::Language, Op::StartsWith, "he"),
            (Field::ContentKind, Op::Equals, "html"),
            (Field::Depth, Op::Ge, "3"),
        ] {
            assert_eq!(
                SegmentMatcher::new(&[seg(1, "S", &[c])]).classify(&facts),
                Some(1),
                "{c:?}"
            );
        }
    }

    #[test]
    fn validation_points_at_the_bad_clause() {
        let bad = vec![
            seg(1, "Blog", &[(Field::Path, Op::Matches, "(unclosed")]),
            seg(2, "blog", &[(Field::WordCount, Op::Contains, "x")]),
            seg(2, "", &[(Field::StatusClass, Op::Equals, "6xx")]),
            seg(4, "Deep", &[(Field::Path, Op::Gt, "3")]),
        ];
        let issues = validate_segments(&bad);
        let at = |p: &str| issues.iter().any(|i| i.path == p);
        assert!(at("segments[0].clauses[0]"));
        assert!(at("segments[1].name"), "{issues:?}");
        assert!(at("segments[1].clauses[0]"));
        assert!(at("segments[2].name"));
        assert!(at("segments[2].id"));
        assert!(at("segments[2].clauses[0]"));
        assert!(at("segments[3].clauses[0]"));
        assert!(validate_segments(&[seg(1, "Blog", &[(Field::Path, Op::Contains, "/blog/")])]).is_empty());
    }

    #[test]
    fn segments_round_trip_with_defaults() {
        let s: Segment = serde_json::from_str(r#"{"id":3,"name":"Shop"}"#).unwrap();
        assert!(s.enabled);
        assert_eq!(s.mode, Match::All);
        let back: Segment = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
        assert_eq!(back, s);
    }
}
