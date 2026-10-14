//! Commands that run a crawl (spec 003, contracts/desktop-commands.md).
//!
//! Every command calls the same engine functions as `spiderlens crawl`; this
//! file only owns the thread the crawl runs on and turns engine events into
//! window events:
//!
//! - `crawl-progress` a few times a second while it runs,
//! - `crawl-warning` once per warning,
//! - `crawl-finished` when it completes, pauses, stops or fails.
//!
//! One crawl runs at a time, as in other desktop crawlers.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use serde::Serialize;
use spiderlens_config::{CrawlConfig, Mode, ModeInput, Severity, validate};
use spiderlens_engine::crawl::{
    Control, CrawlEvent, CrawlSummary, Crawler, NotCrawledReason, Progress, default_file_name,
};
use tauri::{AppHandle, Emitter};
use tokio::sync::{mpsc, watch};

type Cmd<T> = Result<T, String>;

struct Active {
    id: u64,
    control: watch::Sender<Control>,
}

pub struct State {
    /// Where new crawl files go.
    dir: PathBuf,
    active: Mutex<Option<Active>>,
    next_id: AtomicU64,
}

impl State {
    pub fn new(dir: PathBuf) -> Self {
        Self {
            dir,
            active: Mutex::new(None),
            next_id: AtomicU64::new(1),
        }
    }

    /// A crawl is running or paused.
    pub fn is_running(&self) -> bool {
        self.active.lock().expect("crawl lock").is_some()
    }
}

#[derive(Serialize)]
pub struct Started {
    id: u64,
    path: PathBuf,
}

#[derive(Serialize, Clone)]
struct ProgressEvent {
    id: u64,
    #[serde(flatten)]
    progress: Progress,
}

#[derive(Serialize, Clone)]
struct WarningEvent {
    id: u64,
    message: String,
}

#[derive(Serialize, Clone)]
pub struct ReasonCount {
    reason: NotCrawledReason,
    label: &'static str,
    count: u64,
}

/// The summary plus the reason labels, so the UI never keeps its own copy of
/// the wording (spec FR-037).
#[derive(Serialize, Clone)]
pub struct Report {
    path: PathBuf,
    summary: CrawlSummary,
    reasons: Vec<ReasonCount>,
    /// Crawled URLs that got no response, per error class (spec 007 FR-004).
    failures: Vec<FailureCount>,
}

#[derive(Serialize, Clone)]
struct FailureCount {
    class: String,
    label: String,
    count: u64,
}

impl Report {
    fn new(path: PathBuf, summary: CrawlSummary) -> Self {
        let reasons = summary
            .by_reason
            .iter()
            .map(|(r, n)| ReasonCount {
                reason: *r,
                label: r.label(),
                count: *n,
            })
            .collect();
        let failures = summary
            .failed_by_class
            .iter()
            .map(|(c, n)| FailureCount {
                label: spiderlens_results::error_class_label(c).to_string(),
                class: c.clone(),
                count: *n,
            })
            .collect();
        Self {
            path,
            summary,
            reasons,
            failures,
        }
    }
}

#[derive(Serialize, Clone)]
struct FinishedEvent {
    id: u64,
    path: PathBuf,
    report: Option<Report>,
    error: Option<String>,
}

/// A name that does not exist yet in `dir` (files are never overwritten).
fn free_path(dir: &Path, name: &str) -> PathBuf {
    let first = dir.join(name);
    if !first.exists() {
        return first;
    }
    let stem = name.trim_end_matches(".spiderlens");
    (2..)
        .map(|n| dir.join(format!("{stem}-{n}.spiderlens")))
        .find(|p| !p.exists())
        .expect("an unused name exists")
}

/// How the crawl file is reached: created new, or reopened to resume.
enum Open {
    Create(Box<(CrawlConfig, ModeInput)>),
    Resume,
}

/// Starts a crawl with the settings on screen. In List mode the input is the
/// list most recently parsed in this window, as for `config_validate`.
#[tauri::command]
pub fn crawl_start(
    app: AppHandle,
    state: tauri::State<State>,
    settings: tauri::State<crate::settings::State>,
    config: CrawlConfig,
    start_url: Option<String>,
) -> Cmd<Started> {
    let input = match config.mode {
        Mode::Spider => ModeInput::Spider {
            start_url: start_url.ok_or("Enter a start URL.")?,
        },
        Mode::List => ModeInput::List(settings.list().ok_or("Add a list of URLs.")?),
    };
    if let Some(e) = validate(&config, &input)
        .into_iter()
        .find(|i| i.severity == Severity::Error)
    {
        return Err(e.message);
    }
    std::fs::create_dir_all(&state.dir)
        .map_err(|e| format!("Could not create {}: {e}", state.dir.display()))?;
    let path = free_path(&state.dir, &default_file_name(&input));
    launch(app, &state, path, Open::Create(Box::new((config, input))))
}

