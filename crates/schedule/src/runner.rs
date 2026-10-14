//! The tick the OS sends, and the run it starts (plan D1, D5).
//!
//! The tick only decides: it reads the schedule and its history, records a
//! Missed run when catch-up is off, and otherwise tells the caller to start
//! a run as a separate process. The tick then exits, so the OS scheduler
//! never sees a long job and can send the next tick on time; a tick that
//! arrives while a run is still going starts a run that records Skipped.

use std::fs::{self, File, TryLockError};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use jiff::Timestamp;
use jiff::tz::TimeZone;
use serde::{Deserialize, Serialize};
use spiderlens_config::{ListFormat, ModeInput, Severity, parse_list_bytes, validate};
use spiderlens_engine::crawl::{Control, CrawlEvent, CrawlSummary, Crawler};
use spiderlens_results::{UrlScope, ViewQuery, export_file};
use tokio::sync::{mpsc, watch};

use crate::due::{Due, due};
use crate::history;
use crate::model::{
    AfterStep, Run, RunCounts, RunStatus, Schedule, ScheduleInput, StepResult, StepStatus, Trigger,
};
use crate::naming::{exports_dir, free_path, run_stem};
use crate::notify::{Notifier, text, wanted};
use crate::store::{Paths, Store, StoreError};

#[derive(Debug, thiserror::Error)]
pub enum RunError {
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error("Could not write the run history: {0}")]
    Io(#[from] std::io::Error),
}

pub struct Context {
    pub paths: Paths,
    pub tz: TimeZone,
    pub notifier: Arc<dyn Notifier>,
    /// Host name pins for tests that serve `.test` hosts locally.
    pub resolve: Vec<(String, std::net::SocketAddr)>,
}

impl Context {
    fn store(&self) -> Store {
        Store::new(self.paths.clone())
    }
}

/// What a tick decided.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TickAction {
    /// Disabled, removed, or no time has come.
    Nothing,
    /// A Missed run was recorded (catch-up off).
    Missed,
    /// Start a run for this slot.
    Run { slot: Timestamp, trigger: Trigger },
}

/// How a program starts runs of itself.
#[derive(Debug, Clone)]
pub struct Launcher {
    pub program: PathBuf,
    /// Arguments before the id for a tick, for example `["schedule", "tick"]`.
    pub tick_prefix: Vec<String>,
    /// Arguments before the id for a run, for example `["schedule", "run"]`.
    pub run_prefix: Vec<String>,
}

impl Launcher {
    pub fn tick_args(&self, id: &str) -> Vec<String> {
        let mut a = self.tick_prefix.clone();
        a.push(id.to_string());
        a
    }

    pub fn run_args(&self, id: &str, slot: Option<Timestamp>, trigger: Trigger) -> Vec<String> {
        let mut a = self.run_prefix.clone();
        a.push(id.to_string());
        if let Some(slot) = slot {
            a.push("--slot".into());
            a.push(slot.to_string());
        }
        a.push("--trigger".into());
        a.push(trigger_name(trigger).into());
        a
    }

    /// Starts a run as a separate process that outlives this one.
    pub fn spawn_run(&self, id: &str, slot: Option<Timestamp>, trigger: Trigger) -> std::io::Result<()> {
        let mut cmd = Command::new(&self.program);
        cmd.args(self.run_args(id, slot, trigger))
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const DETACHED_PROCESS: u32 = 0x0000_0008;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            cmd.creation_flags(DETACHED_PROCESS | CREATE_NO_WINDOW);
        }
        cmd.spawn().map(|_| ())
    }
}

pub fn trigger_name(t: Trigger) -> &'static str {
    match t {
        Trigger::Scheduled => "scheduled",
        Trigger::CaughtUp => "caught_up",
        Trigger::RunNow => "run_now",
    }
}

pub fn parse_trigger(s: &str) -> Option<Trigger> {
    match s {
        "scheduled" => Some(Trigger::Scheduled),
        "caught_up" => Some(Trigger::CaughtUp),
        "run_now" => Some(Trigger::RunNow),
        _ => None,
    }
}

/// Live progress of an active run, for the app (FR-017).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Progress {
    pub run: u32,
    pub counts: RunCounts,
    pub file: PathBuf,
}

pub fn read_progress(paths: &Paths, id: &str) -> Option<Progress> {
    let bytes = fs::read(progress_path(paths, id)).ok()?;
    serde_json::from_slice(&bytes).ok()
}

