//! Near duplicate content commands (spec 020,
//! specs/020-near-duplicate-content/contracts/desktop-commands.md).
//!
//! Queries run on the results reader thread, like every other results read.
//! The analysis pass runs on its own thread with its own writing connection
//! and announces itself with two events:
//!
//! - `analysis-progress` while it runs,
//! - `analysis-finished` when it completes, fails or is cancelled.
//!
//! A file that cannot be written is analysed on the reader's connection
//! instead, into its `temp` schema, so the results last while it is open.

use std::collections::HashMap;
use std::fs::File;
use std::io::BufWriter;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use serde::{Deserialize, Serialize};
use spiderlens_config::ContentSettings;
use spiderlens_results::near_dup::{
    self, ClustersPage, Comparison, Connection, Listing, MembersPage, NearDupState, NearDupStatus, PairsPage,
    PassSummary, Schema,
};
use tauri::{AppHandle, Emitter, Manager};

use crate::results;

type Cmd<T> = Result<T, String>;

/// Analyses running now, by file, with their cancel flags.
#[derive(Default)]
pub struct State {
    running: Mutex<HashMap<PathBuf, Arc<AtomicBool>>>,
}

impl State {
    fn is_running(&self, path: &Path) -> bool {
        self.running.lock().expect("near dup lock").contains_key(path)
    }

    /// Marks `path` as being analysed; `None` when it already is.
    fn claim(&self, path: &Path) -> Option<Arc<AtomicBool>> {
        let mut running = self.running.lock().expect("near dup lock");
        if running.contains_key(path) {
            return None;
        }
        let cancel = Arc::new(AtomicBool::new(false));
        running.insert(path.to_path_buf(), cancel.clone());
        Some(cancel)
    }

    fn release(&self, path: &Path) {
        self.running.lock().expect("near dup lock").remove(path);
    }
}

#[derive(Serialize, Clone)]
struct ProgressEvent {
    path: PathBuf,
    done: usize,
    total: usize,
    step: String,
}

#[derive(Serialize, Clone)]
struct FinishedEvent {
    path: PathBuf,
    summary: Option<PassSummary>,
    cancelled: bool,
    error: Option<String>,
}

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

fn settings_or_effective(conn: &Connection, settings: Option<ContentSettings>) -> Cmd<ContentSettings> {
    match settings {
        Some(s) => Ok(s),
        None => near_dup::effective_settings(conn).map_err(err),
    }
}

/// Progress callback that emits at most a few events a second.
fn progress(app: &AppHandle, path: &Path) -> impl FnMut(usize, usize, &str) {
    let (app, path) = (app.clone(), path.to_path_buf());
    let mut last: Option<Instant> = None;
    move |done, total, step| {
        if last.is_some_and(|t| t.elapsed().as_millis() < 200) && done != 0 && done != total {
            return;
        }
        last = Some(Instant::now());
        let _ = app.emit(
            "analysis-progress",
            ProgressEvent {
                path: path.clone(),
                done,
                total,
                step: step.to_string(),
            },
        );
    }
}

fn finish(
    app: &AppHandle,
    path: &Path,
    result: Result<PassSummary, spiderlens_results::Error>,
    cancel: &AtomicBool,
) {
    app.state::<State>().release(path);
    let cancelled = cancel.load(Ordering::Relaxed);
    let (summary, error) = match result {
        Ok(s) => (Some(s), None),
        Err(_) if cancelled => (None, None),
        Err(e) => (None, Some(e.to_string())),
    };
    let _ = app.emit(
        "analysis-finished",
        FinishedEvent {
            path: path.to_path_buf(),
            summary,
            cancelled,
            error,
        },
    );
}

/// Runs the pass on a file the app can write, on the calling thread.
fn analyse_writable(app: &AppHandle, path: &Path, settings: Option<ContentSettings>, cancel: &AtomicBool) {
    let result = (|| {
        let settings = match settings {
            Some(s) => s,
            None => near_dup::effective_settings(&near_dup::open_read_only(path)?)?,
        };
        near_dup::analyse_file(path, &settings, true, cancel, progress(app, path)).map(|(_, s)| s)
    })();
    finish(app, path, result, cancel);
}

#[tauri::command]
pub async fn near_dup_status(
    results: tauri::State<'_, results::State>,
    state: tauri::State<'_, State>,
    path: PathBuf,
    settings: Option<ContentSettings>,
) -> Cmd<NearDupStatus> {
    let running = state.is_running(&path);
    let mut status = results
        .with(&path, move |r| {
            let conn = r.connection();
            let settings = settings_or_effective(conn, settings)?;
            near_dup::status(conn, &settings).map_err(err)
        })
        .await?;
    if running {
        status.state = NearDupState::Running;
        status.reason = None;
    }
    Ok(status)
}

