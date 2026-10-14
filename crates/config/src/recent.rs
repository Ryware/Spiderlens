//! The recent crawls list (spec 006 US4, FR-024 to FR-026; data-model.md).
//!
//! One JSON file in the app's configuration folder. Entries point at crawl
//! files and cache what the list shows, so it can be drawn without opening
//! every file; the shell refreshes the cached fields from the files.
//! Removing an entry never touches the crawl file.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// How many crawls the list keeps (FR-024).
pub const RECENT_LIMIT: usize = 20;
const FILE: &str = "recent.json";
const VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecentEntry {
    pub path: PathBuf,
    pub name: String,
    pub start_url: Option<String>,
    pub urls: u64,
    pub status: String,
    #[serde(default)]
    pub recovered: bool,
    pub last_used: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct RecentFile {
    version: u32,
    entries: Vec<RecentEntry>,
}

pub struct RecentStore {
    file: PathBuf,
}

impl RecentStore {
    pub fn open(dir: impl AsRef<Path>) -> io::Result<Self> {
        fs::create_dir_all(dir.as_ref())?;
        Ok(Self {
            file: dir.as_ref().join(FILE),
        })
    }

    /// Most recently used first. A damaged list reads as empty rather than
    /// stopping the app.
    pub fn list(&self) -> Vec<RecentEntry> {
        fs::read(&self.file)
            .ok()
            .and_then(|b| serde_json::from_slice::<RecentFile>(&b).ok())
            .map(|f| f.entries)
            .unwrap_or_default()
    }

    fn save(&self, entries: Vec<RecentEntry>) -> io::Result<()> {
        let body = serde_json::to_vec_pretty(&RecentFile {
            version: VERSION,
            entries,
        })
        .expect("entries serialize");
        // Written beside and renamed, so a crash never leaves half a list.
        let tmp = self.file.with_extension("json.tmp");
        fs::write(&tmp, body)?;
        fs::rename(tmp, &self.file)
    }

    /// Puts `entry` first, replacing any entry for the same file.
    pub fn touch(&self, entry: RecentEntry) -> io::Result<()> {
        let mut entries = self.list();
        entries.retain(|e| e.path != entry.path);
        entries.insert(0, entry);
        entries.truncate(RECENT_LIMIT);
        self.save(entries)
    }

    /// Updates the cached fields of an entry without moving it.
    pub fn update(&self, entry: RecentEntry) -> io::Result<()> {
        let mut entries = self.list();
        if let Some(e) = entries.iter_mut().find(|e| e.path == entry.path) {
            *e = entry;
            self.save(entries)?;
        }
        Ok(())
    }

    /// Takes an entry off the list. The crawl file is not touched (FR-026).
    pub fn remove(&self, path: &Path) -> io::Result<()> {
        let mut entries = self.list();
        entries.retain(|e| e.path != path);
        self.save(entries)
    }

    /// Points an entry at the file's new location (moved by the user, or by
    /// Save As), keeping its place in the list.
    pub fn relocate(&self, from: &Path, to: &Path) -> io::Result<Option<RecentEntry>> {
        let mut entries = self.list();
        entries.retain(|e| e.path != to);
        let Some(e) = entries.iter_mut().find(|e| e.path == from) else {
            return Ok(None);
        };
        e.path = to.to_path_buf();
        let moved = e.clone();
        self.save(entries)?;
        Ok(Some(moved))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(path: &str) -> RecentEntry {
        RecentEntry {
            path: PathBuf::from(path),
            name: path.into(),
            start_url: Some("https://a.test/".into()),
            urls: 3,
            status: "paused".into(),
            recovered: false,
            last_used: "2026-10-13T10:00:00Z".into(),
        }
    }

    #[test]
    fn most_recent_first_without_duplicates_and_capped() {
        let dir = tempfile::tempdir().unwrap();
        let s = RecentStore::open(dir.path()).unwrap();
        for i in 0..25 {
            s.touch(entry(&format!("/c/{i}.spiderlens"))).unwrap();
        }
        s.touch(entry("/c/20.spiderlens")).unwrap();
        let list = s.list();
        assert_eq!(list.len(), RECENT_LIMIT);
        assert_eq!(list[0].path, PathBuf::from("/c/20.spiderlens"));
        assert_eq!(list[1].path, PathBuf::from("/c/24.spiderlens"));
        assert_eq!(
            list.iter()
                .filter(|e| e.path == Path::new("/c/20.spiderlens"))
                .count(),
            1
        );
    }

    #[test]
    fn remove_and_relocate_keep_the_files() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("x.spiderlens");
        fs::write(&file, b"crawl").unwrap();
        let s = RecentStore::open(dir.path()).unwrap();
        s.touch(entry("/c/a.spiderlens")).unwrap();
        s.touch(entry(file.to_str().unwrap())).unwrap();
        s.remove(&file).unwrap();
        assert!(file.exists(), "removing an entry never deletes the crawl");
        assert_eq!(s.list().len(), 1);
        let moved = s
            .relocate(Path::new("/c/a.spiderlens"), Path::new("/d/a.spiderlens"))
            .unwrap();
        assert_eq!(moved.unwrap().path, PathBuf::from("/d/a.spiderlens"));
        assert_eq!(s.list()[0].path, PathBuf::from("/d/a.spiderlens"));
    }

    #[test]
    fn a_damaged_list_reads_as_empty() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join(FILE), b"{oops").unwrap();
        let s = RecentStore::open(dir.path()).unwrap();
        assert!(s.list().is_empty());
        s.touch(entry("/c/a.spiderlens")).unwrap();
        assert_eq!(s.list().len(), 1);
    }
}
