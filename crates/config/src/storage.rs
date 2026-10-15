//! Storage preferences and the cleanup history (spec 019 plan D7,
//! data-model.md).
//!
//! App-level settings, not part of a crawl profile: how long unsaved crawls
//! are kept, the free-space safety reserve, and whether the one-time cleanup
//! notice was shown. Read by the shell at start-up and by the crawl runner,
//! so they live here rather than in the UI's local storage.

use std::fs;
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::validate::{Issue, Severity};

const FILE: &str = "storage.json";
const HISTORY: &str = "cleanup-history.jsonl";
const VERSION: u32 = 1;

const MIB: u64 = 1024 * 1024;
/// Default free space kept on the crawl file's drive (spec 006 FR-027).
pub const DEFAULT_RESERVE: u64 = 1024 * MIB;
pub const MIN_RESERVE: u64 = 256 * MIB;
pub const MAX_RESERVE: u64 = 50 * 1024 * MIB;
/// Days unsaved crawls are kept by default (FR-017).
pub const DEFAULT_CLEANUP_DAYS: u32 = 30;
/// The ages the setting offers.
pub const CLEANUP_DAY_CHOICES: [u32; 4] = [7, 14, 30, 90];
/// Cleanup history entries kept (FR-022).
pub const HISTORY_LIMIT: usize = 500;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoragePrefs {
    #[serde(default = "version")]
    pub version: u32,
    /// None: automatic removal of unsaved crawls is off.
    pub cleanup_days: Option<u32>,
    pub reserve_bytes: u64,
    /// When the one-time cleanup notice was shown (FR-020).
    #[serde(default)]
    pub first_cleanup_announced: Option<String>,
    /// The calendar day (YYYY-MM-DD, local) cleanup last ran (plan D8).
    #[serde(default)]
    pub last_cleanup: Option<String>,
}

fn version() -> u32 {
    VERSION
}

impl Default for StoragePrefs {
    fn default() -> Self {
        Self {
            version: VERSION,
            cleanup_days: Some(DEFAULT_CLEANUP_DAYS),
            reserve_bytes: DEFAULT_RESERVE,
            first_cleanup_announced: None,
            last_cleanup: None,
        }
    }
}

impl StoragePrefs {
    /// Problems with values a user entered (US5.3).
    pub fn validate(&self) -> Vec<Issue> {
        let mut out = Vec::new();
        if !(MIN_RESERVE..=MAX_RESERVE).contains(&self.reserve_bytes) {
            out.push(Issue {
                path: "storage.reserve_bytes".into(),
                code: "reserve_range".into(),
                message: "The safety reserve must be between 256 MB and 50 GB.".into(),
                severity: Severity::Error,
            });
        }
        if let Some(d) = self.cleanup_days
            && !CLEANUP_DAY_CHOICES.contains(&d)
        {
            out.push(Issue {
                path: "storage.cleanup_days".into(),
                code: "cleanup_days".into(),
                message: "Choose 7, 14, 30 or 90 days, or turn automatic cleanup off.".into(),
                severity: Severity::Error,
            });
        }
        out
    }
}

/// Why a file was removed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CleanupReason {
    ManualDelete,
    ManualDeletePermanent,
    AutoAge,
    Leftover,
    HtmlPurge,
}

/// One removal in the cleanup history.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CleanupRecord {
    pub at: String,
    pub path: PathBuf,
    pub bytes: u64,
    pub reason: CleanupReason,
}

/// `storage.json` and `cleanup-history.jsonl` in the app's configuration
/// folder.
pub struct StorageStore {
    dir: PathBuf,
}

impl StorageStore {
    pub fn open(dir: impl AsRef<Path>) -> io::Result<Self> {
        fs::create_dir_all(dir.as_ref())?;
        Ok(Self {
            dir: dir.as_ref().to_path_buf(),
        })
    }

    /// The saved preferences; defaults when missing or damaged, and any
    /// out-of-range value replaced by its default.
    pub fn prefs(&self) -> StoragePrefs {
        let mut p: StoragePrefs = fs::read(self.dir.join(FILE))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        let d = StoragePrefs::default();
        if !(MIN_RESERVE..=MAX_RESERVE).contains(&p.reserve_bytes) {
            p.reserve_bytes = d.reserve_bytes;
        }
        if p.cleanup_days.is_some_and(|x| !CLEANUP_DAY_CHOICES.contains(&x)) {
            p.cleanup_days = d.cleanup_days;
        }
        p
    }

    /// Saves preferences after validation; returns the issues instead when
    /// a value is out of range.
    pub fn set_prefs(&self, prefs: &StoragePrefs) -> Result<(), Vec<Issue>> {
        let issues = prefs.validate();
        if !issues.is_empty() {
            return Err(issues);
        }
        let body = serde_json::to_vec_pretty(prefs).expect("prefs serialize");
        // Written beside and renamed, so a crash never leaves half a file.
        let tmp = self.dir.join(format!("{FILE}.tmp"));
        fs::write(&tmp, body)
            .and_then(|()| fs::rename(&tmp, self.dir.join(FILE)))
            .map_err(|e| {
                vec![Issue {
                    path: "storage".into(),
                    code: "write".into(),
                    message: format!("Could not save the storage settings: {e}"),
                    severity: Severity::Error,
                }]
            })
    }

