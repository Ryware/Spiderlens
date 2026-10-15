//! Non-secret integration state in the app config folder (data-model.md):
//! the list of connections, per-provider settings and the usage ledger.
//! Shared by the desktop app and the CLI. Secrets never go here.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::credentials::StoreKind;
use crate::{Provider, Result};

/// A connected Google account or a saved provider key. The secret itself is
/// only in the credential store.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Connection {
    /// `google` or a provider id for keys.
    pub kind: String,
    /// Google account email, or the name the user gave a key.
    pub label: String,
    /// Google scopes granted, by feature: `search_console`, `analytics`, `sheets`.
    #[serde(default)]
    pub scopes: Vec<String>,
    pub created_at: String,
    pub store: StoreKind,
}

/// Per-provider settings the user chose (FR-001, FR-004).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ProviderSettings {
    #[serde(default)]
    pub enabled: bool,
    /// Requests per minute, within the published limit.
    #[serde(default)]
    pub per_minute: Option<u32>,
    #[serde(default)]
    pub concurrency: Option<u32>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct IntegrationSettings {
    #[serde(default)]
    pub providers: BTreeMap<String, ProviderSettings>,
    /// Use the user's own Google OAuth client (stored in the keychain) instead
    /// of the built-in one (FR-016).
    #[serde(default)]
    pub own_google_client: bool,
}

impl IntegrationSettings {
    pub fn provider(&self, p: Provider) -> ProviderSettings {
        self.providers.get(p.id()).cloned().unwrap_or_default()
    }

    pub fn set_provider(&mut self, p: Provider, s: ProviderSettings) {
        self.providers.insert(p.id().to_string(), s);
    }
}

/// The folder with `connections.json`, `settings.json` and `usage.json`.
#[derive(Debug, Clone)]
pub struct Folder {
    dir: PathBuf,
}

impl Folder {
    pub fn new(app_config_dir: &Path) -> Self {
        Self {
            dir: app_config_dir.join("integrations"),
        }
    }

    /// The folder the desktop app uses, for the CLI.
    pub fn default_location() -> Option<Self> {
        spiderlens_config::dirs::app_config_dir().map(|d| Self::new(&d))
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn connections(&self) -> Vec<Connection> {
        read_json(&self.dir.join("connections.json"))
    }

    pub fn save_connections(&self, c: &[Connection]) -> Result<()> {
        write_json(&self.dir.join("connections.json"), &c)
    }

    pub fn settings(&self) -> IntegrationSettings {
        read_json(&self.dir.join("settings.json"))
    }

    pub fn save_settings(&self, s: &IntegrationSettings) -> Result<()> {
        write_json(&self.dir.join("settings.json"), s)
    }

    pub fn usage_path(&self) -> PathBuf {
        self.dir.join("usage.json")
    }
}

/// A missing or damaged file reads as the default, like profiles do.
pub(crate) fn read_json<T: DeserializeOwned + Default>(path: &Path) -> T {
    std::fs::read(path)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

/// Writes through a temporary file and a rename, so a crash never leaves a
/// half-written file.
pub(crate) fn write_json<T: Serialize + ?Sized>(path: &Path, value: &T) -> Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(value).expect("serializes"))?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_and_damaged_files_read_as_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let f = Folder::new(dir.path());
        assert!(f.connections().is_empty());
        std::fs::create_dir_all(f.dir()).unwrap();
        std::fs::write(f.dir().join("settings.json"), b"{not json").unwrap();
        assert_eq!(f.settings(), IntegrationSettings::default());
    }

    #[test]
    fn settings_round_trip_and_off_by_default() {
        let dir = tempfile::tempdir().unwrap();
        let f = Folder::new(dir.path());
        let mut s = f.settings();
        assert!(!s.provider(Provider::SearchConsole).enabled);
        s.set_provider(
            Provider::SearchConsole,
            ProviderSettings {
                enabled: true,
                per_minute: Some(100),
                concurrency: None,
            },
        );
        f.save_settings(&s).unwrap();
        assert_eq!(f.settings(), s);
    }
}
