//! `spiderlens schedule ...` (spec 021, contracts/cli.md).

use spiderlens_engine::crawl::file::DEFAULT_DISK_RESERVE;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use clap::{ArgMatches, Args, Subcommand, ValueEnum, parser::ValueSource};
use jiff::Timestamp;
use spiderlens_config::{Mode, Severity};
use spiderlens_schedule::manage::{ManageError, Manager, Registered};
use spiderlens_schedule::runner::{self, Context, Launcher, Outcome, TickAction};
use spiderlens_schedule::{
    AfterStep, DEFAULT_KEEP_LAST, ExportTab, Frequency, MonthDay, NotifyLevel, Paths, Run, Schedule,
    ScheduleInput, Store, StoreError, TimeOfDay, Trigger, history, notify, os,
};

use crate::options::CrawlArgs;
use crate::{exit, print_issues, print_json};

#[derive(Subcommand)]
pub enum ScheduleCommand {
    /// List schedules with their next run and last result.
    List(JsonArg),
    /// Show one schedule and its next five runs.
    Show(KeyArg),
    /// Create a schedule and register it with the system scheduler.
    Add(Box<SpecArgs>),
    /// Change a schedule; only the options given change.
    Edit(Box<EditArgs>),
    /// Copy a schedule under a new name (the copy starts disabled).
    Duplicate {
        schedule: String,
        #[arg(long)]
        name: String,
        #[arg(long)]
        json: bool,
    },
    /// Turn a schedule on.
    Enable(KeyArg),
    /// Turn a schedule off (its registration is removed).
    Disable(KeyArg),
    /// Delete a schedule. Its crawl files are kept.
    Remove(KeyArg),
    /// Run a schedule now, in the foreground.
    RunNow(KeyArg),
    /// Past runs, newest first.
    History {
        schedule: String,
        #[arg(long, default_value_t = 20)]
        limit: usize,
        #[arg(long)]
        json: bool,
    },
    /// Remove every schedule and every SpiderLens registration.
    RemoveAll {
        /// Do not ask.
        #[arg(long)]
        yes: bool,
    },
    /// Re-register schedules whose registration is missing or outdated.
    Repair(JsonArg),
    /// What the system scheduler runs every hour.
    #[command(hide = true)]
    Tick { id: String },
    /// A run started by a tick or by Run now.
    #[command(hide = true)]
    Run {
        id: String,
        #[arg(long)]
        slot: Option<Timestamp>,
        #[arg(long, default_value = "scheduled")]
        trigger: String,
    },
}

#[derive(Args)]
pub struct JsonArg {
    #[arg(long)]
    json: bool,
}

#[derive(Args)]
pub struct KeyArg {
    /// Schedule id or name.
    schedule: String,
    #[arg(long)]
    json: bool,
}

#[derive(Clone, Copy, ValueEnum)]
pub enum NotifyArg {
    Always,
    Problems,
    Never,
}

#[derive(Args)]
pub struct SpecArgs {
    #[arg(long)]
    name: Option<String>,
    /// Start URL (Spider mode).
    url: Option<String>,
    /// URL list file, read again at every run (List mode).
    #[arg(long, value_name = "FILE", conflicts_with = "url")]
    list: Option<PathBuf>,
    /// Folder for the crawl files.
    #[arg(long, value_name = "DIR")]
    out: Option<PathBuf>,
    /// Every N hours (1 to 24), at --minute.
    #[arg(long, value_name = "N", group = "freq", help_heading = "When")]
    hourly: Option<u8>,
    /// Minute past the hour for --hourly [default: 0].
    #[arg(long, value_name = "M", requires = "hourly", help_heading = "When")]
    minute: Option<u8>,
    /// Every day at HH:MM.
    #[arg(long, value_name = "HH:MM", group = "freq", help_heading = "When")]
    daily: Option<String>,
    /// On these days at a time, for example mon,thu@07:00.
    #[arg(long, value_name = "DAYS@HH:MM", group = "freq", help_heading = "When")]
    weekly: Option<String>,
    /// On a day of the month (1 to 28, or last) at a time, for example last@06:00.
    #[arg(long, value_name = "DAY@HH:MM", group = "freq", help_heading = "When")]
    monthly: Option<String>,
    /// Five-field cron expression; the minute must be one number.
    #[arg(long, value_name = "EXPR", group = "freq", help_heading = "When")]
    cron: Option<String>,
    /// Record a missed time as Missed instead of running it at the next login.
    #[arg(long, help_heading = "When")]
    no_catch_up: bool,
    /// Catch up a missed time once at the next login (default).
    #[arg(long, conflicts_with = "no_catch_up", help_heading = "When")]
    catch_up: bool,
    /// When to show a notification [default: problems].
    #[arg(long, value_enum, help_heading = "After each run")]
    notify: Option<NotifyArg>,
    /// Export a results tab as CSV after each run: TAB or TAB:FILTER. Repeatable.
    #[arg(long, value_name = "TAB[:FILTER]", help_heading = "After each run")]
    export: Vec<String>,
    /// Remove all after-run exports.
    #[arg(long, help_heading = "After each run")]
    no_export: bool,
    /// Keep the newest N runs' files (1 to 999) and move older ones to the Trash, or "all"
    /// [default: 10].
    #[arg(long, value_name = "N|all", help_heading = "Old runs")]
    keep_last: Option<String>,
    /// Save without turning it on.
    #[arg(long)]
    disabled: bool,
    #[arg(long)]
    json: bool,
    #[command(flatten)]
    crawl: CrawlArgs,
}

