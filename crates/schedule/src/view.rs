//! What the app shows about schedules (contracts/desktop-commands.md).

use std::path::PathBuf;

use jiff::Timestamp;
use jiff::tz::TimeZone;
use serde::Serialize;
use ts_rs::TS;

use crate::frequency::Frequency;
use crate::manage::{Manager, Registered};
use crate::model::{Run, RunCounts, Schedule};
use crate::runner::{is_running, read_progress};

/// A time in the computer's time zone, with its offset: `2026-10-19T07:00:00+03:00`.
pub fn local_time(t: Timestamp, tz: &TimeZone) -> String {
    t.to_zoned(tz.clone()).strftime("%Y-%m-%dT%H:%M:%S%:z").to_string()
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct LiveRun {
    pub run: u32,
    pub counts: RunCounts,
    pub file: PathBuf,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct ScheduleRow {
    pub schedule: Schedule,
    pub describe: String,
    pub next_run: Option<String>,
    pub last: Option<Run>,
    pub registration: Registered,
    /// Set while a run is active (FR-017).
    pub running: Option<LiveRun>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct ScheduleDetail {
    pub row: ScheduleRow,
    pub next_runs: Vec<String>,
    /// The source profile changed since the configuration was copied (FR-003).
    pub profile_changed: bool,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct Preview {
    pub describe: String,
    pub next_runs: Vec<String>,
    pub error: Option<String>,
}

pub fn preview(f: &Frequency, tz: &TimeZone, now: Timestamp) -> Preview {
    match f.cron() {
        Ok(c) => Preview {
            describe: f.describe(),
            next_runs: c.upcoming(tz, now, 5).into_iter().map(|t| local_time(t, tz)).collect(),
            error: None,
        },
        Err(e) => Preview {
            describe: f.describe(),
            next_runs: Vec::new(),
            error: Some(e.to_string()),
        },
    }
}

pub fn row(m: &Manager, s: &Schedule, tz: &TimeZone, now: Timestamp) -> ScheduleRow {
    let paths = m.store.paths();
    let last = crate::runner::history_view(paths, &s.id)
        .ok()
        .and_then(|runs| runs.into_iter().next());
    let running = (is_running(paths, &s.id))
        .then(|| {
            read_progress(paths, &s.id).map(|p| LiveRun {
                run: p.run,
                counts: p.counts,
                file: p.file,
            })
        })
        .flatten();
    let next_run = match (s.enabled, s.frequency.cron()) {
        (true, Ok(c)) => c.next_after(tz, now).map(|t| local_time(t, tz)),
        _ => None,
    };
    ScheduleRow {
        schedule: s.clone(),
        describe: s.frequency.describe(),
        next_run,
        last,
        registration: m.state(s),
        running,
    }
}

pub fn detail(m: &Manager, s: &Schedule, tz: &TimeZone, now: Timestamp, profile_changed: bool) -> ScheduleDetail {
    let next_runs = if s.enabled {
        preview(&s.frequency, tz, now).next_runs
    } else {
        Vec::new()
    };
    ScheduleDetail {
        row: row(m, s, tz, now),
        next_runs,
        profile_changed,
    }
}

/// Newest first, at most `limit`.
pub fn runs(m: &Manager, id: &str, limit: usize) -> std::io::Result<Vec<Run>> {
    let mut all = crate::runner::history_view(m.store.paths(), id)?;
    all.truncate(limit);
    Ok(all)
}
