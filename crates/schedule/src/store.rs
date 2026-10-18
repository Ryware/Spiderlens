//! `schedules.json` and the folders next to it (plan D6).
//!
//! The desktop app and the command line both edit this file, so every
//! change takes an exclusive lock, re-reads, writes a temporary file and
//! renames it over the old one.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use jiff::Timestamp;
use xxhash_rust::xxh3::xxh3_64;

use crate::model::Schedule;

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("No schedule named or numbered \"{0}\".")]
    NotFound(String),
    #[error("A schedule named \"{0}\" already exists.")]
    NameTaken(String),
    #[error("The schedules file {path} cannot be read: {message}. It was left as it is; fix or remove it.")]
    Damaged { path: PathBuf, message: String },
    #[error("Could not read or write the schedules folder: {0}")]
    Io(#[from] io::Error),
}

/// Where everything lives under the app's configuration folder.
#[derive(Debug, Clone)]
pub struct Paths {
    root: PathBuf,
}

impl Paths {
    /// `config_dir` is the app's configuration folder
    /// (`<OS config dir>/com.ryware.spiderlens`).
    pub fn new(config_dir: impl Into<PathBuf>) -> Self {
        Self {
            root: config_dir.into().join("schedules"),
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn file(&self) -> PathBuf {
        self.root.join("schedules.json")
    }

    pub fn history(&self, id: &str) -> PathBuf {
        self.root.join("history").join(format!("{id}.jsonl"))
    }

    pub fn run_lock(&self, id: &str) -> PathBuf {
        self.root.join("locks").join(format!("{id}.lock"))
    }

    pub fn stop_file(&self, id: &str) -> PathBuf {
        self.root.join("locks").join(format!("{id}.stop"))
    }
}

pub struct Store {
    paths: Paths,
}

impl Store {
    pub fn new(paths: Paths) -> Self {
        Self { paths }
    }

    pub fn paths(&self) -> &Paths {
        &self.paths
    }

    pub fn list(&self) -> Result<Vec<Schedule>, StoreError> {
        read(&self.paths.file())
    }

    /// By id, else by name ignoring case.
    pub fn get(&self, key: &str) -> Result<Schedule, StoreError> {
        find(&self.list()?, key)
            .cloned()
            .ok_or_else(|| StoreError::NotFound(key.to_string()))
    }

    /// Inserts or replaces by id. Names stay unique, ignoring case.
    pub fn put(&self, schedule: Schedule) -> Result<Schedule, StoreError> {
        self.update(|all| {
            if all
                .iter()
                .any(|s| s.id != schedule.id && s.name.trim().eq_ignore_ascii_case(schedule.name.trim()))
            {
                return Err(StoreError::NameTaken(schedule.name.trim().to_string()));
            }
            match all.iter_mut().find(|s| s.id == schedule.id) {
                Some(s) => *s = schedule.clone(),
                None => all.push(schedule.clone()),
            }
            Ok(schedule)
        })
    }

    pub fn remove(&self, id: &str) -> Result<Schedule, StoreError> {
        self.update(|all| {
            let i = all
                .iter()
                .position(|s| s.id == id)
                .ok_or_else(|| StoreError::NotFound(id.to_string()))?;
            Ok(all.remove(i))
        })
    }

    pub fn remove_all(&self) -> Result<Vec<Schedule>, StoreError> {
        self.update(|all| Ok(std::mem::take(all)))
    }

    fn update<T>(
        &self,
        f: impl FnOnce(&mut Vec<Schedule>) -> Result<T, StoreError>,
    ) -> Result<T, StoreError> {
        fs::create_dir_all(&self.paths.root)?;
        let lock = File::create(self.paths.root.join("schedules.json.lock"))?;
        lock.lock()?;
        let file = self.paths.file();
        let mut all = read(&file)?;
        let out = f(&mut all)?;
        let tmp = file.with_extension("json.tmp");
        {
            let mut w = OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .open(&tmp)?;
            w.write_all(
                serde_json::to_string_pretty(&all)
                    .expect("schedules serialize")
                    .as_bytes(),
            )?;
            w.sync_all()?;
        }
        fs::rename(&tmp, &file)?;
        Ok(out)
    }
}

fn read(path: &Path) -> Result<Vec<Schedule>, StoreError> {
    match fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes).map_err(|e| StoreError::Damaged {
            path: path.to_path_buf(),
            message: e.to_string(),
        }),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(e) => Err(e.into()),
    }
}

