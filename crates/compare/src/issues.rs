//! Issue deltas (spec 012 story 3, plan D6): the 009 rules evaluated on both
//! crawls with one set of issue settings, and how each rule's affected URLs
//! moved between them.
//!
//! 009 never stores hits in a crawl file, so every comparison evaluates into
//! the side connection's temp schema (FR-024, FR-024a) and copies
//! `(rule id, url id)` into the workspace. The deltas are then joins through
//! `matches`.

use std::path::Path;

use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use spiderlens_config::{IssueSettings, IssueSettingsChoice};
use spiderlens_results::issues::{
    self, Category, Priority, ResolvedSettings, RuleRun, evaluate_on, rule_runs,
};
use ts_rs::TS;

use crate::{Result, Side};

/// What the issue figures were computed with (FR-023, FR-024a).
#[derive(Debug, Clone, Copy, Serialize, TS)]
#[ts(export)]
pub struct IssueBasis {
    /// The rule set both crawls were evaluated with.
    pub rules_version: u32,
    /// Whose issue settings both crawls used.
    pub settings: IssueSettingsChoice,
    /// Those settings differ from the defaults.
    pub custom: bool,
}

/// One rule across the two crawls.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct IssueDelta {
    /// The stable rule id.
    pub rule: String,
    /// The name with the threshold in force.
    pub name: String,
    pub category: Category,
    pub priority: Priority,
    /// URLs affected in each crawl; null when that crawl cannot run the rule.
    #[ts(type = "number | null")]
    pub previous: Option<u64>,
    #[ts(type = "number | null")]
    pub current: Option<u64>,
    /// Affected now and not before (including URLs new in the current crawl).
    #[ts(type = "number")]
    pub newly_affected: u64,
    /// Affected before; present, reached and not affected now.
    #[ts(type = "number")]
    pub fixed: u64,
    /// Affected before; missing from the current crawl or not reached by it.
    #[ts(type = "number")]
    pub gone: u64,
    pub comparable: bool,
    pub reason: Option<String>,
}

impl IssueDelta {
    /// Current minus previous affected URLs.
    pub fn change(&self) -> i64 {
        self.current.unwrap_or(0) as i64 - self.previous.unwrap_or(0) as i64
    }

    /// Anything moved: a count differs or some URLs changed side.
    pub fn changed(&self) -> bool {
        self.comparable && (self.change() != 0 || self.newly_affected + self.fixed + self.gone > 0)
    }
}

/// Which affected URLs of a rule a group lists.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum IssueChange {
    NewlyAffected,
    Fixed,
    Gone,
}

/// The issue settings both sides use: the stored settings of the chosen
/// crawl (defaults when it stored none) or the app's.
pub(crate) fn resolve_settings(
    previous: &Path,
    current: &Path,
    choice: IssueSettingsChoice,
    app: Option<&IssueSettings>,
) -> Result<ResolvedSettings> {
    let stored = |path: &Path| -> Result<IssueSettings> {
        let (conn, _) = spiderlens_results::connect_attachable(path)?;
        Ok(issues::stored_issue_settings_on(&conn)?.unwrap_or_default())
    };
    let source = match choice {
        IssueSettingsChoice::Current => stored(current)?,
        IssueSettingsChoice::Previous => stored(previous)?,
        IssueSettingsChoice::App => app.cloned().unwrap_or_default(),
    };
    // Values out of range keep the default, as everywhere else (009 FR-024).
    Ok(issues::resolve(&source).0)
}

pub(crate) fn basis(choice: IssueSettingsChoice, resolved: &ResolvedSettings) -> IssueBasis {
    IssueBasis {
        rules_version: issues::RULES_VERSION,
        settings: choice,
        custom: resolved.rules != ResolvedSettings::default().rules,
    }
}

/// Evaluates the rules on one side and copies the hits into the workspace
/// as `(side, rule id, url id)`. `ws_file` is attached for the copy only.
pub(crate) fn collect(side: &Side, ws_file: &Path, n: u8, settings: &ResolvedSettings) -> Result<()> {
    let conn = &side.conn;
    evaluate_on(conn, &side.features, settings, &side.derived)?;
    conn.execute("ATTACH DATABASE ?1 AS cmp", [ws_file.to_string_lossy()])?;
    let result = copy_hits(conn, &rule_runs(&side.features, settings), n);
    conn.execute_batch("DETACH DATABASE cmp")?;
    result
}

fn copy_hits(conn: &Connection, runs: &[RuleRun], n: u8) -> Result<()> {
    // Hits carry the catalogue index; the workspace keeps the stable id.
    conn.execute_batch(
        "DROP TABLE IF EXISTS temp.issue_ids;
         CREATE TEMP TABLE issue_ids(idx INTEGER PRIMARY KEY, id TEXT NOT NULL);",
    )?;
    {
        let mut ins = conn.prepare("INSERT INTO temp.issue_ids VALUES (?1, ?2)")?;
        for r in runs.iter().filter(|r| r.runnable()) {
            ins.execute((r.index as i64, r.id))?;
        }
    }
    conn.execute(
        "INSERT INTO cmp.issue_hits (side, rule, id)
         SELECT ?1, i.id, h.url_id FROM temp.issue_hits h JOIN temp.issue_ids i ON i.idx = h.rule",
        [i64::from(n)],
    )?;
    conn.execute_batch("DROP TABLE temp.issue_ids")?;
    Ok(())
}

