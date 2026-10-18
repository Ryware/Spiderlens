//! The Agents summary (spec 016 FR-018): the crawl's score and its parts,
//! what costs the most points, and the checks that could not run. Read from
//! the `agent_scores` pass's table, so it costs a few indexed reads.

use rusqlite::OptionalExtension;
use serde::Serialize;
use ts_rs::TS;

use super::pass::applicability;
use super::score::{self, AgentPart};
use crate::Result;
use crate::issues::RuleKind;
use crate::reader::Reader;
use crate::tabs::Need;

/// Failed rules listed, most points lost first.
pub const TOP_FAILED: usize = 10;

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct AgentPartScore {
    pub part: AgentPart,
    /// Share of the total this part carries when it is scored.
    pub weight: u32,
    /// 0 to 100; null when no rule of the part could be checked.
    pub score: Option<f64>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct AgentFailedRule {
    pub id: String,
    pub name: String,
    pub part: AgentPart,
    pub site_wide: bool,
    /// Pages that fail it; null for a site-wide rule.
    #[ts(type = "number | null")]
    pub pages: Option<u64>,
    /// Points off the crawl score (0 to 100) that this rule costs.
    pub points_lost: f64,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct AgentNotChecked {
    pub id: String,
    pub name: String,
    pub reason: String,
}

/// The Agents summary of one crawl.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct AgentsSummary {
    /// False when there is no score to show, with the reason in
    /// `unavailable_reason`.
    pub available: bool,
    pub unavailable_reason: Option<String>,
    /// The crawl's score, 0 to 100, one decimal.
    pub score: Option<f64>,
    pub parts: Vec<AgentPartScore>,
    /// Internal HTML pages that were scored.
    #[ts(type = "number")]
    pub pages_scored: u64,
    pub top_failed: Vec<AgentFailedRule>,
    pub not_checked: Vec<AgentNotChecked>,
    /// The weight table the score was made with.
    pub weights_version: Option<u32>,
}

impl AgentsSummary {
    fn unavailable(reason: &str) -> Self {
        Self {
            available: false,
            unavailable_reason: Some(reason.into()),
            score: None,
            parts: Vec::new(),
            pages_scored: 0,
            top_failed: Vec::new(),
            not_checked: Vec::new(),
            weights_version: None,
        }
    }
}

fn round1(v: f64) -> f64 {
    (v * 10.0).round() / 10.0
}

impl Reader {
    /// The crawl's agent score and what drives it (spec 016 FR-018). Not
    /// available until the `agent_scores` pass has run on the crawl.
    pub fn agents_summary(&self) -> Result<AgentsSummary> {
        if !self.features.has(Need::Agents) {
            return Ok(AgentsSummary::unavailable(Need::Agents.unavailable_reason()));
        }
        let crawl = self
            .conn
            .query_row(
                "SELECT total, access, discovery, reading, acting, failed_rules, weights_version
                 FROM agent_scores WHERE url_id IS NULL",
                [],
                |r| {
                    Ok((
                        score::Scores {
                            total: r.get(0)?,
                            access: r.get(1)?,
                            discovery: r.get(2)?,
                            reading: r.get(3)?,
                            acting: r.get(4)?,
                        },
                        r.get::<_, String>(5)?,
                        r.get::<_, u32>(6)?,
                    ))
                },
            )
            .optional()?;
        let Some((scores, failed_json, weights_version)) = crawl else {
            return Ok(AgentsSummary::unavailable(
                "Agent checks have not run on this crawl.",
            ));
        };
        let failed: Vec<String> = serde_json::from_str(&failed_json).unwrap_or_default();
        let pages_scored: u64 = self.conn.query_row(
            "SELECT COUNT(*) FROM agent_scores WHERE url_id IS NOT NULL",
            [],
            |r| r.get::<_, i64>(0),
        )? as u64;

        // Pages failing each page rule, from the page rows.
        let mut page_fails: std::collections::HashMap<String, u64> = std::collections::HashMap::new();
        {
            let mut stmt = self.conn.prepare(
                "SELECT j.value, COUNT(*) FROM agent_scores s, json_each(s.failed_rules) j
                 WHERE s.url_id IS NOT NULL GROUP BY j.value",
            )?;
            let mut rows = stmt.query([])?;
            while let Some(r) = rows.next()? {
                page_fails.insert(r.get(0)?, r.get::<_, i64>(1)? as u64);
            }
        }

        let rules = applicability(&self.conn, &self.features)?;
        let checked_ids: Vec<&str> = rules.checked.iter().map(|r| r.id).collect();
        let mut top_failed: Vec<AgentFailedRule> = Vec::new();
        for r in &rules.checked {
            let Some((part, _)) = score::rule_weight(r.id) else {
                continue;
            };
            let site_wide = r.kind == RuleKind::SiteWide;
            let (pages, share) = if site_wide {
                (
                    None,
                    if failed.iter().any(|f| f == r.id) {
                        1.0
                    } else {
                        0.0
                    },
                )
            } else {
                let n = page_fails.get(r.id).copied().unwrap_or(0);
                (
                    Some(n),
                    if pages_scored > 0 {
                        n as f64 / pages_scored as f64
                    } else {
                        0.0
                    },
                )
            };
            if share <= 0.0 {
                continue;
            }
            top_failed.push(AgentFailedRule {
                id: r.id.into(),
                name: r.name.into(),
                part,
                site_wide,
                pages,
                points_lost: round1(score::points_lost(r.id, share, &checked_ids)),
            });
        }
        top_failed.sort_by(|a, b| {
            b.points_lost
                .total_cmp(&a.points_lost)
                .then_with(|| a.id.cmp(&b.id))
        });
        top_failed.truncate(TOP_FAILED);

        Ok(AgentsSummary {
            available: true,
            unavailable_reason: None,
            score: scores.total.map(round1),
            parts: AgentPart::ALL
                .iter()
                .map(|p| AgentPartScore {
                    part: *p,
                    weight: p.weight(),
                    score: scores.part(*p).map(round1),
                })
                .collect(),
            pages_scored,
            top_failed,
            not_checked: rules
                .not_checked
                .iter()
                .map(|(r, why)| AgentNotChecked {
                    id: r.id.into(),
                    name: r.name.into(),
                    reason: why.clone(),
                })
                .collect(),
            weights_version: Some(weights_version),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rounds_to_one_decimal() {
        assert_eq!(round1(69.69697), 69.7);
        assert_eq!(round1(100.0), 100.0);
    }

    #[test]
    fn unavailable_says_why() {
        let s = AgentsSummary::unavailable("Agent checks have not run on this crawl.");
        assert!(!s.available);
        assert_eq!(
            s.unavailable_reason.as_deref(),
            Some("Agent checks have not run on this crawl.")
        );
        assert!(s.score.is_none() && s.parts.is_empty() && s.top_failed.is_empty());
    }
}
