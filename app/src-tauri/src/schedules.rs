//! Commands for scheduled crawls (spec 021, contracts/desktop-commands.md),
//! and the headless entry points the system scheduler starts.
//!
//! Everything is a call into `spiderlens-schedule`; the command line has the
//! same operations (FR-028, FR-029).

use spiderlens_engine::crawl::file::DEFAULT_DISK_RESERVE;
use std::path::PathBuf;
use std::sync::Arc;

use jiff::Timestamp;
use spiderlens_config::{Issue, export_profile};
use spiderlens_schedule::manage::{ManageError, Manager, RepairReport};
use spiderlens_schedule::os::{self, Scheduler};
use spiderlens_schedule::runner::{self, Context, Launcher, TickAction};
use spiderlens_schedule::view::{self, Preview, ScheduleDetail, ScheduleRow};
use spiderlens_schedule::{
    BaselineError, Frequency, Paths, Run, Schedule, Store, Trigger, baseline_for, history, notify,
    store::profile_hash,
};

use crate::settings;

type Cmd<T> = Result<T, String>;

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

pub struct State {
    paths: Paths,
    os: Box<dyn Scheduler>,
}

impl State {
    pub fn new() -> Option<Self> {
        Some(Self {
            paths: Paths::new(spiderlens_schedule::default_config_dir()?),
            os: os::from_env(),
        })
    }

    fn manager(&self) -> Manager<'_> {
        Manager {
            store: Store::new(self.paths.clone()),
            os: self.os.as_ref(),
            launcher: launcher(),
        }
    }
}

/// The desktop executable starts its own ticks and runs: it ships in every
/// bundle, so no separate command-line binary has to be installed.
pub fn launcher() -> Launcher {
    Launcher {
        program: std::env::current_exe().unwrap_or_default(),
        tick_prefix: vec!["--schedule-tick".into()],
        run_prefix: vec!["--schedule-run".into()],
    }
}

/// Handles `--schedule-tick ID` and `--schedule-run ID [--slot T] --trigger X`
/// before any window exists. Returns the exit code, or `None` for a normal start.
pub fn headless(args: &[String]) -> Option<i32> {
    let flag = args.get(1)?.as_str();
    if flag != "--schedule-tick" && flag != "--schedule-run" {
        return None;
    }
    let Some(id) = args.get(2) else {
        return Some(2);
    };
    let Some(dir) = spiderlens_schedule::default_config_dir() else {
        return Some(2);
    };
    let ctx = Context {
        paths: Paths::new(dir),
        tz: spiderlens_schedule::local_tz(),
        notifier: Arc::from(notify::system()),
        resolve: Vec::new(),
        disk_reserve: DEFAULT_DISK_RESERVE,
    };
    let value = |name: &str| args.iter().position(|a| a == name).and_then(|i| args.get(i + 1));
    if flag == "--schedule-tick" {
        return Some(match runner::tick(&ctx, id, spiderlens_schedule::now()) {
            Ok(TickAction::Run { slot, trigger }) => {
                i32::from(launcher().spawn_run(id, Some(slot), trigger).is_err())
            }
            Ok(_) => 0,
            Err(_) => 1,
        });
    }
    let slot: Option<Timestamp> = value("--slot").and_then(|s| s.parse().ok());
    let trigger = value("--trigger")
        .and_then(|t| runner::parse_trigger(t))
        .unwrap_or(Trigger::Scheduled);
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .ok()?;
    Some(match rt.block_on(runner::run(&ctx, id, slot, trigger)) {
        Ok(outcome) => i32::from(runner::exit_code(&outcome)),
        Err(_) => 2,
    })
}

