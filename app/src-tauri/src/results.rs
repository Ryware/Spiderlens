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
use spiderlens_config::{IssueSettings, LinkAnalysisSettings, PositionRule, Segment, SitemapGenerateOptions};
use spiderlens_results::analysis::{AnalysisReport, PassProgress};
use spiderlens_results::issues::{IssuesSummary, RuleCatalogue, SettingsNotice, UrlIssue};
use spiderlens_results::{
    AnalysisProgress, AnalysisRun, CookieFilter, CookiePage, CookieUrlPage, CustomResult, ExportError,
    Features, FilterCounts, HtmlDiff, HtmlVersion, ListKind, ListPage, PageHtml, PageScreenshot, Reader,
    Refresh, RowPage, SegmentCounts, TabCounts, TabDef, TestedLink, UrlDetail, UrlScope, Values, ViewQuery,
    export_file_in, issue_tabs, tabs,
};
use spiderlens_results::{
    GenerateError, GenerateReport, OrphanSummary, SitemapFiles, SitemapOutput, UrlSitemapEntry,
    generate_sitemaps,
};
use tauri::{AppHandle, Emitter, Manager};
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
    pub issues: Mutex<IssueSettings>,
    /// Cancel flag of the running Crawl Analysis, if any.
    analysis: Mutex<Option<Arc<AtomicBool>>>,
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

/// Sends one export job's `export-progress` events; comparison exports
/// (spec 012) use the same events and the same cancel command.
pub struct ExportEvents {
    app: AppHandle,
    job: u32,
    path: PathBuf,
}

impl ExportEvents {
    pub fn new(app: &AppHandle, job: u32, path: PathBuf) -> Self {
        Self {
            app: app.clone(),
            job,
            path,
        }
    }

    pub fn send(&self, rows_written: u64, total: u64, state: &'static str, error: Option<String>) {
        let _ = self.app.emit(
            "export-progress",
            ExportProgress {
                job: self.job,
                rows_written,
                total,
                state,
                error,
                path: self.path.clone(),
            },
        );
    }
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

    /// Queues `f` on the reader of `path` without waiting for it.
    pub(crate) fn send_job(&self, path: &Path, f: impl FnOnce(&mut Reader) + Send + 'static) -> Cmd<()> {
        self.send(path, move |r, _| f(r))
    }

    /// Runs `f` on the reader of `path` and returns its answer.
    pub(crate) async fn with<T: Send + 'static>(
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

    /// A new export job id and its cancel flag.
    pub fn new_export(&self) -> (u32, Arc<AtomicBool>) {
        let job = self.next_export.fetch_add(1, Ordering::Relaxed) + 1;
        let cancel = Arc::new(AtomicBool::new(false));
        self.exports
            .lock()
            .expect("exports lock")
            .insert(job, cancel.clone());
        (job, cancel)
    }

    pub fn end_export(&self, job: u32) {
        self.exports.lock().expect("exports lock").remove(&job);
    }