#[derive(Args)]
pub struct EditArgs {
    /// Schedule id or name.
    schedule: String,
    #[command(flatten)]
    spec: SpecArgs,
}

fn config_paths() -> Result<Paths, ExitCode> {
    match spiderlens_schedule::default_config_dir() {
        Some(dir) => Ok(Paths::new(dir)),
        None => {
            eprintln!("error: no configuration folder on this system");
            Err(ExitCode::from(exit::USAGE))
        }
    }
}

pub fn launcher() -> Launcher {
    Launcher {
        program: std::env::current_exe().unwrap_or_else(|_| PathBuf::from("spiderlens")),
        tick_prefix: vec!["schedule".into(), "tick".into()],
        run_prefix: vec!["schedule".into(), "run".into()],
    }
}

fn manager<'a>(paths: &Paths, scheduler: &'a dyn os::Scheduler) -> Manager<'a> {
    Manager {
        store: Store::new(paths.clone()),
        os: scheduler,
        launcher: launcher(),
    }
}

fn store_error(e: &StoreError) -> ExitCode {
    eprintln!("error: {e}");
    ExitCode::from(exit::USAGE)
}

fn manage_error(e: ManageError) -> ExitCode {
    match e {
        ManageError::Invalid(issues) => {
            print_issues(&issues);
        }
        ManageError::Store(e) => eprintln!("error: {e}"),
    }
    ExitCode::from(exit::USAGE)
}

fn context(paths: Paths) -> Context {
    Context {
        paths,
        tz: spiderlens_schedule::local_tz(),
        notifier: Arc::from(notify::system()),
        resolve: Vec::new(),
        disk_reserve: DEFAULT_DISK_RESERVE,
    }
}

fn time_text(t: Timestamp) -> String {
    t.to_zoned(spiderlens_schedule::local_tz())
        .strftime("%Y-%m-%d %H:%M %Z")
        .to_string()
}

fn next_runs(s: &Schedule, n: usize) -> Vec<Timestamp> {
    match (s.enabled, s.frequency.cron()) {
        (true, Ok(c)) => c.upcoming(&spiderlens_schedule::local_tz(), spiderlens_schedule::now(), n),
        _ => Vec::new(),
    }
}

fn registered_text(r: &Registered) -> String {
    match r {
        Registered::Registered { .. } => "registered".into(),
        Registered::Suspended => "off".into(),
        Registered::NotRegistered { reason } => format!("not registered: {reason}"),
    }
}

