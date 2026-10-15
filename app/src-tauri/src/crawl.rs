//! Commands that run a crawl (spec 003, contracts/desktop-commands.md).
//!
//! Every command calls the same engine functions as `spiderlens crawl`; this
//! file only owns the thread the crawl runs on and turns engine events into
//! window events:
//!
//! - `crawl-progress` a few times a second while it runs,
//! - `crawl-warning` once per warning,
//! - `crawl-notice` when a host pauses, resumes or is given up, or the
//!   network drops (spec 004),
//! - `crawl-finished` when it completes, pauses, stops or fails.
//!
//! One crawl runs at a time, as in other desktop crawlers.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use serde::Deserialize;
use serde::Serialize;
use spiderlens_config::{
    Change, CrawlConfig, Mode, ModeInput, RetrySettings, Severity, SpeedSettings, validate,
};
use spiderlens_engine::crawl::{
    Control, CrawlError, CrawlEvent, CrawlStore, CrawlSummary, Crawler, LocalError, LogEntry,
    NotCrawledReason, Notice, Progress, ResumeOptions, RunCommand, Upgrade, default_file_name,
    retry_preview as engine_retry_preview,
};
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::{mpsc, oneshot, watch};

type Cmd<T> = Result<T, String>;

struct Active {
    id: u64,
    control: watch::Sender<Control>,
    commands: mpsc::UnboundedSender<RunCommand>,
    /// The crawl file; changes when Save As moves a running crawl.
    path: PathBuf,
}

pub struct State {
    /// Where new crawl files go.
    dir: PathBuf,
    active: Mutex<Option<Active>>,
    next_id: AtomicU64,
    /// The user quit while a crawl ran: exit once it has paused (FR-012).
    quitting: AtomicBool,
}

impl State {
    pub fn new(dir: PathBuf) -> Self {
        Self {
            dir,
            active: Mutex::new(None),
            next_id: AtomicU64::new(1),
            quitting: AtomicBool::new(false),
        }
    }

    /// A crawl is running or paused.
    pub fn is_running(&self) -> bool {
        self.active.lock().expect("crawl lock").is_some()
    }

    pub fn active_path(&self) -> Option<PathBuf> {
        self.active
            .lock()
            .expect("crawl lock")
            .as_ref()
            .map(|a| a.path.clone())
    }

    /// Asks the running crawl to move to `dest` when `path` is the running
    /// crawl; None otherwise (spec 006 plan D5).
    pub fn save_as_running(
        &self,
        path: &Path,
        dest: &Path,
    ) -> Option<oneshot::Receiver<Result<PathBuf, String>>> {
        let active = self.active.lock().expect("crawl lock");
        let a = active.as_ref().filter(|a| a.path == path)?;
        let (reply, wait) = oneshot::channel();
        a.commands
            .send(RunCommand::SaveAs {
                dest: dest.to_path_buf(),
                reply,
            })
            .ok()?;
        Some(wait)
    }