    /// The crawl the results window reads, if any.
    pub fn open_path(&self) -> Option<PathBuf> {
        self.actor
            .lock()
            .expect("results lock")
            .as_ref()
            .map(|a| a.path.clone())
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
/// With a crawl file, its custom extraction and search tabs too (spec 013).
#[tauri::command]
pub async fn results_tabs(state: tauri::State<'_, State>, path: Option<PathBuf>) -> Cmd<Vec<TabDef>> {
    match path {
        // One tab per issue rule too (spec 009 D10): `issue:<rule id>`.
        None => Ok(tabs().iter().chain(issue_tabs()).cloned().collect()),
        Some(p) => state.with(&p, |r| r.tabs().map_err(err)).await,
    }
}

/// Every rule's result on one page, for the detail pane (spec 013 FR-024).
#[tauri::command]
pub async fn url_custom(state: tauri::State<'_, State>, path: PathBuf, id: i64) -> Cmd<Vec<CustomResult>> {
    state
        .with(&path, move |r| r.custom_results(id).map_err(err))
        .await
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
                features: r.refresh_features().map_err(err)?.clone(),
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
    segment: Option<u32>,
    values: Option<Values>,
) -> Cmd<FilterCounts> {
    state
        .with(&path, move |r| {
            r.counts_for(&tab, scope, segment, values).map_err(err)
        })
        .await
}

/// URLs per result tab for the sidebar (T014): one pass over the file,
/// answered from memory while the crawl has not written since.
#[tauri::command]
pub async fn tab_counts(state: tauri::State<'_, State>, path: PathBuf) -> Cmd<TabCounts> {
    state.with(&path, |r| r.tab_counts().map_err(err)).await
}

/// Groups the open crawl's URLs by `segments` (spec 027 FR-B06) and saves
/// them in the crawl file when no crawl is writing it (the UI calls again
/// when the crawl ends). Open views and counts follow on their next refresh.
#[tauri::command]
pub async fn segments_set(
    state: tauri::State<'_, State>,
    path: PathBuf,
    segments: Vec<Segment>,
) -> Cmd<SegmentCounts> {
    let issues = spiderlens_config::validate_segments(&segments);
    if let Some(i) = issues.first() {
        return Err(format!("{}: {}", i.path, i.message));
    }
    let counts = state
        .with(&path, move |r| r.set_segments(segments).map_err(err))
        .await?;
    // Busy (a crawl is writing) or read-only: the grouping still applies here.
    let _ = spiderlens_engine::crawl::file::save_segments(&path, &counts.segments);
    Ok(counts)
}

/// URLs per segment in the open crawl (the segment filter's counts).
#[tauri::command]
pub async fn segments_counts(state: tauri::State<'_, State>, path: PathBuf) -> Cmd<SegmentCounts> {
    state.with(&path, |r| r.segment_counts().map_err(err)).await
}

#[tauri::command]
pub async fn view_close(state: tauri::State<'_, State>, path: PathBuf, view: u32) -> Cmd<()> {
    state.with(&path, move |r| r.close_view(view).map_err(err)).await
}

#[tauri::command]
pub async fn url_detail(state: tauri::State<'_, State>, path: PathBuf, id: i64) -> Cmd<UrlDetail> {
    state.with(&path, move |r| r.detail(id).map_err(err)).await
}

/// One page's agent findings, for the detail pane (spec 016 FR-019).
#[tauri::command]
pub async fn agents_page(
    state: tauri::State<'_, State>,
    path: PathBuf,
    id: i64,
) -> Cmd<spiderlens_results::agents::PageAgentFindings> {
    state
        .with(&path, move |r| r.agent_findings(id).map_err(err))
        .await
}

/// The crawl's AI agent score and what drives it (spec 016 FR-018).
#[tauri::command]
pub async fn agents_summary(
    state: tauri::State<'_, State>,
    path: PathBuf,
) -> Cmd<spiderlens_results::agents::AgentsSummary> {
    state.with(&path, move |r| r.agents_summary().map_err(err)).await
}

/// What each host offers to AI agents (spec 016 FR-008).
#[tauri::command]
pub async fn agents_entry_points(
    state: tauri::State<'_, State>,
    path: PathBuf,
) -> Cmd<Vec<spiderlens_results::agents::EntryPointRow>> {
    state
        .with(&path, move |r| r.agent_entry_points().map_err(err))
        .await
}

#[tauri::command]
#[allow(clippy::too_many_arguments)] // Tauri commands take their arguments flat.
pub async fn url_list(
    state: tauri::State<'_, State>,
    path: PathBuf,
    id: i64,
    kind: ListKind,
    from: u32,
    count: u32,
    position: Option<String>,
    values: Option<Values>,
) -> Cmd<ListPage> {
    state
        .with(&path, move |r| {
            r.list_values(id, kind, position.as_deref(), values, from, count)
                .map_err(err)
        })
        .await
}

/// Runs Crawl Analysis on the open file now (spec 023 stage 1).
#[tauri::command]
pub async fn analysis_run(state: tauri::State<'_, State>, path: PathBuf) -> Cmd<AnalysisRun> {
    state.with(&path, move |r| r.analyse().map_err(err)).await
}

#[derive(Serialize, Clone)]
struct AnalysisProgressEvent {
    path: PathBuf,
    #[serde(flatten)]
    progress: AnalysisProgress,
}

#[derive(Serialize, Clone)]
struct AnalysisDone {
    path: PathBuf,
    /// None when cancelled or failed.
    run: Option<AnalysisRun>,
    cancelled: bool,
    error: Option<String>,
}

/// Starts Crawl Analysis on the open file's reader (spec 023 FR-012,
/// FR-013): `analysis-progress` events at most every 100 ms, then one
/// `analysis-done`. Other reader commands wait until it ends.
#[tauri::command]
pub fn analysis_start(app: AppHandle, state: tauri::State<'_, State>, path: PathBuf) -> Cmd<()> {
    let cancel = Arc::new(AtomicBool::new(false));
    if let Some(old) = state
        .analysis
        .lock()
        .expect("analysis lock")
        .replace(cancel.clone())
    {
        old.store(true, Ordering::Relaxed);
    }
    let p = path.clone();
    state.send(&path, move |r, _| {
        let mut last: Option<std::time::Instant> = None;
        let mut on = |progress: AnalysisProgress| {
            let due = last.is_none_or(|t| t.elapsed() >= std::time::Duration::from_millis(100));
            if due || progress.done == progress.total {
                last = Some(std::time::Instant::now());
                let _ = app.emit(
                    "analysis-progress",
                    AnalysisProgressEvent {
                        path: p.clone(),
                        progress,
                    },
                );
            }
        };
        let done = match r.analyse_with(&mut on, &cancel) {
            Ok(run) => AnalysisDone {
                path: p.clone(),
                cancelled: run.is_none(),
                run,
                error: None,
            },
            Err(e) => AnalysisDone {
                path: p.clone(),
                run: None,
                cancelled: false,
                error: Some(e.to_string()),
            },
        };
        // Finished: a late cancel must not interrupt the next job.
        let state = app.state::<State>();
        let mut current = state.analysis.lock().expect("analysis lock");
        if current.as_ref().is_some_and(|c| Arc::ptr_eq(c, &cancel)) {
            *current = None;
        }
        drop(current);
        let _ = app.emit("analysis-done", done);
    })
}

/// Cancels the running Crawl Analysis (FR-013): checked between batches,
/// and the reader's query is interrupted, so it stops within a second.
#[tauri::command]
pub fn analysis_cancel(state: tauri::State<'_, State>) {
    if let Some(c) = state.analysis.lock().expect("analysis lock").take() {
        c.store(true, Ordering::Relaxed);
        if let Some(a) = state.actor.lock().expect("results lock").as_ref() {
            a.interrupt.interrupt();
        }
    }
}

/// Uses `settings` for the open file's link positions and Link Score from
/// now on (spec 023 FR-023): both are worked out again on next use.
#[tauri::command]
pub async fn analysis_settings(
    state: tauri::State<'_, State>,
    path: PathBuf,
    settings: LinkAnalysisSettings,
) -> Cmd<()> {
    state
        .with(&path, move |r| {
            r.set_link_settings(settings);
            Ok(())
        })
        .await
}

/// Distinct cookies the crawl saw, without values (spec 025 FR-027).
#[tauri::command]
pub async fn cookies_list(
    state: tauri::State<'_, State>,
    path: PathBuf,
    filter: CookieFilter,
    sort: Option<String>,
    descending: bool,
    offset: u32,
    limit: u32,
) -> Cmd<CookiePage> {
    state
        .with(&path, move |r| {
            r.cookies(filter, sort.as_deref(), descending, offset, limit)
                .map_err(err)
        })
        .await
}

/// The link position rules tester (spec 023 FR-022). None when the URL is
/// not in the crawl or its HTML was not stored.
#[tauri::command]
pub async fn positions_test(
    state: tauri::State<'_, State>,
    path: PathBuf,
    url: String,
    rules: Vec<PositionRule>,
) -> Cmd<Option<Vec<TestedLink>>> {
    state
        .with(&path, move |r| r.test_positions_url(&url, &rules).map_err(err))
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

/// The URLs that set one cookie (spec 025 FR-028).
#[tauri::command]
pub async fn cookie_urls(
    state: tauri::State<'_, State>,
    path: PathBuf,
    name: String,
    domain: String,
    cookie_path: String,
    offset: u32,
    limit: u32,
) -> Cmd<CookieUrlPage> {
    state
        .with(&path, move |r| {
            r.cookie_urls(&name, &domain, &cookie_path, offset, limit)
                .map_err(err)
        })
        .await
}

/// A page's stored HTML for the source viewer (T029): the raw version, or
/// the rendered one with `version` (spec 014 FR-032).
#[tauri::command]
pub async fn url_html(
    state: tauri::State<'_, State>,
    path: PathBuf,
    id: i64,
    version: Option<HtmlVersion>,
) -> Cmd<PageHtml> {
    state
        .with(&path, move |r| {
            r.page_html_version(id, version.unwrap_or_default()).map_err(err)
        })
        .await
}

/// A rendered page's stored screenshot (spec 014 FR-033).
#[tauri::command]
pub async fn url_screenshot(state: tauri::State<'_, State>, path: PathBuf, id: i64) -> Cmd<PageScreenshot> {
    state
        .with(&path, move |r| r.page_screenshot(id).map_err(err))
        .await
}

/// The differences between a page's normalised raw and rendered HTML:
/// `max_hunks` hunks from hunk `from_hunk` (spec 014 FR-028).
#[tauri::command]
pub async fn url_html_diff(
    state: tauri::State<'_, State>,
    path: PathBuf,
    id: i64,
    from_hunk: u32,
    max_hunks: u32,
) -> Cmd<HtmlDiff> {
    state
        .with(&path, move |r| r.html_diff(id, from_hunk, max_hunks).map_err(err))
        .await
}

/// The tabs that list URL `id` unfiltered, for "show in table".
#[tauri::command]
pub async fn url_tabs(state: tauri::State<'_, State>, path: PathBuf, id: i64) -> Cmd<Vec<&'static str>> {
    state.with(&path, move |r| r.tabs_of(id).map_err(err)).await
}

/// Puts one value (an inspector row's URL) on the clipboard.
#[tauri::command]
pub fn copy_text(app: AppHandle, text: String) -> Cmd<()> {
    app.clipboard()
        .write_text(text)
        .map_err(|e| format!("Could not copy: {e}"))
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
#[allow(clippy::too_many_arguments)]
pub fn export_start(
    app: AppHandle,
    state: tauri::State<'_, State>,
    path: PathBuf,
    query: ViewQuery,
    columns: Vec<String>,
    ranges: Option<Vec<(u32, u32)>>,
    segments: Option<Vec<Segment>>,
    dest: PathBuf,
    all_values: Option<String>,
) -> Cmd<u32> {
    let (job, cancel) = state.new_export();
    let issues = state.issues.lock().expect("issues lock").clone();
    let settings = spiderlens_results::issues::resolve(&issues).0;
    std::thread::Builder::new()
        .name("export".into())
        .spawn(move || {
            let events = ExportEvents::new(&app, job, dest.clone());
            let mut last = std::time::Instant::now();
            let result = export_file_in(
                &path,
                &query,
                &columns,
                ranges.as_deref(),
                all_values.as_deref(),
                &settings,
                segments.as_deref(),
                &dest,
                &cancel,
                &mut |n, total| {
                    if last.elapsed().as_millis() >= 250 || n == 0 {
                        last = std::time::Instant::now();
                        events.send(n, total, "running", None);
                    }
                },
            );
            match result {
                Ok(n) => events.send(n, n, "done", None),
                Err(ExportError::Cancelled) => events.send(0, 0, "cancelled", None),
                Err(e) => events.send(0, 0, "failed", Some(e.to_string())),
            }
            tauri::Manager::state::<State>(&app).end_export(job);
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
            let recorded = settings.clone();
            let notices = state
                .with(&path, move |r| Ok(r.set_issue_settings(&settings)))
                .await?;
            // FR-013: a writable file records the settings its issues now
            // use; a read-only or newer file is left alone.
            std::thread::spawn(move || record_issues(&path, &recorded));
            Ok(notices)
        }
        None => Ok(notices),
    }
}

/// Records the rule set version and issue settings in `path` (FR-013),
/// when the file can be written.
pub fn record_issues(path: &Path, settings: &IssueSettings) {
    let _ = spiderlens_results::issues::record_issue_settings(path, settings);
}

/// Every rule with its count for the open crawl (FR-014); site-wide counts
/// are provisional while `running` (FR-019).
#[tauri::command]
pub async fn issues_summary(
    app: AppHandle,
    state: tauri::State<'_, State>,
    path: PathBuf,
    running: bool,
) -> Cmd<IssuesSummary> {
    let key = path.clone();
    state
        .with(&path, move |r| {
            // Counts so far, about once a second, while a long evaluation runs (FR-008).
            r.set_partial_sink(Some(Box::new(move |summary| {
                let _ = app.emit(
                    "issues-partial",
                    IssuesPartial {
                        path: key.clone(),
                        summary,
                    },
                );
            })));
            let out = r.issues_summary(running).map_err(err);
            r.set_partial_sink(None);
            out
        })
        .await
}

/// The Sitemap files view (spec 011 US3).
#[tauri::command]
pub async fn sitemap_files(state: tauri::State<'_, State>, path: PathBuf) -> Cmd<SitemapFiles> {
    state.with(&path, |r| r.sitemap_files().map_err(err)).await
}

/// The orphan notice above the Sitemaps tab (spec 011 FR-012, FR-013).
#[tauri::command]
pub async fn orphan_summary(state: tauri::State<'_, State>, path: PathBuf) -> Cmd<OrphanSummary> {
    state.with(&path, |r| r.orphan_summary().map_err(err)).await
}

/// The inspector's Sitemaps section (spec 011 FR-026).
#[tauri::command]
pub async fn sitemap_entries_for_url(
    state: tauri::State<'_, State>,
    path: PathBuf,
    id: i64,
) -> Cmd<Vec<UrlSitemapEntry>> {
    state
        .with(&path, move |r| r.sitemap_entries_for_url(id).map_err(err))
        .await
}

/// Counts for the Generate sitemap sheet's live preview (spec 011 FR-018);
/// `existing` lists the files in `folder` that writing would replace.
#[tauri::command]
pub async fn sitemap_preview(
    state: tauri::State<'_, State>,
    path: PathBuf,
    options: SitemapGenerateOptions,
    folder: Option<PathBuf>,
) -> Cmd<GenerateReport> {
    state
        .with(&path, move |r| {
            generate_sitemaps(
                r,
                &options,
                SitemapOutput::DryRun {
                    dir: folder.as_deref(),
                },
            )
            .map_err(err)
        })
        .await
}

/// The `issues-partial` event.
#[derive(Clone, Serialize)]
struct IssuesPartial {
    path: PathBuf,
    summary: IssuesSummary,
}

/// Runs the Crawl Analysis passes that have not run on `path` (spec 009
/// plan D12), sending `passes-progress` events. `cancel` stops it between
/// batches; finished passes stay.
pub fn analyse(
    app: &AppHandle,
    path: &Path,
    skip: &[&str],
    cancel: &AtomicBool,
) -> Result<AnalysisReport, String> {
    let mut progress = |p: PassProgress| {
        let _ = app.emit("passes-progress", p);
    };
    spiderlens_results::analysis::analyse_file_with(path, skip, &mut progress, cancel)
        .map_err(|e| e.to_string())
}

/// Analyses an opened file whose passes are missing or stale.
#[tauri::command]
pub async fn passes_run(app: AppHandle, path: PathBuf) -> Cmd<AnalysisReport> {
    tauri::async_runtime::spawn_blocking(move || {
        // Link analysis only when the file's settings run it automatically.
        let skip = spiderlens_results::analysis::auto_skips(&path).unwrap_or_default();
        analyse(&app, &path, &skip, &AtomicBool::new(false))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// The issues of one URL with evidence (FR-016).
#[tauri::command]
pub async fn url_issues(state: tauri::State<'_, State>, path: PathBuf, id: i64) -> Cmd<Vec<UrlIssue>> {
    state.with(&path, move |r| r.url_issues(id).map_err(err)).await
}

/// Writes the sitemaps into `folder` (spec 011 FR-017 to FR-019). Refused
/// while a crawl runs, so the files describe a finished crawl.
#[tauri::command]
pub async fn sitemap_generate(
    state: tauri::State<'_, State>,
    crawl: tauri::State<'_, crate::crawl::State>,
    path: PathBuf,
    options: SitemapGenerateOptions,
    folder: PathBuf,
    replace: bool,
) -> Cmd<GenerateReport> {
    if crawl.is_running() {
        return Err("Sitemaps can be generated once the crawl has finished.".into());
    }
    state
        .with(&path, move |r| {
            generate_sitemaps(
                r,
                &options,
                SitemapOutput::Folder {
                    dir: &folder,
                    replace,
                },
            )
            .map_err(|e| match e {
                GenerateError::Exists(names) => format!("These files already exist: {}", names.join(", ")),
                e => e.to_string(),
            })
        })
        .await
}
