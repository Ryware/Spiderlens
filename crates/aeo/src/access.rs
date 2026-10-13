//! AI crawler access from robots.txt (spec 015 FR-009, FR-011).
//!
//! Uses the crawler's own robots.txt matcher (`spiderlens_engine::robots`),
//! with each AI crawler's token and documented fallbacks, so SpiderLens and
//! the access table can never disagree about what a rule means.

use std::collections::BTreeMap;

use serde::Serialize;
use spiderlens_config::ai_bots::{AiBot, BotPurpose, Honours};
use spiderlens_engine::robots::{RobotsPolicy, RobotsTxt};
use ts_rs::TS;
use url::Url;

/// What the crawl stored about a host's robots.txt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RobotsSource<'a> {
    /// Fetched: the file's text.
    Ok(&'a str),
    /// 4xx: no robots.txt, everything allowed.
    Missing,
    /// 5xx, 429 or unreachable: compliant crawlers fetch nothing.
    Unavailable,
    /// The crawl ignored robots.txt and did not fetch it.
    NotFetched,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum Verdict {
    Allowed,
    Blocked,
    /// robots.txt was unavailable.
    Unavailable,
    /// robots.txt was not fetched, so nothing can be said.
    Unknown,
}

/// One AI crawler's policy on one host.
pub struct BotPolicy<'b> {
    pub bot: &'b AiBot,
    /// `None` when the robots.txt state gives a fixed verdict for every URL.
    policy: Option<RobotsPolicy>,
    fixed: Option<Verdict>,
}

impl BotPolicy<'_> {
    /// The verdict for one URL, with the deciding rule when blocked.
    pub fn verdict(&self, url: &Url) -> (Verdict, Option<(u32, String)>) {
        if let Some(v) = self.fixed {
            return (v, None);
        }
        let policy = self.policy.as_ref().expect("policy set when not fixed");
        match policy.blocking_rule(url) {
            Some(r) => (Verdict::Blocked, Some((r.line_no, r.line))),
            None => (Verdict::Allowed, None),
        }
    }

    /// The `User-agent` value of the group used, or `*`.
    pub fn matched_group(&self) -> Option<&str> {
        self.policy.as_ref().and_then(|p| p.matched_agent.as_deref())
    }
}

/// Every crawler's policy on one host, built once and reused for all of the
/// host's URLs.
pub fn host_policies<'b>(source: RobotsSource<'_>, bots: &'b [AiBot]) -> Vec<BotPolicy<'b>> {
    let parsed = match source {
        RobotsSource::Ok(text) => Some(RobotsTxt::parse(text.as_bytes())),
        _ => None,
    };
    bots.iter()
        .filter(|b| !b.retired)
        .map(|bot| match (&parsed, source) {
            (Some(txt), _) => BotPolicy {
                bot,
                policy: Some(txt.for_agent_with_fallback(&bot.token, &bot.fallback_tokens)),
                fixed: None,
            },
            (None, RobotsSource::Missing) => BotPolicy {
                bot,
                policy: None,
                fixed: Some(Verdict::Allowed),
            },
            (None, RobotsSource::Unavailable) => BotPolicy {
                bot,
                policy: None,
                fixed: Some(Verdict::Unavailable),
            },
            (None, _) => BotPolicy {
                bot,
                policy: None,
                fixed: Some(Verdict::Unknown),
            },
        })
        .collect()
}

/// Per-host, per-crawler totals for the access table (`aeo_bot_access`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct BotAccess {
    pub bot: String,
    pub home_verdict: Verdict,
    pub matched_group: Option<String>,
    pub urls_total: u64,
    pub urls_blocked: u64,
    /// Deciding rules, most URLs first, then by line.
    pub rules: Vec<RuleCount>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct RuleCount {
    pub line_no: u32,
    pub rule: String,
    pub urls: u64,
}

/// Accumulates verdicts for one host's URLs.
pub struct HostTally<'b> {
    policies: Vec<BotPolicy<'b>>,
    totals: Vec<Totals>,
}

/// URLs seen, URLs blocked, and blocked URLs per deciding rule.
type Totals = (u64, u64, BTreeMap<(u32, String), u64>);

impl<'b> HostTally<'b> {
    pub fn new(policies: Vec<BotPolicy<'b>>) -> Self {
        let totals = policies.iter().map(|_| (0, 0, BTreeMap::new())).collect();
        Self { policies, totals }
    }

    pub fn policies(&self) -> &[BotPolicy<'b>] {
        &self.policies
    }

