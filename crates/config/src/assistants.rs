//! AI assistant access (spec 016 FR-021, FR-022, FR-026; data-model.md).
//!
//! One JSON file in the app's configuration folder, written by the desktop
//! app's Settings › AI assistants and read by the MCP server, which runs as a
//! separate process (the app's executable with `mcp`, or `spiderlens mcp`).
//! Everything is off until the user turns it on.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::model::RobotsMode;
use crate::store::{ProfileStore, StoreError};

const FILE: &str = "assistants.json";
const VERSION: u32 = 1;

/// The desktop app's identifier; its folders are named after it.
pub const APP_IDENTIFIER: &str = "com.ryware.spiderlens";

/// What AI assistants may do through SpiderLens's MCP server.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export)]
pub struct AssistantSettings {
    /// Read crawl files: the recent list and files in `allowed_folders`.
    pub allow_read: bool,
    /// Folders whose crawl files assistants may read (one level deep).
    #[ts(type = "string[]")]
    pub allowed_folders: Vec<PathBuf>,
    /// Start, watch, pause and stop crawls (P3, off by default).
    pub allow_crawl: bool,
    /// Crawl profiles an assistant may start crawls with.
    pub allowed_profiles: Vec<String>,
}

/// A saved profile as Settings › AI assistants offers it for crawls.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct AssistantProfile {
    pub name: String,
    /// False when the profile ignores robots.txt; assistants may not use it.
    pub respects_robots: bool,
    /// Why the profile cannot be allowed.
    pub reason: Option<String>,
}

/// Why an assistant may not crawl with the saved profile `name`, or `None`
/// when it may (spec 016 FR-026: only profiles that respect robots.txt).
pub fn crawl_profile_problem(store: &ProfileStore, name: &str) -> Option<String> {
    match store.load(name) {
        Ok(p) if p.config.robots.mode == RobotsMode::Respect => None,
        Ok(_) => Some(format!(
            "The profile \"{name}\" ignores robots.txt, so an AI assistant cannot use it."
        )),
        Err(StoreError::NotFound(_)) => Some(format!("There is no saved profile named \"{name}\".")),
        Err(e) => Some(format!("The profile \"{name}\" cannot be read: {e}")),
    }
}

/// The saved profiles with whether an assistant may use each.
pub fn assistant_profiles(store: &ProfileStore) -> Vec<AssistantProfile> {
    store
        .list()
        .unwrap_or_default()
        .into_iter()
        .map(|e| {
            let reason = match &e.error {
                Some(err) => Some(format!("The profile cannot be read: {err}")),
                None => crawl_profile_problem(store, &e.name),
            };
            AssistantProfile {
                respects_robots: reason.is_none(),
                reason: reason.map(|r| {
                    if r.contains("ignores robots.txt") {
                        "Ignores robots.txt".to_string()
                    } else {
                        r
                    }
                }),
                name: e.name,
            }
        })
        .collect()
}

impl AssistantSettings {
    /// The settings as they are saved: allowed folders must be existing
    /// folders, listed once. With crawling on, every allowed profile must be
    /// a saved profile that respects robots.txt (spec 016 FR-026).
    pub fn checked(mut self, profiles: &ProfileStore) -> Result<Self, String> {
        let mut folders: Vec<PathBuf> = Vec::new();
        for f in self.allowed_folders {
            if !f.is_dir() {
                return Err(format!("{} is not a folder on this computer.", f.display()));
            }
            if !folders.contains(&f) {
                folders.push(f);
            }
        }
        self.allowed_folders = folders;
        let mut names: Vec<String> = Vec::new();
        for n in self.allowed_profiles {
            if !names.contains(&n) {
                names.push(n);
            }
        }
        if self.allow_crawl
            && let Some(problem) = names.iter().find_map(|n| crawl_profile_problem(profiles, n))
        {
            return Err(problem);
        }
        self.allowed_profiles = names;
        Ok(self)
    }
}

#[derive(Serialize, Deserialize)]
struct SettingsFile {
    version: u32,
    #[serde(flatten)]
    settings: AssistantSettings,
}

pub struct AssistantStore {
    file: PathBuf,
}

impl AssistantStore {
    pub fn open(dir: impl AsRef<Path>) -> io::Result<Self> {
        fs::create_dir_all(dir.as_ref())?;
        Ok(Self {
            file: dir.as_ref().join(FILE),
        })
    }

    /// A missing or damaged file reads as everything off.
    pub fn load(&self) -> AssistantSettings {
        fs::read(&self.file)
            .ok()
            .and_then(|b| serde_json::from_slice::<SettingsFile>(&b).ok())
            .map(|f| f.settings)
            .unwrap_or_default()
    }

    pub fn save(&self, settings: &AssistantSettings) -> io::Result<()> {
        let body = serde_json::to_vec_pretty(&SettingsFile {
            version: VERSION,
            settings: settings.clone(),
        })
        .expect("settings serialize");
        // Written beside and renamed, so the server never reads half a file.
        let tmp = self.file.with_extension("json.tmp");
        fs::write(&tmp, body)?;
        fs::rename(tmp, &self.file)
    }
}

/// The desktop app's folders, found the way Tauri finds them, so a separate
/// process (the CLI, the MCP server) reads the same recent list and settings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppDirs {
    /// Settings, profiles, the recent list and `assistants.json`.
    pub config: PathBuf,
    /// Unsaved crawls (`crawls/`) and the assistant activity log.
    pub data: PathBuf,
}

