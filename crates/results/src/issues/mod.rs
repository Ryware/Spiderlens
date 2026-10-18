//! The issue rule catalogue (spec 009, plan D1).
//!
//! A rule is data: a SQL condition over `urls u`, the URLs it applies to, the
//! data it needs and the texts that explain it. The desktop app, the CLI and
//! later features (report, compare, agents) all read this one catalogue, so
//! a rule can never mean two things (FR-002). Thresholds are read through
//! the `thr('<key>')` SQL function, so a settings change needs no new SQL
//! and no user value ever reaches SQL text (plan D3).
//!
//! Rules live in one file per category under `rules/`; a later spec adds its
//! file and one line in [`catalogue`].

mod eval;
pub mod rules;
mod settings;

use serde::Serialize;
use ts_rs::TS;

pub(crate) use eval::build_derived_with;
pub use eval::{
    IssueCount, IssueTarget, IssuesSummary, PriorityTotal, RuleRun, TARGETS_SHOWN, UrlIssue, evaluate_on,
    record_issue_settings, rule_runs, stored_issue_settings_on,
};
pub use settings::{ResolvedRule, ResolvedSettings, SettingsNotice, resolve};
pub use spiderlens_config::Priority;

use crate::tabs::{Derived, Need};

/// Increases whenever a rule is added, removed or changes its condition
/// (FR-003).
pub const RULES_VERSION: u32 = 6;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, rename = "IssueCategory")]
pub enum Category {
    ResponseCodes,
    Links,
    PageTitles,
    MetaDescription,
    H1,
    H2,
    Headings,
    Content,
    Images,
    Canonicals,
    Directives,
    Hreflang,
    Pagination,
    StructuredData,
    Security,
    Url,
    Sitemaps,
    Serp,
    Pdf,
    Spelling,
    Amp,
    Accessibility,
    Mobile,
    Agents,
}

impl Category {
    pub fn label(self) -> &'static str {
        match self {
            Self::ResponseCodes => "Response codes",
            Self::Links => "Links",
            Self::PageTitles => "Page titles",
            Self::MetaDescription => "Meta description",
            Self::H1 => "H1",
            Self::H2 => "H2",
            Self::Headings => "Headings",
            Self::Content => "Content",
            Self::Images => "Images",
            Self::Canonicals => "Canonicals",
            Self::Directives => "Directives",
            Self::Hreflang => "Hreflang",
            Self::Pagination => "Pagination",
            Self::StructuredData => "Structured data",
            Self::Security => "Security",
            Self::Url => "URL",
            Self::Sitemaps => "Sitemaps",
            Self::Serp => "SERP",
            Self::Pdf => "PDFs",
            Self::Spelling => "Spelling",
            Self::Amp => "AMP",
            Self::Accessibility => "Accessibility",
            Self::Mobile => "Mobile",
            Self::Agents => "AI agents",
        }
    }
}

/// Which URLs a rule looks at. Each maps to one SQL clause; rules sharing a
/// scope are evaluated in one scan (plan D4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, rename = "RuleScope")]
pub enum Scope {
    /// Every URL with a response or a blocking decision (Response codes tab).
    AllUrls,
    /// Internal URLs that were requested.
    InternalUrls,
    /// External URLs that were checked.
    ExternalUrls,
    /// Internal HTML pages that returned 2xx (spec 007 FR-010).
    Pages,
    /// Image URLs (Images tab).
    Images,
    /// Every URL the crawl knows of, crawled or not: sitemap URLs that were
    /// not requested are listed too (spec 011 FR-017).
    Known,
}

impl Scope {
    pub const ALL: [Scope; 6] = [
        Self::AllUrls,
        Self::InternalUrls,
        Self::ExternalUrls,
        Self::Pages,
        Self::Images,
        Self::Known,
    ];

    pub fn sql(self) -> &'static str {
        match self {
            Self::AllUrls => crate::tabs::RESPONSES,
            Self::InternalUrls => crate::tabs::INTERNAL,
            Self::ExternalUrls => "u.internal = 0 AND u.state = 'crawled'",
            Self::Pages => crate::tabs::HTML_OK,
            Self::Images => crate::tabs::IMAGES,
            Self::Known => "1",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::AllUrls => "All URLs with a response or a blocking decision",
            Self::InternalUrls => "Internal URLs that were requested",
            Self::ExternalUrls => "External URLs that were checked",
            Self::Pages => "Internal HTML pages that returned 2xx",
            Self::Images => "Images",
            Self::Known => "Every URL the crawl found, crawled or not",
        }
    }
}