    /// Adds one URL; returns the bitmask of built-in crawlers that may not
    /// fetch it (bit = `AiBot.index`; custom crawlers are not in the mask).
    pub fn add(&mut self, url: &Url) -> u64 {
        let mut mask = 0u64;
        for (p, t) in self.policies.iter().zip(self.totals.iter_mut()) {
            t.0 += 1;
            let (v, rule) = p.verdict(url);
            if matches!(v, Verdict::Blocked | Verdict::Unavailable) {
                t.1 += 1;
                if !p.bot.custom && p.bot.index < 64 {
                    mask |= 1 << p.bot.index;
                }
                if let Some(rule) = rule {
                    *t.2.entry(rule).or_default() += 1;
                }
            }
        }
        mask
    }

    /// The totals, with each crawler's verdict for `home`.
    pub fn finish(self, home: &Url) -> Vec<BotAccess> {
        self.policies
            .iter()
            .zip(self.totals)
            .map(|(p, (total, blocked, rules))| {
                let mut rules: Vec<RuleCount> = rules
                    .into_iter()
                    .map(|((line_no, rule), urls)| RuleCount { line_no, rule, urls })
                    .collect();
                rules.sort_by(|a, b| b.urls.cmp(&a.urls).then(a.line_no.cmp(&b.line_no)));
                BotAccess {
                    bot: p.bot.token.clone(),
                    home_verdict: p.verdict(home).0,
                    matched_group: p.matched_group().map(str::to_string),
                    urls_total: total,
                    urls_blocked: blocked,
                    rules,
                }
            })
            .collect()
    }
}

/// How much of a host one purpose may fetch, for the per-vendor sentence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum PurposeAccess {
    Allowed,
    Partly,
    Blocked,
    Unknown,
}

/// Groups a host's access rows by vendor and purpose: "OpenAI: training
/// blocked, search allowed, user fetches allowed" (FR-011).
pub fn vendor_summary(bots: &[AiBot], rows: &[BotAccess]) -> Vec<(String, Vec<(BotPurpose, PurposeAccess)>)> {
    let mut by_vendor: BTreeMap<&str, BTreeMap<BotPurpose, PurposeAccess>> = BTreeMap::new();
    for row in rows {
        let Some(bot) = bots.iter().find(|b| b.token == row.bot) else {
            continue;
        };
        let access = if row.home_verdict == Verdict::Unknown {
            PurposeAccess::Unknown
        } else if row.urls_blocked == 0 && row.home_verdict == Verdict::Allowed {
            PurposeAccess::Allowed
        } else if (row.urls_total > 0 && row.urls_blocked >= row.urls_total)
            || (row.home_verdict != Verdict::Allowed && row.urls_total == 0)
        {
            PurposeAccess::Blocked
        } else {
            PurposeAccess::Partly
        };
        let slot = by_vendor
            .entry(&bot.vendor)
            .or_default()
            .entry(bot.purpose)
            .or_insert(access);
        // Several crawlers of one purpose: the most restrictive wins.
        *slot = worst(*slot, access);
    }
    by_vendor
        .into_iter()
        .map(|(v, m)| (v.to_string(), m.into_iter().collect()))
        .collect()
}

fn worst(a: PurposeAccess, b: PurposeAccess) -> PurposeAccess {
    use PurposeAccess::*;
    let rank = |x| match x {
        Allowed => 0,
        Unknown => 1,
        Partly => 2,
        Blocked => 3,
    };
    if rank(b) > rank(a) { b } else { a }
}

/// One line per vendor, for the overview and the CLI.
pub fn vendor_sentence(vendor: &str, parts: &[(BotPurpose, PurposeAccess)]) -> String {
    let words: Vec<String> = parts
        .iter()
        .map(|(purpose, access)| {
            let p = match purpose {
                BotPurpose::Training => "training",
                BotPurpose::Search => "search",
                BotPurpose::UserFetch => "user fetches",
                BotPurpose::Other => "other crawlers",
            };
            let a = match access {
                PurposeAccess::Allowed => "allowed",
                PurposeAccess::Partly => "partly blocked",
                PurposeAccess::Blocked => "blocked",
                PurposeAccess::Unknown => "unknown",
            };
            format!("{p} {a}")
        })
        .collect();
    format!("{vendor}: {}", words.join(", "))
}

