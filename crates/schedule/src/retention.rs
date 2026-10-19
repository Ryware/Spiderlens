//! Which old run files of a schedule to remove (keep the last N runs).
//!
//! This only decides; removing is the desktop app's job, so the files can go
//! to the Trash. A run's file counts only while it still sits in the
//! schedule's folder: one the user moved or saved elsewhere is theirs.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::model::{Run, RunStatus};

/// The run files beyond the newest `keep_last`, newest first.
///
/// Only finished runs count, and only their files that are inside
/// `output_dir` and still exist (`exists`). `None` keeps everything.
pub fn plan(
    runs: &[Run],
    keep_last: Option<u32>,
    output_dir: &Path,
    exists: &dyn Fn(&Path) -> bool,
) -> Vec<PathBuf> {
    let Some(keep) = keep_last.filter(|n| *n >= 1) else {
        return Vec::new();
    };
    let mut files: Vec<(u32, &Path)> = runs
        .iter()
        .filter(|r| r.status != RunStatus::Running && r.ended_at.is_some())
        .filter_map(|r| r.file.as_deref().map(|f| (r.run, f)))
        .filter(|(_, f)| f.starts_with(output_dir) && exists(f))
        .collect();
    files.sort_by_key(|(run, _)| std::cmp::Reverse(*run));
    let mut seen: HashSet<&Path> = HashSet::new();
    files.retain(|(_, f)| seen.insert(*f));
    files
        .into_iter()
        .skip(keep as usize)
        .map(|(_, f)| f.to_path_buf())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Trigger;
    use jiff::Timestamp;

    const DIR: &str = "/out";

    fn file(n: u32) -> PathBuf {
        PathBuf::from(format!("/out/r{n}.spiderlens"))
    }

    fn files(list: &[u32]) -> Vec<PathBuf> {
        list.iter().map(|n| file(*n)).collect()
    }

    fn finished(n: u32) -> Run {
        let mut r = Run::new(n, None, Trigger::RunNow, RunStatus::Finished);
        r.ended_at = Some(Timestamp::from_second(1_760_000_000 + i64::from(n)).unwrap());
        r.file = Some(file(n));
        r
    }

    fn five() -> Vec<Run> {
        (1..=5).map(finished).collect()
    }

    fn all_exist(_: &Path) -> bool {
        true
    }

    fn plan_all(runs: &[Run], keep: Option<u32>) -> Vec<PathBuf> {
        plan(runs, keep, Path::new(DIR), &all_exist)
    }

    #[test]
    fn none_keeps_everything() {
        assert!(plan_all(&five(), None).is_empty());
    }

    #[test]
    fn more_slots_than_runs_removes_nothing() {
        assert!(plan_all(&five(), Some(10)).is_empty());
    }

    #[test]
    fn exactly_n_removes_nothing() {
        assert!(plan_all(&five(), Some(5)).is_empty());
    }

    #[test]
    fn the_oldest_beyond_n_go() {
        // History order does not matter; run numbers decide.
        let mut runs = five();
        runs.reverse();
        assert_eq!(plan_all(&runs, Some(3)), files(&[2, 1]));
    }

    #[test]
    fn zero_is_not_a_limit() {
        assert!(plan_all(&five(), Some(0)).is_empty());
    }

    #[test]
    fn files_outside_the_folder_are_ignored() {
        let mut runs = five();
        runs[0].file = Some(PathBuf::from("/elsewhere/r1.spiderlens"));
        runs[1].file = Some(PathBuf::from("/outside/r2.spiderlens"));
        // Only runs 3 to 5 count: keeping 3 removes nothing.
        assert!(plan_all(&runs, Some(3)).is_empty());
        assert_eq!(plan_all(&runs, Some(2)), files(&[3]));
    }

    #[test]
    fn missing_files_do_not_count() {
        let exists = |p: &Path| p != file(5);
        // Run 5's file is gone, so runs 4 and 3 are kept and 2 and 1 go.
        assert_eq!(plan(&five(), Some(2), Path::new(DIR), &exists), files(&[2, 1]));
    }

    #[test]
    fn a_file_shared_by_two_runs_is_listed_once() {
        let mut runs = five();
        runs[0].file = Some(file(2));
        // Runs 1 and 2 point at one file; it counts once, at run 2.
        assert_eq!(plan_all(&runs, Some(3)), files(&[2]));
        assert!(plan_all(&runs, Some(4)).is_empty());
    }

    #[test]
    fn unfinished_runs_are_never_returned() {
        let mut runs = five();
        runs[0].status = RunStatus::Running;
        runs[0].ended_at = None;
        runs[1].ended_at = None;
        // Runs 1 and 2 have not ended; only runs 3 to 5 count.
        assert!(plan_all(&runs, Some(3)).is_empty());
        assert_eq!(plan_all(&runs, Some(1)), files(&[4, 3]));
    }

    #[test]
    fn runs_without_a_file_are_skipped() {
        let mut runs = five();
        runs[4].file = None;
        assert_eq!(plan_all(&runs, Some(3)), files(&[1]));
    }
}
