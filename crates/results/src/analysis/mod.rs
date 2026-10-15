//! Crawl Analysis: whole-crawl computations that run after fetching ends
//! (spec 009 plan D12, specs 022 and 023).
//!
//! Two parts live here. Link positions and Link Score (spec 023 stage 1) are
//! built by the reader in its `temp` schema on demand, like the other derived
//! tables (plan D6). Columns name the tables unqualified, so a stored copy in
//! `main` and a fresh copy in `temp` are read the same way (plan D5).
//!
//! The shared runner from spec 009 (plan D12) follows: each global
//! computation is a [`Pass`] listed in [`PASSES`]; the runner orders them by
//! their dependencies, reruns the stale ones, records each in the `analysis`
//! meta key and reports progress. Passes run when a crawl ends, by the
//! process that wrote it, and on demand for an opened file; on a read-only
//! file they run into the reader connection's `temp` schema. Moving Link
//! Score onto it is spec 023 stage 2.

pub mod link_metrics;
pub mod link_positions;

use std::collections::{BTreeMap, HashSet};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::features::Features;
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

/// Every pass, in tie-break order. A feature adds its pass file and one
/// line here.
pub static PASSES: &[&dyn Pass] = &[&crate::sitemaps::SitemapAudit, &crate::sitemaps::Orphans];

/// One global computation over the crawl.
pub trait Pass: Sync {
    /// Also names its output tables: "link_score", "near_duplicates", ...
    fn id(&self) -> &'static str;
    /// Bump to make files analysed by an older build rerun this pass.
    fn version(&self) -> u32;
    /// Passes whose output this one reads.
    fn after(&self) -> &'static [&'static str] {
        &[]
    }
    /// Crawl data it reads; when missing the pass is skipped with the reason.
    fn needs(&self) -> &'static [Need] {
        &[]
    }
    /// Target time per 100k URLs, as stated in the pass's own spec.
    fn budget(&self) -> Duration;
    /// Replaces the pass's output tables in `cx.schema`.
    fn run(&self, cx: &mut PassContext<'_>) -> Result<()>;
}

/// What a pass gets to work with.
pub struct PassContext<'a> {
    pub conn: &'a Connection,
    /// `main`, or `temp` for a read-only file.
    pub schema: &'static str,
    /// URLs in the crawl, for batching and progress.
    pub urls: u64,
    pass: &'static str,
    progress: &'a mut dyn FnMut(PassProgress),
    cancel: &'a AtomicBool,
}

impl PassContext<'_> {
    /// Reports progress ("Link Score", 40 of 100) and fails with
    /// [`Error::Cancelled`] once the phase was cancelled: call it between
    /// batches.
    pub fn progress(&mut self, done: u64, total: u64, label: &str) -> Result<()> {
        (self.progress)(PassProgress {
            pass: self.pass.into(),
            label: label.into(),
            done,
            total,
        });
        self.check()
    }

    pub fn check(&self) -> Result<()> {
        if self.cancel.load(Ordering::Relaxed) {
            return Err(Error::Cancelled);
        }
        Ok(())
    }
}

/// The `passes-progress` event (plan D12).
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PassProgress {
    pub pass: String,
    pub label: String,
    #[ts(type = "number")]
    pub done: u64,
    #[ts(type = "number")]
    pub total: u64,
}

/// What a run did.
#[derive(Debug, Clone, Default, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AnalysisReport {
    /// Passes run, with their time in ms.
    #[ts(type = "Array<[string, number]>")]
    pub ran: Vec<(String, u64)>,
    /// Passes skipped, with the reason (missing crawl data).
    pub skipped: Vec<(String, String)>,
    /// False when the results are in the session's temp schema only.
    pub saved: bool,
}

/// One pass's entry in the `analysis` meta key.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
struct Record {
    version: u32,
    /// [`data_stamp`] when the pass ran.
    data_version: i64,
    ms: u64,
}

type Records = BTreeMap<String, Record>;

/// Changes whenever more URLs are crawled (a resume).
fn data_stamp(conn: &Connection) -> Result<i64> {
    Ok(conn.query_row(
        "SELECT COALESCE(MAX(fetched_at), 0) + COUNT(*) FROM urls",
        [],
        |r| r.get(0),
    )?)
}

fn records(conn: &Connection) -> Result<Records> {
    let raw: Option<String> = conn
        .query_row("SELECT value FROM meta WHERE key = 'analysis'", [], |r| r.get(0))
        .optional()?;
    Ok(raw
        .and_then(|r| serde_json::from_str(&r).ok())
        .unwrap_or_default())
}

/// `passes` in an order where each comes after the ones it reads; ties keep
/// the list order. Unknown or circular dependencies are an error.
pub fn order<'p>(passes: &[&'p dyn Pass]) -> Result<Vec<&'p dyn Pass>> {
    let ids: HashSet<_> = passes.iter().map(|p| p.id()).collect();
    for p in passes {
        if let Some(d) = p.after().iter().find(|d| !ids.contains(*d)) {
            return Err(Error::invalid(format!("pass {} reads unknown pass {d}", p.id())));
        }
    }
    let mut done = HashSet::new();
    let mut out = Vec::with_capacity(passes.len());
    while out.len() < passes.len() {
        let next = passes
            .iter()
            .find(|p| !done.contains(p.id()) && p.after().iter().all(|d| done.contains(d)))
            .ok_or_else(|| Error::invalid("analysis passes depend on each other in a circle"))?;
        done.insert(next.id());
        out.push(*next);
    }
    Ok(out)
}

