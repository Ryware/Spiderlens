//! The assistant activity log (spec 016 FR-028, plan D12).
//!
//! One JSON line per tool call in `assistant-activity.jsonl` in the app data
//! folder: when, which assistant, which tool, which crawl id, how big the
//! answer was and whether it was refused. Never arguments or results, so no
//! crawled content or path ends up in it.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use ts_rs::TS;

pub const FILE: &str = "assistant-activity.jsonl";
/// The log keeps at most about this many lines.
const MAX_LINES: usize = 2_000;
const KEEP_LINES: usize = 1_000;
/// Lines are short; only a file this large can be over the line cap.
const CHECK_ABOVE: u64 = 200 * 1024;

/// One tool call, as the activity list in Settings shows it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[ts(export, rename = "ActivityEntry")]
pub struct Entry {
    pub at: String,
    pub client: Option<String>,
    pub tool: String,
    pub crawl: Option<String>,
    pub bytes: usize,
    pub refused: bool,
    pub error: Option<String>,
}

pub struct Log {
    file: PathBuf,
}

impl Log {
    pub fn new(data_dir: &Path) -> Self {
        Self {
            file: data_dir.join(FILE),
        }
    }

    /// Best effort: a log that cannot be written never fails a call.
    pub fn append(&self, entry: &Entry) {
        let _ = self.try_append(entry);
    }

    fn try_append(&self, entry: &Entry) -> std::io::Result<()> {
        if let Some(dir) = self.file.parent() {
            fs::create_dir_all(dir)?;
        }
        let mut line = serde_json::to_vec(entry)?;
        line.push(b'\n');
        let mut f = OpenOptions::new().create(true).append(true).open(&self.file)?;
        f.write_all(&line)?;
        drop(f);
        if fs::metadata(&self.file)?.len() > CHECK_ABOVE {
            self.rotate()?;
        }
        Ok(())
    }

    /// The newest `limit` entries, newest first. Damaged lines are skipped.
    pub fn read(&self, limit: usize) -> Vec<Entry> {
        let Ok(text) = fs::read_to_string(&self.file) else {
            return Vec::new();
        };
        text.lines()
            .rev()
            .filter_map(|l| serde_json::from_str(l).ok())
            .take(limit)
            .collect()
    }

    pub fn clear(&self) -> std::io::Result<()> {
        match fs::remove_file(&self.file) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        }
    }

    fn rotate(&self) -> std::io::Result<()> {
        let text = fs::read_to_string(&self.file)?;
        let lines: Vec<&str> = text.lines().collect();
        if lines.len() <= MAX_LINES {
            return Ok(());
        }
        let mut keep = lines[lines.len() - KEEP_LINES..].join("\n");
        keep.push('\n');
        let tmp = self.file.with_extension("jsonl.tmp");
        fs::write(&tmp, keep)?;
        fs::rename(tmp, &self.file)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(i: usize) -> Entry {
        Entry {
            at: "2026-10-14T05:00:00Z".into(),
            client: Some("claude-code 2.4".into()),
            tool: format!("tool_{i}_{}", "x".repeat(100)),
            crawl: Some("c_0011aabb".into()),
            bytes: 10,
            refused: false,
            error: None,
        }
    }

    #[test]
    fn appends_lines_and_keeps_the_newest_when_full() {
        let dir = tempfile::tempdir().unwrap();
        let log = Log::new(dir.path());
        for i in 0..2_100 {
            log.append(&entry(i));
        }
        let text = fs::read_to_string(dir.path().join(FILE)).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        assert!(lines.len() <= MAX_LINES);
        assert!(lines.last().unwrap().contains("tool_2099_"));
        let newest = log.read(3);
        assert_eq!(newest.len(), 3);
        assert!(newest[0].tool.starts_with("tool_2099_"));
        log.clear().unwrap();
        assert!(log.read(10).is_empty());
        log.clear().unwrap();
    }
}
