//! Commands for crawl comparison (spec 012,
//! specs/012-crawl-comparison/contracts/desktop-commands.md).
//!
//! One comparison thread owns every open comparison (each holds its own
//! read-only connections to both crawl files and its workspace), so the
//! crawl thread and the results reader are never involved. Commands send
//! it jobs and await the answer, as the results reader does.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex, mpsc};

use serde::Serialize;
use spiderlens_compare::{
    Comparison, ComparisonReport, ElementDelta, Error, Group, GroupPage, GroupSort, MappingTest, PairDetail,
    Phase, Setup, Summary,
};
use spiderlens_config::{CompareSettings, ElementSetting, MapRule};
use tauri::{AppHandle, Emitter, Manager};

type Cmd<T> = Result<T, String>;
type Open = HashMap<u32, Comparison>;
type Job = Box<dyn FnOnce(&mut Open) + Send>;

pub struct State {
    jobs: Mutex<Option<mpsc::Sender<Job>>>,
    cancels: Mutex<HashMap<u32, Arc<AtomicBool>>>,
    next: AtomicU32,
}

impl Default for State {
    fn default() -> Self {
        Self {
            jobs: Mutex::new(None),
            cancels: Mutex::new(HashMap::new()),
            next: AtomicU32::new(1),
        }
    }
}

const STOPPED: &str = "The comparison stopped.";

impl State {
    fn send(&self, job: Job) -> Cmd<()> {
        let mut jobs = self.jobs.lock().expect("compare lock");
        if jobs.is_none() {
            let (tx, rx) = mpsc::channel::<Job>();
            std::thread::Builder::new()
                .name("compare".into())
                .spawn(move || {
                    let mut open = Open::new();
                    while let Ok(job) = rx.recv() {
                        job(&mut open);
                    }
                })
                .map_err(|e| format!("Could not start the comparison: {e}"))?;
            *jobs = Some(tx);
        }
        jobs.as_ref()
            .expect("just set")
            .send(job)
            .map_err(|_| STOPPED.to_string())
    }

    /// Runs `f` on comparison `id` and returns its answer.
    async fn with<T: Send + 'static>(
        &self,
        id: u32,
        f: impl FnOnce(&mut Comparison) -> spiderlens_compare::Result<T> + Send + 'static,
    ) -> Cmd<T> {
        let (reply, answer) = tokio::sync::oneshot::channel();
        self.send(Box::new(move |open: &mut Open| {
            let r = match open.get_mut(&id) {
                Some(c) => f(c).map_err(err),
                None => Err("This comparison is closed.".to_string()),
            };
            let _ = reply.send(r);
        }))?;
        answer.await.map_err(|_| STOPPED.to_string())?
    }
}

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> Cmd<T> + Send + 'static) -> Cmd<T> {
    tokio::task::spawn_blocking(f).await.map_err(err)?
}

/// The crawl to compare `current` with by default: the latest earlier
/// crawl of the same site in the app's crawl folder (FR-001).
#[tauri::command]
pub async fn compare_suggest(
    crawls: tauri::State<'_, crate::crawl::State>,
    current: PathBuf,
) -> Cmd<Option<PathBuf>> {
    let dir = crawls.dir().to_path_buf();
    blocking(move || Ok(spiderlens_compare::suggest_previous(&dir, &current))).await
}

/// Both files described, with the configuration differences (FR-003).
#[tauri::command]
pub async fn compare_setup(previous: PathBuf, current: PathBuf) -> Cmd<Setup> {
    blocking(move || spiderlens_compare::setup(&previous, &current).map_err(err)).await
}

/// The mapping tester (FR-008).
#[tauri::command]
pub async fn compare_test_mapping(
    previous: PathBuf,
    current: PathBuf,
    rules: Vec<MapRule>,
    samples: Vec<String>,
    from_previous: u32,
) -> Cmd<Vec<MappingTest>> {
    blocking(move || {
        spiderlens_compare::test_mapping(&previous, &current, &rules, &samples, from_previous).map_err(err)
    })
    .await
}

#[derive(Serialize, Clone)]
struct CompareProgress {
    comparison: u32,
    phase: &'static str,
    done: u64,
    total: u64,
    error: Option<String>,
}

fn phase_name(p: Phase) -> &'static str {
    match p {
        Phase::Open => "open",
        Phase::Previous => "previous",
        Phase::Current => "current",
        Phase::Match => "match",
        Phase::Counts => "counts",
        Phase::Done => "done",
    }
}

/// Starts a comparison; `compare-progress` events follow and end with
/// `done`, `failed` or `cancelled`.
#[tauri::command]
pub fn compare_start(
    app: AppHandle,
    state: tauri::State<'_, State>,
    previous: PathBuf,
    current: PathBuf,
    settings: CompareSettings,
) -> Cmd<u32> {
    settings.validate().map_err(err)?;
    let id = state.next.fetch_add(1, Ordering::Relaxed);
    let cancel = Arc::new(AtomicBool::new(false));
    state
        .cancels
        .lock()
        .expect("compare lock")
        .insert(id, cancel.clone());
    state.send(Box::new(move |open: &mut Open| {
        let emit = |phase, done, total, error| {
            let _ = app.emit(
                "compare-progress",
                CompareProgress {
                    comparison: id,
                    phase,
                    done,
                    total,
                    error,
                },
            );
        };
        let mut last = std::time::Instant::now();
        let mut last_phase = None;
        let result = Comparison::run(&previous, &current, settings, &cancel, &mut |p| {
            // Phase changes always go out; batches at most four times a second.
            if p.phase == Phase::Done {
                return;
            }
            if last_phase != Some(p.phase) || last.elapsed().as_millis() >= 250 {
                last = std::time::Instant::now();
                last_phase = Some(p.phase);
                emit(phase_name(p.phase), p.done, p.total, None);
            }
        });
        match result {
            Ok(c) => {
                open.insert(id, c);
                emit("done", 1, 1, None);
            }
            Err(Error::Cancelled) => emit("cancelled", 0, 0, None),
            Err(e) => emit("failed", 0, 0, Some(e.to_string())),
        }
        app.state::<State>()
            .cancels
            .lock()
            .expect("compare lock")
            .remove(&id);
    }))?;
    Ok(id)
}