    /// Changes preferences with `f` and saves them.
    pub fn update(&self, f: impl FnOnce(&mut StoragePrefs)) -> Result<StoragePrefs, Vec<Issue>> {
        let mut p = self.prefs();
        f(&mut p);
        self.set_prefs(&p)?;
        Ok(p)
    }

    /// Newest first. Unreadable lines are skipped.
    pub fn history(&self) -> Vec<CleanupRecord> {
        let Ok(f) = fs::File::open(self.dir.join(HISTORY)) else {
            return Vec::new();
        };
        let mut out: Vec<CleanupRecord> = io::BufReader::new(f)
            .lines()
            .map_while(Result::ok)
            .filter_map(|l| serde_json::from_str(&l).ok())
            .collect();
        out.reverse();
        out
    }

    /// Adds records and keeps the last [`HISTORY_LIMIT`].
    pub fn record(&self, records: &[CleanupRecord]) -> io::Result<()> {
        if records.is_empty() {
            return Ok(());
        }
        let mut all = self.history();
        all.reverse();
        all.extend_from_slice(records);
        let skip = all.len().saturating_sub(HISTORY_LIMIT);
        let tmp = self.dir.join(format!("{HISTORY}.tmp"));
        {
            let mut f = io::BufWriter::new(fs::File::create(&tmp)?);
            for r in &all[skip..] {
                serde_json::to_writer(&mut f, r).expect("record serializes");
                f.write_all(b"\n")?;
            }
            f.flush()?;
        }
        fs::rename(tmp, self.dir.join(HISTORY))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rec(i: u64, reason: CleanupReason) -> CleanupRecord {
        CleanupRecord {
            at: format!("2026-10-13T10:00:{:02}Z", i % 60),
            path: PathBuf::from(format!("/c/{i}.spiderlens")),
            bytes: i,
            reason,
        }
    }

    #[test]
    fn defaults_when_missing_or_damaged() {
        let dir = tempfile::tempdir().unwrap();
        let s = StorageStore::open(dir.path()).unwrap();
        assert_eq!(s.prefs(), StoragePrefs::default());
        fs::write(dir.path().join(FILE), b"{not json").unwrap();
        assert_eq!(s.prefs(), StoragePrefs::default());
    }

    #[test]
    fn saves_and_reads_back() {
        let dir = tempfile::tempdir().unwrap();
        let s = StorageStore::open(dir.path()).unwrap();
        let p = s
            .update(|p| {
                p.cleanup_days = None;
                p.reserve_bytes = 5 * 1024 * MIB;
            })
            .unwrap();
        assert_eq!(s.prefs(), p);
        assert!(!dir.path().join(format!("{FILE}.tmp")).exists());
    }

    #[test]
    fn us5_3_out_of_range_values_are_refused() {
        let dir = tempfile::tempdir().unwrap();
        let s = StorageStore::open(dir.path()).unwrap();
        for bad in [0, MIN_RESERVE - 1, MAX_RESERVE + 1] {
            let err = s.update(|p| p.reserve_bytes = bad).unwrap_err();
            assert_eq!(err[0].code, "reserve_range");
        }
        assert_eq!(
            s.update(|p| p.cleanup_days = Some(3)).unwrap_err()[0].code,
            "cleanup_days"
        );
        for ok in [MIN_RESERVE, MAX_RESERVE] {
            s.update(|p| p.reserve_bytes = ok).unwrap();
        }
        for d in CLEANUP_DAY_CHOICES {
            s.update(|p| p.cleanup_days = Some(d)).unwrap();
        }
    }

    #[test]
    fn a_hand_edited_bad_value_reads_as_its_default() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join(FILE),
            br#"{"version":1,"cleanup_days":3,"reserve_bytes":1}"#,
        )
        .unwrap();
        let p = StorageStore::open(dir.path()).unwrap().prefs();
        assert_eq!(p.cleanup_days, Some(DEFAULT_CLEANUP_DAYS));
        assert_eq!(p.reserve_bytes, DEFAULT_RESERVE);
    }

    #[test]
    fn history_is_newest_first_and_capped() {
        let dir = tempfile::tempdir().unwrap();
        let s = StorageStore::open(dir.path()).unwrap();
        for chunk in (0..520).collect::<Vec<u64>>().chunks(40) {
            let recs: Vec<_> = chunk.iter().map(|&i| rec(i, CleanupReason::AutoAge)).collect();
            s.record(&recs).unwrap();
        }
        let h = s.history();
        assert_eq!(h.len(), HISTORY_LIMIT);
        assert_eq!(h[0].bytes, 519);
        assert_eq!(h.last().unwrap().bytes, 20);
    }
}
