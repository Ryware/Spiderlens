//! Commands for the results table, detail pane, copy and export (spec 007,
//! specs/007-results-ui/contracts/desktop-commands.md).
//!
//! One reader thread owns the read-only connection to the open crawl file
//! (rusqlite connections are not shared between threads). Commands send it
//! jobs and await the answer, so heavy queries never run on the threads
//! that keep the window responsive, and never on the crawl's thread.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex, mpsc};

use serde::Serialize;
use spiderlens_config::IssueSettings;
use spiderlens_results::analysis::{AnalysisProgress, AnalysisReport};
use spiderlens_results::issues::{IssuesSummary, RuleCatalogue, SettingsNotice, UrlIssue};
use spiderlens_results::{
    ExportError, Features, FilterCounts, ListKind, ListPage, Reader, Refresh, RowPage, TabDef, UrlDetail,
    UrlScope, ViewQuery, export_file_with, issue_tabs, tabs,
};
use tauri::{AppHandle, Emitter};
use tauri_plugin_clipboard_manager::ClipboardExt;

type Cmd<T> = Result<T, String>;
type Job = Box<dyn FnOnce(&mut Reader) + Send>;

struct Actor {
    path: PathBuf,
    jobs: mpsc::Sender<Job>,
    interrupt: spiderlens_results::InterruptHandle,
    /// A view's full order is being built: a newer view may interrupt it.
    building: Arc<AtomicBool>,
}

#[derive(Default)]
pub struct State {
    actor: Mutex<Option<Actor>>,
    exports: Mutex<HashMap<u32, Arc<AtomicBool>>>,
    next_export: AtomicU32,
    /// Issue settings of the profile in use (spec 009 plan D9), applied to
    /// every reader and export.
    issues: Mutex<IssueSettings>,
}

#[derive(Serialize, Clone)]
struct ViewReady {
    view: u32,
    total: u32,
}

#[derive(Serialize, Clone)]
struct ExportProgress {
    job: u32,
    rows_written: u64,
    total: u64,
    state: &'static str,
    error: Option<String>,
    path: PathBuf,
}

fn start_actor(path: &Path, issues: &IssueSettings) -> Cmd<Actor> {
    let mut reader = Reader::open(path).map_err(|e| e.to_string())?;
    // Already checked by issues_apply; a rejected change keeps the defaults.
    reader.set_issue_settings(issues);
    let interrupt = reader.interrupt_handle();
    let (tx, rx) = mpsc::channel::<Job>();
    std::thread::Builder::new()
        .name("results".into())
        .spawn(move || {
            while let Ok(job) = rx.recv() {
                job(&mut reader);
            }
        })
        .map_err(|e| format!("Could not open the results: {e}"))?;
    Ok(Actor {
        path: path.to_path_buf(),
        jobs: tx,
        interrupt,
        building: Arc::new(AtomicBool::new(false)),
    })
}

impl State {
    /// Queues `f` on the reader of `path`, opening it first if needed.
    fn send(&self, path: &Path, f: impl FnOnce(&mut Reader, &Arc<AtomicBool>) + Send + 'static) -> Cmd<()> {
        let mut actor = self.actor.lock().expect("results lock");
        if actor.as_ref().is_none_or(|a| a.path != path) {
            let issues = self.issues.lock().expect("issues lock").clone();
            *actor = Some(start_actor(path, &issues)?);
        }
        let a = actor.as_ref().expect("just set");
        let building = a.building.clone();
        a.jobs
            .send(Box::new(move |r: &mut Reader| f(r, &building)))
            .map_err(|_| "The results reader stopped.".to_string())
    }

    /// Runs `f` on the reader of `path` and returns its answer.
    async fn with<T: Send + 'static>(
        &self,
        path: &Path,
        f: impl FnOnce(&mut Reader) -> Result<T, String> + Send + 'static,
    ) -> Cmd<T> {
        let (reply, answer) = tokio::sync::oneshot::channel();
        self.send(path, move |r, _| {
            let _ = reply.send(f(r));
        })?;
        answer
            .await
            .map_err(|_| "The results reader stopped.".to_string())?
    }

    /// Lets go of `path` (before it is deleted or moved by Save As).
    pub fn close(&self, path: &Path) {
        let mut actor = self.actor.lock().expect("results lock");
        if actor.as_ref().is_some_and(|a| a.path == path) {
            *actor = None;
        }
    }

    fn interrupt_build(&self) {
        if let Some(a) = self.actor.lock().expect("results lock").as_ref()
            && a.building.load(Ordering::Relaxed)
        {
            a.interrupt.interrupt();
        }
    }
}

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