/// The note shown next to a verdict for crawlers whose vendor says
/// robots.txt may not apply to them.
pub fn honours_note(bot: &AiBot) -> Option<&'static str> {
    match bot.honours_robots {
        Honours::MayNotApply => Some("robots.txt may not apply"),
        Honours::No => Some("documented as not following robots.txt"),
        Honours::Unknown => Some("robots.txt compliance not documented"),
        Honours::Yes => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use spiderlens_config::ai_bots;

    fn u(s: &str) -> Url {
        Url::parse(s).unwrap()
    }

    fn tally(robots: RobotsSource<'_>, urls: &[&str]) -> (Vec<u64>, Vec<BotAccess>) {
        let bots = ai_bots::all();
        let mut t = HostTally::new(host_policies(robots, bots));
        let masks = urls.iter().map(|x| t.add(&u(x))).collect();
        (masks, t.finish(&u("https://a.test/")))
    }

    fn row<'a>(rows: &'a [BotAccess], token: &str) -> &'a BotAccess {
        rows.iter().find(|r| r.bot == token).unwrap()
    }

    #[test]
    fn gptbot_only_block() {
        // US1.1
        let (masks, rows) = tally(
            RobotsSource::Ok("User-agent: GPTBot\nDisallow: /\n"),
            &["https://a.test/", "https://a.test/a"],
        );
        let gpt = row(&rows, "gptbot");
        assert_eq!(
            (gpt.home_verdict, gpt.urls_blocked, gpt.urls_total),
            (Verdict::Blocked, 2, 2)
        );
        assert_eq!(
            gpt.rules,
            [RuleCount {
                line_no: 2,
                rule: "Disallow: /".into(),
                urls: 2
            }]
        );
        for other in [
            "oai-searchbot",
            "chatgpt-user",
            "claudebot",
            "perplexitybot",
            "googlebot",
        ] {
            assert_eq!(row(&rows, other).urls_blocked, 0, "{other}");
        }
        let gpt_bit = 1 << ai_bots::by_token("gptbot").unwrap().index;
        assert_eq!(masks, [gpt_bit, gpt_bit]);
        let summary = vendor_summary(ai_bots::all(), &rows);
        let openai = summary.iter().find(|(v, _)| v == "OpenAI").unwrap();
        assert_eq!(
            vendor_sentence(&openai.0, &openai.1),
            "OpenAI: training blocked, search allowed, user fetches allowed, other crawlers allowed"
        );
    }

    #[test]
    fn wildcard_blocks_everyone() {
        // US1.2
        let (_, rows) = tally(
            RobotsSource::Ok("User-agent: *\nDisallow: /\n"),
            &["https://a.test/x"],
        );
        assert!(
            rows.iter()
                .all(|r| r.urls_blocked == 1 && r.matched_group.as_deref() == Some("*"))
        );
    }

    #[test]
    fn claudebot_path_block_is_partial() {
        // US1.3
        let (_, rows) = tally(
            RobotsSource::Ok("User-agent: ClaudeBot\nDisallow: /blog/\n"),
            &[
                "https://a.test/",
                "https://a.test/blog/1",
                "https://a.test/blog/2",
                "https://a.test/about",
            ],
        );
        let c = row(&rows, "claudebot");
        assert_eq!(
            (c.home_verdict, c.urls_blocked, c.urls_total),
            (Verdict::Allowed, 2, 4)
        );
        let summary = vendor_summary(ai_bots::all(), &rows);
        let anthropic = summary.iter().find(|(v, _)| v == "Anthropic").unwrap();
        assert!(
            anthropic
                .1
                .contains(&(BotPurpose::Training, PurposeAccess::Partly))
        );
    }

    #[test]
    fn google_extended_does_not_touch_googlebot() {
        // US1.4
        let (_, rows) = tally(
            RobotsSource::Ok("User-agent: Google-Extended\nDisallow: /\n"),
            &["https://a.test/"],
        );
        assert_eq!(row(&rows, "google-extended").urls_blocked, 1);
        assert_eq!(row(&rows, "googlebot").urls_blocked, 0);
    }

    #[test]
    fn applebot_follows_googlebot() {
        let (_, rows) = tally(
            RobotsSource::Ok("User-agent: Googlebot\nDisallow: /g/\n"),
            &["https://a.test/g/1"],
        );
        assert_eq!(row(&rows, "applebot").urls_blocked, 1);
        assert_eq!(row(&rows, "applebot").matched_group.as_deref(), Some("googlebot"));
        assert_eq!(row(&rows, "claudebot").urls_blocked, 0);
    }

    #[test]
    fn unavailable_missing_and_not_fetched() {
        // US1.6
        let (_, rows) = tally(RobotsSource::Unavailable, &["https://a.test/"]);
        assert!(
            rows.iter()
                .all(|r| r.home_verdict == Verdict::Unavailable && r.urls_blocked == 1)
        );
        let (masks, rows) = tally(RobotsSource::Missing, &["https://a.test/"]);
        assert!(rows.iter().all(|r| r.home_verdict == Verdict::Allowed));
        assert_eq!(masks, [0]);
        let (_, rows) = tally(RobotsSource::NotFetched, &["https://a.test/"]);
        let summary = vendor_summary(ai_bots::all(), &rows);
        assert!(
            summary
                .iter()
                .all(|(_, parts)| parts.iter().all(|(_, a)| *a == PurposeAccess::Unknown))
        );
    }

    #[test]
    fn notes_for_crawlers_that_may_ignore_robots() {
        assert_eq!(
            honours_note(ai_bots::by_token("chatgpt-user").unwrap()),
            Some("robots.txt may not apply")
        );
        assert_eq!(honours_note(ai_bots::by_token("gptbot").unwrap()), None);
    }
}
