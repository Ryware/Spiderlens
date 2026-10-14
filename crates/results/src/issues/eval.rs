//! Evaluating the catalogue over a crawl file (plan D4).
//!
//! Hits go to `temp.issue_hits(rule, url_id)`, rebuilt per data version and
//! settings: one scan of `urls` per scope, testing that scope's enabled
//! rules with one `CASE`. Counts, a URL's issues and an issue's rows are then
//! index reads. Nothing is written to the crawl file.

use rusqlite::{Connection, OptionalExtension};
use serde::Serialize;
use ts_rs::TS;

use super::{Category, Priority, ResolvedSettings, RuleKind, Scope, catalogue, display_name};
use crate::Result;
use crate::features::Features;
use crate::reader::Reader;
use crate::tabs::Derived;

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct IssuesSummary {
    /// Changes whenever the counts may have changed.
    pub version: u32,
    pub rules_version: u32,
    pub by_priority: Vec<PriorityTotal>,
    /// Every rule, ordered by priority then count (FR-014).
    pub issues: Vec<IssueCount>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct PriorityTotal {
    pub priority: Priority,
    /// Issues (rules with at least one URL) at this priority.
    pub issues: u32,
    /// Affected URLs summed over those issues.
    #[ts(type = "number")]
    pub urls: u64,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct IssueCount {
    pub id: String,
    pub name: String,
    pub category: Category,
    pub priority: Priority,
    pub enabled: bool,
    /// Null when the rule is off or cannot run on this file.
    #[ts(type = "number | null")]
    pub count: Option<u64>,
    /// Can still change before the crawl ends (FR-019).
    pub provisional: bool,
    /// Why the rule cannot run on this file (FR-010).
    pub unavailable: Option<String>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct UrlIssue {
    pub id: String,
    pub name: String,
    pub priority: Priority,
    pub evidence: Option<String>,
}

/// The rules that run on this file with these settings, by catalogue index.
fn runnable(features: &Features, settings: &ResolvedSettings) -> Vec<usize> {
    catalogue()
        .iter()
        .enumerate()
        .filter(|(i, r)| settings.rules[*i].enabled && features.has(r.need))
        .map(|(i, _)| i)
        .collect()
}

/// Derived tables the runnable rules read.
pub(crate) fn dependencies(features: &Features, settings: &ResolvedSettings) -> Vec<Derived> {
    let mut out: Vec<Derived> = Vec::new();
    for i in runnable(features, settings) {
        for d in catalogue()[i].derived {
            if !out.contains(d) {
                out.push(*d);
            }
        }
    }
    out
}

/// Builds `temp.issue_rules` and `temp.issue_hits` on `conn`. The derived
/// tables in [`dependencies`] must be current.
pub(crate) fn build_hits(conn: &Connection, features: &Features, settings: &ResolvedSettings) -> Result<()> {
    let rules = runnable(features, settings);
    conn.execute_batch(
        "DROP TABLE IF EXISTS temp.issue_hits;
         DROP TABLE IF EXISTS temp.issue_rules;
         CREATE TEMP TABLE issue_rules(idx INTEGER PRIMARY KEY, scope INTEGER NOT NULL, prio INTEGER NOT NULL);
         CREATE TEMP TABLE issue_hits(rule INTEGER NOT NULL, url_id INTEGER NOT NULL, PRIMARY KEY (rule, url_id)) WITHOUT ROWID;",
    )?;
    {
        let mut ins = conn.prepare("INSERT INTO temp.issue_rules VALUES (?1, ?2, ?3)")?;
        for &i in &rules {
            let scope = Scope::ALL
                .iter()
                .position(|s| *s == catalogue()[i].scope)
                .unwrap_or(0);
            ins.execute((i as i64, scope as i64, settings.rules[i].priority as i64))?;
        }
    }
    for (si, scope) in Scope::ALL.iter().enumerate() {
        let mine: Vec<usize> = rules
            .iter()
            .copied()
            .filter(|&i| catalogue()[i].scope == *scope)
            .collect();
        if mine.is_empty() {
            continue;
        }
        let mut case = String::from("CASE r.idx");
        for i in &mine {
            case.push_str(&format!(" WHEN {i} THEN ({})", catalogue()[*i].cond));
        }
        case.push_str(" ELSE 0 END");
        let sql = format!(
            "INSERT INTO temp.issue_hits SELECT r.idx, u.id FROM urls u NOT INDEXED \
             JOIN temp.issue_rules r ON r.scope = {si} WHERE ({}) AND {case}",
            scope.sql()
        );
        conn.execute(&sql, [])?;
    }
    conn.execute_batch("CREATE INDEX temp.issue_hits_url ON issue_hits(url_id);")?;
    Ok(())
}

impl Reader {
    /// Every rule with its count (FR-014). `running` marks site-wide counts
    /// as provisional (FR-019).
    pub fn issues_summary(&mut self, running: bool) -> Result<IssuesSummary> {
        self.ensure_derived([Derived::IssueHits])?;
        let version = self.version()? as u32;
        let mut counts = vec![0u64; catalogue().len()];
        {
            let mut stmt = self
                .conn
                .prepare("SELECT rule, COUNT(*) FROM temp.issue_hits GROUP BY rule")?;
            let mut rows = stmt.query([])?;
            while let Some(r) = rows.next()? {
                let i: i64 = r.get(0)?;
                let n: i64 = r.get(1)?;
                if let Some(c) = counts.get_mut(i as usize) {
                    *c = n as u64;
                }
            }
        }
        let settings = &self.issue_settings;
        let mut issues: Vec<IssueCount> = catalogue()
            .iter()
            .enumerate()
            .map(|(i, r)| {
                let s = settings.rules[i];
                let available = self.features.has(r.need);
                IssueCount {
                    id: r.id.into(),
                    name: display_name(r, s.threshold),
                    category: r.category,
                    priority: s.priority,
                    enabled: s.enabled,
                    count: (s.enabled && available).then_some(counts[i]),
                    provisional: running && r.kind == RuleKind::SiteWide,
                    unavailable: (!available).then(|| r.need.unavailable_reason().to_string()),
                }
            })
            .collect();
        issues.sort_by(|a, b| {
            a.priority
                .cmp(&b.priority)
                .then_with(|| b.count.unwrap_or(0).cmp(&a.count.unwrap_or(0)))
        });
        let by_priority = Priority::ALL
            .into_iter()
            .map(|p| {
                let at: Vec<_> = issues
                    .iter()
                    .filter(|c| c.priority == p && c.count.unwrap_or(0) > 0)
                    .collect();
                PriorityTotal {
                    priority: p,
                    issues: at.len() as u32,
                    urls: at.iter().map(|c| c.count.unwrap_or(0)).sum(),
                }
            })
            .collect();
        Ok(IssuesSummary {
            version,
            rules_version: super::RULES_VERSION,
            by_priority,
            issues,
        })
    }

    /// The issues of one URL with their evidence (FR-012, FR-017).
    pub fn url_issues(&mut self, url_id: i64) -> Result<Vec<UrlIssue>> {
        self.ensure_derived([Derived::IssueHits])?;
        let hits: Vec<usize> = {
            let mut stmt = self
                .conn
                .prepare("SELECT rule FROM temp.issue_hits WHERE url_id = ?1")?;
            stmt.query_map([url_id], |r| r.get::<_, i64>(0))?
                .map(|r| r.map(|i| i as usize))
                .collect::<rusqlite::Result<_>>()?
        };
        let mut out = Vec::with_capacity(hits.len());
        for i in hits {
            let r = &catalogue()[i];
            let s = self.issue_settings.rules[i];
            let evidence: Option<String> = self
                .conn
                .query_row(
                    &format!("SELECT {} FROM urls u WHERE u.id = ?1", r.evidence),
                    [url_id],
                    |row| row.get::<_, rusqlite::types::Value>(0),
                )
                .map(|v| match v {
                    rusqlite::types::Value::Text(t) => Some(t),
                    rusqlite::types::Value::Integer(n) => Some(n.to_string()),
                    rusqlite::types::Value::Real(f) => Some(f.to_string()),
                    _ => None,
                })?;
            out.push(UrlIssue {
                id: r.id.into(),
                name: display_name(r, s.threshold),
                priority: s.priority,
                evidence,
            });
        }
        out.sort_by_key(|u| u.priority);
        Ok(out)
    }

    /// The issue settings saved in the crawl file (plan D8), if any. A
    /// value this version cannot read counts as none.
    pub fn stored_issue_settings(&self) -> Result<Option<spiderlens_config::IssueSettings>> {
        let text: Option<String> = self
            .conn
            .query_row("SELECT value FROM meta WHERE key = 'issue_settings'", [], |r| {
                r.get(0)
            })
            .optional()?;
        Ok(text.and_then(|t| serde_json::from_str(&t).ok()))
    }

    pub fn issue_settings(&self) -> &ResolvedSettings {
        &self.issue_settings
    }

    /// Applies new issue settings to this reader (FR-023). Values out of
    /// range reject the whole change and keep the previous settings
    /// (FR-024); unknown rules are ignored with a notice.
    pub fn set_issue_settings(&mut self, s: &spiderlens_config::IssueSettings) -> Vec<super::SettingsNotice> {
        let (resolved, notices) = super::resolve(s);
        if notices.iter().any(|n| n.error) {
            return notices;
        }
        if resolved != self.issue_settings {
            *self.thresholds.write().unwrap_or_else(|e| e.into_inner()) = resolved.thresholds();
            self.issue_settings = resolved;
            // Everything reading thresholds or hits is stale now.
            self.derived.remove(&Derived::IssueHits);
            self.bump_version();
        }
        notices
    }

    pub(crate) fn build_issue_hits(&mut self) -> Result<()> {
        let deps = dependencies(&self.features, &self.issue_settings);
        self.ensure_derived(deps)?;
        build_hits(&self.conn, &self.features, &self.issue_settings)
    }
}

/// Builds `derived` on a connection that has no [`Reader`] (CSV export).
pub(crate) fn build_derived_with(
    conn: &Connection,
    features: &Features,
    settings: &ResolvedSettings,
    derived: &[Derived],
) -> Result<()> {
    let mut all: Vec<Derived> = derived
        .iter()
        .copied()
        .filter(|d| *d != Derived::IssueHits)
        .collect();
    let hits = derived.contains(&Derived::IssueHits);
    if hits {
        for d in dependencies(features, settings) {
            if !all.contains(&d) {
                all.push(d);
            }
        }
    }
    for d in all {
        crate::tabs::build_derived(conn, d)?;
    }
    if hits {
        build_hits(conn, features, settings)?;
    }
    Ok(())
}