/// Whether a rule's result for a URL depends on other URLs. Site-wide
/// results can change until the crawl ends (FR-019).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, rename = "IssueRuleKind")]
pub enum RuleKind {
    PerUrl,
    SiteWide,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum Unit {
    Chars,
    Ms,
    Bytes,
    Words,
    Hops,
    Links,
    Depth,
    Score,
    Days,
    Entries,
    Px,
}

impl Unit {
    pub fn label(self) -> &'static str {
        match self {
            Self::Chars => "characters",
            Self::Ms => "ms",
            Self::Bytes => "bytes",
            Self::Words => "words",
            Self::Hops => "hops",
            Self::Links => "links",
            Self::Depth => "levels",
            Self::Score => "points",
            Self::Days => "days",
            Self::Entries => "entries",
            Self::Px => "pixels",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct Threshold {
    pub unit: Unit,
    pub default: f64,
    pub min: f64,
    pub max: f64,
}

/// One issue check (FR-001). The rule id doubles as its threshold key.
#[derive(Debug, Clone, Copy)]
pub struct Rule {
    /// Stable, never reused (FR-003): `<category>.<check>`.
    pub id: &'static str,
    pub name: &'static str,
    pub category: Category,
    pub priority: Priority,
    pub scope: Scope,
    pub kind: RuleKind,
    /// SQL boolean over `urls u`, evaluated within [`Scope::sql`].
    pub cond: &'static str,
    pub threshold: Option<Threshold>,
    pub need: Need,
    pub derived: &'static [Derived],
    /// SQL text expression: why this URL has the issue ("74 characters").
    pub evidence: &'static str,
    /// For link rules: SQL condition over `links l JOIN urls t ON t.id =
    /// l.to_id` with `l.from_id` the page, selecting the targets that cause
    /// the issue, listed in the detail pane (US2 scenario 2).
    pub targets: Option<&'static str>,
    /// Standards references, such as a WCAG 2.2 success criterion.
    pub standards: &'static [&'static str],
    pub why: &'static str,
    pub fix: &'static str,
}

/// Starts a rule; the `const fn` setters below fill in the rest, the way
/// 007 builds its filters.
pub const fn rule(
    id: &'static str,
    name: &'static str,
    category: Category,
    priority: Priority,
    scope: Scope,
    cond: &'static str,
) -> Rule {
    Rule {
        id,
        name,
        category,
        priority,
        scope,
        kind: RuleKind::PerUrl,
        cond,
        threshold: None,
        need: Need::Base,
        derived: &[],
        evidence: "NULL",
        targets: None,
        standards: &[],
        why: "",
        fix: "",
    }
}

impl Rule {
    /// Derived tables the condition reads: those declared, plus
    /// `temp.page_flags` when the condition tests its bits.
    pub fn derived_tables(&self) -> impl Iterator<Item = Derived> + '_ {
        let stats = self
            .cond
            .contains("temp.page_flags")
            .then_some(Derived::PageFlags);
        self.derived.iter().copied().chain(stats)
    }

    pub const fn site_wide(mut self) -> Self {
        self.kind = RuleKind::SiteWide;
        self
    }
    pub const fn threshold(mut self, unit: Unit, default: f64, min: f64, max: f64) -> Self {
        self.threshold = Some(Threshold {
            unit,
            default,
            min,
            max,
        });
        self
    }
    pub const fn needs(mut self, need: Need) -> Self {
        self.need = need;
        self
    }
    pub const fn reads(mut self, derived: &'static [Derived]) -> Self {
        self.derived = derived;
        self
    }
    pub const fn evidence(mut self, sql: &'static str) -> Self {
        self.evidence = sql;
        self
    }
    pub const fn targets(mut self, cond: &'static str) -> Self {
        self.targets = Some(cond);
        self
    }
    pub const fn standards(mut self, refs: &'static [&'static str]) -> Self {
        self.standards = refs;
        self
    }
    pub const fn explain(mut self, why: &'static str, fix: &'static str) -> Self {
        self.why = why;
        self.fix = fix;
        self
    }
}

/// Every rule, in display order within its category.
pub fn catalogue() -> &'static [Rule] {
    static ALL: std::sync::LazyLock<Vec<Rule>> = std::sync::LazyLock::new(|| {
        [
            rules::response::RULES,
            rules::links::RULES,
            rules::titles::RULES,
            rules::meta::RULES,
            rules::headings::RULES,
            rules::content::RULES,
            rules::images::RULES,
            rules::canonicals::RULES,
            rules::directives::RULES,
            rules::hreflang::RULES,
            rules::other::RULES,
            rules::security::RULES,
            rules::url::RULES,
            rules::sitemaps::RULES,
            rules::serp::RULES,
            rules::pdf::RULES,
            rules::spelling::RULES,
            rules::amp::RULES,
            rules::accessibility::RULES,
            rules::mobile::RULES,
            rules::agents::RULES,
        ]
        .concat()
    });
    &ALL
}