#[tauri::command]
pub async fn near_dup_clusters(
    results: tauri::State<'_, results::State>,
    path: PathBuf,
    offset: u32,
    limit: u32,
) -> Cmd<ClustersPage> {
    results
        .with(&path, move |r| {
            near_dup::clusters(r.connection(), offset, limit).map_err(err)
        })
        .await
}

/// One cluster's pages, representative first (FR-017).
#[tauri::command]
pub async fn near_dup_members(
    results: tauri::State<'_, results::State>,
    path: PathBuf,
    cluster: u32,
    offset: u32,
    limit: u32,
) -> Cmd<MembersPage> {
    results
        .with(&path, move |r| {
            near_dup::members(r.connection(), cluster, offset, limit).map_err(err)
        })
        .await
}

#[tauri::command]
pub async fn near_dup_pairs(
    results: tauri::State<'_, results::State>,
    path: PathBuf,
    url_id: i64,
) -> Cmd<PairsPage> {
    results
        .with(&path, move |r| {
            near_dup::pairs(r.connection(), url_id).map_err(err)
        })
        .await
}

#[tauri::command]
pub async fn compare_pages(
    results: tauri::State<'_, results::State>,
    path: PathBuf,
    a: i64,
    b: i64,
    full: bool,
) -> Cmd<Comparison> {
    results
        .with(&path, move |r| {
            let conn = r.connection();
            let settings = near_dup::effective_settings(conn).map_err(err)?;
            near_dup::compare(conn, a, b, full, &settings).map_err(err)
        })
        .await
}

/// Starts the analysis on demand (FR-013); progress arrives as events.
/// `settings` re-runs with changed content settings.
#[tauri::command]
pub fn run_near_dup(
    app: AppHandle,
    results: tauri::State<'_, results::State>,
    state: tauri::State<'_, State>,
    crawl: tauri::State<'_, crate::crawl::State>,
    path: PathBuf,
    settings: Option<ContentSettings>,
) -> Cmd<()> {
    if crawl.is_running(&path) {
        return Err("The analysis runs when this crawl finishes or is paused.".into());
    }
    let Some(cancel) = state.claim(&path) else {
        return Ok(());
    };
    let writable = !read_only(&path) && Connection::open_with_flags(&path, rw_flags()).is_ok();
    if writable {
        let thread_path = path.clone();
        let thread_app = app.clone();
        let thread_cancel = cancel.clone();
        let spawned = std::thread::Builder::new()
            .name("analysis".into())
            .spawn(move || analyse_writable(&thread_app, &thread_path, settings, &thread_cancel));
        if let Err(e) = spawned {
            state.release(&path);
            return Err(format!("Could not start the analysis: {e}"));
        }
        return Ok(());
    }
    let job_path = path.clone();
    let sent = results.send_job(&path, move |r| {
        let result = (|| {
            let conn = r.connection();
            let settings = match settings {
                Some(s) => s,
                None => near_dup::effective_settings(conn)?,
            };
            near_dup::run_pass(conn, Schema::Temp, &settings, &cancel, progress(&app, &job_path))
        })();
        finish(&app, &job_path, result, &cancel);
    });
    if let Err(e) = sent {
        state.release(&path);
        return Err(e);
    }
    Ok(())
}

#[tauri::command]
pub fn cancel_near_dup(state: tauri::State<'_, State>, path: PathBuf) {
    if let Some(c) = state.running.lock().expect("near dup lock").get(&path) {
        c.store(true, Ordering::Relaxed);
    }
}

#[derive(Deserialize, Clone, Copy)]
#[serde(rename_all = "snake_case")]
pub enum ExportKind {
    Pages,
    Pairs,
    Clusters,
}

/// Writes a CSV of pages, pairs (optionally of one cluster) or clusters
/// (FR-021) and returns the number of rows.
#[tauri::command]
pub async fn export_near_dup(
    results: tauri::State<'_, results::State>,
    path: PathBuf,
    kind: ExportKind,
    cluster: Option<u32>,
    dest: PathBuf,
) -> Cmd<u64> {
    results
        .with(&path, move |r| {
            let listing = match kind {
                ExportKind::Pages => Listing::Pages,
                ExportKind::Pairs => Listing::Pairs { cluster },
                ExportKind::Clusters => Listing::Clusters,
            };
            let file = File::create(&dest).map_err(|e| format!("Could not write {}: {e}", dest.display()))?;
            near_dup::export::write_csv(r.connection(), listing, BufWriter::new(file)).map_err(err)
        })
        .await
}

fn rw_flags() -> spiderlens_results::near_dup::OpenFlags {
    spiderlens_results::near_dup::OpenFlags::SQLITE_OPEN_READ_WRITE
        | spiderlens_results::near_dup::OpenFlags::SQLITE_OPEN_NO_MUTEX
}

fn read_only(path: &Path) -> bool {
    std::fs::metadata(path).is_ok_and(|m| m.permissions().readonly())
}