fn progress_path(paths: &Paths, id: &str) -> PathBuf {
    paths.run_lock(id).with_extension("progress")
}

/// Whether a run of this schedule is active now.
pub fn is_running(paths: &Paths, id: &str) -> bool {
    let path = paths.run_lock(id);
    let Ok(f) = File::options().read(true).write(true).open(&path) else {
        return false;
    };
    matches!(f.try_lock(), Err(TryLockError::WouldBlock))
}

/// History for display: a `running` record whose run is not active any more
/// reads as Interrupted (plan D4).
pub fn history_view(paths: &Paths, id: &str) -> std::io::Result<Vec<Run>> {
    let mut runs = history::read(&paths.history(id))?;
    if runs.iter().any(|r| r.status == RunStatus::Running) && !is_running(paths, id) {
        for r in runs.iter_mut().filter(|r| r.status == RunStatus::Running) {
            interrupt(r);
        }
    }
    Ok(runs)
}

fn interrupt(r: &mut Run) {
    r.status = RunStatus::Interrupted;
    r.reason = Some(
        "SpiderLens was closed or the computer shut down during the run. Open the crawl file to resume it by hand."
            .into(),
    );
}

/// What the OS calls every hour and at login.
pub fn tick(ctx: &Context, id: &str, now: Timestamp) -> Result<TickAction, RunError> {
    let schedule = match ctx.store().get(id) {
        Ok(s) => s,
        Err(StoreError::NotFound(_)) => return Ok(TickAction::Nothing),
        Err(e) => return Err(e.into()),
    };
    if !schedule.enabled {
        return Ok(TickAction::Nothing);
    }
    let Ok(cron) = schedule.frequency.cron() else {
        return Ok(TickAction::Nothing);
    };
    let runs = history::read(&ctx.paths.history(id))?;
    let last = history::last_slot(&runs).unwrap_or(schedule.created_at);
    Ok(match due(&cron, &ctx.tz, last, now, schedule.catch_up) {
        Due::Nothing => TickAction::Nothing,
        Due::Run { slot, trigger } => TickAction::Run { slot, trigger },
        Due::Missed { slot } => {
            let mut run = Run::new(
                history::next_number(&runs),
                Some(slot),
                Trigger::Scheduled,
                RunStatus::Missed,
            );
            run.reason =
                Some("The computer was off, asleep or logged out at this time, and catch-up is off.".into());
            history::append(&ctx.paths.history(id), &run)?;
            notify(ctx, &schedule, &run);
            TickAction::Missed
        }
    })
}

/// The result of a run request.
#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    /// Another process already ran or is running this slot.
    AlreadyHandled,
    Done(Run),
}

/// Exit code for `tick`/`run` (contracts/cli.md).
pub fn exit_code(o: &Outcome) -> u8 {
    match o {
        Outcome::AlreadyHandled => 0,
        Outcome::Done(r) => match r.status {
            RunStatus::Finished | RunStatus::StoppedAtLimit | RunStatus::Running | RunStatus::Missed => 0,
            RunStatus::Failed | RunStatus::Interrupted => 4,
            RunStatus::StoppedByUser => 5,
            RunStatus::PausedDisk => 6,
            RunStatus::Skipped => 7,
        },
    }
}

/// Runs one crawl of a schedule. Holds the schedule's run lock throughout.
pub async fn run(
    ctx: &Context,
    id: &str,
    slot: Option<Timestamp>,
    trigger: Trigger,
) -> Result<Outcome, RunError> {
    let schedule = ctx.store().get(id)?;
    let id = schedule.id.clone();
    let hist = ctx.paths.history(&id);
    let lock_path = ctx.paths.run_lock(&id);
    fs::create_dir_all(lock_path.parent().expect("locks folder"))?;
    let lock = File::options()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(&lock_path)?;
    let mut runs = history::read(&hist)?;
    let slot_taken = |runs: &[Run]| slot.is_some() && runs.iter().any(|r| r.slot == slot);

    match lock.try_lock() {
        Ok(()) => {}
        Err(TryLockError::WouldBlock) => {
            if slot_taken(&runs) {
                return Ok(Outcome::AlreadyHandled);
            }
            let mut run = Run::new(history::next_number(&runs), slot, trigger, RunStatus::Skipped);
            run.reason = Some("The previous run was still going.".into());
            history::append(&hist, &run)?;
            notify(ctx, &schedule, &run);
            return Ok(Outcome::Done(run));
        }
        Err(TryLockError::Error(e)) => return Err(e.into()),
    }
    if slot_taken(&runs) {
        return Ok(Outcome::AlreadyHandled);
    }
    // Anything still "running" belongs to a process that is gone.
    for r in runs.iter_mut().filter(|r| r.status == RunStatus::Running) {
        interrupt(r);
        history::append(&hist, r)?;
    }
    let stop = ctx.paths.stop_file(&id);
    let _ = fs::remove_file(&stop);

    let mut run = Run::new(history::next_number(&runs), slot, trigger, RunStatus::Running);
    run.started_at = Some(Timestamp::now());
    history::append(&hist, &run)?;

    execute(ctx, &schedule, &mut run).await;
    if run.file.is_some() {
        run.steps = after_steps(&schedule, &run);
    }
    run.ended_at = Some(Timestamp::now());
    history::append(&hist, &run)?;
    let _ = fs::remove_file(progress_path(&ctx.paths, &id));
    let _ = fs::remove_file(&stop);
    notify(ctx, &schedule, &run);
    drop(lock);
    Ok(Outcome::Done(run))
}

