//! The `agent_scores` Crawl Analysis pass (spec 016 plan D6): evaluates the
//! agent rules over the stored tables and writes one score per internal
//! 200 HTML page, plus one row for the crawl (`url_id` NULL).
//!
//! The rules are the `agents` category of spec 009's catalogue. Their
//! conditions are the same SQL the issue list evaluates; here one scan of
//! the pages tests all page rules as columns (as 009's evaluation does), and
//! each site-wide rule is one `EXISTS` over the crawl. Nothing here runs on
//! the crawl path.
//!
//! A rule whose data is missing (no rendering, no AI checks, no entry
//! points looked for) is not checked: it is left out of the score and
//! counted in `not_checked`, so a crawl is never rewarded for a check that
//! did not happen. Page scores cover the page rules only; the site-wide
//! rules (entry points, `llms.txt`, search) count once, in the crawl row.

use std::time::Duration;

use rusqlite::{Connection, params};

use super::score::{self, WEIGHTS_VERSION};
use crate::Result;
use crate::analysis::{Pass, PassContext};
use crate::features::Features;
use crate::issues::{Category, Rule, RuleKind, catalogue};
use crate::tabs::{HTML_OK, Need};

pub struct AgentScores;

impl Pass for AgentScores {
    fn id(&self) -> &'static str {
        "agent_scores"
    }
    fn version(&self) -> u32 {
        1
    }
    /// Reads the entry points the collection step wrote.
    fn after(&self) -> &'static [&'static str] {
        &["agent_entry_points"]
    }
    fn needs(&self) -> &'static [Need] {
        &[Need::Agents]
    }
    /// One scan of the pages with about fifteen primary key lookups each.
    fn budget(&self) -> Duration {
        Duration::from_secs(3)
    }
    fn run(&self, cx: &mut PassContext<'_>) -> Result<()> {
        let conn = cx.conn;
        let s = cx.schema;
        create(conn, s)?;
        conn.execute(&format!("DELETE FROM {s}.agent_scores"), [])?;
        let features = Features::detect(conn)?;
        let rules = applicability(conn, &features)?;
        let not_checked = rules.not_checked.len() as i64;
        let (page_rules, site_rules): (Vec<&Rule>, Vec<&Rule>) = rules
            .checked
            .iter()
            .copied()
            .partition(|r| r.kind == RuleKind::PerUrl);
        const LABEL: &str = "Agent checks";
        cx.progress(0, cx.urls, LABEL)?;

        // Site-wide rules: one answer each.
        let mut site_failed: Vec<&str> = Vec::new();
        for r in &site_rules {
            let failed: bool = conn.query_row(
                &format!(
                    "SELECT EXISTS (SELECT 1 FROM urls u WHERE {} AND ({}))",
                    r.scope.sql(),
                    r.cond
                ),
                [],
                |row| row.get(0),
            )?;
            if failed {
                site_failed.push(r.id);
            }
        }

        // Page rules: one scan, a column per rule. Only pages whose raw
        // HTML was checked are scored.
        let mut cols = String::from("u.id");
        for r in &page_rules {
            cols.push_str(&format!(", CASE WHEN ({}) THEN 1 ELSE 0 END", r.cond));
        }
        let mut scan = conn.prepare(&format!(
            "SELECT {cols} FROM urls u WHERE {HTML_OK} \
             AND EXISTS (SELECT 1 FROM page_agent a WHERE a.url_id = u.id AND a.source = 'raw')"
        ))?;
        let mut put = conn.prepare(&format!(
            "INSERT INTO {s}.agent_scores
                 (url_id, total, access, discovery, reading, acting, not_checked, failed_rules, weights_version)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)"
        ))?;
        let mut fails = vec![0u64; page_rules.len()];
        let mut pages = 0u64;
        let mut rows = scan.query([])?;
        while let Some(row) = rows.next()? {
            let id: i64 = row.get(0)?;
            let mut outcomes: Vec<(&str, f64)> = Vec::with_capacity(page_rules.len());
            let mut failed: Vec<&str> = Vec::new();
            for (i, r) in page_rules.iter().enumerate() {
                let hit = row.get::<_, i64>(i + 1)? == 1;
                if hit {
                    fails[i] += 1;
                    failed.push(r.id);
                }
                outcomes.push((r.id, if hit { 1.0 } else { 0.0 }));
            }
            let sc = score::score(outcomes);
            put.execute(params![
                id,
                sc.total,
                sc.access,
                sc.discovery,
                sc.reading,
                sc.acting,
                not_checked,
                failed_json(&failed),
                WEIGHTS_VERSION,
            ])?;
            pages += 1;
            if pages.is_multiple_of(4096) {
                cx.progress(pages, cx.urls.max(pages), LABEL)?;
            }
        }
        drop(rows);

        // The crawl: a page rule counts as the share of pages it fails on,
        // a site-wide rule as failed or not.
        let mut outcomes: Vec<(&str, f64)> = Vec::new();
        let mut failed: Vec<&str> = Vec::new();
        if pages > 0 {
            for (r, n) in page_rules.iter().zip(&fails) {
                outcomes.push((r.id, *n as f64 / pages as f64));
                if *n > 0 {
                    failed.push(r.id);
                }
            }
        }
        for r in &site_rules {
            let hit = site_failed.contains(&r.id);
            outcomes.push((r.id, if hit { 1.0 } else { 0.0 }));
            if hit {
                failed.push(r.id);
            }
        }
        let sc = score::score(outcomes);
        // Page rules with no page to look at were not checked either.
        let unchecked = not_checked + if pages == 0 { page_rules.len() as i64 } else { 0 };
        put.execute(params![
            None::<i64>,
            sc.total,
            sc.access,
            sc.discovery,
            sc.reading,
            sc.acting,
            unchecked,
            failed_json(&failed),
            WEIGHTS_VERSION,
        ])?;
        cx.progress(pages, pages.max(1), LABEL)?;
        Ok(())
    }
}