/// The passes that need to run: never run, older version, or run before
/// more URLs were crawled; and every pass that reads one of those.
fn stale<'p>(ordered: &[&'p dyn Pass], records: &Records, stamp: i64) -> Vec<&'p dyn Pass> {
    let mut rerun: HashSet<&str> = HashSet::new();
    let mut out = Vec::new();
    for p in ordered {
        let current = records
            .get(p.id())
            .is_some_and(|r| r.version == p.version() && r.data_version == stamp);
        if !current || p.after().iter().any(|d| rerun.contains(d)) {
            rerun.insert(p.id());
            out.push(*p);
        }
    }
    out
}

/// True when some pass of [`PASSES`] has not run on this crawl's data.
pub fn needs_analysis(conn: &Connection) -> Result<bool> {
    let ordered = order(PASSES)?;
    Ok(!stale(&ordered, &records(conn)?, data_stamp(conn)?).is_empty())
}

/// Runs the stale passes of [`PASSES`] on a crawl file and saves their
/// outputs (the process that wrote the crawl, or `spiderlens analyse`).
pub fn analyse_file(
    path: &Path,
    progress: &mut dyn FnMut(PassProgress),
    cancel: &AtomicBool,
) -> Result<AnalysisReport> {
    let conn = crate::reader::connect_writable(path)?;
    crate::sql::register(&conn)?;
    run(&conn, "main", PASSES, progress, cancel)
}

/// Runs `passes` (the stale ones) on `conn` into `schema`. Each pass and its
/// record commit together; a failed or cancelled pass leaves the earlier
/// ones in place.
pub(crate) fn run(
    conn: &Connection,
    schema: &'static str,
    passes: &[&dyn Pass],
    progress: &mut dyn FnMut(PassProgress),
    cancel: &AtomicBool,
) -> Result<AnalysisReport> {
    let ordered = order(passes)?;
    let stamp = data_stamp(conn)?;
    let saved = schema == "main";
    let mut recs = if saved { records(conn)? } else { Records::new() };
    let todo = stale(&ordered, &recs, stamp);
    let mut report = AnalysisReport {
        saved,
        ..Default::default()
    };
    if todo.is_empty() {
        return Ok(report);
    }
    let features = Features::detect(conn)?;
    let urls = conn.query_row("SELECT COUNT(*) FROM urls", [], |r| r.get::<_, i64>(0))? as u64;
    for p in todo {
        if let Some(n) = p.needs().iter().find(|n| !features.has(**n)) {
            report
                .skipped
                .push((p.id().into(), n.unavailable_reason().into()));
            continue;
        }
        let t0 = Instant::now();
        conn.execute_batch("SAVEPOINT analysis_pass")?;
        let mut cx = PassContext {
            conn,
            schema,
            urls,
            pass: p.id(),
            progress: &mut *progress,
            cancel,
        };
        let result = cx.check().and_then(|()| p.run(&mut cx)).and_then(|()| {
            let ms = t0.elapsed().as_millis() as u64;
            if saved {
                recs.insert(
                    p.id().into(),
                    Record {
                        version: p.version(),
                        data_version: stamp,
                        ms,
                    },
                );
                conn.execute(
                    "INSERT INTO meta (key, value) VALUES ('analysis', ?1)
                     ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                    [serde_json::to_string(&recs).expect("records serialize")],
                )?;
                log_time(conn, p.id(), ms)?;
            }
            Ok(ms)
        });
        match result {
            Ok(ms) => {
                conn.execute_batch("RELEASE analysis_pass")?;
                report.ran.push((p.id().into(), ms));
            }
            Err(e) => {
                conn.execute_batch("ROLLBACK TO analysis_pass; RELEASE analysis_pass")?;
                return Err(e);
            }
        }
    }
    Ok(report)
}

/// The time per pass goes to the crawl log so slow passes are visible.
fn log_time(conn: &Connection, pass: &str, ms: u64) -> Result<()> {
    let has_events = conn
        .prepare("SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'events'")?
        .exists([])?;
    if has_events {
        use spiderlens_engine::crawl::store::{APP_VERSION, os_name};
        conn.execute(
            "INSERT INTO events (at, kind, app_version, os, detail) VALUES (?1, 'analysis', ?2, ?3, ?4)",
            (
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_or(0, |d| d.as_millis() as i64),
                APP_VERSION,
                os_name(),
                serde_json::json!({ "pass": pass, "ms": ms }).to_string(),
            ),
        )?;
    }
    Ok(())
}

impl crate::Reader {
    /// Runs every pass into this connection's `temp` schema, for a file this
    /// session cannot write (a read-only viewer): results show until the
    /// file is closed.
    pub fn analyse_in_session(
        &mut self,
        progress: &mut dyn FnMut(PassProgress),
        cancel: &AtomicBool,
    ) -> Result<AnalysisReport> {
        let report = run(&self.conn, "temp", PASSES, progress, cancel)?;
        if !report.ran.is_empty() {
            for n in crate::features::pass_needs(&self.conn)? {
                if !self.features.available.contains(&n) {
                    self.features.available.push(n);
                }
            }
            self.derived.remove(&crate::tabs::Derived::IssueHits);
            self.bump_version();
        }
        Ok(report)
    }
}

#[cfg(test)]
mod tests;
