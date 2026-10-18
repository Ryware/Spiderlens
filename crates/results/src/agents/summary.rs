//! The Agents summary (spec 016 FR-018): the crawl's score and its parts,
//! what costs the most points, and the checks that could not run. Read from
//! the `agent_scores` pass's table, so it costs a few indexed reads.

use rusqlite::OptionalExtension;
use serde::Serialize;
use spiderlens_config::ai_bots::{self, AiBot, BotPurpose};
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

/// What robots.txt says to one user-fetch agent, over every host checked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum AgentRobots {
    Allowed,
    Blocked,
    NotChecked,
}

/// What the user agent probe found for one user-fetch agent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum AgentProbe {
    Ok,
    Blocked,
    Challenged,
    NotProbed,
}

/// Whether one AI agent that fetches pages for a person can reach the site
/// (spec 016 FR-013, FR-014), from spec 015's stored results. Names are the
/// registry's own text, not crawled content.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct AgentAccessRow {
    pub bot: String,
    pub vendor: String,
    pub purpose: BotPurpose,
    pub robots: AgentRobots,
    pub probe: AgentProbe,
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
    /// Each user-fetch agent of the registry; empty until the AI checks ran.
    pub access: Vec<AgentAccessRow>,
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
            access: Vec::new(),
        }
    }
}

/// The user-fetch agents with their robots.txt verdict (`token`, home
/// verdict, URLs blocked) and probe outcomes (`token`, outcome) over every
/// host. Blocked wins over challenged, and either over fine.
fn access_rows(
    bots: &[&AiBot],
    robots: &[(String, String, i64)],
    probes: &[(String, String)],
) -> Vec<AgentAccessRow> {
    bots.iter()
        .map(|b| {
            let mine: Vec<&(String, String, i64)> = robots.iter().filter(|r| r.0 == b.token).collect();
            let robots = if mine.iter().any(|r| r.1 == "blocked" || r.2 > 0) {
                AgentRobots::Blocked
            } else if mine.iter().any(|r| r.1 != "unknown") {
                AgentRobots::Allowed
            } else {
                AgentRobots::NotChecked
            };
            let outcomes: Vec<&str> = probes
                .iter()
                .filter(|p| p.0 == b.token)
                .map(|p| p.1.as_str())
                .collect();
            let probe = if outcomes.is_empty() {
                AgentProbe::NotProbed
            } else if outcomes.contains(&"blocked") {
                AgentProbe::Blocked
            } else if outcomes.contains(&"challenge") {
                AgentProbe::Challenged
            } else {
                AgentProbe::Ok
            };
            AgentAccessRow {
                bot: b.name.clone(),
                vendor: b.vendor.clone(),
                purpose: b.purpose,
                robots,
                probe,
            }
        })
        .collect()
}

fn round1(v: f64) -> f64 {
    (v * 10.0).round() / 10.0
}

impl Reader {
    /// The crawl's agent score and what drives it (spec 016 FR-018). Not
    /// available until the `agent_scores` pass has run on the crawl.
    /// The registry's user-fetch agents with what 015 stored for them.
    fn agent_access(&self) -> Result<Vec<AgentAccessRow>> {
        if !self.features.has(Need::AiChecks) {
            return Ok(Vec::new());
        }
        let bots: Vec<&AiBot> = ai_bots::active()
            .filter(|b| b.purpose == BotPurpose::UserFetch && !b.token_only)
            .collect();
        let robots = self
            .conn
            .prepare("SELECT bot, home_verdict, urls_blocked FROM aeo_bot_access")?
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
            .collect::<rusqlite::Result<Vec<(String, String, i64)>>>()?;
        let probes = self
            .conn
            .prepare("SELECT bot, outcome FROM aeo_probe")?
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<rusqlite::Result<Vec<(String, String)>>>()?;
        Ok(access_rows(&bots, &robots, &probes))
    }

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
            access: self.agent_access()?,
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

    fn bot(token: &str) -> AiBot {
        let mut b = ai_bots::active().next().expect("a bot").clone();
        b.token = token.into();
        b.name = format!("{token}-name");
        b.purpose = BotPurpose::UserFetch;
        b
    }

    #[test]
    fn access_rows_combine_robots_and_probe_results() {
        let (a, b, c) = (bot("a"), bot("b"), bot("c"));
        let robots = vec![
            ("a".to_string(), "allowed".to_string(), 0),
            ("b".to_string(), "allowed".to_string(), 2),
            ("c".to_string(), "unknown".to_string(), 0),
        ];
        let probes = vec![
            ("a".to_string(), "same".to_string()),
            ("a".to_string(), "challenge".to_string()),
            ("b".to_string(), "blocked".to_string()),
            ("b".to_string(), "challenge".to_string()),
        ];
        let rows = access_rows(&[&a, &b, &c], &robots, &probes);
        let got: Vec<(AgentRobots, AgentProbe)> = rows.iter().map(|r| (r.robots, r.probe)).collect();
        assert_eq!(
            got,
            [
                (AgentRobots::Allowed, AgentProbe::Challenged),
                (AgentRobots::Blocked, AgentProbe::Blocked),
                (AgentRobots::NotChecked, AgentProbe::NotProbed),
            ]
        );
        assert_eq!(rows[0].bot, "a-name");
    }

    #[test]
    fn a_probe_with_only_fine_outcomes_is_ok() {
        let a = bot("a");
        let rows = access_rows(&[&a], &[], &[("a".to_string(), "same".to_string())]);
        assert_eq!(
            (rows[0].robots, rows[0].probe),
            (AgentRobots::NotChecked, AgentProbe::Ok)
        );
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