/// Every schedule that keeps only its last N runs, with its recorded runs,
/// for cleanup (`manage::run_cleanup`). Nothing when the schedules file
/// cannot be read: cleanup then leaves every file alone.
pub fn retention_sets() -> Vec<(Schedule, Vec<Run>)> {
    let Some(dir) = spiderlens_schedule::default_config_dir() else {
        return Vec::new();
    };
    let paths = Paths::new(dir);
    let Ok(all) = Store::new(paths.clone()).list() else {
        return Vec::new();
    };
    all.into_iter()
        .filter(|s| s.keep_last.is_some())
        .map(|s| {
            let runs = history::read(&paths.history(&s.id)).unwrap_or_default();
            (s, runs)
        })
        .collect()
}

fn now_tz() -> (Timestamp, jiff::tz::TimeZone) {
    (Timestamp::now(), spiderlens_schedule::local_tz())
}

fn invalid(e: ManageError) -> String {
    match e {
        ManageError::Invalid(issues) => issues
            .iter()
            .filter(|i| i.severity == spiderlens_config::Severity::Error)
            .map(|i| i.message.clone())
            .collect::<Vec<_>>()
            .join(" "),
        ManageError::Store(e) => e.to_string(),
    }
}

/// Current hash of a stored profile, in the same form `schedule_save` records.
fn current_profile_hash(profiles: &settings::State, name: &str) -> Option<String> {
    let p = profiles.load_profile(name)?;
    Some(profile_hash(&export_profile(
        &p.name,
        &p.config,
        p.start_url.as_deref(),
    )))
}

#[tauri::command]
pub fn schedules_list(state: tauri::State<State>) -> Cmd<Vec<ScheduleRow>> {
    let m = state.manager();
    let (now, tz) = now_tz();
    Ok(m.store
        .list()
        .map_err(err)?
        .iter()
        .map(|s| view::row(&m, s, &tz, now))
        .collect())
}

fn detail(state: &State, profiles: &settings::State, s: &Schedule) -> ScheduleDetail {
    let m = state.manager();
    let (now, tz) = now_tz();
    let changed = match (&s.source_profile, &s.source_hash) {
        (Some(name), Some(hash)) => current_profile_hash(profiles, name).is_some_and(|h| &h != hash),
        _ => false,
    };
    view::detail(&m, s, &tz, now, changed)
}

#[tauri::command]
pub fn schedule_get(
    state: tauri::State<State>,
    profiles: tauri::State<settings::State>,
    id: String,
) -> Cmd<ScheduleDetail> {
    let s = state.manager().store.get(&id).map_err(err)?;
    Ok(detail(&state, &profiles, &s))
}

#[tauri::command]
pub fn schedule_preview(frequency: Frequency) -> Preview {
    let (now, tz) = now_tz();
    view::preview(&frequency, &tz, now)
}

#[tauri::command]
pub fn schedule_validate(draft: Schedule) -> Vec<Issue> {
    let mut s = draft;
    spiderlens_schedule::validate::normalize(&mut s);
    spiderlens_schedule::validate::validate_schedule(&s)
}

#[derive(serde::Serialize)]
pub struct Saved {
    detail: ScheduleDetail,
    warnings: Vec<Issue>,
    /// Why the system scheduler refused the registration, if it did.
    refused: Option<String>,
}

#[tauri::command]
pub fn schedule_save(
    state: tauri::State<State>,
    profiles: tauri::State<settings::State>,
    mut draft: Schedule,
) -> Cmd<Saved> {
    if let (Some(name), None) = (&draft.source_profile, &draft.source_hash) {
        draft.source_hash = current_profile_hash(&profiles, name);
    }
    let (s, warnings, refused) = state.manager().save(draft).map_err(invalid)?;
    Ok(Saved {
        detail: detail(&state, &profiles, &s),
        warnings,
        refused,
    })
}

#[tauri::command]
pub fn schedule_set_enabled(
    state: tauri::State<State>,
    profiles: tauri::State<settings::State>,
    id: String,
    enabled: bool,
) -> Cmd<Saved> {
    let (s, refused) = state.manager().set_enabled(&id, enabled).map_err(invalid)?;
    Ok(Saved {
        detail: detail(&state, &profiles, &s),
        warnings: Vec::new(),
        refused,
    })
}

