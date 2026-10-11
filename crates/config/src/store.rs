//! Profiles on disk: one JSON file per profile (FR-031, FR-032; research R9).
//!
//! The shell passes in the folder, so this crate never needs to know about
//! platform paths or the desktop framework.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::CrawlConfig;
use crate::profile::{ImportedProfile, ProfileError, export_profile, import_profile};

const INDEX: &str = "index.json";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ProfileEntry {
    pub name: String,
    pub is_default: bool,
    /// Set when the file exists but cannot be read.
    pub error: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("A profile named \"{0}\" already exists.")]
    NameTaken(String),
    #[error("No profile named \"{0}\".")]
    NotFound(String),
    #[error("The profile name cannot be empty.")]
    EmptyName,
    #[error(transparent)]
    Profile(#[from] ProfileError),
    #[error("Could not read or write the profiles folder: {0}")]
    Io(#[from] io::Error),
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct Index {
    default: Option<String>,
}

/// A profile file and the result of reading it.
type Scanned = (PathBuf, Result<ImportedProfile, StoreError>);

pub struct ProfileStore {
    dir: PathBuf,
}

impl ProfileStore {
    pub fn open(dir: impl Into<PathBuf>) -> Result<Self, StoreError> {
        let dir = dir.into();
        fs::create_dir_all(&dir)?;
        Ok(Self { dir })
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn list(&self) -> Result<Vec<ProfileEntry>, StoreError> {
        let default = self.read_index().default;
        let mut entries = Vec::new();
        for (path, result) in self.scan()? {
            entries.push(match result {
                Ok(p) => ProfileEntry {
                    is_default: default.as_deref().is_some_and(|d| same_name(d, &p.name)),
                    name: p.name,
                    error: None,
                },
                Err(e) => ProfileEntry {
                    name: path
                        .file_stem()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .into_owned(),
                    is_default: false,
                    error: Some(e.to_string()),
                },
            });
        }
        entries.sort_by_key(|e| e.name.to_lowercase());
        Ok(entries)
    }

    pub fn load(&self, name: &str) -> Result<ImportedProfile, StoreError> {
        let path = self
            .find(name)?
            .ok_or_else(|| StoreError::NotFound(name.into()))?;
        Ok(import_profile(&fs::read_to_string(path)?)?)
    }

    /// Saves a profile. With `overwrite` false, an existing name is an error
    /// so the UI can ask (US5-2).
    pub fn save(
        &self,
        name: &str,
        config: &CrawlConfig,
        start_url: Option<&str>,
        overwrite: bool,
    ) -> Result<(), StoreError> {
        let name = name.trim();
        if name.is_empty() {
            return Err(StoreError::EmptyName);
        }
        let existing = self.find(name)?;
        if existing.is_some() && !overwrite {
            return Err(StoreError::NameTaken(name.into()));
        }
        let path = existing.unwrap_or_else(|| self.dir.join(file_name(name)));
        write_atomic(&path, export_profile(name, config, start_url).as_bytes())?;
        Ok(())
    }

    pub fn rename(&self, from: &str, to: &str) -> Result<(), StoreError> {
        let profile = self.load(from)?;
        if !same_name(from, to) && self.find(to)?.is_some() {
            return Err(StoreError::NameTaken(to.into()));
        }
        let was_default = self.default_profile().is_some_and(|d| same_name(&d, from));
        self.delete(from)?;
        self.save(to, &profile.config, profile.start_url.as_deref(), true)?;
        if was_default {
            self.set_default(Some(to))?;
        }
        Ok(())
    }

    pub fn duplicate(&self, from: &str, to: &str) -> Result<(), StoreError> {
        let profile = self.load(from)?;
        self.save(to, &profile.config, profile.start_url.as_deref(), false)
    }

    /// Deleting the default profile makes the built-in defaults the default
    /// again.
    pub fn delete(&self, name: &str) -> Result<(), StoreError> {
        let path = self
            .find(name)?
            .ok_or_else(|| StoreError::NotFound(name.into()))?;
        fs::remove_file(path)?;
        if self.default_profile().is_some_and(|d| same_name(&d, name)) {
            self.set_default(None)?;
        }
        Ok(())
    }

    pub fn set_default(&self, name: Option<&str>) -> Result<(), StoreError> {
        if let Some(n) = name
            && self.find(n)?.is_none()
        {
            return Err(StoreError::NotFound(n.into()));
        }
        let index = Index {
            default: name.map(str::to_string),
        };
        write_atomic(
            &self.dir.join(INDEX),
            serde_json::to_string_pretty(&index)
                .expect("index serializes")
                .as_bytes(),
        )?;
        Ok(())
    }

    pub fn default_profile(&self) -> Option<String> {
        self.read_index().default
    }

    /// The configuration new crawls start with (FR-032).
    pub fn default_config(&self) -> CrawlConfig {
        self.default_profile()
            .and_then(|n| self.load(&n).ok())
            .map(|p| p.config)
            .unwrap_or_default()
    }

    fn read_index(&self) -> Index {
        fs::read_to_string(self.dir.join(INDEX))
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    fn scan(&self) -> Result<Vec<Scanned>, StoreError> {
        let mut out = Vec::new();
        for entry in fs::read_dir(&self.dir)? {
            let path = entry?.path();
            let is_profile =
                path.extension().is_some_and(|e| e == "json") && path.file_name().is_some_and(|f| f != INDEX);
            if is_profile {
                let loaded = fs::read_to_string(&path)
                    .map_err(StoreError::from)
                    .and_then(|t| import_profile(&t).map_err(StoreError::from));
                out.push((path, loaded));
            }
        }
        Ok(out)
    }

    /// Names are unique ignoring case.
    fn find(&self, name: &str) -> Result<Option<PathBuf>, StoreError> {
        Ok(self
            .scan()?
            .into_iter()
            .find(|(_, p)| p.as_ref().is_ok_and(|p| same_name(&p.name, name)))
            .map(|(path, _)| path))
    }
}

fn same_name(a: &str, b: &str) -> bool {
    a.trim().to_lowercase() == b.trim().to_lowercase()
}

/// Readable slug plus a stable hash, so "A b" and "a-b" never collide.
fn file_name(name: &str) -> String {
    let slug: String = name
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .take(8)
        .collect::<Vec<_>>()
        .join("-");
    // FNV-1a: stable across Rust versions, unlike the std hasher.
    let hash = name
        .trim()
        .to_lowercase()
        .bytes()
        .fold(0xcbf2_9ce4_8422_2325_u64, |h, b| {
            (h ^ b as u64).wrapping_mul(0x0100_0000_01b3)
        });
    format!("{slug}-{:08x}.json", hash as u32)
}

/// Writes to a temporary file in the same folder, then renames it into place,
/// so a crash mid-write never leaves a damaged profile.
fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, bytes)?;
    fs::rename(&tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SpeedSettings;

    fn store() -> (tempfile::TempDir, ProfileStore) {
        let dir = tempfile::tempdir().unwrap();
        let s = ProfileStore::open(dir.path().join("profiles")).unwrap();
        (dir, s)
    }

    fn fast() -> CrawlConfig {
        CrawlConfig {
            speed: SpeedSettings::FAST,
            ..Default::default()
        }
    }

    #[test]
    fn save_reopen() {
        let (dir, s) = store();
        s.save("Client A", &fast(), None, false).unwrap();
        let reopened = ProfileStore::open(dir.path().join("profiles")).unwrap();
        assert_eq!(reopened.load("client a").unwrap().config, fast());
        assert_eq!(reopened.list().unwrap()[0].name, "Client A");
    }

    #[test]
    fn name_taken() {
        let (_d, s) = store();
        s.save("Client A", &fast(), None, false).unwrap();
        assert!(matches!(
            s.save("client A", &fast(), None, false),
            Err(StoreError::NameTaken(_))
        ));
        s.save("Client A", &CrawlConfig::default(), None, true).unwrap();
        assert_eq!(s.list().unwrap().len(), 1);
        assert_eq!(s.load("Client A").unwrap().config, CrawlConfig::default());
    }

    #[test]
    fn default_profile_applies_and_is_cleared_on_delete() {
        let (_d, s) = store();
        assert_eq!(s.default_config(), CrawlConfig::default());
        s.save("Fast", &fast(), None, false).unwrap();
        s.set_default(Some("Fast")).unwrap();
        assert_eq!(s.default_config(), fast());
        assert!(s.list().unwrap()[0].is_default);
        s.delete("Fast").unwrap();
        assert_eq!(s.default_profile(), None);
        assert_eq!(s.default_config(), CrawlConfig::default());
    }

    #[test]
    fn rename_and_duplicate() {
        let (_d, s) = store();
        s.save("One", &fast(), None, false).unwrap();
        s.set_default(Some("One")).unwrap();
        s.rename("One", "Uno").unwrap();
        s.duplicate("Uno", "Copy").unwrap();
        let names: Vec<_> = s
            .list()
            .unwrap()
            .into_iter()
            .map(|e| (e.name, e.is_default))
            .collect();
        assert_eq!(names, vec![("Copy".into(), false), ("Uno".into(), true)]);
    }

    #[test]
    fn damaged_file_is_listed_with_error_and_others_still_load() {
        let (_d, s) = store();
        s.save("Good", &fast(), None, false).unwrap();
        fs::write(s.dir().join("broken.json"), "{oops").unwrap();
        let list = s.list().unwrap();
        assert_eq!(list.len(), 2);
        assert!(list.iter().any(|e| e.name == "broken" && e.error.is_some()));
        assert!(s.load("Good").is_ok());
    }

    #[test]
    fn similar_names_get_different_files() {
        assert_ne!(file_name("A b"), file_name("a-b"));
        assert!(file_name("Client A / staging").starts_with("client-a-staging-"));
    }
}
