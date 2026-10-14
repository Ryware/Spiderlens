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
    /// Start, pause and stop crawls (P3, off by default).
    pub allow_crawl: bool,
    /// Crawl profiles an assistant may start crawls with.
    pub allowed_profiles: Vec<String>,
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
            allow_crawl: false,
            allowed_profiles: vec!["Polite".into()],
        };
        s.save(&want).unwrap();
        assert_eq!(s.load(), want);
        let raw: serde_json::Value =
            serde_json::from_slice(&fs::read(dir.path().join(FILE)).unwrap()).unwrap();
        assert_eq!(raw["version"], 1);
        assert_eq!(raw["allow_read"], true);
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
