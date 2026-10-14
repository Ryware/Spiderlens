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
    /// For link rules, the targets causing it, in page order (at most
    /// [`TARGETS_SHOWN`]), each openable in the table (US2 scenario 2).
    pub targets: Vec<IssueTarget>,
    /// All such targets, when more than are listed.
    pub targets_total: u32,
}

/// One linked URL behind a page's issue.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct IssueTarget {
    #[ts(type = "number")]
    pub id: i64,
    pub url: String,
    #[ts(type = "number | null")]
    pub status: Option<i64>,
}

/// Targets listed per issue in the detail pane; a page linking to 5,000
/// broken URLs shows the count and the first ones (edge case).
pub const TARGETS_SHOWN: usize = 200;

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

/// The summary for `counts` by catalogue index. `running` marks site-wide
/// counts provisional (FR-019); `partial` marks every count (FR-008).
fn summarize(
    counts: &[u64],
    settings: &ResolvedSettings,
    features: &Features,
    running: bool,
    version: u32,
) -> IssuesSummary {
    summarize_with(counts, settings, features, running, false, version)
}

fn summarize_with(
    counts: &[u64],
    settings: &ResolvedSettings,
    features: &Features,
    running: bool,
    partial: bool,
    version: u32,
) -> IssuesSummary {
    let mut issues: Vec<IssueCount> = catalogue()
        .iter()
        .enumerate()
        .map(|(i, r)| {
            let s = settings.rules[i];
            let available = features.has(r.need);
            IssueCount {
                id: r.id.into(),
                name: display_name(r, s.threshold),
                category: r.category,
                priority: s.priority,
                enabled: s.enabled,
                count: (s.enabled && available).then_some(counts[i]),
                provisional: partial || (running && r.kind == RuleKind::SiteWide),
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
    IssuesSummary {
        version,
        rules_version: super::RULES_VERSION,
        by_priority,
        issues,
    }
}

/// Builds `temp.issue_rules` and `temp.issue_hits` on `conn`. The derived
/// tables in [`dependencies`] must be current.
pub(crate) fn build_hits(
    conn: &Connection,
    features: &Features,
    settings: &ResolvedSettings,
    progress: &mut dyn FnMut(&[u64]),
    missing: &[Derived],
) -> Result<()> {
    let rules = runnable(features, settings);
    conn.execute_batch(
        "DROP TABLE IF EXISTS temp.issue_hits;
         DROP TABLE IF EXISTS temp.issue_rules;
         CREATE TEMP TABLE issue_rules(idx INTEGER PRIMARY KEY, scope INTEGER NOT NULL, prio INTEGER NOT NULL);
         CREATE TEMP TABLE issue_hits(rule INTEGER NOT NULL, url_id INTEGER NOT NULL, PRIMARY KEY (rule, url_id)) WITHOUT ROWID;",
    )?;
    fill_rules(conn, &rules, settings)?;
    // Rules that read only the crawl run first, so partial counts show
    // while the derived tables the others read (`missing`) are built.
    let (ready, waiting): (Vec<usize>, Vec<usize>) = rules
        .iter()
        .partition(|&&i| catalogue()[i].derived_tables().all(|d| !missing.contains(&d)));
    let mut counts = vec![0u64; catalogue().len()];
    eval_rules(conn, &ready, &mut counts, progress)?;
    if !waiting.is_empty() {
        for d in missing {
            crate::tabs::build_derived(conn, *d)?;
        }
        eval_rules(conn, &waiting, &mut counts, progress)?;
    }
    conn.execute_batch("CREATE INDEX temp.issue_hits_url ON issue_hits(url_id);")?;
    Ok(())
}

/// Brings built hits from `old` settings to `new` (SC-004): priorities
/// are a table update, and only rules switched on or off or given another
/// threshold are evaluated again. Each rule reads only its own threshold.
fn update_hits(
    conn: &Connection,
    features: &Features,
    old: &ResolvedSettings,
    new: &ResolvedSettings,
) -> Result<()> {
    let rules = runnable(features, new);
    let changed: Vec<usize> = (0..catalogue().len())
        .filter(|&i| {
            old.rules[i].enabled != new.rules[i].enabled || old.rules[i].threshold != new.rules[i].threshold
        })
        .collect();
    conn.execute_batch("SAVEPOINT issue_update")?;
    let result = (|| {
        conn.execute("DELETE FROM temp.issue_rules", [])?;
        fill_rules(conn, &rules, new)?;
        let mut del = conn.prepare("DELETE FROM temp.issue_hits WHERE rule = ?1")?;
        for &i in &changed {
            del.execute([i as i64])?;
        }
        let again: Vec<usize> = changed.iter().copied().filter(|i| rules.contains(i)).collect();
        eval_rules(conn, &again, &mut vec![0; catalogue().len()], &mut |_| {})
    })();
    match result {
        Ok(()) => conn.execute_batch("RELEASE issue_update")?,
        Err(_) => conn.execute_batch("ROLLBACK TO issue_update; RELEASE issue_update")?,
    }
    result
}

fn fill_rules(conn: &Connection, rules: &[usize], settings: &ResolvedSettings) -> Result<()> {
    let mut ins = conn.prepare("INSERT INTO temp.issue_rules VALUES (?1, ?2, ?3)")?;
    for &i in rules {
        let scope = Scope::ALL
            .iter()
            .position(|s| *s == catalogue()[i].scope)
            .unwrap_or(0);
        ins.execute((i as i64, scope as i64, settings.rules[i].priority as i64))?;
    }
    Ok(())
}

/// How often partial counts are reported during a long evaluation (FR-008).
const PARTIAL_EVERY: std::time::Duration = if cfg!(test) {
    std::time::Duration::ZERO
} else {
    std::time::Duration::from_secs(1)
};

/// Adds the hits of `rules` to `temp.issue_hits` and to `counts` (by
/// catalogue index). `progress` gets the counts so far about once a second.
fn eval_rules(
    conn: &Connection,
    rules: &[usize],
    counts: &mut [u64],
    progress: &mut dyn FnMut(&[u64]),
) -> Result<()> {
    let mut hits: Vec<(i64, i64)> = Vec::new();
    let mut last = std::time::Instant::now();
    let mut read = 0u64;
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
                    counts[*i] += 1;
                }
            }
            read += 1;
            if read.is_multiple_of(4096) && last.elapsed() >= PARTIAL_EVERY {
                progress(counts);
                last = std::time::Instant::now();
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
        Ok(summarize(
            &counts,
            &self.issue_settings,
            &self.features,
            running,
            version,
        ))
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
            let (targets, targets_total) = match r.targets {
                Some(cond) => self.issue_targets(url_id, cond)?,
                None => (Vec::new(), 0),
            };
            out.push(UrlIssue {
                id: r.id.into(),
                name: display_name(r, s.threshold),
                priority: s.priority,
                evidence,
                targets,
                targets_total,
            });
        }
        out.sort_by_key(|u| u.priority);
        Ok(out)
    }

    fn issue_targets(&self, url_id: i64, cond: &str) -> Result<(Vec<IssueTarget>, u32)> {
        let from = format!("FROM links l JOIN urls t ON t.id = l.to_id WHERE l.from_id = ?1 AND {cond}");
        let total: i64 =
            self.conn
                .query_row(&format!("SELECT COUNT(DISTINCT t.id) {from}"), [url_id], |r| {
                    r.get(0)
                })?;
        let mut stmt = self.conn.prepare(&format!(
            "SELECT t.id, t.url, t.status {from} GROUP BY t.id ORDER BY MIN(l.pos) LIMIT {TARGETS_SHOWN}"
        ))?;
        let targets = stmt
            .query_map([url_id], |r| {
                Ok(IssueTarget {
                    id: r.get(0)?,
                    url: r.get(1)?,
                    status: r.get(2)?,
                })
            })?
            .collect::<rusqlite::Result<_>>()?;
        Ok((targets, total as u32))
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
            let old = std::mem::replace(&mut self.issue_settings, resolved);
            // Hits built on current data are updated in place; a newly
            // enabled rule's derived tables are built first.
            let current = self.derived.contains_key(&Derived::IssueHits)
                && self.version().is_ok()
                && self.derived.get(&Derived::IssueHits) == Some(&self.data_gen);
            let updated = current
                && self
                    .ensure_derived(dependencies(&self.features, &self.issue_settings))
                    .and_then(|()| update_hits(&self.conn, &self.features, &old, &self.issue_settings))
                    .is_ok();
            if !updated {
                self.derived.remove(&Derived::IssueHits);
            }
            // Views and counts reading thresholds or hits refresh.
            self.bump_version();
        }
        notices
    }

    pub(crate) fn build_issue_hits(&mut self) -> Result<()> {
        let version = self.version()? as u32;
        let g = self.data_gen;
        // Built inside build_hits, after the rules that do not read them.
        let missing: Vec<Derived> = dependencies(&self.features, &self.issue_settings)
            .into_iter()
            .filter(|d| self.derived.get(d) != Some(&g))
            .collect();
        let (settings, features) = (&self.issue_settings, &self.features);
        let mut sink = self.partial.take();
        let mut progress = |counts: &[u64]| {
            if let Some(f) = sink.as_mut() {
                f(summarize_with(counts, settings, features, false, true, version));
            }
        };
        let result = build_hits(&self.conn, features, settings, &mut progress, &missing);
        self.partial = sink;
        result?;
        for d in missing {
            self.derived.insert(d, g);
        }
        Ok(())
    }

    /// Receives partial summaries while the issues are being evaluated, so
    /// a long first evaluation shows counts as they grow (FR-008).
    pub fn set_partial_sink(&mut self, sink: Option<Box<dyn FnMut(IssuesSummary) + Send>>) {
        self.partial = sink;
    }
}

/// Records in the crawl file the rule set version and the issue settings
/// its issues were last evaluated with (FR-013, plan D8). Meta is
/// key/value, so no migration is needed. A file this version cannot write
/// (another format, or not writable) is left alone and the error returned.
pub fn record_issue_settings(path: &std::path::Path, s: &spiderlens_config::IssueSettings) -> Result<()> {
    let conn = crate::reader::connect_writable(path)?;
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
        build_hits(conn, features, settings, &mut |_| {}, &[])?;
    }
    Ok(())
}