/// Continues a paused or stopped crawl with the settings stored in its file.
#[tauri::command]
pub fn crawl_resume(app: AppHandle, state: tauri::State<State>, path: PathBuf) -> Cmd<Started> {
    launch(app, &state, path, Open::Resume)
}

/// Finish the requests in flight, keep the queue, save as paused.
#[tauri::command]
pub fn crawl_pause(state: tauri::State<State>, id: u64) {
    send(&state, id, Control::Pause);
}

/// Finish the requests in flight; the rest of the queue is "Crawl stopped".
#[tauri::command]
pub fn crawl_stop(state: tauri::State<State>, id: u64) {
    send(&state, id, Control::Stop);
}

#[tauri::command]
pub fn crawl_summary(path: PathBuf) -> Cmd<Report> {
    let summary = CrawlSummary::read(&path).map_err(|e| e.to_string())?;
    Ok(Report::new(path, summary))
}

fn send(state: &State, id: u64, c: Control) {
    if let Some(a) = state.active.lock().expect("crawl lock").as_ref()
        && a.id == id
    {
        let _ = a.control.send(c);
    }
}

fn launch(app: AppHandle, state: &State, path: PathBuf, open: Open) -> Cmd<Started> {
    let mut active = state.active.lock().expect("crawl lock");
    if active.is_some() {
        return Err("A crawl is already running.".into());
    }
    let id = state.next_id.fetch_add(1, Ordering::Relaxed);
    let (control_tx, control_rx) = watch::channel(Control::Run);
    *active = Some(Active {
        id,
        control: control_tx,
    });
    drop(active);

    let file = path.clone();
    // The crawl gets its own thread and runtime, exactly as in the CLI, so a
    // busy crawl never competes with the commands that keep the window
    // responsive. Creating the file happens there too: seeding a large list
    // takes a moment.
    std::thread::Builder::new()
        .name("crawl".into())
        .spawn(move || {
            let result = run(&app, id, &file, open, control_rx);
            let state = tauri::Manager::state::<State>(&app);
            state.active.lock().expect("crawl lock").take();
            let (report, error) = match result {
                Ok(summary) => (Some(Report::new(file.clone(), summary)), None),
                Err(e) => (None, Some(e)),
            };
            let _ = app.emit(
                "crawl-finished",
                FinishedEvent {
                    id,
                    path: file,
                    report,
                    error,
                },
            );
        })
        .map_err(|e| format!("Could not start the crawl: {e}"))?;
    Ok(Started { id, path })
}

fn run(
    app: &AppHandle,
    id: u64,
    path: &Path,
    open: Open,
    control: watch::Receiver<Control>,
) -> Result<CrawlSummary, String> {
    let crawler = match open {
        Open::Create(new) => {
            let (config, input) = *new;
            Crawler::create(path, config, &input)
        }
        Open::Resume => Crawler::open(path),
    }
    .map_err(|e| e.to_string())?;
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|e| format!("Could not start the crawl: {e}"))?;
    rt.block_on(async move {
        let (event_tx, mut event_rx) = mpsc::unbounded_channel();
        let events = app.clone();
        tokio::spawn(async move {
            while let Some(e) = event_rx.recv().await {
                let _ = match e {
                    CrawlEvent::Progress(progress) => {
                        events.emit("crawl-progress", ProgressEvent { id, progress })
                    }
                    CrawlEvent::Warning(message) => {
                        events.emit("crawl-warning", WarningEvent { id, message })
                    }
                };
            }
        });
        crawler.run(control, Some(event_tx)).await
    })
    .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn free_path_never_reuses_a_name() {
        let dir = std::env::temp_dir().join(format!("spiderlens-free-path-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let name = "example.com-20261012T000000Z.spiderlens";
        assert_eq!(free_path(&dir, name), dir.join(name));
        std::fs::write(dir.join(name), b"").unwrap();
        assert_eq!(
            free_path(&dir, name),
            dir.join("example.com-20261012T000000Z-2.spiderlens")
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