#[tauri::command]
pub fn schedule_duplicate(state: tauri::State<State>, id: String, name: String) -> Cmd<Schedule> {
    state.manager().duplicate(&id, &name).map_err(invalid)
}

#[tauri::command]
pub fn schedule_delete(state: tauri::State<State>, id: String) -> Cmd<()> {
    state.manager().delete(&id).map(|_| ()).map_err(invalid)
}

#[tauri::command]
pub fn schedule_run_now(state: tauri::State<State>, id: String) -> Cmd<()> {
    let s = state.manager().store.get(&id).map_err(err)?;
    launcher().spawn_run(&s.id, None, Trigger::RunNow).map_err(err)
}

#[tauri::command]
pub fn schedule_stop(state: tauri::State<State>, id: String) -> Cmd<()> {
    runner::request_stop(&state.paths, &id).map_err(err)
}

#[tauri::command]
pub fn schedule_history(state: tauri::State<State>, id: String, limit: usize) -> Cmd<Vec<Run>> {
    view::runs(&state.manager(), &id, limit).map_err(err)
}

/// The earlier run a scheduled crawl is compared with (spec 012 FR-002a).
#[derive(serde::Serialize)]
pub struct ScheduleBaseline {
    path: PathBuf,
    schedule: String,
    run: u32,
}

/// The previous finished run of the schedule `current` belongs to: `None`
/// when `current` is not a scheduled run, an error when it has no earlier
/// finished run.
#[tauri::command]
pub async fn compare_schedule_baseline(
    state: tauri::State<'_, State>,
    current: PathBuf,
) -> Cmd<Option<ScheduleBaseline>> {
    let paths = state.paths.clone();
    tokio::task::spawn_blocking(move || match baseline_for(&paths, &current) {
        Ok(b) => Ok(Some(ScheduleBaseline {
            path: b.path,
            schedule: b.schedule,
            run: b.run,
        })),
        Err(BaselineError::NotScheduled) => Ok(None),
        Err(e) => Err(e.to_string()),
    })
    .await
    .map_err(err)?
}

#[tauri::command]
pub fn schedule_update_from_profile(
    state: tauri::State<State>,
    profiles: tauri::State<settings::State>,
    id: String,
) -> Cmd<Saved> {
    let m = state.manager();
    let mut s = m.store.get(&id).map_err(err)?;
    let name = s
        .source_profile
        .clone()
        .ok_or("This schedule was not made from a profile.")?;
    let p = profiles
        .load_profile(&name)
        .ok_or_else(|| format!("The profile \"{name}\" no longer exists."))?;
    s.config = p.config.clone();
    s.source_hash = Some(profile_hash(&export_profile(
        &p.name,
        &p.config,
        p.start_url.as_deref(),
    )));
    let (s, warnings, refused) = m.save(s).map_err(invalid)?;
    Ok(Saved {
        detail: detail(&state, &profiles, &s),
        warnings,
        refused,
    })
}

/// Run at start-up by the UI (FR-025); also a button.
#[tauri::command]
pub fn schedules_repair(state: tauri::State<State>) -> Cmd<RepairReport> {
    state.manager().repair().map_err(invalid)
}

#[tauri::command]
pub fn schedules_remove_all(state: tauri::State<State>) -> Cmd<usize> {
    state.manager().remove_all().map_err(invalid)
}

#[tauri::command]
pub fn schedule_default_output() -> Option<PathBuf> {
    // Documents/SpiderLens schedules, a place the user can find.
    Some(
        std::env::var_os("HOME")
            .or_else(|| std::env::var_os("USERPROFILE"))
            .map(PathBuf::from)?
            .join("Documents")
            .join("SpiderLens schedules"),
    )
}