pub fn find<'a>(all: &'a [Schedule], key: &str) -> Option<&'a Schedule> {
    all.iter().find(|s| s.id == key).or_else(|| {
        all.iter()
            .find(|s| s.name.trim().eq_ignore_ascii_case(key.trim()))
    })
}

/// Eight lowercase letters and digits, unique enough for one user's schedules.
pub fn new_id(name: &str, now: Timestamp) -> String {
    const ALPHABET: &[u8] = b"abcdefghijkmnpqrstuvwxyz23456789";
    let seed = format!("{name}\0{}\0{}", now.as_nanosecond(), std::process::id());
    let mut h = xxh3_64(seed.as_bytes());
    (0..8)
        .map(|_| {
            let c = ALPHABET[(h % 32) as usize] as char;
            h /= 32;
            c
        })
        .collect()
}

/// Hash of a profile's JSON, to tell when it changed after copying (FR-003).
pub fn profile_hash(profile_json: &str) -> String {
    format!("{:016x}", xxh3_64(profile_json.as_bytes()))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::frequency::{Frequency, TimeOfDay};
    use crate::model::{NotifyLevel, ScheduleInput};

    pub fn sample(id: &str, name: &str) -> Schedule {
        let now = Timestamp::from_second(1_760_000_000).unwrap();
        Schedule {
            id: id.into(),
            name: name.into(),
            enabled: true,
            config: Default::default(),
            source_profile: None,
            source_hash: None,
            input: ScheduleInput::Spider {
                start_url: "https://example.com/".into(),
            },
            frequency: Frequency::Daily {
                time: TimeOfDay { hour: 7, minute: 0 },
            },
            output_dir: PathBuf::from("/tmp/out"),
            catch_up: true,
            notify: NotifyLevel::Problems,
            after: vec![],
            keep_last: None,
            created_at: now,
            updated_at: now,
        }
    }

    #[test]
    fn put_get_remove() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(Paths::new(dir.path()));
        assert!(store.list().unwrap().is_empty());
        store.put(sample("a1", "Weekly")).unwrap();
        store.put(sample("b2", "Daily")).unwrap();
        assert_eq!(store.get("b2").unwrap().name, "Daily");
        assert_eq!(store.get("weekly").unwrap().id, "a1");
        assert!(matches!(
            store.put(sample("c3", " WEEKLY ")),
            Err(StoreError::NameTaken(_))
        ));
        let mut renamed = sample("a1", "Weekly A");
        renamed.enabled = false;
        store.put(renamed).unwrap();
        assert!(!store.get("a1").unwrap().enabled);
        assert_eq!(store.list().unwrap().len(), 2);
        store.remove("a1").unwrap();
        assert!(matches!(store.get("a1"), Err(StoreError::NotFound(_))));
        assert_eq!(store.remove_all().unwrap().len(), 1);
        assert!(store.list().unwrap().is_empty());
    }

    #[test]
    fn damaged_file_is_reported_and_kept() {
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths::new(dir.path());
        fs::create_dir_all(paths.root()).unwrap();
        fs::write(paths.file(), b"{not json").unwrap();
        let store = Store::new(paths.clone());
        assert!(matches!(store.list(), Err(StoreError::Damaged { .. })));
        assert!(store.put(sample("a", "A")).is_err());
        assert_eq!(fs::read(paths.file()).unwrap(), b"{not json");
    }

    #[test]
    fn ids_are_short_and_differ() {
        let now = Timestamp::now();
        let a = new_id("x", now);
        let b = new_id("y", now);
        assert_eq!(a.len(), 8);
        assert_ne!(a, b);
        assert!(a.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit()));
    }
}
