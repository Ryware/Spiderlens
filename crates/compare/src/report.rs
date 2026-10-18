//! The comparison section of the end-of-crawl report (FR-033, plan D12)
//! and the JSON the command line prints (FR-029).

use serde::Serialize;
use spiderlens_config::{IssueSettingsChoice, MapRule};
use spiderlens_results::issues::{Priority, fmt_num};
use spiderlens_results::report::{Block, Cell, Figure, Section, SectionState};
use ts_rs::TS;

use crate::info::CrawlInfo;
use crate::issues::{IssueBasis, IssueDelta};
use crate::summary::{FilterDelta, Summary};
use crate::{Comparison, Group, Result};

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct ElementReport {
    pub element: String,
    pub label: String,
    #[ts(type = "number")]
    pub changed: u64,
    pub comparable: bool,
    /// Up to 20 changed URLs (current address, else previous).
    pub examples: Vec<String>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct ComparisonReport {
    pub summary: Summary,
    /// The ten comparable filters with the largest absolute change.
    pub top_filters: Vec<FilterDelta>,
    pub elements: Vec<ElementReport>,
    /// True when the issue deltas below were computed.
    pub issues_available: bool,
    /// Comparable issues that changed, by priority.
    pub issues: Vec<IssueDelta>,
    pub issue_basis: IssueBasis,
    pub mapping: Vec<MapRule>,
}

impl Comparison {
    pub fn report(&mut self) -> Result<ComparisonReport> {
        let summary = self.summary.clone();
        let mut top: Vec<FilterDelta> = summary
            .filters
            .iter()
            .filter(|f| f.comparable && f.change() != 0)
            .cloned()
            .collect();
        top.sort_by_key(|f| std::cmp::Reverse(f.change().unsigned_abs()));
        top.truncate(10);
        let mut elements = Vec::new();
        for e in summary.elements.iter().filter(|e| e.on) {
            let mut examples = Vec::new();
            if e.changed > 0 {
                let page = self.open_group(
                    &Group::Element {
                        element: e.element.clone(),
                    },
                    "",
                    None,
                    20,
                )?;
                examples = page
                    .rows
                    .iter()
                    .filter_map(|r| r.values[1].as_str().or(r.values[0].as_str()).map(str::to_string))
                    .collect();
                self.close_group(page.view)?;
            }
            elements.push(ElementReport {
                element: e.element.clone(),
                label: e.label.clone(),
                changed: e.changed,
                comparable: e.comparable,
                examples,
            });
        }
        let issues = summary.issues.iter().filter(|i| i.changed()).cloned().collect();
        Ok(ComparisonReport {
            top_filters: top,
            elements,
            issues_available: true,
            issues,
            issue_basis: summary.issue_basis,
            mapping: self.settings.mapping.clone(),
            summary,
        })
    }
}

fn int(n: u64) -> String {
    fmt_num(n as f64)
}

/// A change with its sign: "+12", "-3", "0".
fn signed(n: i64) -> String {
    match n {
        0 => "0".into(),
        n if n > 0 => format!("+{}", fmt_num(n as f64)),
        n => fmt_num(n as f64),
    }
}

/// A count, or a dash when that crawl cannot give one.
fn count(n: Option<u64>) -> String {
    n.map_or_else(|| "–".into(), int)
}

fn text_cell(text: impl Into<String>) -> Cell {
    Cell {
        text: text.into(),
        target: None,
    }
}

fn row(cells: impl IntoIterator<Item = String>) -> Vec<Cell> {
    cells.into_iter().map(text_cell).collect()
}

/// "2026-10-14 · 1,200 URLs": the crawl's date (the day it started, when
/// recorded) and its size.
fn crawl_line(c: &CrawlInfo) -> String {
    let date = c
        .started_at
        .as_deref()
        .and_then(|t| t.get(..10))
        .unwrap_or("date not recorded");
    format!("{date} · {} URLs", int(c.urls))
}

fn basis_line(b: &IssueBasis) -> String {
    let whose = match b.settings {
        IssueSettingsChoice::Current => "the current crawl's",
        IssueSettingsChoice::Previous => "the previous crawl's",
        IssueSettingsChoice::App => "the app's",
    };
    format!(
        "Both crawls were evaluated with rule set v{} and {whose} issue settings{}.",
        b.rules_version,
        if b.custom { " (customised)" } else { "" }
    )
}

impl ComparisonReport {
    /// The report section "Changes since the previous crawl" (FR-033). It
    /// uses only the report's own block kinds, so the desktop view, the text
    /// and JSON outputs and the HTML file all render it; its figures point
    /// into another file and so carry no targets.
    pub fn section(&self) -> Section {
        let s = &self.summary;
        let mut blocks = vec![Block::Figures {
            items: vec![
                Figure::new("Previous crawl", crawl_line(&s.previous), None),
                Figure::new("Current crawl", crawl_line(&s.current), None),
                Figure::new("New", int(s.new), None),
                Figure::new("Missing", int(s.missing), None),
                Figure::new("Not reached", int(s.not_reached), None),
                Figure::new("Matched", int(s.matched), None),
            ],
        }];
        if !s.config_diffs.is_empty() {
            blocks.push(Block::Table {
                columns: ["Setting", "Previous", "Current"].map(String::from).to_vec(),
                rows: s
                    .config_diffs
                    .iter()
                    .map(|d| row([d.label.clone(), d.previous.clone(), d.current.clone()]))
                    .collect(),
            });
        }
        if !s.by_state.is_empty() {
            blocks.push(Block::Table {
                columns: ["State", "Previous", "Current", "Change"]
                    .map(String::from)
                    .to_vec(),
                rows: s
                    .by_state
                    .iter()
                    .map(|d| {
                        row([
                            d.state.clone(),
                            int(d.previous),
                            int(d.current),
                            signed(d.current as i64 - d.previous as i64),
                        ])
                    })
                    .collect(),
            });
        }
        if !self.top_filters.is_empty() {
            blocks.push(Block::Table {
                columns: ["Filter", "Previous", "Current", "Change"]
                    .map(String::from)
                    .to_vec(),
                rows: self
                    .top_filters
                    .iter()
                    .map(|f| {
                        row([
                            format!("{} › {}", f.tab_title, f.filter_label),
                            count(f.previous),
                            count(f.current),
                            signed(f.change()),
                        ])
                    })
                    .collect(),
            });
        }
        if self.issues_available {
            blocks.push(Block::Text {
                text: basis_line(&self.issue_basis),
            });
            if self.issues.is_empty() {
                blocks.push(Block::Text {
                    text: "No issue changed between the crawls.".into(),
                });
            } else {
                // Grouped by priority: the list is already in priority order.
                let label = |p: Priority| p.label().to_string();
                blocks.push(Block::Table {
                    columns: ["Priority", "Issue", "Newly affected", "Fixed", "Change"]
                        .map(String::from)
                        .to_vec(),
                    rows: self
                        .issues
                        .iter()
                        .map(|i| {
                            row([
                                label(i.priority),
                                i.name.clone(),
                                int(i.newly_affected),
                                int(i.fixed),
                                signed(i.change()),
                            ])
                        })
                        .collect(),
                });
            }
        }
        for e in self.elements.iter().filter(|e| e.comparable && e.changed > 0) {
            blocks.push(Block::Table {
                columns: vec![format!("{}: {} changed pages", e.label, int(e.changed))],
                rows: e.examples.iter().take(20).map(|u| row([u.clone()])).collect(),
            });
        }
        if !self.mapping.is_empty() {
            let rules: Vec<String> = self
                .mapping
                .iter()
                .enumerate()
                .map(|(n, r)| {
                    format!(
                        "{}. {}{} → {}",
                        n + 1,
                        if r.regex { "(regex) " } else { "" },
                        r.find,
                        r.replace
                    )
                })
                .collect();
            blocks.push(Block::Text {
                text: format!(
                    "URL mapping rules, applied to the previous crawl in order: {}",
                    rules.join("; ")
                ),
            });
        }
        Section {
            id: "comparison".into(),
            title: "Changes since the previous crawl".into(),
            state: SectionState::Available,
            blocks,
        }
    }
}
