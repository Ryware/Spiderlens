//! The Crawl Analysis phase (spec 009 plan D12): global computations over a
//! whole crawl (Link Score, near duplicates, orphans, ...) that are too slow
//! to repeat on every open. Each is a [`Pass`] listed in [`PASSES`]; the
//! runner orders them by their dependencies, reruns the stale ones, records
//! each in the `analysis` meta key and reports progress.
//!
//! Passes run when a crawl ends, by the process that wrote it, and on demand
//! for an opened file. On a read-only file they run into the reader
//! connection's `temp` schema, so results show for the session only.

use std::collections::{BTreeMap, HashSet};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::features::Features;
use crate::tabs::Need;
use crate::{Error, Result};

/// Every pass, in tie-break order. A feature adds its pass file and one
/// line here.
pub static PASSES: &[&dyn Pass] = &[];

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
    progress: &'a mut dyn FnMut(AnalysisProgress),
    cancel: &'a AtomicBool,
}

impl PassContext<'_> {
    /// Reports progress ("Link Score", 40 of 100) and fails with
    /// [`Error::Cancelled`] once the phase was cancelled: call it between
    /// batches.
    pub fn progress(&mut self, done: u64, total: u64, label: &str) -> Result<()> {
        (self.progress)(AnalysisProgress {
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

/// The `analysis_progress` event (plan D12).
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AnalysisProgress {
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
    progress: &mut dyn FnMut(AnalysisProgress),
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
    progress: &mut dyn FnMut(AnalysisProgress),
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
        progress: &mut dyn FnMut(AnalysisProgress),
        cancel: &AtomicBool,
    ) -> Result<AnalysisReport> {
        let report = run(&self.conn, "temp", PASSES, progress, cancel)?;
        if !report.ran.is_empty() {
            self.derived.remove(&crate::tabs::Derived::IssueHits);
            self.bump_version();
        }
        Ok(report)
    }
}

#[cfg(test)]
mod tests;