pub fn run(cmd: ScheduleCommand, matches: &ArgMatches) -> ExitCode {
    let paths = match config_paths() {
        Ok(p) => p,
        Err(code) => return code,
    };
    let scheduler = os::from_env();
    let m = manager(&paths, scheduler.as_ref());
    match cmd {
        ScheduleCommand::List(JsonArg { json }) => list(&m, &paths, json),
        ScheduleCommand::Show(KeyArg { schedule, json }) => match m.store.get(&schedule) {
            Ok(s) => {
                show(&m, &s, json);
                ExitCode::from(exit::FETCHED)
            }
            Err(e) => store_error(&e),
        },
        ScheduleCommand::Add(spec) => {
            let sub = sub_matches(matches, "add");
            add(&m, *spec, sub)
        }
        ScheduleCommand::Edit(args) => {
            let sub = sub_matches(matches, "edit");
            edit(&m, *args, sub)
        }
        ScheduleCommand::Duplicate { schedule, name, json } => match m.duplicate(&schedule, &name) {
            Ok(s) => {
                show(&m, &s, json);
                ExitCode::from(exit::FETCHED)
            }
            Err(e) => manage_error(e),
        },
        ScheduleCommand::Enable(KeyArg { schedule, json }) => set_enabled(&m, &schedule, true, json),
        ScheduleCommand::Disable(KeyArg { schedule, json }) => set_enabled(&m, &schedule, false, json),
        ScheduleCommand::Remove(KeyArg { schedule, .. }) => match m.delete(&schedule) {
            Ok(s) => {
                eprintln!(
                    "removed \"{}\"; its crawl files in {} are kept",
                    s.name,
                    s.output_dir.display()
                );
                ExitCode::from(exit::FETCHED)
            }
            Err(e) => manage_error(e),
        },
        ScheduleCommand::RunNow(KeyArg { schedule, json }) => match m.store.get(&schedule) {
            Ok(s) => run_blocking(paths, &s.id, None, Trigger::RunNow, json, true),
            Err(e) => store_error(&e),
        },
        ScheduleCommand::History {
            schedule,
            limit,
            json,
        } => match m.store.get(&schedule) {
            Ok(s) => match runner::history_view(&paths, &s.id) {
                Ok(mut runs) => {
                    runs.truncate(limit);
                    if json {
                        print_json(&runs);
                    } else {
                        for r in &runs {
                            println!("{}", run_line(r));
                        }
                    }
                    ExitCode::from(exit::FETCHED)
                }
                Err(e) => {
                    eprintln!("error: {e}");
                    ExitCode::FAILURE
                }
            },
            Err(e) => store_error(&e),
        },
        ScheduleCommand::RemoveAll { yes } => {
            if !yes {
                eprintln!("error: this removes every schedule and registration; add --yes to confirm");
                return ExitCode::from(exit::USAGE);
            }
            match m.remove_all() {
                Ok(n) => {
                    eprintln!("removed {n} schedules and every SpiderLens registration");
                    ExitCode::from(exit::FETCHED)
                }
                Err(e) => manage_error(e),
            }
        }
        ScheduleCommand::Repair(JsonArg { json }) => match m.repair() {
            Ok(report) => {
                if json {
                    print_json(&report);
                } else {
                    for n in &report.repaired {
                        println!("repaired  {n}");
                    }
                    for (n, why) in &report.failed {
                        println!("failed    {n}: {why}");
                    }
                    for id in &report.orphans {
                        println!("removed   registration {id} (no schedule)");
                    }
                }
                ExitCode::from(if report.failed.is_empty() {
                    exit::FETCHED
                } else {
                    exit::FAILED
                })
            }
            Err(e) => manage_error(e),
        },
        ScheduleCommand::Tick { id } => {
            let ctx = context(paths);
            match runner::tick(&ctx, &id, spiderlens_schedule::now()) {
                Ok(TickAction::Run { slot, trigger }) => match launcher().spawn_run(&id, Some(slot), trigger)
                {
                    Ok(()) => ExitCode::from(exit::FETCHED),
                    Err(e) => {
                        eprintln!("error: could not start the run: {e}");
                        ExitCode::FAILURE
                    }
                },
                Ok(_) => ExitCode::from(exit::FETCHED),
                Err(e) => {
                    eprintln!("error: {e}");
                    ExitCode::FAILURE
                }
            }
        }
        ScheduleCommand::Run { id, slot, trigger } => {
            let Some(trigger) = runner::parse_trigger(&trigger) else {
                eprintln!("error: unknown trigger \"{trigger}\"");
                return ExitCode::from(exit::USAGE);
            };
            run_blocking(paths, &id, slot, trigger, false, false)
        }
    }
}

fn sub_matches<'a>(matches: &'a ArgMatches, name: &str) -> &'a ArgMatches {
    matches
        .subcommand_matches("schedule")
        .and_then(|m| m.subcommand_matches(name))
        .expect("subcommand matches")
}

