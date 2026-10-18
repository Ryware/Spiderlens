//! The agent readiness score (spec 016 plan D5): which part each rule
//! belongs to, how much it weighs, and the formula. Pure functions, no
//! database: the `agent_scores` pass feeds them the outcomes it found.
//!
//! - a part's score is `100 x (1 - failed weight / checked weight)` over the
//!   rules checked in that part;
//! - the total is the parts weighted Access 30, Reading 25, Acting 30 and
//!   Discovery 15, over the parts that have at least one checked rule with
//!   a weight above zero;
//! - a rule that could not be checked (missing data) is left out of both
//!   sums, so it neither helps nor hurts.
//!
//! A rule's outcome is the share of what it looks at that fails, from 0 to
//! 1: a page rule on one page is 0 or 1, the same rule over a crawl is the
//! share of pages it fails on, and a site-wide rule is 0 or 1. The total of
//! a crawl is then the mean of its page totals when every page checks the
//! same rules, which is the case within one crawl.

use serde::Serialize;
use ts_rs::TS;

/// Increases whenever a weight or a part weight changes. Stored with every
/// score row, so an older file is recognised and evaluated again.
pub const WEIGHTS_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum AgentPart {
    /// Can agents get in at all: robots.txt, challenges.
    Access,
    /// Can they find out what the site offers: catalogs, cards, llms.txt.
    Discovery,
    /// Can they read the content: HTML they see, Markdown.
    Reading,
    /// Can they act: forms, buttons and links with names.
    Acting,
}

impl AgentPart {
    pub const ALL: [AgentPart; 4] = [Self::Access, Self::Discovery, Self::Reading, Self::Acting];

    /// Share of the total (research R7).
    pub fn weight(self) -> u32 {
        match self {
            Self::Access => 30,
            Self::Discovery => 15,
            Self::Reading => 25,
            Self::Acting => 30,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Access => "Access",
            Self::Discovery => "Discovery",
            Self::Reading => "Reading",
            Self::Acting => "Acting",
        }
    }

    fn index(self) -> usize {
        Self::ALL.iter().position(|p| *p == self).unwrap_or(0)
    }
}

/// Part and weight of every rule in the `agents` category, version
/// [`WEIGHTS_VERSION`]. Rules not built yet are listed too, so their weight
/// is fixed before they land (data-model.md "Rules").
const WEIGHTS: &[(&str, AgentPart, u32)] = &[
    ("agents.form_rendered_only", AgentPart::Acting, 8),
    ("agents.unlabelled_fields", AgentPart::Acting, 5),
    ("agents.placeholder_only_fields", AgentPart::Acting, 2),
    ("agents.nameless_buttons", AgentPart::Acting, 4),
    ("agents.actions_without_link", AgentPart::Acting, 4),
    ("agents.form_challenge", AgentPart::Acting, 5),
    ("agents.form_no_submit", AgentPart::Acting, 3),
    ("agents.tool_missing_description", AgentPart::Acting, 1),
    ("agents.no_search_action", AgentPart::Acting, 1),
    ("agents.search_action_out_of_scope", AgentPart::Acting, 2),
    ("agents.content_rendered_only", AgentPart::Reading, 10),
    ("agents.nav_rendered_only", AgentPart::Reading, 5),
    ("agents.no_markdown_alternate", AgentPart::Reading, 1),
    ("agents.markdown_alternate_broken", AgentPart::Reading, 2),
    ("agents.prompt_injection_text", AgentPart::Reading, 0),
    ("agents.robots_blocks_agents", AgentPart::Access, 10),
    ("agents.probe_blocked", AgentPart::Access, 10),
    ("agents.bot_challenge_page", AgentPart::Access, 8),
    ("agents.entry_point_invalid", AgentPart::Discovery, 4),
    ("agents.mcp_endpoint_unreachable", AgentPart::Discovery, 4),
    ("agents.no_entry_points", AgentPart::Discovery, 2),
    ("agents.llms_txt_problems", AgentPart::Discovery, 2),
];

/// The part and weight of a rule, `None` for a rule that is not an agent
/// rule.
pub fn rule_weight(id: &str) -> Option<(AgentPart, u32)> {
    WEIGHTS.iter().find(|(r, ..)| *r == id).map(|(_, p, w)| (*p, *w))
}

/// Every rule id with a weight, in table order.
pub fn weighted_rules() -> impl Iterator<Item = &'static str> {
    WEIGHTS.iter().map(|(id, ..)| *id)
}

