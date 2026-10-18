//! The previous run of a schedule as a comparison baseline (spec 012
//! FR-002a, spec 021 FR-022).
//!
//! The crawl file stores which schedule and run it belongs to; the history
//! of that schedule says which earlier run finished and still has its file.

use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::history;
use crate::store::Paths;

/// The schedule run a crawl file records about itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScheduleRun {
    pub id: String,
    pub name: String,
    pub run: u32,
}

/// The baseline for a scheduled run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Baseline {
    pub path: PathBuf,
    /// The schedule's name.
    pub schedule: String,
    pub run: u32,
}

#[derive(Debug, thiserror::Error)]
pub enum BaselineError {
    #[error("This crawl is not a run of a schedule, so it has no previous run to compare with.")]
    NotScheduled,
    #[error("No earlier finished run of the schedule “{0}” is available to compare with.")]
    NoneFound(String),
    #[error("The run history of the schedule “{name}” cannot be read: {source}")]
    History {
        name: String,
        #[source]
        source: std::io::Error,
    },
}

#[derive(Deserialize)]
struct Stored {
    id: String,
    name: String,
    run: u32,
}

/// The schedule run stored in a crawl file's `schedule` info, or `None` for
/// a crawl that was not started by a schedule (or cannot be read). The file
/// is opened read-only.
pub fn schedule_of(crawl: &Path) -> Option<ScheduleRun> {
    let (conn, _) = spiderlens_results::connect_attachable(crawl).ok()?;
    let value: String = conn
        .query_row("SELECT value FROM meta WHERE key = 'schedule'", [], |r| r.get(0))
        .ok()?;
    let s: Stored = serde_json::from_str(&value).ok()?;
    Some(ScheduleRun {
        id: s.id,
        name: s.name,
        run: s.run,
    })
}

/// The crawl file of the latest run of schedule `id` below run `before`
/// that finished (or stopped at its limit) and whose file still exists.
pub fn previous_complete_run(paths: &Paths, id: &str, before: u32) -> std::io::Result<Option<PathBuf>> {
    Ok(previous_run(paths, id, before)?.map(|(_, file)| file))
}

/// `previous_complete_run` with the run's number.
fn previous_run(paths: &Paths, id: &str, before: u32) -> std::io::Result<Option<(u32, PathBuf)>> {
    // `read` returns the newest run first.
    Ok(history::read(&paths.history(id))?
        .into_iter()
        .filter(|r| r.run < before && r.status.is_complete())
        .find_map(|r| r.file.filter(|f| f.exists()).map(|f| (r.run, f))))
}

/// The baseline for `current`, which must be a run of a schedule.
pub fn baseline_for(paths: &Paths, current: &Path) -> Result<Baseline, BaselineError> {
    let of = schedule_of(current).ok_or(BaselineError::NotScheduled)?;
    match previous_run(paths, &of.id, of.run) {
        Ok(Some((run, path))) => Ok(Baseline {
            path,
            schedule: of.name,
            run,
        }),
        Ok(None) => Err(BaselineError::NoneFound(of.name)),
        Err(source) => Err(BaselineError::History {
            name: of.name,
            source,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Run, RunStatus, Trigger};

    fn record(paths: &Paths, id: &str, n: u32, status: RunStatus, file: Option<PathBuf>) {
        let mut run = Run::new(n, None, Trigger::RunNow, status);
        run.file = file;
        history::append(&paths.history(id), &run).unwrap();
    }

    #[test]
    fn picks_the_latest_complete_run_with_a_file() {
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths::new(dir.path());
        let file = |n: u32| dir.path().join(format!("run{n}.spiderlens"));
        for n in [1, 2, 4] {
            std::fs::write(file(n), b"").unwrap();
        }
        record(&paths, "s", 1, RunStatus::Finished, Some(file(1)));
        record(&paths, "s", 2, RunStatus::StoppedAtLimit, Some(file(2)));
        // Failed runs, runs without a file and deleted files are passed over.
        record(&paths, "s", 3, RunStatus::Failed, Some(file(3)));
        record(&paths, "s", 4, RunStatus::Finished, Some(file(4)));
        record(&paths, "s", 5, RunStatus::Finished, None);
        record(&paths, "s", 6, RunStatus::Finished, Some(file(6)));
        record(&paths, "s", 7, RunStatus::Running, None);

        let got = |before| previous_run(&paths, "s", before).unwrap();
        assert_eq!(previous_complete_run(&paths, "s", 7).unwrap(), Some(file(4)));
        assert_eq!(got(7), Some((4, file(4))));
        assert_eq!(got(4), Some((2, file(2))));
        assert_eq!(got(2), Some((1, file(1))));
        assert_eq!(got(1), None);
        assert_eq!(previous_complete_run(&paths, "other", 9).unwrap(), None);
    }

    #[test]
    fn a_file_that_is_not_a_crawl_is_not_scheduled() {
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths::new(dir.path());
        let missing = dir.path().join("nothing.spiderlens");
        assert!(matches!(
            baseline_for(&paths, &missing),
            Err(BaselineError::NotScheduled)
        ));
    }
}