/// A rule's position in the catalogue: its id in temp tables.
pub fn index_of(id: &str) -> Option<usize> {
    catalogue().iter().position(|r| r.id == id)
}

pub fn get(id: &str) -> Option<&'static Rule> {
    catalogue().iter().find(|r| r.id == id)
}

/// What the app and CLI show for a rule (data-model.md `RuleInfo`).
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct RuleInfo {
    pub id: String,
    pub name: String,
    pub category: Category,
    pub category_label: String,
    pub priority: Priority,
    pub default_priority: Priority,
    pub enabled: bool,
    pub kind: RuleKind,
    pub scope: String,
    pub threshold: Option<ThresholdInfo>,
    pub standards: Vec<String>,
    pub why: String,
    pub fix: String,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct ThresholdInfo {
    pub unit: Unit,
    pub unit_label: String,
    pub value: f64,
    pub default: f64,
    pub min: f64,
    pub max: f64,
}

/// The catalogue with a profile's issue settings applied, and what was
/// wrong with them (the desktop `rule_catalogue` command, Settings › Issues).
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct RuleCatalogue {
    pub rules: Vec<RuleInfo>,
    pub notices: Vec<SettingsNotice>,
}

/// Resolves `s` and lists every rule with it applied. With an error
/// notice, the rules show the defaults (the change would be rejected).
pub fn rule_catalogue(s: &spiderlens_config::IssueSettings) -> RuleCatalogue {
    let (resolved, notices) = resolve(s);
    let resolved = if notices.iter().any(|n| n.error) {
        ResolvedSettings::default()
    } else {
        resolved
    };
    RuleCatalogue {
        rules: rule_infos(&resolved),
        notices,
    }
}

/// The catalogue with `settings` applied.
pub fn rule_infos(settings: &ResolvedSettings) -> Vec<RuleInfo> {
    catalogue()
        .iter()
        .zip(&settings.rules)
        .map(|(r, s)| RuleInfo {
            id: r.id.into(),
            name: display_name(r, s.threshold),
            category: r.category,
            category_label: r.category.label().into(),
            priority: s.priority,
            default_priority: r.priority,
            enabled: s.enabled,
            kind: r.kind,
            scope: r.scope.label().into(),
            threshold: r.threshold.map(|t| ThresholdInfo {
                unit: t.unit,
                unit_label: t.unit.label().into(),
                value: s.threshold.unwrap_or(t.default),
                default: t.default,
                min: t.min,
                max: t.max,
            }),
            standards: r.standards.iter().map(|s| s.to_string()).collect(),
            why: r.why.into(),
            fix: r.fix.into(),
        })
        .collect()
}

/// The rule's name with its current threshold: "Page title: over 70
/// characters" (FR-016, US4 scenario 1).
pub fn display_name(r: &Rule, value: Option<f64>) -> String {
    match (r.threshold, value) {
        (Some(t), v) => r.name.replace("{n}", &fmt_num(v.unwrap_or(t.default))),
        (None, _) => r.name.to_string(),
    }
}

pub fn fmt_num(v: f64) -> String {
    if v.fract() == 0.0 && v.abs() < 1e15 {
        let n = v as i64;
        let s = n.unsigned_abs().to_string();
        let mut out = String::new();
        for (i, c) in s.chars().enumerate() {
            if i > 0 && (s.len() - i).is_multiple_of(3) {
                out.push(',');
            }
            out.push(c);
        }
        if n < 0 { format!("-{out}") } else { out }
    } else {
        format!("{v}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn flag_rules_test_the_page_flags_bit_they_mean() {
        use crate::tabs::page_flag::*;
        for (id, bits) in [
            ("links.broken_internal", vec![BROKEN_INTERNAL]),
            ("links.broken_external", vec![BROKEN_EXTERNAL]),
            ("links.to_redirect", vec![TO_REDIRECT]),
            ("links.nofollow_internal", vec![NOFOLLOW_INTERNAL]),
            ("links.no_internal_outlinks", vec![INTERNAL_OUT]),
            ("links.weak_anchor", vec![WEAK_ANCHOR]),
            ("security.mixed_content", vec![HTTP_IMAGE]),
            ("canonical.outside_head", vec![CANONICAL_IN_BODY]),
            ("canonical.bad_target", vec![CANONICAL_BAD_TARGET]),
            ("hreflang.no_self", vec![HREFLANG, HREFLANG_SELF]),
            ("hreflang.no_x_default", vec![HREFLANG, HREFLANG_X_DEFAULT]),
            ("hreflang.invalid_code", vec![HREFLANG_BAD_CODE]),
            ("hreflang.bad_target", vec![HREFLANG_BAD_TARGET]),
            ("pagination.bad_target", vec![PAGINATION_BAD_TARGET]),
            ("hreflang.no_return", vec![HREFLANG]),
            ("title.outside_head", vec![TITLE_IN_BODY]),
            ("headings.non_sequential", vec![HEADING_SKIP]),
        ] {
            let r = get(id).unwrap_or_else(|| panic!("{id}"));
            for bit in bits {
                assert!(r.cond.contains(&format!("& {bit})")), "{id}: {}", r.cond);
            }
            assert!(r.derived_tables().any(|d| d == Derived::PageFlags), "{id}");
        }
    }

    #[test]
    fn catalogue_is_complete_and_consistent() {
        // FR-001, FR-003, FR-004, SC-008.
        let all = catalogue();
        assert_eq!(
            all.len(),
            161,
            "first rule set (spec table, with readability), spec 011's 27, spec 020's near duplicate, spec 027's SERP (5), PDF (5), spelling (4) and AMP (9) rules, spec 026's accessibility and mobile (21) and spec 016's 15"
        );
        let mut ids = HashSet::new();
        let mut cats = HashSet::new();
        for r in all {
            assert!(ids.insert(r.id), "duplicate rule id {}", r.id);
            cats.insert(r.category);
            let (cat, check) = r.id.split_once('.').expect("id is category.check");
            assert!(!cat.is_empty() && !check.is_empty(), "{}", r.id);
            assert!(
                r.id.chars()
                    .all(|c| c.is_ascii_lowercase() || c == '.' || c == '_' || c.is_ascii_digit()),
                "{}",
                r.id
            );
            assert!(!r.name.is_empty(), "{} name", r.id);
            assert!(r.why.len() > 20, "{} why", r.id);
            assert!(r.fix.len() > 20, "{} fix", r.id);
            assert!(!r.cond.trim().is_empty(), "{} cond", r.id);
            assert_ne!(r.evidence, "NULL", "{} has no evidence", r.id);
            match r.threshold {
                Some(t) => {
                    assert!(r.name.contains("{n}"), "{} names its threshold", r.id);
                    assert!(
                        r.cond.contains(&format!("thr('{}')", r.id)),
                        "{} reads its threshold",
                        r.id
                    );
                    assert!(
                        t.min <= t.default && t.default <= t.max,
                        "{} default in range",
                        r.id
                    );
                }
                None => {
                    assert!(!r.name.contains("{n}"), "{}", r.id);
                    assert!(!r.cond.contains("thr("), "{} reads a threshold it lacks", r.id);
                }
            }
        }
        assert_eq!(cats.len(), 24);
    }

    #[test]
    fn names_carry_the_current_threshold() {
        let r = get("title.over").unwrap();
        assert_eq!(display_name(r, None), "Page title: over 60 characters");
        assert_eq!(display_name(r, Some(70.0)), "Page title: over 70 characters");
        assert_eq!(fmt_num(1_048_576.0), "1,048,576");
        assert_eq!(fmt_num(2.5), "2.5");
    }

    #[test]
    fn partial_counts_grow_to_the_final_ones() {
        use std::sync::{Arc, Mutex};
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("c.spiderlens");
        crate::synth::Synth {
            urls: 20_000,
            links_per_page: 3,
            seed: 9,
            ..Default::default()
        }
        .write(&path)
        .unwrap();
        let mut r = crate::Reader::open(&path).unwrap();
        let seen = Arc::new(Mutex::new(Vec::new()));
        let sink = seen.clone();
        r.set_partial_sink(Some(Box::new(move |s| sink.lock().unwrap().push(s))));
        let last = r.issues_summary(false).unwrap();
        let seen = seen.lock().unwrap();
        assert!(!seen.is_empty(), "partial counts were reported");
        let count = |s: &IssuesSummary, id: &str| s.issues.iter().find(|i| i.id == id).unwrap().count;
        for p in seen.iter() {
            assert!(p.issues.iter().all(|i| i.provisional));
            for i in &last.issues {
                assert!(count(p, &i.id).unwrap_or(0) <= i.count.unwrap_or(0), "{}", i.id);
            }
        }
        assert!(last.issues.iter().all(|i| !i.provisional));
        // The same hits as the export path, which builds every table first.
        let settings = ResolvedSettings::default();
        let (conn, features, _) = crate::reader::connect(&path, &settings).unwrap();
        eval::build_derived_with(&conn, &features, &settings, &[crate::tabs::Derived::IssueHits]).unwrap();
        for i in &last.issues {
            let Some(n) = i.count else { continue };
            let idx = catalogue().iter().position(|r| r.id == i.id).unwrap();
            let m: u64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM temp.issue_hits WHERE rule = ?1",
                    [idx as i64],
                    |r| r.get::<_, i64>(0),
                )
                .unwrap() as u64;
            assert_eq!(n, m, "{}", i.id);
        }
    }
}