/// Every enabled rule's counts and moves, most important first. Runs on the
/// workspace once both sides' hits and the matches are in.
pub(crate) fn deltas(
    ws: &Connection,
    sides: &[Side; 2],
    settings: &ResolvedSettings,
) -> Result<Vec<IssueDelta>> {
    let runs = [
        rule_runs(&sides[0].features, settings),
        rule_runs(&sides[1].features, settings),
    ];
    let mut totals: [std::collections::HashMap<String, u64>; 2] = Default::default();
    {
        let mut stmt = ws.prepare("SELECT side, rule, COUNT(*) FROM issue_hits GROUP BY side, rule")?;
        let mut rows = stmt.query([])?;
        while let Some(r) = rows.next()? {
            let side: i64 = r.get(0)?;
            totals[(side as usize).min(1)].insert(r.get(1)?, r.get::<_, i64>(2)? as u64);
        }
    }
    // Per rule: [newly affected, fixed, gone].
    let mut moves: std::collections::HashMap<String, [u64; 3]> = std::collections::HashMap::new();
    {
        let mut stmt = ws.prepare(
            "SELECT h.rule, COUNT(*) FROM issue_hits h JOIN matches m ON m.cur_id = h.id
             WHERE h.side = 1 AND NOT EXISTS
               (SELECT 1 FROM issue_hits q WHERE q.side = 0 AND q.rule = h.rule AND q.id = m.prev_id)
             GROUP BY h.rule",
        )?;
        let mut rows = stmt.query([])?;
        while let Some(r) = rows.next()? {
            moves.entry(r.get(0)?).or_default()[0] = r.get::<_, i64>(1)? as u64;
        }
    }
    {
        let mut stmt = ws.prepare(
            "SELECT h.rule,
               SUM(CASE WHEN m.cur_id IS NOT NULL AND c.reach = 0 AND ch.id IS NULL THEN 1 ELSE 0 END),
               SUM(CASE WHEN m.cur_id IS NULL OR (c.reach > 0 AND ch.id IS NULL) THEN 1 ELSE 0 END)
             FROM issue_hits h
             JOIN matches m ON m.prev_id = h.id
             LEFT JOIN side_rows c ON c.side = 1 AND c.id = m.cur_id
             LEFT JOIN issue_hits ch ON ch.side = 1 AND ch.rule = h.rule AND ch.id = m.cur_id
             WHERE h.side = 0
             GROUP BY h.rule",
        )?;
        let mut rows = stmt.query([])?;
        while let Some(r) = rows.next()? {
            let m = moves.entry(r.get(0)?).or_default();
            m[1] = r.get::<_, Option<i64>>(1)?.unwrap_or(0) as u64;
            m[2] = r.get::<_, Option<i64>>(2)?.unwrap_or(0) as u64;
        }
    }
    let mut out: Vec<IssueDelta> = runs[1]
        .iter()
        .zip(&runs[0])
        .filter(|(c, _)| c.enabled)
        .map(|(c, p)| {
            let avail = [p.runnable(), c.runnable()];
            let comparable = avail[0] && avail[1];
            let [newly_affected, fixed, gone] = if comparable {
                moves.get(c.id).copied().unwrap_or_default()
            } else {
                [0; 3]
            };
            IssueDelta {
                rule: c.id.into(),
                name: c.name.clone(),
                category: c.category,
                priority: c.priority,
                previous: avail[0].then(|| totals[0].get(c.id).copied().unwrap_or(0)),
                current: avail[1].then(|| totals[1].get(c.id).copied().unwrap_or(0)),
                newly_affected,
                fixed,
                gone,
                comparable,
                reason: (!comparable).then(|| {
                    let (side, run) = if avail[0] { ("current", c) } else { ("previous", p) };
                    format!("The {side} crawl: {}", run.unavailable.unwrap_or_default())
                }),
            }
        })
        .collect();
    // By priority, then by how far the count moved; the sort is stable, so
    // ties keep catalogue order.
    out.sort_by(|a, b| {
        a.priority
            .cmp(&b.priority)
            .then_with(|| b.change().unsigned_abs().cmp(&a.change().unsigned_abs()))
    });
    Ok(out)
}

/// The SQL condition (over the pair join) selecting `change`'s URLs of
/// `rule`. The rule must be a catalogue id: it is written into the text.
pub(crate) fn group_sql(rule: &str, change: IssueChange) -> Result<String> {
    if issues::get(rule).is_none() {
        return Err(crate::Error::invalid(format!("unknown issue {rule:?}")));
    }
    let hit = |side: u8, id: &str| {
        format!(
            "EXISTS (SELECT 1 FROM issue_hits q WHERE q.side = {side} AND q.rule = '{rule}' AND q.id = {id})"
        )
    };
    let (before, now) = (hit(0, "m.prev_id"), hit(1, "m.cur_id"));
    Ok(match change {
        IssueChange::NewlyAffected => format!("{now} AND NOT {before}"),
        IssueChange::Fixed => format!("m.cur_id IS NOT NULL AND c.reach = 0 AND {before} AND NOT {now}"),
        IssueChange::Gone => format!("{before} AND (m.cur_id IS NULL OR (c.reach > 0 AND NOT {now}))"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn group_conditions_name_only_catalogue_rules() {
        assert!(group_sql("title.over", IssueChange::Fixed).is_ok());
        assert!(group_sql("x'; DROP TABLE matches; --", IssueChange::Gone).is_err());
    }
}
