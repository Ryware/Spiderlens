//! Page and site scores (spec 015 FR-034 to FR-040, plan D11).
//!
//! Pure integer arithmetic in hundredths of a point, so the same outcomes
//! give the same score on every platform (SC-006, SC-007).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::rules::{AeoRule, Part};

/// The result of one check on one page or site.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum Outcome {
    Pass,
    Fail,
    /// The check does not apply to this page type.
    NotApplicable,
    /// The data the check needs is missing (FR-025, FR-036).
    NotAvailable,
}

/// Part weights (maximum points); the default profile uses the part maxima.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Weights {
    pub access: u32,
    pub readable: u32,
    pub answer: u32,
    pub trust: u32,
    pub structured: u32,
}

impl Default for Weights {
    fn default() -> Self {
        Self {
            access: Part::Access.default_max(),
            readable: Part::Readable.default_max(),
            answer: Part::Answer.default_max(),
            trust: Part::Trust.default_max(),
            structured: Part::Structured.default_max(),
        }
    }
}

impl Weights {
    pub fn of(&self, part: Part) -> u32 {
        match part {
            Part::Access => self.access,
            Part::Readable => self.readable,
            Part::Answer => self.answer,
            Part::Trust => self.trust,
            Part::Structured => self.structured,
            Part::Site => Part::Site.default_max(),
        }
    }
}