fn fail(run: &mut Run, reason: impl Into<String>) {
    run.status = RunStatus::Failed;
    run.reason = Some(reason.into());
}

async fn execute(ctx: &Context, s: &Schedule, run: &mut Run) {
    // FR-014: the folder must take a file before anything is requested.
    if let Err(e) = probe_dir(&s.output_dir) {
        return fail(
            run,
            format!(
                "The output folder {} cannot be written: {e}",
                s.output_dir.display()
            ),
        );
    }
    let input = match &s.input {
        ScheduleInput::Spider { start_url } => ModeInput::Spider {
            start_url: start_url.clone(),
        },
        ScheduleInput::List { path } => match fs::read(path) {
            Ok(bytes) => {
                let format = ListFormat::from_file_name(&path.to_string_lossy());
                ModeInput::List(parse_list_bytes(&bytes, format, &mut |_| {}))
            }
            Err(e) => {
                return fail(
                    run,
                    format!("The list file {} cannot be read: {e}", path.display()),
                );
            }
        },
    };
    let errors: Vec<String> = validate(&s.config, &input)
        .into_iter()
        .filter(|i| i.severity == Severity::Error)
        .map(|i| i.message)
        .collect();
    if !errors.is_empty() {
        return fail(
            run,
            format!("The crawl settings are not valid: {}", errors.join(" ")),
        );
    }
    let at = run.slot.unwrap_or_else(Timestamp::now).to_zoned(ctx.tz.clone());
    let path = free_path(&s.output_dir, &run_stem(&s.name, &at));
    let crawler = match Crawler::create(&path, s.config.clone(), &input) {
        Ok(c) => c.with_resolve(ctx.resolve.clone()),
        Err(e) => return fail(run, format!("The crawl could not start: {e}")),
    };
    let meta = serde_json::json!({
        "id": s.id,
        "name": s.name,
        "slot": run.slot,
        "trigger": run.trigger,
        "run": run.run,
    });
    if let Err(e) = crawler.set_meta("schedule", &meta.to_string()) {
        return fail(run, format!("The crawl file could not be written: {e}"));
    }
    run.file = Some(path.clone());

    let (control_tx, control_rx) = watch::channel(Control::Run);
    let (event_tx, mut event_rx) = mpsc::unbounded_channel();
    let stopped = Arc::new(AtomicBool::new(false));
    let watcher = {
        let stop = ctx.paths.stop_file(&s.id);
        let stopped = stopped.clone();
        let progress = progress_path(&ctx.paths, &s.id);
        let (number, file) = (run.run, path.clone());
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(Duration::from_secs(1));
            let mut last = RunCounts::default();
            let mut since_write = 5;
            loop {
                tokio::select! {
                    _ = ticker.tick() => {
                        if stop.exists() && !stopped.swap(true, Ordering::SeqCst) {
                            let _ = control_tx.send(Control::Stop);
                        }
                        since_write += 1;
                        if since_write >= 5 {
                            since_write = 0;
                            let p = Progress { run: number, counts: last.clone(), file: file.clone() };
                            let _ = fs::write(&progress, serde_json::to_vec(&p).expect("progress serializes"));
                        }
                    }
                    e = event_rx.recv() => match e {
                        Some(CrawlEvent::Progress(p)) => {
                            last = RunCounts { discovered: p.crawled + p.queued, crawled: p.crawled, not_crawled: 0, failed: 0 };
                        }
                        Some(_) => {}
                        None => break,
                    }
                }
            }
        })
    };
    let result = crawler.run(control_rx, Some(event_tx)).await;
    let _ = watcher.await;
    match result {
        Ok(summary) => finish(run, &summary, stopped.load(Ordering::SeqCst)),
        Err(e) => fail(run, format!("The crawl stopped with an error: {e}")),
    }
}