impl AppDirs {
    /// `SPIDERLENS_CONFIG_DIR` and `SPIDERLENS_DATA_DIR` override the
    /// platform folders (tests, portable setups).
    pub fn locate() -> Option<Self> {
        let pick = |var: &str, base: Option<PathBuf>| {
            std::env::var_os(var)
                .filter(|v| !v.is_empty())
                .map(PathBuf::from)
                .or_else(|| base.map(|b| b.join(APP_IDENTIFIER)))
        };
        Some(Self {
            config: pick("SPIDERLENS_CONFIG_DIR", dirs::config_dir())?,
            data: pick("SPIDERLENS_DATA_DIR", dirs::data_dir())?,
        })
    }

    /// The folder for crawls not saved yet (spec 006).
    pub fn unsaved_crawls(&self) -> PathBuf {
        self.data.join("crawls")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CrawlConfig;

    #[test]
    fn missing_or_damaged_file_means_everything_off() {
        let dir = tempfile::tempdir().unwrap();
        let s = AssistantStore::open(dir.path()).unwrap();
        assert_eq!(s.load(), AssistantSettings::default());
        fs::write(dir.path().join(FILE), b"{not json").unwrap();
        assert!(!s.load().allow_read);
    }

    #[test]
    fn round_trips_with_a_version() {
        let dir = tempfile::tempdir().unwrap();
        let s = AssistantStore::open(dir.path()).unwrap();
        let want = AssistantSettings {
            allow_read: true,
            allowed_folders: vec![PathBuf::from("/crawls")],
            allow_crawl: true,
            allowed_profiles: vec!["Polite".into()],
        };
        s.save(&want).unwrap();
        assert_eq!(s.load(), want);
        let raw: serde_json::Value =
            serde_json::from_slice(&fs::read(dir.path().join(FILE)).unwrap()).unwrap();
        assert_eq!(raw["version"], 1);
        assert_eq!(raw["allow_read"], true);
    }

    fn profiles(dir: &Path) -> ProfileStore {
        let store = ProfileStore::open(dir.join("profiles")).unwrap();
        let polite = CrawlConfig::default();
        let mut rude = CrawlConfig::default();
        rude.robots.mode = RobotsMode::Ignore;
        let mut report = CrawlConfig::default();
        report.robots.mode = RobotsMode::IgnoreReport;
        store.save("Polite", &polite, None, false).unwrap();
        store.save("Rude", &rude, None, false).unwrap();
        store.save("Report", &report, None, false).unwrap();
        store
    }

    #[test]
    fn checked_keeps_existing_folders_once() {
        let dir = tempfile::tempdir().unwrap();
        let store = profiles(dir.path());
        let ok = AssistantSettings {
            allow_read: true,
            allowed_folders: vec![dir.path().into(), dir.path().into()],
            ..AssistantSettings::default()
        }
        .checked(&store)
        .unwrap();
        assert_eq!(ok.allowed_folders, vec![dir.path().to_path_buf()]);
        assert!(!ok.allow_crawl);
        let missing = AssistantSettings {
            allowed_folders: vec![dir.path().join("nope")],
            ..AssistantSettings::default()
        };
        assert!(missing.checked(&store).unwrap_err().contains("not a folder"));
    }

    #[test]
    fn crawling_needs_saved_profiles_that_respect_robots() {
        let dir = tempfile::tempdir().unwrap();
        let store = profiles(dir.path());
        let with = |names: &[&str]| AssistantSettings {
            allow_crawl: true,
            allowed_profiles: names.iter().map(|n| n.to_string()).collect(),
            ..AssistantSettings::default()
        };
        let ok = with(&["Polite", "Polite"]).checked(&store).unwrap();
        assert!(ok.allow_crawl);
        assert_eq!(ok.allowed_profiles, vec!["Polite".to_string()]);
        assert!(
            with(&["Rude"])
                .checked(&store)
                .unwrap_err()
                .contains("ignores robots.txt")
        );
        assert!(
            with(&["Report"])
                .checked(&store)
                .unwrap_err()
                .contains("ignores robots.txt")
        );
        assert!(
            with(&["Gone"])
                .checked(&store)
                .unwrap_err()
                .contains("no saved profile")
        );
        // Off: a profile deleted or changed later never blocks saving.
        let off = AssistantSettings {
            allowed_profiles: vec!["Gone".into()],
            ..AssistantSettings::default()
        };
        assert!(off.checked(&store).is_ok());
    }

    #[test]
    fn profiles_are_listed_with_the_reason_they_are_refused() {
        let dir = tempfile::tempdir().unwrap();
        let list = assistant_profiles(&profiles(dir.path()));
        let get = |n: &str| list.iter().find(|p| p.name == n).unwrap().clone();
        assert!(get("Polite").respects_robots && get("Polite").reason.is_none());
        assert!(!get("Rude").respects_robots);
        assert_eq!(get("Rude").reason.as_deref(), Some("Ignores robots.txt"));
        assert!(!get("Report").respects_robots);
    }

    #[test]
    fn unknown_fields_and_partial_files_are_accepted() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join(FILE),
            br#"{"version": 2, "allow_read": true, "later": 1}"#,
        )
        .unwrap();
        let s = AssistantStore::open(dir.path()).unwrap().load();
        assert!(s.allow_read);
        assert!(s.allowed_folders.is_empty());
    }
}
