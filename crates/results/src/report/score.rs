//! The site health score (spec 010 FR-008 to FR-012, plan D3).
//!
//! Each internal HTML page that returned 2xx loses the weight of its worst
//! enabled issue; the score is 100 times one minus the average loss. The
//! hits are walked in `url_id` order, one page at a time, so memory does
//! not grow with the crawl.

use std::collections::HashMap;

use serde::Serialize;
use ts_rs::TS;

use crate::Result;
use crate::issues::{IssuesSummary, Priority, catalogue, display_name};
use crate::reader::Reader;
use crate::tabs::{Derived, HTML_OK};

/// Bump when the formula or the weights change, so scores of different
/// versions are never compared as equals (FR-011, spec 012).
pub const SCORE_VERSION: u32 = 1;

/// Points a page loses for its worst issue, by priority.
pub const WEIGHTS: [(Priority, f64); 4] = [
    (Priority::High, 1.0),
    (Priority::Medium, 0.5),
    (Priority::Low, 0.2),
    (Priority::Opportunity, 0.0),
];

pub fn weight(p: Priority) -> f64 {
    WEIGHTS.iter().find(|(q, _)| *q == p).map_or(0.0, |(_, w)| *w)
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct Score {
    /// 0 to 100; null when there are no pages to score (FR-012).
    pub value: Option<u8>,
    pub unavailable: Option<String>,
    /// Some enabled rules could not run on this crawl file (FR-012).
    pub partial: bool,
    /// Pages scored: internal HTML pages that returned 2xx.
    #[ts(type = "number")]
    pub pages: u64,
    /// Points lost per priority; they add up to 100 minus the score.
    pub by_priority: Vec<PriorityLoss>,
    /// Issues on scored pages, the costliest first (FR-009).
    pub issues: Vec<IssueCost>,
    /// Names of enabled rules that could not run on this file.
    pub not_run: Vec<String>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct PriorityLoss {
    pub priority: Priority,
    /// Score points lost to pages whose worst issue has this priority.
    pub points: f64,
    #[ts(type = "number")]
    pub pages: u64,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct IssueCost {
    pub id: String,
    pub name: String,
    pub priority: Priority,
    /// Scored pages with this issue.
    #[ts(type = "number")]
    pub pages: u64,
    /// This issue's part of the points lost: each page's loss split equally
    /// among its issues of the worst priority. Shares add up exactly.
    pub share: f64,
    /// Points the score would gain if no page had this issue.
    pub gain: f64,
}

#[derive(Default)]
struct Acc {
    pages: u64,
    share: f64,
    gain: f64,
}

/// The arithmetic of one page, kept apart from SQL so it can be tested on
/// its own. `hits` are (rule index, priority) of one page.
fn page_loss(hits: &[(usize, Priority)], per_rule: &mut HashMap<usize, Acc>, by_prio: &mut [(f64, u64); 4]) {
    let Some(worst) = hits.iter().map(|h| h.1).min() else {
        return;
    };
    let penalty = weight(worst);
    let at_worst = hits.iter().filter(|h| h.1 == worst).count();
    // The next worst priority among the other rules, for the gain.
    let mut seen = Vec::with_capacity(hits.len());
    for (rule, prio) in hits {
        if seen.contains(rule) {
            continue;
        }
        seen.push(*rule);
        let acc = per_rule.entry(*rule).or_default();
        acc.pages += 1;
        if *prio == worst {
            acc.share += penalty / at_worst as f64;
            if at_worst == 1 {
                let next = hits
                    .iter()
                    .filter(|h| h.0 != *rule)
                    .map(|h| h.1)
                    .min()
                    .map_or(0.0, weight);
                acc.gain += penalty - next;
            }
        }
    }
    let slot = &mut by_prio[worst as usize];
    slot.0 += penalty;
    slot.1 += 1;
}

/// Turns sums of page losses into score points.
fn finish(
    pages: u64,
    per_rule: HashMap<usize, Acc>,
    by_prio: [(f64, u64); 4],
    summary: &IssuesSummary,
    settings: &crate::issues::ResolvedSettings,
) -> Score {
    let not_run: Vec<String> = summary
        .issues
        .iter()
        .filter(|i| i.enabled && i.unavailable.is_some())
        .map(|i| i.name.clone())
        .collect();
    if pages == 0 {
        return Score {
            value: None,
            unavailable: Some("There are no internal HTML pages that returned 2xx to score.".into()),
            partial: !not_run.is_empty(),
            pages: 0,
            by_priority: Vec::new(),
            issues: Vec::new(),
            not_run,
        };
    }
    let points = |loss: f64| loss * 100.0 / pages as f64;
    let by_priority: Vec<PriorityLoss> = Priority::ALL
        .into_iter()
        .map(|p| PriorityLoss {
            priority: p,
            points: points(by_prio[p as usize].0),
            pages: by_prio[p as usize].1,
        })
        .collect();
    let lost: f64 = by_priority.iter().map(|p| p.points).sum();
    let value = (100.0 - lost).round().clamp(0.0, 100.0) as u8;
    let mut issues: Vec<IssueCost> = per_rule
        .into_iter()
        .map(|(i, acc)| {
            let r = &catalogue()[i];
            let s = settings.rules[i];
            IssueCost {
                id: r.id.into(),
                name: display_name(r, s.threshold),
                priority: s.priority,
                pages: acc.pages,
                share: points(acc.share),
                gain: points(acc.gain),
            }
        })
        .collect();
    issues.sort_by(|a, b| {
        b.gain
            .total_cmp(&a.gain)
            .then(b.share.total_cmp(&a.share))
            .then(a.priority.cmp(&b.priority))
            .then(b.pages.cmp(&a.pages))
            .then(a.id.cmp(&b.id))
    });
    Score {
        value: Some(value),
        unavailable: None,
        partial: !not_run.is_empty(),
        pages,
        by_priority,
        issues,
        not_run,
    }
}

impl Reader {
    /// The score for the issue settings in force (FR-008 to FR-012).
    pub fn score(&mut self, summary: &IssuesSummary) -> Result<Score> {
        self.ensure_derived([Derived::IssueHits])?;
        let pages: i64 =
            self.conn
                .query_row(&format!("SELECT COUNT(*) FROM urls u WHERE {HTML_OK}"), [], |r| {
                    r.get(0)
                })?;
        let mut per_rule: HashMap<usize, Acc> = HashMap::new();
        let mut by_prio = [(0.0, 0u64); 4];
        {
            let mut stmt = self.conn.prepare(&format!(
                "SELECT h.url_id, r.prio, h.rule FROM temp.issue_hits h \
                 JOIN temp.issue_rules r ON r.idx = h.rule \
                 JOIN urls u ON u.id = h.url_id \
                 WHERE {HTML_OK} ORDER BY h.url_id"
            ))?;
            let mut rows = stmt.query([])?;
            let mut current: Option<i64> = None;
            let mut hits: Vec<(usize, Priority)> = Vec::new();
            while let Some(row) = rows.next()? {
                let url: i64 = row.get(0)?;
                let prio = Priority::ALL[(row.get::<_, i64>(1)? as usize).min(3)];
                let rule = row.get::<_, i64>(2)? as usize;
                if current != Some(url) {
                    page_loss(&hits, &mut per_rule, &mut by_prio);
                    hits.clear();
                    current = Some(url);
                }
                hits.push((rule, prio));
            }
            page_loss(&hits, &mut per_rule, &mut by_prio);
        }
        Ok(finish(
            pages as u64,
            per_rule,
            by_prio,
            summary,
            &self.issue_settings,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::issues::ResolvedSettings;

    fn empty_summary() -> IssuesSummary {
        IssuesSummary {
            version: 1,
            rules_version: 1,
            by_priority: Vec::new(),
            issues: Vec::new(),
        }
    }

    fn run(pages: u64, page_hits: &[&[(usize, Priority)]]) -> Score {
        let mut per_rule = HashMap::new();
        let mut by_prio = [(0.0, 0u64); 4];
        for h in page_hits {
            page_loss(h, &mut per_rule, &mut by_prio);
        }
        finish(
            pages,
            per_rule,
            by_prio,
            &empty_summary(),
            &ResolvedSettings::default(),
        )
    }

    use Priority::*;

    #[test]
    fn no_issues_is_100() {
        // US2 scenario 1.
        assert_eq!(run(10, &[]).value, Some(100));
    }

    #[test]
    fn worst_issue_counts_once_per_page() {
        // One page of four with a High and a Low issue: loses 1.0 / 4.
        let s = run(4, &[&[(0, High), (5, Low)]]);
        assert_eq!(s.value, Some(75));
        assert_eq!(s.by_priority[0].points, 25.0);
        assert_eq!(s.by_priority[2].points, 0.0);
    }

    #[test]
    fn shares_add_up_to_points_lost() {
        // US2 scenario 2, SC-008.
        let s = run(
            7,
            &[
                &[(0, High), (1, High), (2, Medium)],
                &[(2, Medium)],
                &[(3, Low), (4, Opportunity)],
                &[(1, High)],
            ],
        );
        let lost: f64 = s.by_priority.iter().map(|p| p.points).sum();
        let shares: f64 = s.issues.iter().map(|i| i.share).sum();
        assert!((lost - shares).abs() < 1e-9);
        assert!((100.0 - lost - s.value.unwrap() as f64).abs() <= 0.5);
    }

    #[test]
    fn gain_is_what_fixing_one_issue_returns() {
        // US2 scenario 3: removing rule 2 changes the score by its gain.
        let before: &[&[(usize, Priority)]] = &[
            &[(2, Medium), (3, Low)],
            &[(2, Medium)],
            &[(1, High), (2, Medium)],
        ];
        let after: &[&[(usize, Priority)]] = &[&[(3, Low)], &[], &[(1, High)]];
        let a = run(5, before);
        let b = run(5, after);
        let lost = |s: &Score| s.by_priority.iter().map(|p| p.points).sum::<f64>();
        let gain = a.issues.iter().find(|i| i.id == catalogue()[2].id).unwrap().gain;
        assert!((lost(&a) - lost(&b) - gain).abs() < 1e-9);
        // Top issues are ordered by gain.
        assert!(a.issues.windows(2).all(|w| w[0].gain >= w[1].gain));
    }

    #[test]
    fn two_issues_at_the_worst_priority_gain_nothing_alone() {
        let s = run(2, &[&[(0, High), (1, High)]]);
        assert!(s.issues.iter().all(|i| i.gain == 0.0));
        assert!(s.issues.iter().all(|i| (i.share - 25.0).abs() < 1e-9));
    }

    #[test]
    fn no_pages_is_not_available() {
        // Edge case: empty crawls never score 100.
        let s = run(0, &[]);
        assert_eq!(s.value, None);
        assert!(s.unavailable.is_some());
    }

    #[test]
    fn opportunity_never_lowers_the_score() {
        assert_eq!(run(1, &[&[(4, Opportunity)]]).value, Some(100));
    }

    #[test]
    fn score_stays_in_range() {
        assert_eq!(run(3, &[&[(0, High)], &[(0, High)], &[(0, High)]]).value, Some(0));
    }
}
