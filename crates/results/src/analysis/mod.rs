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

use std::sync::atomic::{AtomicBool, Ordering};

use rusqlite::OptionalExtension;
use serde::Serialize;
use ts_rs::TS;

use crate::reader::Reader;
use crate::tabs::{Derived, Need};
use crate::{Error, Result};

/// Where a running analysis is (FR-013), for the status bar and the CLI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct AnalysisProgress {
    /// "positions" or "link_score".
    pub phase: &'static str,
    /// Pages classified, or URLs whose links were read.
    #[ts(type = "number")]
    pub done: u64,
    #[ts(type = "number")]
    pub total: u64,
}

/// Progress reports and the cancel flag, handed to each computation.
pub struct Watch<'a> {
    on: &'a mut dyn FnMut(AnalysisProgress),
    cancel: &'a AtomicBool,
}

impl<'a> Watch<'a> {
    pub fn new(on: &'a mut dyn FnMut(AnalysisProgress), cancel: &'a AtomicBool) -> Self {
        Self { on, cancel }
    }

    /// Runs `f` with nobody watching and no way to cancel.
    pub fn quiet<R>(f: impl FnOnce(&mut Watch<'_>) -> R) -> R {
        let never = AtomicBool::new(false);
        let mut nop = |_| {};
        f(&mut Watch::new(&mut nop, &never))
    }

    /// Reports progress; fails with SQLITE_INTERRUPT once cancelled, the
    /// same error an interrupted query gives.
    pub(crate) fn tick(&mut self, phase: &'static str, done: u64, total: u64) -> rusqlite::Result<()> {
        (self.on)(AnalysisProgress { phase, done, total });
        if self.cancel.load(Ordering::Relaxed) {
            return Err(rusqlite::Error::SqliteFailure(
                rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_INTERRUPT),
                Some("analysis cancelled".into()),
            ));
        }
        Ok(())
    }

    /// Runs `f` inside savepoint `name`, rolled back if `f` fails.
    pub(crate) fn savepoint<T>(
        conn: &rusqlite::Connection,
        name: &str,
        f: impl FnOnce() -> rusqlite::Result<T>,
    ) -> rusqlite::Result<T> {
        conn.execute_batch(&format!("SAVEPOINT {name}"))?;
        match f() {
            Ok(v) => {
                conn.execute_batch(&format!("RELEASE {name}"))?;
                Ok(v)
            }
            Err(e) => {
                let _ = conn.execute_batch(&format!("ROLLBACK TO {name}; RELEASE {name}"));
                Err(e)
            }
        }
    }

    fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }
}

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
        let never = AtomicBool::new(false);
        let run = self.analyse_with(&mut |_| {}, &never)?;
        Ok(run.expect("never cancelled"))
    }

    /// [`Reader::analyse`] with progress reports. Setting `cancel` (or
    /// interrupting the connection) stops it: the result is then None and
    /// the results from before the run are left as they were (FR-013).
    pub fn analyse_with(
        &mut self,
        on: &mut dyn FnMut(AnalysisProgress),
        cancel: &AtomicBool,
    ) -> Result<Option<AnalysisRun>> {
        if !self.features.has(Need::Analysis) {
            return Err(Error::invalid(Need::Analysis.unavailable_reason()));
        }
        let started = std::time::Instant::now();
        let mut watch = Watch::new(on, cancel);
        // One savepoint around both tables: a cancelled run rolls back to
        // the tables (or their absence) from before.
        let conn = &self.conn;
        let settings = &self.features.link_settings;
        let built = Watch::savepoint(conn, "analysis", || {
            let positions = link_positions::build(conn, &settings.position_rules, &mut watch)?;
            let metrics = link_metrics::build(conn, settings.weighting.as_ref(), &mut watch)?;
            Ok((positions, metrics))
        });
        let (positions, metrics) = match built {
            Ok(b) => b,
            Err(_) if watch.cancelled() => return Ok(None),
            Err(e) => return Err(e.into()),
        };
        self.mark_built(Derived::LinkPositions)?;
        self.mark_built(Derived::LinkMetrics)?;
        Ok(Some(AnalysisRun {
            urls: metrics.urls,
            links: positions.links,
            rounds: metrics.rounds,
            pages_without_html: positions.pages_without_html,
            weighted: self.features.link_settings.weighting.is_some(),
            ms: started.elapsed().as_millis().min(u128::from(u32::MAX)) as u32,
        }))
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