    /// Quitting with a crawl running: pause it and exit when it has
    /// stopped. Returns false when nothing is running.
    pub fn pause_for_quit(&self) -> bool {
        let active = self.active.lock().expect("crawl lock");
        let Some(a) = active.as_ref() else { return false };
        if self.quitting.swap(true, Ordering::SeqCst) {
            // A second quit while pausing: exit now; the file stays
            // consistent (spec 006 plan D12).
            return false;
        }
        let _ = a.control.send(Control::Pause);
        true
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
struct NoticeEvent {
    id: u64,
    #[serde(flatten)]
    notice: Notice,
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
    /// A disk or write problem paused the crawl (spec 006 FR-029).
    local: Option<LocalError>,
}

#[derive(Serialize, Clone)]
struct MovedEvent {
    id: u64,
    from: PathBuf,
    to: PathBuf,
}

/// A name that does not exist yet in `dir` (files are never overwritten).
pub fn free_path(dir: &Path, name: &str) -> PathBuf {
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

/// How the crawl file is reached: created new, reopened to resume, or
/// reopened with its transient failures queued again (spec 004 US5).
enum Open {
    Create(Box<(CrawlConfig, ModeInput)>),
    Resume(Box<Crawler>),
    RetryFailed,
}

/// Speed and URL-limit changes for a resume (FR-011).
#[derive(Deserialize, Default)]
pub struct ResumeChanges {
    speed: Option<SpeedSettings>,
    #[serde(default, deserialize_with = "some_value")]
    max_urls: Option<Option<u64>>,
}

/// Keeps "set to null" (no limit) apart from "not given".
fn some_value<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<Option<u64>>, D::Error> {
    Option::<u64>::deserialize(d).map(Some)
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

/// Continues a paused, stopped or recovered crawl with the settings stored
/// in its file. Errors the window answers with a prompt carry a code before
/// the colon: `needs_upgrade`, `needs_secrets`, `read_only`, `in_use`.
#[tauri::command]
pub async fn crawl_resume(
    app: AppHandle,
    path: PathBuf,
    changes: Option<ResumeChanges>,
    secrets: Option<std::collections::HashMap<String, String>>,
    upgrade: Option<String>,
) -> Cmd<Started> {
    let mut options = ResumeOptions {
        upgrade: match upgrade.as_deref() {
            Some("keep_copy") => Some(Upgrade::KeepCopy),
            Some("no_copy") => Some(Upgrade::NoCopy),
            _ => None,
        },
        secrets: secrets.unwrap_or_default().into_iter().collect(),
        changes: Vec::new(),
    };
    if let Some(c) = changes {
        if let Some(s) = c.speed {
            options.changes.push(Change::Speed(s));
        }
        if let Some(m) = c.max_urls {
            options.changes.push(Change::MaxUrls(m));
        }
    }
    let file = path.clone();
    let crawler = tauri::async_runtime::spawn_blocking(move || Crawler::open_with(&file, &options))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| match e {
            CrawlError::NeedsUpgrade { .. } => format!("needs_upgrade: {e}"),
            CrawlError::NeedsSecrets(names) => format!("needs_secrets: {}", names.join(", ")),
            CrawlError::ReadOnly(_) => format!("read_only: {e}"),
            CrawlError::Lock(l) => format!("in_use: {l}"),
            other => other.to_string(),
        })?;
    let state = app.state::<State>();
    launch(app.clone(), &state, path, Open::Resume(Box::new(crawler)))
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

/// Ends a host's pause now and sends it a test request (spec 004 US4.5).
/// `host` is the host key shown in the paused-hosts list.
#[tauri::command]
pub fn crawl_resume_host(state: tauri::State<State>, id: u64, host: String) {
    command(&state, id, RunCommand::ResumeHost(host));
}

/// Applies changed speed or retry settings to the running crawl. The change
/// is recorded in the crawl's stored configuration (spec 004 D13).
#[tauri::command]
pub fn crawl_apply_settings(
    state: tauri::State<State>,
    id: u64,
    speed: Option<SpeedSettings>,
    retry: Option<RetrySettings>,
) {
    if let Some(speed) = speed {
        command(&state, id, RunCommand::Apply(Change::Speed(speed)));
    }
    if let Some(retry) = retry {
        command(&state, id, RunCommand::Apply(Change::Retry(retry)));
    }
}

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct ClassCount {
    class: String,
    label: String,
    count: u64,
}

/// What "Retry failed URLs" would do, shown before anything changes
/// (spec 004 US5.1, FR-034).
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct RetryPreview {
    total: u64,
    by_class: Vec<ClassCount>,
    available: bool,
    reason: Option<String>,
}

#[tauri::command]
pub fn retry_preview(state: tauri::State<State>, path: PathBuf) -> Cmd<RetryPreview> {
    let running = state
        .active
        .lock()
        .expect("crawl lock")
        .as_ref()
        .is_some_and(|a| a.path == path);
    preview(&path, running)
}

fn preview(path: &Path, running: bool) -> Cmd<RetryPreview> {
    let by_class: Vec<ClassCount> = engine_retry_preview(path)
        .map_err(|e| e.to_string())?
        .into_iter()
        .map(|(class, count)| ClassCount {
            label: spiderlens_results::error_class_label(&class).to_string(),
            class,
            count,
        })
        .collect();
    let total = by_class.iter().map(|c| c.count).sum();
    let reason = if running {
        Some("Wait until the crawl is paused or finished.".to_string())
    } else if total == 0 {
        Some("No URL failed for a reason that could change.".to_string())
    } else {
        None
    };
    Ok(RetryPreview {
        total,
        by_class,
        available: reason.is_none(),
        reason,
    })
}

/// Queues the crawl's transient failures again and runs them; progress and
/// the finish arrive as the usual crawl events (spec 004 US5.2).
#[tauri::command]
pub fn retry_start(app: AppHandle, state: tauri::State<State>, path: PathBuf) -> Cmd<Started> {
    launch(app, &state, path, Open::RetryFailed)
}

/// The newest entries of the crawl log (spec 004 FR-035).
#[tauri::command]
pub fn crawl_log(path: PathBuf, limit: Option<usize>) -> Cmd<Vec<LogEntry>> {
    let store = CrawlStore::open(&path).map_err(|e| e.to_string())?;
    store.log_entries(limit.unwrap_or(500)).map_err(|e| e.to_string())
}

fn send(state: &State, id: u64, c: Control) {
    if let Some(a) = state.active.lock().expect("crawl lock").as_ref()
        && a.id == id
    {
        let _ = a.control.send(c);
    }
}

fn command(state: &State, id: u64, c: RunCommand) {
    if let Some(a) = state.active.lock().expect("crawl lock").as_ref()
        && a.id == id
    {
        let _ = a.commands.send(c);
    }
}

fn launch(app: AppHandle, state: &State, path: PathBuf, open: Open) -> Cmd<Started> {
    let mut active = state.active.lock().expect("crawl lock");
    if active.is_some() {
        return Err("A crawl is already running.".into());
    }
    let id = state.next_id.fetch_add(1, Ordering::Relaxed);
    let (control_tx, control_rx) = watch::channel(Control::Run);
    let (command_tx, command_rx) = mpsc::unbounded_channel();
    *active = Some(Active {
        id,
        control: control_tx,
        commands: command_tx,
        path: path.clone(),
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
            let result = run(&app, id, &file, open, control_rx, command_rx);
            let state = app.state::<State>();
            // Save As may have moved the crawl while it ran.
            let file = state
                .active
                .lock()
                .expect("crawl lock")
                .take()
                .map_or(file, |a| a.path);
            // Spec 015 FR-002: the AI checks run once a crawl completes.
            let ai_checks = matches!(&result, Ok((s, true)) if s.status == "completed");
            let (report, error, local) = match result {
                Ok((summary, _)) => (Some(Report::new(file.clone(), summary)), None, None),
                Err(CrawlError::Local(l)) => (
                    CrawlSummary::read(&file)
                        .ok()
                        .map(|s| Report::new(file.clone(), s)),
                    None,
                    Some(l),
                ),
                Err(e) => (None, Some(e.to_string()), None),
            };
            let urls = report.as_ref().map(|r| r.summary.discovered);
            app.state::<crate::files::State>().touch(&file, urls);
            let _ = app.emit(
                "crawl-finished",
                FinishedEvent {
                    id,
                    path: file.clone(),
                    report,
                    error,
                    local,
                },
            );
            if ai_checks && !state.quitting.load(Ordering::SeqCst) {
                let aeo = app.state::<crate::aeo::State>();
                let _ = crate::aeo::start(app.clone(), &aeo, file);
            }
            if state.quitting.load(Ordering::SeqCst) {
                app.exit(0);
            }
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
    commands: mpsc::UnboundedReceiver<RunCommand>,
) -> Result<(CrawlSummary, bool), CrawlError> {
    let crawler = match open {
        Open::Create(new) => {
            let (config, input) = *new;
            let c = Crawler::create(path, config, &input)?;
            app.state::<crate::files::State>().touch(path, None);
            c
        }
        Open::Resume(c) => *c,
        Open::RetryFailed => Crawler::retry_failed(path)?.0,
    };
    let reserve = app.state::<crate::files::State>().disk_reserve();
    let mut crawler = crawler.with_disk_reserve(Some(reserve));
    // The AI checks read page content while the crawl has each page parsed.
    if crawler.config().ai_checks.auto {
        crawler.set_page_hook(spiderlens_aeo::content::page_hook());
    }
    let ai_checks = crawler.config().ai_checks.auto;
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|e| CrawlError::Config(format!("Could not start the crawl: {e}")))?;
    // Save As requests pass through here so the window hears about the move.
    let (forward_tx, forward_rx) = mpsc::unbounded_channel();
    let mut commands = commands;
    let moves = app.clone();
    let from = path.to_path_buf();
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
                    CrawlEvent::Notice(notice) => events.emit("crawl-notice", NoticeEvent { id, notice }),
                };
            }
        });
        tokio::spawn(async move {
            let mut from = from;
            while let Some(cmd) = commands.recv().await {
                // Only Save As needs an answer; the rest go straight through.
                let RunCommand::SaveAs { dest, reply } = cmd else {
                    if forward_tx.send(cmd).is_err() {
                        break;
                    }
                    continue;
                };
                let (inner, wait) = oneshot::channel();
                if forward_tx
                    .send(RunCommand::SaveAs { dest, reply: inner })
                    .is_err()
                {
                    break;
                }
                let result = wait.await.unwrap_or_else(|_| Err("The crawl ended.".into()));
                if let Ok(to) = &result {
                    let state = moves.state::<State>();
                    if let Some(a) = state.active.lock().expect("crawl lock").as_mut() {
                        a.path = to.clone();
                    }
                    let _ = moves.emit(
                        "crawl-moved",
                        MovedEvent {
                            id,
                            from: from.clone(),
                            to: to.clone(),
                        },
                    );
                    from = to.clone();
                }
                let _ = reply.send(result);
            }
        });
        crawler.run_with(control, Some(event_tx), Some(forward_rx)).await
    })
    .map(|s| (s, ai_checks))
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

    #[test]
    fn retry_preview_explains_why_it_is_unavailable() {
        let dir = std::env::temp_dir().join(format!("spiderlens-retry-preview-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("t.spiderlens");
        drop(CrawlStore::create(&path).unwrap());
        let p = preview(&path, false).unwrap();
        assert_eq!(p.total, 0);
        assert!(!p.available);
        assert_eq!(
            p.reason.as_deref(),
            Some("No URL failed for a reason that could change.")
        );
        let p = preview(&path, true).unwrap();
        assert_eq!(
            p.reason.as_deref(),
            Some("Wait until the crawl is paused or finished.")
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