/// A rule's points under `weights`, in hundredths.
pub fn scaled_points(rule: &AeoRule, weights: &Weights) -> u64 {
    let max = u64::from(rule.part.default_max());
    if max == 0 {
        return 0;
    }
    u64::from(rule.points) * u64::from(weights.of(rule.part)) * 100 / max
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct PartScore {
    pub part: Part,
    /// Earned, scaled to the part's weight; `None` when no check counted.
    pub score: Option<u32>,
    pub max: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct PageScore {
    pub score: u32,
    pub parts: Vec<PartScore>,
    pub checks_used: u32,
    pub checks_total: u32,
    pub provisional: bool,
    /// Capped because no AI search crawler may fetch the page (FR-035).
    pub capped: bool,
    /// (rule id, points lost in hundredths), largest first.
    pub lost: Vec<(&'static str, u64)>,
}

/// Scores one page from its outcomes (one per rule of a page part, in any
/// order). `blocked_for_all_search` applies the cap of FR-035.
pub fn page_score(
    outcomes: &[(&'static AeoRule, Outcome)],
    weights: &Weights,
    blocked_for_all_search: bool,
    blocked_cap: u32,
) -> PageScore {
    let mut earned = 0u64;
    let mut possible = 0u64;
    let mut per_part: BTreeMap<Part, (u64, u64)> = BTreeMap::new();
    let mut used = 0u32;
    let mut total = 0u32;
    let mut lost = Vec::new();
    for (rule, outcome) in outcomes {
        if rule.part == Part::Site || rule.points == 0 {
            continue;
        }
        if *outcome != Outcome::NotApplicable {
            total += 1;
        }
        let pts = scaled_points(rule, weights);
        match outcome {
            Outcome::Pass | Outcome::Fail => {
                used += 1;
                possible += pts;
                let e = per_part.entry(rule.part).or_default();
                e.1 += pts;
                if *outcome == Outcome::Pass {
                    earned += pts;
                    e.0 += pts;
                } else {
                    lost.push((rule.rule.id, pts));
                }
            }
            Outcome::NotApplicable | Outcome::NotAvailable => {}
        }
    }
    let mut score = if possible == 0 {
        0
    } else {
        round_div(earned * 100, possible) as u32
    };
    let capped = blocked_for_all_search && score > blocked_cap;
    if capped {
        score = blocked_cap;
    }
    lost.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
    let parts = Part::PAGE
        .iter()
        .map(|&part| {
            let max = weights.of(part);
            let score = per_part
                .get(&part)
                .filter(|(_, p)| *p > 0)
                .map(|(e, p)| round_div(e * u64::from(max), *p) as u32);
            PartScore { part, score, max }
        })
        .collect();
    PageScore {
        score,
        parts,
        checks_used: used,
        checks_total: total,
        provisional: used * 2 < total,
        capped,
        lost,
    }
}

/// Site score: 70% the mean page score, 30% site checks (FR-037).
pub fn site_score(page_scores: &[u32], site_outcomes: &[(&'static AeoRule, Outcome)]) -> (u32, u32, u32) {
    let pages = if page_scores.is_empty() {
        0
    } else {
        round_div(
            page_scores.iter().map(|&s| u64::from(s)).sum(),
            page_scores.len() as u64,
        ) as u32
    };
    let (mut earned, mut possible) = (0u64, 0u64);
    for (rule, outcome) in site_outcomes.iter().filter(|(r, _)| r.part == Part::Site) {
        if matches!(outcome, Outcome::Pass | Outcome::Fail) {
            possible += u64::from(rule.points);
            if *outcome == Outcome::Pass {
                earned += u64::from(rule.points);
            }
        }
    }
    let site = if possible == 0 {
        100
    } else {
        round_div(earned * 100, possible) as u32
    };
    let total = round_div(u64::from(pages) * 70 + u64::from(site) * 30, 100) as u32;
    (total, pages, site)
}

/// One entry of the top fixes list (FR-039).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Fix {
    pub rule_id: String,
    pub pages: u64,
    /// Site score points gained if fixed everywhere, in hundredths.
    pub gain: u64,
}

/// `page_losses`: per rule, the sum over pages of points lost (hundredths of
/// page-score points, i.e. already divided by each page's possible points)
/// and the number of pages. `site_losses`: site rules that failed.
pub fn top_fixes(
    page_losses: &BTreeMap<&'static str, (u64, u64)>,
    pages_scored: u64,
    site_losses: &[&'static AeoRule],
    limit: usize,
) -> Vec<Fix> {
    let mut fixes: Vec<Fix> = page_losses
        .iter()
        .filter(|(_, (_, n))| *n > 0)
        .map(|(id, (lost, n))| Fix {
            rule_id: (*id).to_string(),
            pages: *n,
            gain: if pages_scored == 0 {
                0
            } else {
                lost * 70 / (100 * pages_scored)
            },
        })
        .collect();
    let site_possible: u64 = crate::rules::RULES
        .iter()
        .filter(|r| r.part == Part::Site)
        .map(|r| u64::from(r.points))
        .sum();
    for r in site_losses {
        fixes.push(Fix {
            rule_id: r.rule.id.to_string(),
            pages: 0,
            gain: u64::from(r.points) * 100 * 30 / site_possible.max(1),
        });
    }
    fixes.sort_by(|a, b| {
        b.gain
            .cmp(&a.gain)
            .then(b.pages.cmp(&a.pages))
            .then(a.rule_id.cmp(&b.rule_id))
    });
    fixes.truncate(limit);
    fixes
}

/// Integer division rounding half up.
fn round_div(n: u64, d: u64) -> u64 {
    (n + d / 2) / d
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::{RULES, by_id};

    fn all_page(outcome: Outcome) -> Vec<(&'static AeoRule, Outcome)> {
        RULES
            .iter()
            .filter(|r| r.part != Part::Site && r.rule.id != "ai.readable.js_suspected")
            .map(|r| (r, outcome))
            .collect()
    }

    fn set(
        mut v: Vec<(&'static AeoRule, Outcome)>,
        id: &str,
        o: Outcome,
    ) -> Vec<(&'static AeoRule, Outcome)> {
        v.iter_mut().find(|(r, _)| r.rule.id == id).unwrap().1 = o;
        v
    }

    #[test]
    fn perfect_page() {
        // US6.1
        let s = page_score(&all_page(Outcome::Pass), &Weights::default(), false, 20);
        assert_eq!(s.score, 100);
        assert!(s.parts.iter().all(|p| p.score == Some(p.max)));
        assert!(s.lost.is_empty());
        assert!(!s.provisional);
    }

    #[test]
    fn one_failure_loses_its_points() {
        // SC-007: score equals the formula.
        let s = page_score(
            &set(all_page(Outcome::Pass), "ai.answer.no_facts", Outcome::Fail),
            &Weights::default(),
            false,
            20,
        );
        assert_eq!(s.score, 95);
        assert_eq!(s.lost, [("ai.answer.no_facts", 500)]);
        assert_eq!(
            s.parts.iter().find(|p| p.part == Part::Answer).unwrap().score,
            Some(15)
        );
    }

    #[test]
    fn blocked_pages_are_capped() {
        // US6.2
        let s = page_score(
            &set(all_page(Outcome::Pass), "ai.access.search_blocked", Outcome::Fail),
            &Weights::default(),
            true,
            20,
        );
        assert_eq!(s.score, 20);
        assert!(s.capped);
    }

    #[test]
    fn unavailable_checks_are_left_out() {
        // US6.3
        let mut v = all_page(Outcome::Pass);
        for (r, o) in v.iter_mut() {
            if r.rule.needs.contains(&crate::rules::RuleNeed::RawHtml) {
                *o = Outcome::NotAvailable;
            }
        }
        let s = page_score(&v, &Weights::default(), false, 20);
        assert_eq!(s.score, 100);
        assert!(s.checks_used < s.checks_total);
        let none = page_score(&all_page(Outcome::NotAvailable), &Weights::default(), false, 20);
        assert!(none.provisional);
        assert_eq!(none.score, 0);
    }

    #[test]
    fn not_applicable_checks_do_not_count() {
        let mut v = all_page(Outcome::Pass);
        for (r, o) in v.iter_mut() {
            if r.part == Part::Trust {
                *o = Outcome::NotApplicable;
            }
        }
        let s = page_score(&v, &Weights::default(), false, 20);
        assert_eq!(s.score, 100);
        assert_eq!(
            s.parts.iter().find(|p| p.part == Part::Trust).unwrap().score,
            None
        );
    }

    #[test]
    fn weights_change_scores() {
        let w = Weights {
            access: 20,
            readable: 20,
            answer: 20,
            trust: 20,
            structured: 20,
        };
        let s = page_score(
            &set(all_page(Outcome::Pass), "ai.structured.errors", Outcome::Fail),
            &w,
            false,
            20,
        );
        // errors = 4 of 10 structured points; at weight 20 that is 8 of 100.
        assert_eq!(s.score, 92);
    }

    #[test]
    fn site_score_is_70_30() {
        let site: Vec<_> = RULES
            .iter()
            .filter(|r| r.part == Part::Site)
            .map(|r| (r, Outcome::Pass))
            .collect();
        assert_eq!(site_score(&[100, 50], &site), (83, 75, 100));
        let failed = site
            .iter()
            .map(|(r, _)| {
                (
                    *r,
                    if r.rule.id == "ai.site.llms_txt" {
                        Outcome::Fail
                    } else {
                        Outcome::Pass
                    },
                )
            })
            .collect::<Vec<_>>();
        // llms.txt is 2 of 30 site points: at most 2 of the site score's 100 (FR-046).
        let (total, _, site_part) = site_score(&[100], &failed);
        assert_eq!((total, site_part), (98, 93));
    }

    #[test]
    fn top_fixes_are_ordered() {
        let mut losses = BTreeMap::new();
        losses.insert("ai.answer.no_facts", (500 * 10, 10));
        losses.insert("ai.trust.no_author", (400 * 20, 20));
        let fixes = top_fixes(&losses, 100, &[by_id("ai.site.entity").unwrap()], 10);
        let ids: Vec<_> = fixes.iter().map(|f| f.rule_id.as_str()).collect();
        assert_eq!(
            ids,
            ["ai.site.entity", "ai.trust.no_author", "ai.answer.no_facts"]
        );
    }
}