fn failed_json(ids: &[&str]) -> String {
    serde_json::to_string(ids).expect("rule ids serialize")
}

/// The pass's table in `schema` (also created by crawl file step 16, where
/// it is a no-op).
fn create(conn: &Connection, schema: &str) -> rusqlite::Result<()> {
    conn.execute_batch(&format!(
        "CREATE TABLE IF NOT EXISTS {schema}.agent_scores (
             url_id INTEGER, total REAL, access REAL, discovery REAL, reading REAL, acting REAL,
             not_checked INTEGER NOT NULL, failed_rules TEXT NOT NULL, weights_version INTEGER NOT NULL);
         CREATE UNIQUE INDEX IF NOT EXISTS {schema}.agent_scores_url ON agent_scores(url_id);
         CREATE UNIQUE INDEX IF NOT EXISTS {schema}.agent_scores_crawl ON agent_scores((url_id IS NULL)) WHERE url_id IS NULL;"
    ))
}

/// The agent rules that can be checked on this crawl, and why the others
/// cannot.
pub(crate) struct Applicability {
    pub checked: Vec<&'static Rule>,
    pub not_checked: Vec<(&'static Rule, String)>,
}

/// Splits the `agents` rules by whether their data is in the file: the
/// rule's [`Need`], and for rules that read what an earlier step collected,
/// that the step ran.
pub(crate) fn applicability(conn: &Connection, features: &Features) -> rusqlite::Result<Applicability> {
    let mut out = Applicability {
        checked: Vec::new(),
        not_checked: Vec::new(),
    };
    for r in catalogue().iter().filter(|r| r.category == Category::Agents) {
        let reason = if features.has(r.need) {
            missing_data(conn, r.id)?.map(str::to_string)
        } else {
            Some(r.need.unavailable_reason().to_string())
        };
        match reason {
            Some(reason) => out.not_checked.push((r, reason)),
            None => out.checked.push(r),
        }
    }
    Ok(out)
}

/// Why a rule whose [`Need`] is met still has nothing to look at.
fn missing_data(conn: &Connection, id: &str) -> rusqlite::Result<Option<&'static str>> {
    let has = |sql: &str| conn.query_row(sql, [], |r| r.get::<_, bool>(0));
    Ok(match id {
        "agents.entry_point_invalid" | "agents.no_entry_points"
            if !has("SELECT EXISTS (SELECT 1 FROM agent_entry_points)")? =>
        {
            Some("Agent entry points were not looked for in this crawl.")
        }
        "agents.no_entry_points"
            if !has(
                "SELECT EXISTS (SELECT 1 FROM site_files WHERE path = '/llms.txt' AND outcome IN ('ok', 'not_found'))",
            )? =>
        {
            Some("llms.txt was not read in this crawl.")
        }
        "agents.robots_blocks_agents"
            if !has(
                "SELECT EXISTS (SELECT 1 FROM aeo_bot_access WHERE ai_bot_purpose(bot) = 'user_fetch' \
                 AND home_verdict <> 'unknown')",
            )? =>
        {
            Some("robots.txt was not evaluated for AI agents that act for a person in this crawl.")
        }
        // Nothing here starts the probe: it is 015's explicit, optional run.
        "agents.probe_blocked"
            if !has("SELECT EXISTS (SELECT 1 FROM aeo_probe WHERE ai_bot_purpose(bot) = 'user_fetch')")? =>
        {
            Some(
                "The AI agent test was not run for agents that act for a person. Run it from the AI search tab.",
            )
        }
        "agents.form_rendered_only"
            if !has("SELECT EXISTS (SELECT 1 FROM page_agent WHERE source = 'rendered')")? =>
        {
            Some("Pages were not rendered in this crawl.")
        }
        "agents.markdown_alternate_broken"
            if !has(
                "SELECT EXISTS (SELECT 1 FROM page_agent WHERE md_outcome IN ('ok', 'wrong_type', 'error'))",
            )? =>
        {
            Some("Markdown versions were not requested in this crawl.")
        }
        "agents.mcp_endpoint_unreachable"
            if !has("SELECT EXISTS (SELECT 1 FROM agent_entry_points WHERE mcp_discovery IS NOT NULL)")? =>
        {
            Some("No MCP endpoint was checked in this crawl.")
        }
        _ => None,
    })
}
