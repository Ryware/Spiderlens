//! Scheduled crawls (spec 021).
//!
//! The operating system starts SpiderLens once an hour for each schedule
//! (and at login); this crate decides whether a run is due, runs the crawl,
//! records the history and keeps the OS registrations in shape. It has no
//! dependency on the desktop shell, so the command line can do everything.

pub mod cron;
pub mod due;
pub mod frequency;
pub mod history;
pub mod manage;
pub mod model;
pub mod naming;
pub mod notify;
pub mod os;
pub mod runner;
pub mod store;
pub mod validate;

pub use cron::{Cron, CronError};
pub use due::{Due, due};
pub use frequency::{Frequency, MonthDay, TimeOfDay};
pub use model::{
    AfterStep, ExportTab, NotifyLevel, Run, RunCounts, RunStatus, Schedule, ScheduleInput, StepResult,
    StepStatus, Trigger,
};
pub use store::{Paths, Store, StoreError};

/// The app's identifier, which names its configuration folder.
pub const APP_ID: &str = "com.ryware.spiderlens";

/// The configuration folder the desktop app uses (Tauri's `app_config_dir`),
/// or `SPIDERLENS_CONFIG_DIR` when set (tests).
pub fn default_config_dir() -> Option<std::path::PathBuf> {
    match std::env::var_os("SPIDERLENS_CONFIG_DIR") {
        Some(dir) => Some(dir.into()),
        None => dirs::config_dir().map(|d| d.join(APP_ID)),
    }
}

/// The local time zone, or `SPIDERLENS_TZ` when set (tests).
pub fn local_tz() -> jiff::tz::TimeZone {
    std::env::var("SPIDERLENS_TZ")
        .ok()
        .and_then(|name| jiff::tz::TimeZone::get(&name).ok())
        .unwrap_or_else(jiff::tz::TimeZone::system)
}

/// Now, or `SPIDERLENS_NOW` (an RFC 3339 time) when `SPIDERLENS_TEST=1`
/// is also set, so tests can run a tick at any moment.
pub fn now() -> jiff::Timestamp {
    if std::env::var_os("SPIDERLENS_TEST").is_some_and(|v| v == "1")
        && let Some(t) = std::env::var("SPIDERLENS_NOW").ok().and_then(|v| v.parse().ok())
    {
        return t;
    }
    jiff::Timestamp::now()
}