/// Tab, column and filter definitions (D2): the UI keeps no copy of them.
#[tauri::command]
pub fn results_tabs() -> Vec<&'static TabDef> {
    // One tab per issue rule too (spec 009 D10): `issue:<rule id>`.
    tabs().iter().chain(issue_tabs()).collect()
}

#[derive(Serialize)]
pub struct Opened {
    features: Features,
}

/// Opens a crawl file for browsing. Fails while a new crawl has not yet
/// written its file; the UI tries again on the next progress event.
#[tauri::command]
pub async fn results_open(state: tauri::State<'_, State>, path: PathBuf) -> Cmd<Opened> {
    state
        .with(&path, |r| {
            Ok(Opened {
                features: r.features().clone(),
            })
        })
        .await
}

/// First screen of a view; its full order is built right after, and
/// `view-ready` says when (plan D4).
#[tauri::command]
pub async fn view_open(
    app: AppHandle,
    state: tauri::State<'_, State>,
    path: PathBuf,
    query: ViewQuery,
    count: u32,
) -> Cmd<RowPage> {
    state.interrupt_build();
    let (reply, answer) = tokio::sync::oneshot::channel();
    // The first screen goes back at once; the same job then builds the
    // full order and announces it.
    state.send(&path, move |r, building| {
        let page = r.open_view(&query, count).map_err(err);
        let view = page.as_ref().ok().map(|p| p.view);
        let _ = reply.send(page);
        if let Some(view) = view {
            building.store(true, Ordering::Relaxed);
            let total = r.materialize(view);
            building.store(false, Ordering::Relaxed);
            if let Ok(total) = total {
                let _ = app.emit("view-ready", ViewReady { view, total });
            }
        }
    })?;
    answer
        .await
        .map_err(|_| "The results reader stopped.".to_string())?
}

#[tauri::command]
pub async fn view_rows(
    state: tauri::State<'_, State>,
    path: PathBuf,
    view: u32,
    from: u32,
    count: u32,
) -> Cmd<RowPage> {
    state
        .with(&path, move |r| r.rows(view, from, count).map_err(err))
        .await
}

#[tauri::command]
pub async fn view_locate(
    state: tauri::State<'_, State>,
    path: PathBuf,
    view: u32,
    ids: Vec<i64>,
) -> Cmd<Vec<Option<u32>>> {
    state
        .with(&path, move |r| r.locate(view, &ids).map_err(err))
        .await
}

/// Rebuilds the view when the crawl wrote since; null when nothing changed.
#[tauri::command]
pub async fn view_refresh(
    state: tauri::State<'_, State>,
    path: PathBuf,
    view: u32,
    anchors: Vec<i64>,
) -> Cmd<Option<Refresh>> {
    state
        .with(&path, move |r| r.refresh(view, &anchors).map_err(err))
        .await
}

#[tauri::command]
pub async fn view_counts(
    state: tauri::State<'_, State>,
    path: PathBuf,
    tab: String,
    scope: UrlScope,
) -> Cmd<FilterCounts> {
    state
        .with(&path, move |r| r.counts(&tab, scope).map_err(err))
        .await
}

#[tauri::command]
pub async fn view_close(state: tauri::State<'_, State>, path: PathBuf, view: u32) -> Cmd<()> {
    state.with(&path, move |r| r.close_view(view).map_err(err)).await
}

#[tauri::command]
pub async fn url_detail(state: tauri::State<'_, State>, path: PathBuf, id: i64) -> Cmd<UrlDetail> {
    state.with(&path, move |r| r.detail(id).map_err(err)).await
}

#[tauri::command]
pub async fn url_list(
    state: tauri::State<'_, State>,
    path: PathBuf,
    id: i64,
    kind: ListKind,
    from: u32,
    count: u32,
) -> Cmd<ListPage> {
    state
        .with(&path, move |r| r.list(id, kind, from, count).map_err(err))
        .await
}

/// Puts the selected rows on the clipboard as TSV with a header (FR-026)
/// and returns the text; over 10,000 rows is refused. Written from here
/// because a web view only allows clipboard writes during the click itself.
#[tauri::command]
pub async fn copy_rows(
    app: AppHandle,
    state: tauri::State<'_, State>,
    path: PathBuf,
    view: u32,
    ranges: Vec<(u32, u32)>,
    columns: Vec<String>,
) -> Cmd<String> {
    let text = state
        .with(&path, move |r| r.copy_tsv(view, &ranges, &columns).map_err(err))
        .await?;
    app.clipboard()
        .write_text(text.clone())
        .map_err(|e| format!("Could not copy: {e}"))?;
    Ok(text)
}