fn run_blocking(
    paths: Paths,
    id: &str,
    slot: Option<Timestamp>,
    trigger: Trigger,
    json: bool,
    loud: bool,
) -> ExitCode {
    let rt = match tokio::runtime::Builder::new_multi_thread().enable_all().build() {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("error: could not start: {e}");
            return ExitCode::FAILURE;
        }
    };
    let ctx = context(paths.clone());
    if loud {
        // Ctrl-C asks the run to stop, the same way the app's Stop does.
        let (paths, id) = (paths.clone(), id.to_string());
        rt.spawn(async move {
            if tokio::signal::ctrl_c().await.is_ok() {
                eprintln!("\nstopping...");
                let _ = runner::request_stop(&paths, &id);
            }
        });
        eprintln!("running...");
    }
    match rt.block_on(runner::run(&ctx, id, slot, trigger)) {
        Ok(outcome) => {
            if let Outcome::Done(r) = &outcome {
                if json {
                    print_json(r);
                } else if loud {
                    println!("{}", run_line(r));
                    if let Some(f) = &r.file {
                        println!("File        {}", f.display());
                    }
                    for s in &r.steps {
                        for o in &s.outputs {
                            println!("Output      {}", o.display());
                        }
                    }
                }
            }
            ExitCode::from(runner::exit_code(&outcome))
        }
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::from(exit::USAGE)
        }
    }
}

fn run_line(r: &Run) -> String {
    let when = r.slot.or(r.started_at).map(time_text).unwrap_or_default();
    let counts = r
        .counts
        .as_ref()
        .map(|c| format!("  {} crawled, {} failed", c.crawled, c.failed))
        .unwrap_or_default();
    let reason = r.reason.as_deref().map(|x| format!("  {x}")).unwrap_or_default();
    format!(
        "#{:<4} {when:<26} {:<16}{counts}{reason}",
        r.run,
        r.status.label()
    )
}

fn list(m: &Manager, paths: &Paths, json: bool) -> ExitCode {
    let all = match m.store.list() {
        Ok(a) => a,
        Err(e) => return store_error(&e),
    };
    let rows: Vec<serde_json::Value> = all
        .iter()
        .map(|s| {
            let last = history::read(&paths.history(&s.id))
                .ok()
                .and_then(|r| r.into_iter().next());
            serde_json::json!({
                "id": s.id,
                "name": s.name,
                "enabled": s.enabled,
                "frequency": s.frequency.describe(),
                "next_run": next_runs(s, 1).first(),
                "last": last,
                "registration": m.state(s),
            })
        })
        .collect();
    if json {
        print_json(&rows);
    } else {
        for (s, row) in all.iter().zip(&rows) {
            let next = next_runs(s, 1)
                .first()
                .map(|t| time_text(*t))
                .unwrap_or_else(|| "-".into());
            let last = row["last"]["status"].as_str().unwrap_or("-");
            println!(
                "{}  {:<24} {:<34} next {next:<24} last {last:<16} {}",
                s.id,
                s.name,
                s.frequency.describe(),
                registered_text(&m.state(s))
            );
        }
    }
    ExitCode::from(exit::FETCHED)
}

fn show(m: &Manager, s: &Schedule, json: bool) {
    let next: Vec<Timestamp> = next_runs(s, 5);
    if json {
        let mut v = serde_json::to_value(s).expect("schedule serializes");
        v["next_runs"] = serde_json::to_value(&next).expect("times serialize");
        v["registration"] = serde_json::to_value(m.state(s)).expect("state serializes");
        print_json(&v);
        return;
    }
    let input = match &s.input {
        ScheduleInput::Spider { start_url } => start_url.clone(),
        ScheduleInput::List { path } => format!("list {}", path.display()),
    };
    println!("{:<12} {}", "Name", s.name);
    println!("{:<12} {}", "Id", s.id);
    println!("{:<12} {input}", "Crawl");
    println!("{:<12} {}", "When", s.frequency.describe());
    println!("{:<12} {}", "Output", s.output_dir.display());
    println!("{:<12} {}", "Catch up", if s.catch_up { "yes" } else { "no" });
    let keeps = s.keep_last.map_or("all runs".to_string(), |n| format!("last {n} runs"));
    println!("{:<12} {keeps}", "Keeps");
    println!("{:<12} {}", "System", registered_text(&m.state(s)));
    for step in &s.after {
        if let AfterStep::Export { tabs } = step {
            let list: Vec<String> = tabs.iter().map(|t| format!("{}:{}", t.tab, t.filter)).collect();
            println!("{:<12} {}", "Export", list.join(", "));
        }
    }
    for (i, t) in next.iter().enumerate() {
        println!("{:<12} {}", if i == 0 { "Next runs" } else { "" }, time_text(*t));
    }
}