/// Scores from 0 to 100; `None` where nothing was checked.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Scores {
    pub total: Option<f64>,
    pub access: Option<f64>,
    pub discovery: Option<f64>,
    pub reading: Option<f64>,
    pub acting: Option<f64>,
}

impl Scores {
    pub fn part(&self, part: AgentPart) -> Option<f64> {
        match part {
            AgentPart::Access => self.access,
            AgentPart::Discovery => self.discovery,
            AgentPart::Reading => self.reading,
            AgentPart::Acting => self.acting,
        }
    }
}

/// `100 x (1 - failed / checked)` over `(weight, failed share)` pairs;
/// `None` when the weights add up to nothing.
pub fn part_score(rules: &[(u32, f64)]) -> Option<f64> {
    let checked: f64 = rules.iter().map(|(w, _)| f64::from(*w)).sum();
    if checked <= 0.0 {
        return None;
    }
    let failed: f64 = rules.iter().map(|(w, s)| f64::from(*w) * s.clamp(0.0, 1.0)).sum();
    Some(100.0 * (1.0 - failed / checked))
}

/// The total over the parts that have a score.
pub fn total_score(parts: &[(AgentPart, Option<f64>)]) -> Option<f64> {
    let (sum, weights) = parts
        .iter()
        .filter_map(|(p, s)| s.map(|s| (f64::from(p.weight()) * s, f64::from(p.weight()))))
        .fold((0.0, 0.0), |(a, b), (x, y)| (a + x, b + y));
    (weights > 0.0).then(|| sum / weights)
}

/// Scores the checked rules given as `(rule id, failed share)`. Ids that
/// are not agent rules are ignored.
pub fn score<'a>(outcomes: impl IntoIterator<Item = (&'a str, f64)>) -> Scores {
    let mut by_part: [Vec<(u32, f64)>; 4] = Default::default();
    for (id, share) in outcomes {
        if let Some((part, weight)) = rule_weight(id) {
            by_part[part.index()].push((weight, share));
        }
    }
    let part = |p: AgentPart| part_score(&by_part[p.index()]);
    let parts = AgentPart::ALL.map(|p| (p, part(p)));
    Scores {
        total: total_score(&parts),
        access: part(AgentPart::Access),
        discovery: part(AgentPart::Discovery),
        reading: part(AgentPart::Reading),
        acting: part(AgentPart::Acting),
    }
}