/// Starts a CSV export on its own thread; progress arrives as
/// `export-progress` events (FR-030).
#[tauri::command]
pub fn export_start(
    app: AppHandle,
    state: tauri::State<'_, State>,
    path: PathBuf,
    query: ViewQuery,
    columns: Vec<String>,
    ranges: Option<Vec<(u32, u32)>>,
    dest: PathBuf,
) -> Cmd<u32> {
    let job = state.next_export.fetch_add(1, Ordering::Relaxed) + 1;
    let cancel = Arc::new(AtomicBool::new(false));
    let issues = state.issues.lock().expect("issues lock").clone();
    let settings = spiderlens_results::issues::resolve(&issues).0;
    state
        .exports
        .lock()
        .expect("exports lock")
        .insert(job, cancel.clone());
    std::thread::Builder::new()
        .name("export".into())
        .spawn(move || {
            let send = |rows_written, total, state, error: Option<String>| {
                let _ = app.emit(
                    "export-progress",
                    ExportProgress {
                        job,
                        rows_written,
                        total,
                        state,
                        error,
                        path: dest.clone(),
                    },
                );
            };
            let mut last = std::time::Instant::now();
            let result = export_file_with(
                &path,
                &query,
                &columns,
                ranges.as_deref(),
                &settings,
                &dest,
                &cancel,
                &mut |n, total| {
                    if last.elapsed().as_millis() >= 250 || n == 0 {
                        last = std::time::Instant::now();
                        send(n, total, "running", None);
                    }
                },
            );
            match result {
                Ok(n) => send(n, n, "done", None),
                Err(ExportError::Cancelled) => send(0, 0, "cancelled", None),
                Err(e) => send(0, 0, "failed", Some(e.to_string())),
            }
            let state = tauri::Manager::state::<State>(&app);
            state.exports.lock().expect("exports lock").remove(&job);
        })
        .map_err(|e| format!("Could not start the export: {e}"))?;
    Ok(job)
}

#[tauri::command]
pub fn export_cancel(state: tauri::State<'_, State>, job: u32) {
    if let Some(c) = state.exports.lock().expect("exports lock").get(&job) {
        c.store(true, Ordering::Relaxed);
    }
}

/// The rule catalogue with `settings` applied, and what is wrong with them
/// (spec 009, Settings › Issues).
#[tauri::command]
pub fn rule_catalogue(settings: IssueSettings) -> RuleCatalogue {
    spiderlens_results::issues::rule_catalogue(&settings)
}

/// Makes `settings` the issue settings in force for the open crawl, later
/// files and exports (FR-023). Out-of-range values reject the whole change
/// (FR-024); the notices say why.
#[tauri::command]
pub async fn issues_apply(
    state: tauri::State<'_, State>,
    path: Option<PathBuf>,
    settings: IssueSettings,
) -> Cmd<Vec<SettingsNotice>> {
    let (_, notices) = spiderlens_results::issues::resolve(&settings);
    if notices.iter().any(|n| n.error) {
        return Ok(notices);
    }
    *state.issues.lock().expect("issues lock") = settings.clone();
    match path {
        Some(path) => {
            state
                .with(&path, move |r| Ok(r.set_issue_settings(&settings)))
                .await
        }
        None => Ok(notices),
    }
}

/// Every rule with its count for the open crawl (FR-014); site-wide counts
/// are provisional while `running` (FR-019).
#[tauri::command]
pub async fn issues_summary(
    state: tauri::State<'_, State>,
    path: PathBuf,
    running: bool,
) -> Cmd<IssuesSummary> {
    state
        .with(&path, move |r| r.issues_summary(running).map_err(err))
        .await
}

/// Runs the Crawl Analysis passes that have not run on `path` (spec 009
/// plan D12), sending `analysis-progress` events. `cancel` stops it between
/// batches; finished passes stay.
pub fn analyse(app: &AppHandle, path: &Path, cancel: &AtomicBool) -> Result<AnalysisReport, String> {
    let mut progress = |p: AnalysisProgress| {
        let _ = app.emit("analysis-progress", p);
    };
    spiderlens_results::analysis::analyse_file(path, &mut progress, cancel).map_err(|e| e.to_string())
}

/// Analyses an opened file whose passes are missing or stale.
#[tauri::command]
pub async fn analysis_run(app: AppHandle, path: PathBuf) -> Cmd<AnalysisReport> {
    tauri::async_runtime::spawn_blocking(move || analyse(&app, &path, &AtomicBool::new(false)))
        .await
        .map_err(|e| e.to_string())?
}

/// The issues of one URL with evidence (FR-016).
#[tauri::command]
pub async fn url_issues(state: tauri::State<'_, State>, path: PathBuf, id: i64) -> Cmd<Vec<UrlIssue>> {
    state.with(&path, move |r| r.url_issues(id).map_err(err)).await
}
