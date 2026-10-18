//! The end-of-crawl report (spec 010, plan D1).
//!
//! A report is one value: a short document made of sections and blocks,
//! built from the crawl file and the live issues of spec 009. The desktop
//! Report view, `spiderlens report --audit` (text and JSON) and the HTML
//! report file all render this value, so they cannot disagree (FR-005).
//! Nothing is stored in the crawl file (plan D5).

pub mod html;
mod score;
mod sections;

use serde::Serialize;
use ts_rs::TS;

pub use score::{IssueCost, PriorityLoss, SCORE_VERSION, Score, WEIGHTS, weight};
pub use sections::{SECTIONS, SectionDef};

use crate::Result;
use crate::issues::Priority;
use crate::reader::Reader;
use crate::view::UrlScope;

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct Report {
    pub meta: ReportMeta,
    /// The crawl can still change: figures are marked provisional (FR-006).
    pub provisional: bool,
    /// Set when the crawl did not finish everything it found (FR-007).
    pub incomplete: Option<Incomplete>,
    pub score: Score,
    pub sections: Vec<Section>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct ReportMeta {
    pub start_url: Option<String>,
    /// The crawl's status as stored: running, paused, finished, stopped.
    pub status: String,
    #[ts(type = "number")]
    pub active_ms: u64,
    pub profile: Option<String>,
    pub app_version: String,
    pub rules_version: u32,
    pub score_version: u32,
    /// Issue settings differ from the defaults (US2 scenario 4).
    pub custom_issue_settings: bool,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct Incomplete {
    #[ts(type = "number")]
    pub not_crawled: u64,
    /// Why, with counts: "Over URL limit", "Crawl stopped", "Still queued",
    /// "Failed" (by error class lives in the completeness section).
    pub reasons: Vec<ReasonCount>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct ReasonCount {
    pub label: String,
    #[ts(type = "number")]
    pub count: u64,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct Section {
    pub id: String,
    pub title: String,
    pub state: SectionState,
    pub blocks: Vec<Block>,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(export)]
pub enum SectionState {
    Available,
    Provisional,
    Unavailable { reason: String },
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(export)]
pub enum Block {
    Figures {
        items: Vec<Figure>,
    },
    Table {
        columns: Vec<String>,
        rows: Vec<Vec<Cell>>,
    },
    Bars {
        items: Vec<Bar>,
    },
    Text {
        text: String,
    },
    /// One issue with its texts and, in the report file, some of its URLs.
    Issue {
        issue: IssueBlock,
    },
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct Figure {
    pub label: String,
    pub value: String,
    pub target: Option<Target>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct Cell {
    pub text: String,
    pub target: Option<Target>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct Bar {
    pub label: String,
    #[ts(type = "number")]
    pub value: u64,
    /// Share of the largest bar, 0 to 1.
    pub share: f64,
    pub target: Option<Target>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct IssueBlock {
    pub id: String,
    pub name: String,
    pub category: String,
    pub priority: Priority,
    /// Null when the rule is site-level (plan D4) or cannot run.
    #[ts(type = "number | null")]
    pub count: Option<u64>,
    pub provisional: bool,
    pub unavailable: Option<String>,
    pub why: String,
    pub fix: String,
    /// Score points fixing this issue alone would return (plan D3).
    pub gain: f64,
}

/// Where a figure leads in the results table (FR-004).
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(export)]
pub enum Target {
    Tab {
        tab: String,
        filter: String,
        scope: UrlScope,
    },
    Issue {
        id: String,
    },
}

impl Target {
    pub fn tab(tab: &str, filter: &str) -> Self {
        Self::Tab {
            tab: tab.into(),
            filter: filter.into(),
            scope: UrlScope::All,
        }
    }
    pub fn scoped(tab: &str, filter: &str, scope: UrlScope) -> Self {
        Self::Tab {
            tab: tab.into(),
            filter: filter.into(),
            scope,
        }
    }
    pub fn issue(id: &str) -> Self {
        Self::Issue { id: id.into() }
    }
}

impl Figure {
    pub fn new(label: impl Into<String>, value: impl ToString, target: Option<Target>) -> Self {
        Self {
            label: label.into(),
            value: value.to_string(),
            target,
        }
    }
}

/// What the caller knows that the file does not.
#[derive(Debug, Clone, Default)]
pub struct ReportOptions {
    /// The crawl is running or paused in this process: counts can change.
    pub running: bool,
    /// The active profile's name, when the app has one.
    pub profile: Option<String>,
    /// Sections the caller built from other sources, appended after the
    /// built-in ones (the comparison section of spec 012 FR-033). Every
    /// output renders them like any other section.
    pub extra_sections: Vec<Section>,
}

impl Reader {
    /// Builds the report from what the crawl file holds now (plan D5).
    pub fn report(&mut self, opts: &ReportOptions) -> Result<Report> {
        let status: String = self.meta("status")?.unwrap_or_else(|| "finished".into());
        let running = opts.running || status == "running";
        let summary = self.issues_summary(running)?;
        let score = self.score(&summary)?;
        let ctx = sections::Ctx {
            running,
            summary: &summary,
            score: &score,
        };
        let mut out = Vec::with_capacity(SECTIONS.len());
        for def in SECTIONS {
            out.push(sections::build(self, def, &ctx));
        }
        out.extend(opts.extra_sections.iter().cloned());
        let incomplete = sections::incomplete(self)?;
        let meta = ReportMeta {
            start_url: self.meta("start_url")?,
            status,
            active_ms: self.meta("active_ms")?.and_then(|v| v.parse().ok()).unwrap_or(0),
            profile: opts.profile.clone(),
            app_version: spiderlens_engine::BuildInfo::current()
                .product_version
                .to_string(),
            rules_version: crate::issues::RULES_VERSION,
            score_version: SCORE_VERSION,
            custom_issue_settings: self.issue_settings.rules
                != crate::issues::ResolvedSettings::default().rules,
        };
        Ok(Report {
            meta,
            provisional: running,
            incomplete,
            score,
            sections: out,
        })
    }

    pub(crate) fn meta(&self, key: &str) -> Result<Option<String>> {
        use rusqlite::OptionalExtension;
        Ok(self
            .conn
            .query_row("SELECT value FROM meta WHERE key = ?1", [key], |r| r.get(0))
            .optional()?)
    }
}

impl Reader {
    /// The first `limit` URLs of an issue, in address order, with their
    /// evidence, and the total (FR-015).
    pub fn issue_urls(&mut self, id: &str, limit: u32) -> Result<html::IssueUrls> {
        self.ensure_derived([crate::tabs::Derived::IssueHits])?;
        let Some(idx) = crate::issues::index_of(id) else {
            return Ok(html::IssueUrls::default());
        };
        let rule = &crate::issues::catalogue()[idx];
        let total: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM temp.issue_hits WHERE rule = ?1",
            [idx as i64],
            |r| r.get(0),
        )?;
        let mut stmt = self.conn.prepare(&format!(
            "SELECT u.url, {} FROM temp.issue_hits h JOIN urls u ON u.id = h.url_id \
             WHERE h.rule = ?1 ORDER BY u.url LIMIT ?2",
            rule.evidence
        ))?;
        let urls = stmt
            .query_map((idx as i64, limit as i64), |r| {
                let ev = match r.get::<_, rusqlite::types::Value>(1)? {
                    rusqlite::types::Value::Text(t) => Some(t),
                    rusqlite::types::Value::Integer(n) => Some(n.to_string()),
                    rusqlite::types::Value::Real(f) => Some(f.to_string()),
                    _ => None,
                };
                Ok((r.get::<_, String>(0)?, ev))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(html::IssueUrls {
            urls,
            total: total as u64,
        })
    }

    /// The report file (FR-013 to FR-017).
    pub fn report_html(&mut self, opts: &ReportOptions, file: &html::FileOptions) -> Result<String> {
        file.validate().map_err(crate::Error::Invalid)?;
        let report = self.report(opts)?;
        let mut lists = std::collections::HashMap::new();
        if file.urls_per_issue > 0 {
            for s in &report.sections {
                for b in &s.blocks {
                    if let Block::Issue { issue } = b {
                        lists.insert(issue.id.clone(), self.issue_urls(&issue.id, file.urls_per_issue)?);
                    }
                }
            }
        }
        Ok(html::render(&report, file, |id| {
            lists.remove(id).unwrap_or_default()
        }))
    }
}
