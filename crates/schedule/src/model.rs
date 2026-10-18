//! Schedules and runs as stored on disk (data-model.md).

use std::path::PathBuf;

use jiff::Timestamp;
use serde::{Deserialize, Serialize};
use spiderlens_config::CrawlConfig;
use ts_rs::TS;

use crate::frequency::Frequency;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ScheduleInput {
    Spider {
        start_url: String,
    },
    /// The list file is read again at every run (FR-001).
    List {
        path: PathBuf,
    },
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum NotifyLevel {
    Always,
    #[default]
    Problems,
    Never,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ExportTab {
    pub tab: String,
    #[serde(default = "all")]
    pub filter: String,
}

fn all() -> String {
    "all".into()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(export)]
pub enum AfterStep {
    /// CSV exports of results tabs (spec 007), FR-020.
    Export { tabs: Vec<ExportTab> },
    /// End-of-crawl report (spec 010), FR-021. Offered once 010 ships.
    Report { formats: Vec<String> },
    /// Comparison with the previous run (spec 012), FR-022. Offered once 012 ships.
    Compare,
}

impl AfterStep {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Export { .. } => "export",
            Self::Report { .. } => "report",
            Self::Compare => "compare",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Schedule {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    #[ts(type = "import(\"./CrawlConfig\").CrawlConfig")]
    pub config: CrawlConfig,
    pub source_profile: Option<String>,
    /// Hash of the source profile when the configuration was copied (FR-003).
    pub source_hash: Option<String>,
    pub input: ScheduleInput,
    pub frequency: Frequency,
    pub output_dir: PathBuf,
    pub catch_up: bool,
    pub notify: NotifyLevel,
    pub after: Vec<AfterStep>,
    // How many finished runs of this schedule to keep (1 to 999); `None`
    // keeps them all. Old schedules saved without it keep everything.
    #[serde(default)]
    pub keep_last: Option<u32>,
    #[ts(type = "string")]
    pub created_at: Timestamp,
    #[ts(type = "string")]
    pub updated_at: Timestamp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum Trigger {
    Scheduled,
    CaughtUp,
    RunNow,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum RunStatus {
    Running,
    Finished,
    StoppedAtLimit,
    StoppedByUser,
    Failed,
    PausedDisk,
    Interrupted,
    Skipped,
    Missed,
}

impl RunStatus {
    /// Whether this run's crawl can be the baseline of a comparison (FR-022).
    pub fn is_complete(self) -> bool {
        matches!(self, Self::Finished | Self::StoppedAtLimit)
    }

    pub fn is_problem(self) -> bool {
        !matches!(
            self,
            Self::Running | Self::Finished | Self::StoppedAtLimit | Self::StoppedByUser
        )
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Running => "Running",
            Self::Finished => "Finished",
            Self::StoppedAtLimit => "Stopped at limit",
            Self::StoppedByUser => "Stopped by user",
            Self::Failed => "Failed",
            Self::PausedDisk => "Paused: disk",
            Self::Interrupted => "Interrupted",
            Self::Skipped => "Skipped",
            Self::Missed => "Missed",
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct RunCounts {
    #[ts(type = "number")]
    pub discovered: u64,
    #[ts(type = "number")]
    pub crawled: u64,
    #[ts(type = "number")]
    pub not_crawled: u64,
    #[ts(type = "number")]
    pub failed: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum StepStatus {
    Done,
    Skipped,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct StepResult {
    pub kind: String,
    pub status: StepStatus,
    pub outputs: Vec<PathBuf>,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Run {
    pub run: u32,
    /// The scheduled time; `None` for Run now.
    #[ts(type = "string | null")]
    pub slot: Option<Timestamp>,
    pub trigger: Trigger,
    pub status: RunStatus,
    pub reason: Option<String>,
    #[ts(type = "string | null")]
    pub started_at: Option<Timestamp>,
    #[ts(type = "string | null")]
    pub ended_at: Option<Timestamp>,
    pub counts: Option<RunCounts>,
    pub file: Option<PathBuf>,
    #[serde(default)]
    pub steps: Vec<StepResult>,
}

impl Run {
    pub fn new(run: u32, slot: Option<Timestamp>, trigger: Trigger, status: RunStatus) -> Self {
        Self {
            run,
            slot,
            trigger,
            status,
            reason: None,
            started_at: None,
            ended_at: None,
            counts: None,
            file: None,
            steps: Vec::new(),
        }
    }
}