fn finish(run: &mut Run, s: &CrawlSummary, stopped: bool) {
    let failed: u64 = s.failed_by_class.values().sum();
    run.counts = Some(RunCounts {
        discovered: s.discovered,
        crawled: s.crawled,
        not_crawled: s.not_crawled,
        failed,
    });
    if stopped {
        run.status = RunStatus::StoppedByUser;
        run.reason = Some("Stopped from SpiderLens.".into());
    } else if s.crawled == 0 {
        // The engine explains why the start URL went nowhere.
        let why = if s.warnings.is_empty() {
            "the start URL is excluded by the settings or blocked by robots.txt.".to_string()
        } else {
            s.warnings.join(" ")
        };
        fail(run, format!("Nothing was crawled: {why}"));
    } else if failed == s.crawled {
        let classes: Vec<&str> = s.failed_by_class.keys().map(String::as_str).collect();
        fail(
            run,
            format!("The site could not be reached ({}).", classes.join(", ")),
        );
    } else if s.url_limit_reached {
        run.status = RunStatus::StoppedAtLimit;
    } else if s.status == "completed" {
        run.status = RunStatus::Finished;
    } else {
        // A pause the run did not ask for (spec 006 disk reserve) or anything new.
        run.status = RunStatus::PausedDisk;
        run.reason = Some(format!("The crawl paused ({}). Open it to resume.", s.status));
    }
}

fn probe_dir(dir: &Path) -> std::io::Result<()> {
    let probe = dir.join(format!(".spiderlens-probe-{}", std::process::id()));
    File::create(&probe)?;
    fs::remove_file(&probe)
}

/// FR-020 to FR-023: in order, each independent of the others' outcome.
fn after_steps(s: &Schedule, run: &Run) -> Vec<StepResult> {
    let file = run.file.as_deref().expect("steps need a crawl file");
    s.after
        .iter()
        .map(|step| match step {
            AfterStep::Export { tabs } => export_step(file, tabs),
            AfterStep::Report { .. } | AfterStep::Compare => StepResult {
                kind: step.kind().into(),
                status: StepStatus::Skipped,
                outputs: vec![],
                reason: Some("Not available in this version of SpiderLens.".into()),
            },
        })
        .collect()
}

fn export_step(file: &Path, tabs: &[crate::model::ExportTab]) -> StepResult {
    let dir = exports_dir(file);
    let mut outputs = Vec::new();
    let mut problems = Vec::new();
    if let Err(e) = fs::create_dir_all(&dir) {
        problems.push(format!("{}: {e}", dir.display()));
    } else {
        let never = AtomicBool::new(false);
        for t in tabs {
            let dest = dir.join(format!("{}-{}.csv", t.tab, t.filter));
            let q = ViewQuery {
                tab: t.tab.clone(),
                filter: t.filter.clone(),
                scope: UrlScope::All,
                search: String::new(),
                sort: None,
            };
            match export_file(file, &q, &[], None, &dest, &never, &mut |_, _| {}) {
                Ok(_) => outputs.push(dest),
                Err(e) => problems.push(format!("{} › {}: {e}", t.tab, t.filter)),
            }
        }
    }
    StepResult {
        kind: "export".into(),
        status: if problems.is_empty() {
            StepStatus::Done
        } else {
            StepStatus::Failed
        },
        outputs,
        reason: (!problems.is_empty()).then(|| problems.join("; ")),
    }
}

fn notify(ctx: &Context, s: &Schedule, run: &Run) {
    if wanted(s.notify, run) {
        let (title, body) = text(&s.name, run);
        ctx.notifier.notify(&title, &body);
    }
}

/// Asks an active run to stop (FR-017).
pub fn request_stop(paths: &Paths, id: &str) -> std::io::Result<()> {
    let stop = paths.stop_file(id);
    fs::create_dir_all(stop.parent().expect("locks folder"))?;
    fs::write(stop, b"")
}
