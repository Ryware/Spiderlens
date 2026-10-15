//! AI search checks commands (spec 015 contracts/desktop-commands.md).
//!
//! The same `spiderlens_aeo` functions as `spiderlens aeo`; a run gets its
//! own thread and reports through window events:
//!
//! - `aeo-progress` per pass and per chunk of pages,
//! - `aeo-finished` with the summary or an error.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use rusqlite::{Connection, OpenFlags};
use serde::Serialize;
use spiderlens_aeo::analyze::{self, AnalyzeOptions, PageKey, PageResult, Progress, Summary};
use spiderlens_aeo::rules::{AeoRule, RULES};
use spiderlens_config::ai_bots::{self, AiBot};
use tauri::{AppHandle, Emitter};

type Cmd<T> = Result<T, String>;

#[derive(Default)]
pub struct State {
    runs: Mutex<HashMap<u64, Arc<AtomicBool>>>,
    next_id: AtomicU64,
}

#[derive(Serialize)]
pub struct RunStarted {
    run_id: u64,
}

#[derive(Serialize, Clone)]
struct ProgressEvent {
    run_id: u64,
    path: PathBuf,
    #[serde(flatten)]
    progress: Progress,
}

#[derive(Serialize, Clone)]
struct FinishedEvent {
    run_id: u64,
    path: PathBuf,
    summary: Option<Summary>,
    error: Option<String>,
}

/// Runs the AI checks on a crawl file, replacing earlier results.
#[tauri::command]
pub fn aeo_run(app: AppHandle, state: tauri::State<State>, path: PathBuf) -> Cmd<RunStarted> {
    start(app, &state, path)
}

/// Stops a run; the results of the previous run stay in place.
#[tauri::command]
pub fn aeo_cancel(state: tauri::State<State>, run_id: u64) {
    if let Some(c) = state.runs.lock().expect("aeo lock").get(&run_id) {
        c.store(true, Ordering::Relaxed);
    }
}

/// Stored results; `null` when the checks have not run on this file.
#[tauri::command]
pub async fn aeo_summary(path: PathBuf) -> Cmd<Option<Summary>> {
    blocking(move || analyze::summary(&read_only(&path)?).map_err(|e| e.to_string())).await
}

/// One page's result for the detail section.
#[tauri::command]
pub async fn aeo_page(path: PathBuf, url_id: i64) -> Cmd<Option<PageResult>> {
    blocking(move || analyze::page_result(&read_only(&path)?, PageKey::Id(url_id)).map_err(|e| e.to_string()))
        .await
}

#[derive(Serialize)]
pub struct Draft {
    text: String,
}

/// A draft llms.txt; the UI saves it through the save dialog.
#[tauri::command]
pub async fn aeo_llms_txt_draft(path: PathBuf, max_links: usize) -> Cmd<Draft> {
    blocking(move || {
        analyze::draft_llms_txt(&read_only(&path)?, max_links)
            .map(|text| Draft { text })
            .map_err(|e| e.to_string())
    })
    .await
}

#[derive(Serialize)]
pub struct BotList {
    version: u32,
    date: &'static str,
    bots: &'static [AiBot],
}

#[tauri::command]
pub fn ai_bots_list() -> BotList {
    BotList {
        version: ai_bots::version(),
        date: ai_bots::date(),
        bots: ai_bots::all(),
    }
}

/// The checks with their names, explanations and fixes (FR-038).
#[tauri::command]
pub fn aeo_rules() -> &'static [AeoRule] {
    RULES
}

/// Starts a run; the crawl's finish step uses this too.
pub fn start(app: AppHandle, state: &State, path: PathBuf) -> Cmd<RunStarted> {
    let run_id = state.next_id.fetch_add(1, Ordering::Relaxed) + 1;
    let cancel = Arc::new(AtomicBool::new(false));
    state
        .runs
        .lock()
        .expect("aeo lock")
        .insert(run_id, cancel.clone());
    std::thread::Builder::new()
        .name("aeo".into())
        .spawn(move || {
            let opts = AnalyzeOptions {
                cancel: Some(cancel),
                ..Default::default()
            };
            let events = app.clone();
            let file = path.clone();
            let result = analyze::analyze(&path, &opts, &|progress| {
                let _ = events.emit(
                    "aeo-progress",
                    ProgressEvent {
                        run_id,
                        path: file.clone(),
                        progress,
                    },
                );
            });
            let state = tauri::Manager::state::<State>(&app);
            state.runs.lock().expect("aeo lock").remove(&run_id);
            let (summary, error) = match result {
                Ok(s) => (Some(s), None),
                Err(e) => (None, Some(e.to_string())),
            };
            let _ = app.emit(
                "aeo-finished",
                FinishedEvent {
                    run_id,
                    path,
                    summary,
                    error,
                },
            );
        })
        .map_err(|e| format!("Could not start the AI checks: {e}"))?;
    Ok(RunStarted { run_id })
}

fn read_only(path: &Path) -> Result<Connection, String> {
    if !path.exists() {
        return Err(format!("{} does not exist", path.display()));
    }
    Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|e| e.to_string())
}

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> Cmd<T> + Send + 'static) -> Cmd<T> {
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|e| e.to_string())?
}