fn set_enabled(m: &Manager, key: &str, on: bool, json: bool) -> ExitCode {
    match m.set_enabled(key, on) {
        Ok((s, refused)) => {
            if let Some(why) = refused {
                eprintln!("warning: saved, but the system scheduler refused it: {why}");
            }
            show(m, &s, json);
            ExitCode::from(exit::FETCHED)
        }
        Err(e) => manage_error(e),
    }
}

fn day_number(name: &str) -> Option<u8> {
    const DAYS: [&str; 7] = ["sun", "mon", "tue", "wed", "thu", "fri", "sat"];
    let n = name.trim().to_ascii_lowercase();
    DAYS.iter()
        .position(|d| n.starts_with(d))
        .map(|i| i as u8)
        .or_else(|| n.parse().ok().filter(|d| *d < 7))
}

fn frequency(a: &SpecArgs) -> Result<Option<Frequency>, String> {
    let time = |t: &str| TimeOfDay::parse(t).ok_or_else(|| format!("\"{t}\" is not a time like 07:00"));
    if let Some(every) = a.hourly {
        return Ok(Some(Frequency::Hours {
            every,
            minute: a.minute.unwrap_or(0),
        }));
    }
    if let Some(t) = &a.daily {
        return Ok(Some(Frequency::Daily { time: time(t)? }));
    }
    if let Some(w) = &a.weekly {
        let (days, t) = w
            .split_once('@')
            .ok_or("use --weekly DAYS@HH:MM, for example mon,thu@07:00")?;
        let days = days
            .split(',')
            .map(|d| day_number(d).ok_or_else(|| format!("\"{d}\" is not a day")))
            .collect::<Result<Vec<_>, _>>()?;
        return Ok(Some(Frequency::Weekly { days, time: time(t)? }));
    }
    if let Some(mo) = &a.monthly {
        let (day, t) = mo
            .split_once('@')
            .ok_or("use --monthly DAY@HH:MM, for example 1@07:00 or last@07:00")?;
        let day = if day.eq_ignore_ascii_case("last") {
            MonthDay::Last
        } else {
            MonthDay::Day(
                day.parse()
                    .map_err(|_| format!("\"{day}\" is not a day of the month"))?,
            )
        };
        return Ok(Some(Frequency::Monthly { day, time: time(t)? }));
    }
    Ok(a.cron.as_ref().map(|c| Frequency::Advanced { cron: c.clone() }))
}

/// `--keep-last`: a number of runs, or `all` for no limit. The range is
/// checked when the schedule is saved.
fn keep_last(text: &str) -> Result<Option<u32>, String> {
    let t = text.trim();
    if t.eq_ignore_ascii_case("all") {
        return Ok(None);
    }
    t.parse()
        .map(Some)
        .map_err(|_| format!("\"{t}\" is not a number of runs (1 to 999) or \"all\""))
}

fn exports(list: &[String]) -> Vec<ExportTab> {
    list.iter()
        .map(|e| match e.split_once(':') {
            Some((tab, filter)) => ExportTab {
                tab: tab.into(),
                filter: filter.into(),
            },
            None => ExportTab {
                tab: e.clone(),
                filter: "all".into(),
            },
        })
        .collect()
}

/// Whether any crawl option (or --profile) was typed for this command.
fn crawl_options_given(sub: &ArgMatches) -> bool {
    const OWN: &[&str] = &[
        "schedule",
        "name",
        "url",
        "list",
        "out",
        "hourly",
        "minute",
        "daily",
        "weekly",
        "monthly",
        "cron",
        "no_catch_up",
        "catch_up",
        "notify",
        "export",
        "no_export",
        "keep_last",
        "disabled",
        "json",
        "freq",
    ];
    sub.ids()
        .filter(|id| !OWN.contains(&id.as_str()))
        .any(|id| sub.value_source(id.as_str()) == Some(ValueSource::CommandLine))
}

