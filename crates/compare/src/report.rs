//! The comparison section of the end-of-crawl report (FR-033, plan D12)
//! and the JSON the command line prints (FR-029).

use serde::Serialize;
use spiderlens_config::MapRule;
use ts_rs::TS;

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
    /// Issue deltas arrive with the issue rules engine (spec 009); until
    /// then this is false and the list is empty.
    pub issues_available: bool,
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
        Ok(ComparisonReport {
            top_filters: top,
            elements,
            issues_available: false,
            mapping: self.settings.mapping.clone(),
            summary,
        })
    }
}
