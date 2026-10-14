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
        for d in catalogue()[i].derived_tables() {
            if !out.contains(&d) {
                out.push(d);
            }
        }
    }
    out
}

/// The page-field lookup rule conditions share with the 007 filters.
const PAGE_FIELD: (&str, &str) = ("(SELECT p.", " FROM pages p WHERE p.url_id = u.id)");

/// Rewrites each `(SELECT p.<field> FROM pages p WHERE p.url_id = u.id)`
/// in `cond` as `pj.<field>`, for a query that joins `pages pj` once per
/// URL. Equivalent: the subquery and the LEFT JOIN both give NULL when the
/// URL has no pages row. A page rule otherwise repeats that lookup for
/// every field it reads, about 300 lookups per URL for the whole set.
fn inline_pages(cond: &str) -> String {
    let (head, tail) = PAGE_FIELD;
    let mut out = String::with_capacity(cond.len());
    let mut rest = cond;
    while let Some(at) = rest.find(head) {
        let after = &rest[at + head.len()..];
        let field_len = after
            .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
            .unwrap_or(after.len());
        if field_len > 0 && after[field_len..].starts_with(tail) {
            out.push_str(&rest[..at]);
            out.push_str("pj.");
            out.push_str(&after[..field_len]);
            rest = &after[field_len + tail.len()..];
        } else {
            out.push_str(&rest[..at + head.len()]);
            rest = after;
        }
    }
    out.push_str(rest);
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
    let mut hits: Vec<(i64, i64)> = Vec::new();
    for scope in Scope::ALL.iter() {
        let mine: Vec<usize> = rules
            .iter()
            .copied()
            .filter(|&i| catalogue()[i].scope == *scope)
            .collect();
        if mine.is_empty() {
            continue;
        }
        let pages = *scope == Scope::Pages;
        // One pass per scope: each URL (and its pages row) is read once and
        // every rule is a column, so no rule rescans the table.
        let mut cols = String::from("u.id");
        for i in &mine {
            let cond = catalogue()[*i].cond;
            let cond = if pages {
                inline_pages(cond)
            } else {
                cond.to_string()
            };
            cols.push_str(&format!(", CASE WHEN ({cond}) THEN 1 END"));
        }
        let from = if pages {
            "urls u NOT INDEXED LEFT JOIN pages pj ON pj.url_id = u.id"
        } else {
            "urls u NOT INDEXED"
        };
        let mut stmt = conn.prepare(&format!("SELECT {cols} FROM {from} WHERE {}", scope.sql()))?;
        let mut rows = stmt.query([])?;
        while let Some(r) = rows.next()? {
            let id: i64 = r.get(0)?;
            for (c, i) in mine.iter().enumerate() {
                if r.get_ref(c + 1)? != rusqlite::types::ValueRef::Null {
                    hits.push((*i as i64, id));
                }
            }
        }
    }
    // Sorted by the primary key, the insert appends to the b-tree.
    hits.sort_unstable();
    conn.execute_batch("SAVEPOINT issue_hits")?;
    {
        let mut ins = conn.prepare("INSERT INTO temp.issue_hits VALUES (?1, ?2)")?;
        for (rule, id) in hits {
            ins.execute((rule, id))?;
        }
    }
    conn.execute_batch("RELEASE issue_hits")?;
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

    /// The issue settings saved in the crawl file (plan D8): the last ones
    /// recorded, else the profile's when the crawl started. A value this
    /// version cannot read counts as none.
    pub fn stored_issue_settings(&self) -> Result<Option<spiderlens_config::IssueSettings>> {
        if let Some(t) = self.meta("issue_settings")? {
            return Ok(serde_json::from_str(&t).ok());
        }
        Ok(self
            .meta("config")?
            .and_then(|t| serde_json::from_str::<spiderlens_config::StoredCrawlConfig>(&t).ok())
            .map(|c| c.config.issues))
    }

    /// The rule set version the file's issues were last recorded with, if
    /// any (FR-013: older files are simply evaluated again).
    pub fn recorded_rules_version(&self) -> Result<Option<u32>> {
        Ok(self.meta("rules_version")?.and_then(|v| v.parse().ok()))
    }

    fn meta(&self, key: &str) -> Result<Option<String>> {
        Ok(self
            .conn
            .query_row("SELECT value FROM meta WHERE key = ?1", [key], |r| r.get(0))
            .optional()?)
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

/// Records in the crawl file the rule set version and the issue settings
/// its issues were last evaluated with (FR-013, plan D8). Meta is
/// key/value, so no migration is needed. A file this version cannot write
/// (another format, or not writable) is left alone and the error returned.
pub fn record_issue_settings(path: &std::path::Path, s: &spiderlens_config::IssueSettings) -> Result<()> {
    let conn = Connection::open_with_flags(
        path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    conn.busy_timeout(std::time::Duration::from_secs(5))?;
    let format: Option<String> = conn
        .query_row("SELECT value FROM meta WHERE key = 'format'", [], |r| r.get(0))
        .optional()?;
    if format.as_deref() != Some(spiderlens_engine::crawl::store::FORMAT) {
        return Err(crate::Error::invalid("not a crawl file this version writes"));
    }
    let json = serde_json::to_string(s).expect("settings serialize");
    let mut set = conn
        .prepare("INSERT INTO meta(key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = ?2")?;
    set.execute(("rules_version", super::RULES_VERSION.to_string()))?;
    set.execute(("issue_settings", json))?;
    Ok(())
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