#[tauri::command]
pub fn compare_cancel(state: tauri::State<'_, State>, comparison: u32) {
    if let Some(c) = state.cancels.lock().expect("compare lock").get(&comparison) {
        c.store(true, Ordering::Relaxed);
    }
}

#[tauri::command]
pub async fn compare_summary(state: tauri::State<'_, State>, comparison: u32) -> Cmd<Summary> {
    state.with(comparison, |c| Ok(c.summary().clone())).await
}

#[tauri::command]
pub async fn compare_settings(state: tauri::State<'_, State>, comparison: u32) -> Cmd<CompareSettings> {
    state.with(comparison, |c| Ok(c.settings().clone())).await
}

/// New thresholds or tracked elements, recounted without a rescan (FR-019).
#[tauri::command]
pub async fn compare_set_elements(
    state: tauri::State<'_, State>,
    comparison: u32,
    elements: Vec<ElementSetting>,
) -> Cmd<Vec<ElementDelta>> {
    state.with(comparison, move |c| c.set_elements(elements)).await
}

#[tauri::command]
pub async fn compare_open_group(
    state: tauri::State<'_, State>,
    comparison: u32,
    group: Group,
    search: String,
    sort: Option<GroupSort>,
    count: u32,
) -> Cmd<GroupPage> {
    state
        .with(comparison, move |c| {
            c.open_group(&group, &search, sort.as_ref(), count)
        })
        .await
}

#[tauri::command]
pub async fn compare_rows(
    state: tauri::State<'_, State>,
    comparison: u32,
    view: u32,
    from: u32,
    count: u32,
) -> Cmd<GroupPage> {
    state.with(comparison, move |c| c.rows(view, from, count)).await
}

#[tauri::command]
pub async fn compare_close_group(state: tauri::State<'_, State>, comparison: u32, view: u32) -> Cmd<()> {
    state.with(comparison, move |c| c.close_group(view)).await
}

#[tauri::command]
pub async fn compare_detail(
    state: tauri::State<'_, State>,
    comparison: u32,
    prev_id: Option<i64>,
    cur_id: Option<i64>,
) -> Cmd<PairDetail> {
    state.with(comparison, move |c| c.detail(prev_id, cur_id)).await
}

#[tauri::command]
pub async fn compare_report(state: tauri::State<'_, State>, comparison: u32) -> Cmd<ComparisonReport> {
    state.with(comparison, |c| c.report()).await
}

/// Exports an open group list as CSV; progress arrives as the results
/// table's `export-progress` events and `export_cancel` stops it.
#[tauri::command]
pub fn compare_export(
    app: AppHandle,
    state: tauri::State<'_, State>,
    results: tauri::State<'_, crate::results::State>,
    comparison: u32,
    view: u32,
    dest: PathBuf,
) -> Cmd<u32> {
    let (job, cancel) = results.new_export();
    state.send(Box::new(move |open: &mut Open| {
        let progress = crate::results::ExportEvents::new(&app, job, dest.clone());
        let result = match open.get_mut(&comparison) {
            Some(c) => {
                let mut last = std::time::Instant::now();
                c.export_group(view, &dest, &cancel, &mut |n, total| {
                    if last.elapsed().as_millis() >= 250 || n == 0 {
                        last = std::time::Instant::now();
                        progress.send(n, total, "running", None);
                    }
                })
            }
            None => Err(Error::Invalid("This comparison is closed.".into())),
        };
        match result {
            Ok(n) => progress.send(n, n, "done", None),
            Err(Error::Cancelled) => progress.send(0, 0, "cancelled", None),
            Err(e) => progress.send(0, 0, "failed", Some(e.to_string())),
        }
        app.state::<crate::results::State>().end_export(job);
    }))?;
    Ok(job)
}

/// Closes a comparison and deletes its workspace (FR-031).
#[tauri::command]
pub async fn compare_close(state: tauri::State<'_, State>, comparison: u32) -> Cmd<()> {
    if let Some(c) = state.cancels.lock().expect("compare lock").get(&comparison) {
        c.store(true, Ordering::Relaxed);
    }
    let (reply, answer) = tokio::sync::oneshot::channel();
    state.send(Box::new(move |open: &mut Open| {
        open.remove(&comparison);
        let _ = reply.send(());
    }))?;
    answer.await.map_err(|_| STOPPED.to_string())
}

/// Comparison settings saved to and loaded from a file of the user's
/// choosing (FR-009).
#[tauri::command]
pub async fn compare_settings_load(path: PathBuf) -> Cmd<CompareSettings> {
    blocking(move || {
        let text =
            std::fs::read_to_string(&path).map_err(|e| format!("Could not read {}: {e}", path.display()))?;
        CompareSettings::from_json(&text).map_err(err)
    })
    .await
}

#[tauri::command]
pub async fn compare_settings_save(path: PathBuf, settings: CompareSettings) -> Cmd<()> {
    blocking(move || {
        settings.validate().map_err(err)?;
        std::fs::write(&path, settings.to_json())
            .map_err(|e| format!("Could not write {}: {e}", path.display()))
    })
    .await
}
