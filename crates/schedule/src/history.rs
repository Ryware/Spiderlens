//! Run history: one JSON object per line, per schedule (plan D4).
//!
//! A run appends a `running` record when it starts and its final record when
//! it ends. Readers keep the last record of each run number.

use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::{self, BufRead, BufReader, Write};
use std::path::Path;

use jiff::Timestamp;

use crate::model::Run;

pub fn append(path: &Path, run: &Run) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let mut line = serde_json::to_string(run).expect("run serializes");
    line.push('\n');
    let mut f = OpenOptions::new().create(true).append(true).open(path)?;
    // One write call per record keeps lines whole even if two processes append.
    f.write_all(line.as_bytes())
}

/// All runs, newest first. Lines that cannot be read (a torn last line after
/// a power cut) are skipped.
pub fn read(path: &Path) -> io::Result<Vec<Run>> {
    let file = match fs::File::open(path) {
        Ok(f) => f,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e),
    };
    let mut runs: BTreeMap<u32, Run> = BTreeMap::new();
    for line in BufReader::new(file).lines() {
        if let Ok(run) = serde_json::from_str::<Run>(&line?) {
            runs.insert(run.run, run);
        }
    }
    Ok(runs.into_values().rev().collect())
}

pub fn next_number(runs: &[Run]) -> u32 {
    runs.first().map_or(1, |r| r.run + 1)
}

/// The newest scheduled time recorded (Run now does not count, US1-5).
pub fn last_slot(runs: &[Run]) -> Option<Timestamp> {
    runs.iter().filter_map(|r| r.slot).max()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{RunStatus, Trigger};

    #[test]
    fn last_record_per_run_wins() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("h").join("x.jsonl");
        assert!(read(&path).unwrap().is_empty());
        let slot = Timestamp::from_second(1_760_000_000).unwrap();
        append(
            &path,
            &Run::new(1, Some(slot), Trigger::Scheduled, RunStatus::Running),
        )
        .unwrap();
        append(
            &path,
            &Run::new(1, Some(slot), Trigger::Scheduled, RunStatus::Finished),
        )
        .unwrap();
        append(&path, &Run::new(2, None, Trigger::RunNow, RunStatus::Running)).unwrap();
        // A torn line is ignored.
        OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap()
            .write_all(b"{\"run\":3,")
            .unwrap();
        let runs = read(&path).unwrap();
        assert_eq!(runs.len(), 2);
        assert_eq!((runs[0].run, runs[0].status), (2, RunStatus::Running));
        assert_eq!((runs[1].run, runs[1].status), (1, RunStatus::Finished));
        assert_eq!(next_number(&runs), 3);
        assert_eq!(last_slot(&runs), Some(slot));
    }
}
