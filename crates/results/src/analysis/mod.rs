//! Crawl Analysis: whole-crawl computations that run after fetching ends
//! (spec 009 plan D12, specs 022 and 023).
//!
//! Each computation writes its own table. Until the shared runner from spec
//! 009 lands, the reader builds these tables in its `temp` schema on demand,
//! like the other derived tables (spec 023 plan D6, "stage 1"). Columns name
//! the tables unqualified, so a stored copy in `main` and a fresh copy in
//! `temp` are read the same way (plan D5).

pub mod link_metrics;
pub mod link_positions;

use rusqlite::OptionalExtension;
use serde::Serialize;
use ts_rs::TS;

use crate::reader::Reader;
use crate::tabs::{Derived, Need};
use crate::{Error, Result};

/// What one analysis run covered (FR-014), for the status bar and the
/// detail of the result.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct AnalysisRun {
    #[ts(type = "number")]
    pub urls: u64,
    #[ts(type = "number")]
    pub links: u64,
    pub rounds: u32,
    /// Links on pages whose HTML was not stored (position Unknown).
    #[ts(type = "number")]
    pub pages_without_html: u64,
    pub weighted: bool,
    pub ms: u32,
}

impl Reader {
    /// Runs Crawl Analysis now (stage 1: into this reader's temp tables):
    /// link positions, then Link Score and the link counts.
    pub fn analyse(&mut self) -> Result<AnalysisRun> {
        if !self.features.has(Need::Analysis) {
            return Err(Error::invalid(Need::Analysis.unavailable_reason()));
        }
        let started = std::time::Instant::now();
        let positions = link_positions::build(&self.conn, &self.features.link_settings.position_rules)?;
        self.mark_built(Derived::LinkPositions)?;
        let metrics = link_metrics::build(&self.conn, self.features.link_settings.weighting.as_ref())?;
        self.mark_built(Derived::LinkMetrics)?;
        Ok(AnalysisRun {
            urls: metrics.urls,
            links: positions.links,
            rounds: metrics.rounds,
            pages_without_html: positions.pages_without_html,
            weighted: self.features.link_settings.weighting.is_some(),
            ms: started.elapsed().as_millis().min(u128::from(u32::MAX)) as u32,
        })
    }

    /// [`Reader::test_positions`] for a URL as typed. None when the URL is
    /// not in the crawl or its HTML was not stored.
    pub fn test_positions_url(
        &self,
        url: &str,
        rules: &[spiderlens_config::PositionRule],
    ) -> Result<Option<Vec<link_positions::TestedLink>>> {
        let url = url.trim();
        let id: Option<i64> = self
            .conn
            .query_row(
                "SELECT id FROM urls WHERE url = ?1 OR url = ?1 || '/' ORDER BY length(url) LIMIT 1",
                [url],
                |r| r.get(0),
            )
            .optional()?;
        match id {
            Some(id) => self.test_positions(id, rules),
            None => Ok(None),
        }
    }

    /// The rules tester (FR-022): `rules` applied to one stored page,
    /// nothing written. None when the page's HTML was not stored.
    pub fn test_positions(
        &self,
        url_id: i64,
        rules: &[spiderlens_config::PositionRule],
    ) -> Result<Option<Vec<link_positions::TestedLink>>> {
        Ok(link_positions::test_page(&self.conn, url_id, rules)?)
    }
}