/// Applies the options that were given to `s`.
fn apply(s: &mut Schedule, a: &SpecArgs, sub: &ArgMatches, new: bool) -> Result<(), String> {
    if let Some(name) = &a.name {
        s.name = name.clone();
    }
    if let Some(url) = &a.url {
        s.input = ScheduleInput::Spider {
            start_url: url.clone(),
        };
    }
    if let Some(list) = &a.list {
        let path = std::path::absolute(list).map_err(|e| e.to_string())?;
        s.input = ScheduleInput::List { path };
    }
    if let Some(out) = &a.out {
        s.output_dir = std::path::absolute(out).map_err(|e| e.to_string())?;
    }
    if let Some(f) = frequency(a)? {
        s.frequency = f;
    }
    if a.no_catch_up {
        s.catch_up = false;
    }
    if a.catch_up {
        s.catch_up = true;
    }
    if let Some(n) = a.notify {
        s.notify = match n {
            NotifyArg::Always => NotifyLevel::Always,
            NotifyArg::Problems => NotifyLevel::Problems,
            NotifyArg::Never => NotifyLevel::Never,
        };
    }
    if a.no_export || !a.export.is_empty() {
        s.after.retain(|step| !matches!(step, AfterStep::Export { .. }));
    }
    if !a.export.is_empty() {
        s.after.insert(
            0,
            AfterStep::Export {
                tabs: exports(&a.export),
            },
        );
    }
    if let Some(keep) = &a.keep_last {
        s.keep_last = keep_last(keep)?;
    }
    if a.disabled {
        s.enabled = false;
    }
    if new || crawl_options_given(sub) {
        let mode = match s.input {
            ScheduleInput::Spider { .. } => Mode::Spider,
            ScheduleInput::List { .. } => Mode::List,
        };
        let resolved = a.crawl.resolve(mode)?;
        for n in &resolved.notices {
            eprintln!("notice: {n}");
        }
        s.config = resolved.config;
        s.source_profile = a.crawl.profile.as_ref().map(|p| p.display().to_string());
        s.source_hash = a
            .crawl
            .profile
            .as_ref()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .map(|t| spiderlens_schedule::store::profile_hash(&t));
    }
    Ok(())
}

fn add(m: &Manager, a: SpecArgs, sub: &ArgMatches) -> ExitCode {
    let mut missing = Vec::new();
    if a.name.is_none() {
        missing.push("--name");
    }
    if a.url.is_none() && a.list.is_none() {
        missing.push("a start URL or --list FILE");
    }
    if a.out.is_none() {
        missing.push("--out DIR");
    }
    if a.hourly.is_none()
        && a.daily.is_none()
        && a.weekly.is_none()
        && a.monthly.is_none()
        && a.cron.is_none()
    {
        missing.push("one of --hourly, --daily, --weekly, --monthly, --cron");
    }
    if !missing.is_empty() {
        eprintln!("error: a schedule needs {}", missing.join(", "));
        return ExitCode::from(exit::USAGE);
    }
    let now = Timestamp::now();
    let mut s = Schedule {
        id: String::new(),
        name: String::new(),
        enabled: true,
        config: Default::default(),
        source_profile: None,
        source_hash: None,
        input: ScheduleInput::Spider {
            start_url: String::new(),
        },
        frequency: Frequency::Daily {
            time: TimeOfDay { hour: 7, minute: 0 },
        },
        output_dir: PathBuf::new(),
        catch_up: true,
        notify: NotifyLevel::Problems,
        after: Vec::new(),
        keep_last: Some(DEFAULT_KEEP_LAST),
        created_at: now,
        updated_at: now,
    };
    if let Err(e) = apply(&mut s, &a, sub, true) {
        eprintln!("error: {e}");
        return ExitCode::from(exit::USAGE);
    }
    save(m, s, a.json)
}

fn edit(m: &Manager, a: EditArgs, sub: &ArgMatches) -> ExitCode {
    let mut s = match m.store.get(&a.schedule) {
        Ok(s) => s,
        Err(e) => return store_error(&e),
    };
    if let Err(e) = apply(&mut s, &a.spec, sub, false) {
        eprintln!("error: {e}");
        return ExitCode::from(exit::USAGE);
    }
    save(m, s, a.spec.json)
}

fn save(m: &Manager, s: Schedule, json: bool) -> ExitCode {
    match m.save(s) {
        Ok((s, warnings, refused)) => {
            for w in warnings.iter().filter(|w| w.severity == Severity::Warning) {
                eprintln!("warning: {}: {}", w.path, w.message);
            }
            if let Some(why) = &refused {
                eprintln!("warning: saved, but the system scheduler refused it: {why}");
            }
            show(m, &s, json);
            ExitCode::from(exit::FETCHED)
        }
        Err(e) => manage_error(e),
    }
}
