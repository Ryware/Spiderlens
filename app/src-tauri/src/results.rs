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
use spiderlens_results::{
    ExportError, Features, FilterCounts, ListKind, ListPage, Reader, Refresh, RowPage, TabDef, UrlDetail,
    UrlScope, ViewQuery, export_file, tabs,
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

fn start_actor(path: &Path) -> Cmd<Actor> {
    let mut reader = Reader::open(path).map_err(|e| e.to_string())?;
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
            *actor = Some(start_actor(path)?);
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
pub fn results_tabs() -> &'static [TabDef] {
    tabs()
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

/// One visualisation scene for the open file (spec 022): the crawl tree or
/// the directory tree, as `state` asks.
#[tauri::command]
pub async fn viz_scene(
    state: tauri::State<'_, State>,
    path: PathBuf,
    view: spiderlens_results::viz::ViewState,
) -> Cmd<spiderlens_results::viz::Scene> {
    state.with(&path, move |r| r.viz_scene(&view).map_err(err)).await
}

/// Exports the view as SVG, PNG or CSV (spec 022 FR-024): the scene is
/// rebuilt from `view`, so the file is what is on screen; drawing and
/// writing run off the reader.
#[tauri::command]
pub async fn viz_export(
    state: tauri::State<'_, State>,
    path: PathBuf,
    view: spiderlens_results::viz::ViewState,
    options: spiderlens_results::viz::export::ExportOptions,
    file: PathBuf,
) -> Cmd<spiderlens_results::viz::export::ExportDone> {
    let colour = view.colour;
    let (scene, header) = state
        .with(&path, move |r| {
            let scene = r.viz_scene(&view).map_err(err)?;
            Ok((scene, r.viz_header().map_err(err)?))
        })
        .await?;
    tauri::async_runtime::spawn_blocking(move || {
        spiderlens_results::viz::export::export(&scene, colour, &header, &options, &file).map_err(err)
    })
    .await
    .map_err(err)?
}

/// The links behind one term of an anchor text cloud (spec 022 FR-018):
/// `total` of them, the first `limit` listed.
#[tauri::command]
pub async fn viz_term_links(
    state: tauri::State<'_, State>,
    path: PathBuf,
    view: spiderlens_results::viz::ViewState,
    term: String,
    limit: Option<usize>,
) -> Cmd<spiderlens_results::viz::anchors::TermLinks> {
    let limit = limit.unwrap_or(500).min(5_000);
    state
        .with(&path, move |r| r.viz_term_links(&view, &term, limit).map_err(err))
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
            let result = export_file(
                &path,
                &query,
                &columns,
                ranges.as_deref(),
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