/// How many points of the total (0 to 100) a rule takes off when `share` of
/// what it looks at fails, given the rules that were checked. The points of
/// all failed rules add up to `100 - total`.
pub fn points_lost(rule: &str, share: f64, checked: &[&str]) -> f64 {
    let Some((part, weight)) = rule_weight(rule) else {
        return 0.0;
    };
    let mut part_weight = [0u32; 4];
    for id in checked {
        if let Some((p, w)) = rule_weight(id) {
            part_weight[p.index()] += w;
        }
    }
    let present: u32 = AgentPart::ALL
        .iter()
        .filter(|p| part_weight[p.index()] > 0)
        .map(|p| p.weight())
        .sum();
    let in_part = part_weight[part.index()];
    if present == 0 || in_part == 0 {
        return 0.0;
    }
    100.0 * f64::from(part.weight()) / f64::from(present) * f64::from(weight) / f64::from(in_part)
        * share.clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: Option<f64>, b: f64) {
        let a = a.expect("a score");
        assert!((a - b).abs() < 1e-9, "{a} is not {b}");
    }

    #[test]
    fn weights_cover_the_catalogue_and_nothing_else_is_missing() {
        for r in crate::issues::catalogue()
            .iter()
            .filter(|r| r.category == crate::issues::Category::Agents)
        {
            assert!(rule_weight(r.id).is_some(), "{} has no weight", r.id);
        }
        let mut seen = std::collections::HashSet::new();
        for id in weighted_rules() {
            assert!(id.starts_with("agents."), "{id}");
            assert!(seen.insert(id), "{id} twice");
        }
        assert_eq!(AgentPart::ALL.iter().map(|p| p.weight()).sum::<u32>(), 100);
    }

    #[test]
    fn nothing_failed_scores_100() {
        let s = score([
            ("agents.unlabelled_fields", 0.0),
            ("agents.content_rendered_only", 0.0),
        ]);
        close(s.total, 100.0);
        close(s.acting, 100.0);
        close(s.reading, 100.0);
        assert_eq!((s.access, s.discovery), (None, None));
    }

    #[test]
    fn a_part_is_the_failed_share_of_its_checked_weight() {
        // Acting: unlabelled (5) fails, placeholder (2) and nameless (4)
        // pass: 100 x (1 - 5/11).
        let s = score([
            ("agents.unlabelled_fields", 1.0),
            ("agents.placeholder_only_fields", 0.0),
            ("agents.nameless_buttons", 0.0),
        ]);
        close(s.acting, 100.0 * 6.0 / 11.0);
        // Only one part has checked rules, so it is the total.
        close(s.total, 100.0 * 6.0 / 11.0);
    }

    #[test]
    fn the_total_weights_the_parts_that_were_checked() {
        // Acting: 5 failed of 5 + 4 = 44.444; Reading: all pass = 100.
        // (30 x 44.444 + 25 x 100) / 55 = 69.697.
        let s = score([
            ("agents.unlabelled_fields", 1.0),
            ("agents.nameless_buttons", 0.0),
            ("agents.content_rendered_only", 0.0),
            ("agents.no_markdown_alternate", 0.0),
        ]);
        close(s.acting, 100.0 * 4.0 / 9.0);
        close(s.reading, 100.0);
        close(s.total, (30.0 * (100.0 * 4.0 / 9.0) + 25.0 * 100.0) / 55.0);
    }

    #[test]
    fn a_rule_failing_on_half_the_pages_takes_half_its_weight() {
        // nameless buttons (4) fail on half the pages, unlabelled (5) on
        // none: 100 x (1 - 2/9).
        let s = score([
            ("agents.unlabelled_fields", 0.0),
            ("agents.nameless_buttons", 0.5),
        ]);
        close(s.acting, 100.0 * 7.0 / 9.0);
    }

    #[test]
    fn all_four_parts_use_the_stated_weights() {
        // Access 0 (bot challenge fails), Discovery 100, Reading 33.33
        // (content, 10, fails and nav, 5, passes: 100 x 5/15), Acting 100.
        let s = score([
            ("agents.bot_challenge_page", 1.0),
            ("agents.no_entry_points", 0.0),
            ("agents.content_rendered_only", 1.0),
            ("agents.nav_rendered_only", 0.0),
            ("agents.form_no_submit", 0.0),
        ]);
        close(s.access, 0.0);
        close(s.discovery, 100.0);
        close(s.reading, 100.0 * 5.0 / 15.0);
        close(s.acting, 100.0);
        // (30 x 0 + 15 x 100 + 25 x 33.33 + 30 x 100) / 100.
        close(
            s.total,
            (15.0 * 100.0 + 25.0 * (100.0 / 3.0) + 30.0 * 100.0) / 100.0,
        );
    }

    #[test]
    fn nothing_checked_and_weightless_rules_score_nothing() {
        assert_eq!(score([]), Scores::default());
        // Weight 0: informational, never a score.
        let s = score([("agents.prompt_injection_text", 1.0)]);
        assert_eq!(s, Scores::default());
        // Not an agent rule.
        assert_eq!(score([("title.over", 1.0)]), Scores::default());
    }

    #[test]
    fn points_lost_add_up_to_what_the_total_lacks() {
        let checked = [
            "agents.unlabelled_fields",
            "agents.nameless_buttons",
            "agents.content_rendered_only",
        ];
        // Acting 30 of 55, unlabelled 5 of 9: 100 x 30/55 x 5/9 = 30.303.
        let lost = points_lost("agents.unlabelled_fields", 1.0, &checked);
        assert!((lost - 100.0 * 30.0 / 55.0 * 5.0 / 9.0).abs() < 1e-9, "{lost}");
        let s = score([
            ("agents.unlabelled_fields", 1.0),
            ("agents.nameless_buttons", 0.0),
            ("agents.content_rendered_only", 0.0),
        ]);
        close(s.total, 100.0 - lost);
        // Half the pages: half the points. Unchecked or unknown: none.
        assert!((points_lost("agents.unlabelled_fields", 0.5, &checked) - lost / 2.0).abs() < 1e-9);
        assert_eq!(points_lost("agents.bot_challenge_page", 1.0, &checked), 0.0);
        assert_eq!(points_lost("title.over", 1.0, &checked), 0.0);
    }
}
