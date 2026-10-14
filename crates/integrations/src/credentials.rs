//! Credentials in the operating system's keychain (plan D2, FR-013, FR-014).
//!
//! macOS Keychain, Windows Credential Manager and the Linux Secret Service
//! through the `keyring` crate. When none is usable, credentials are kept in
//! memory until the process ends and the UI says so. Values are
//! `SecretString` everywhere: redacted in `Debug`, wiped on drop, never
//! serialized by this crate.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{Error, Result};

/// The keychain service name: the Tauri bundle identifier.
pub const SERVICE: &str = spiderlens_config::dirs::APP_ID;

/// Which keychain entry: `<kind>/<label>`, for example `google/ana@example.com`
/// or `ahrefs/Agency key`. Never a secret itself.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CredentialId {
    pub kind: String,
    pub label: String,
}

impl CredentialId {
    pub fn new(kind: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            kind: kind.into(),
            label: label.into(),
        }
    }

    pub fn account(&self) -> String {
        format!("{}/{}", self.kind, self.label)
    }
}

/// Where credentials are kept.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum StoreKind {
    Keychain,
    /// No usable keychain: kept until the app quits (FR-014).
    MemoryOnly,
}

pub trait CredentialStore: Send + Sync {
    fn get(&self, id: &CredentialId) -> Result<Option<SecretString>>;
    fn set(&self, id: &CredentialId, secret: &SecretString) -> Result<()>;
    fn delete(&self, id: &CredentialId) -> Result<()>;
    fn kind(&self) -> StoreKind;
}

/// The OS keychain.
pub struct KeyringStore {
    service: String,
}

impl KeyringStore {
    pub fn new(service: impl Into<String>) -> Self {
        Self {
            service: service.into(),
        }
    }

    fn entry(&self, id: &CredentialId) -> Result<keyring::Entry> {
        keyring::Entry::new(&self.service, &id.account()).map_err(cred_err)
    }

    /// True when the keychain answers at all (a missing entry is fine).
    pub fn usable(&self) -> bool {
        let probe = CredentialId::new("probe", "availability");
        match keyring::Entry::new(&self.service, &probe.account()).and_then(|e| e.get_password()) {
            Ok(_) | Err(keyring::Error::NoEntry) => true,
            Err(_) => false,
        }
    }
}

fn cred_err(e: keyring::Error) -> Error {
    Error::Credentials(e.to_string())
}

impl CredentialStore for KeyringStore {
    fn get(&self, id: &CredentialId) -> Result<Option<SecretString>> {
        match self.entry(id)?.get_password() {
            Ok(v) => Ok(Some(SecretString::from(v))),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(cred_err(e)),
        }
    }

    fn set(&self, id: &CredentialId, secret: &SecretString) -> Result<()> {
        self.entry(id)?
            .set_password(secret.expose_secret())
            .map_err(cred_err)
    }

    fn delete(&self, id: &CredentialId) -> Result<()> {
        match self.entry(id)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(cred_err(e)),
        }
    }

    fn kind(&self) -> StoreKind {
        StoreKind::Keychain
    }
}

/// Credentials held in this process only.
#[derive(Default)]
pub struct MemoryStore {
    map: Mutex<HashMap<CredentialId, SecretString>>,
}

impl CredentialStore for MemoryStore {
    fn get(&self, id: &CredentialId) -> Result<Option<SecretString>> {
        Ok(self.map.lock().expect("lock").get(id).cloned())
    }

    fn set(&self, id: &CredentialId, secret: &SecretString) -> Result<()> {
        self.map
            .lock()
            .expect("lock")
            .insert(id.clone(), secret.clone());
        Ok(())
    }

    fn delete(&self, id: &CredentialId) -> Result<()> {
        self.map.lock().expect("lock").remove(id);
        Ok(())
    }

    fn kind(&self) -> StoreKind {
        StoreKind::MemoryOnly
    }
}

/// The keychain when it works, else memory only (FR-014).
pub fn default_store() -> Arc<dyn CredentialStore> {
    let k = KeyringStore::new(SERVICE);
    if k.usable() {
        Arc::new(k)
    } else {
        Arc::new(MemoryStore::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_store_round_trip_and_delete() {
        let s = MemoryStore::default();
        let id = CredentialId::new("ahrefs", "Agency");
        assert!(s.get(&id).unwrap().is_none());
        s.set(&id, &SecretString::from("k-123")).unwrap();
        assert_eq!(s.get(&id).unwrap().unwrap().expose_secret(), "k-123");
        s.delete(&id).unwrap();
        assert!(s.get(&id).unwrap().is_none());
        assert_eq!(s.kind(), StoreKind::MemoryOnly);
    }

    #[test]
    fn secrets_are_redacted_in_debug() {
        let secret = SecretString::from("super-secret-token");
        assert!(!format!("{secret:?}").contains("super-secret-token"));
    }

    /// Runs against the real keychain where one is available (macOS,
    /// Windows, and the Linux CI job with an unlocked gnome-keyring).
    #[test]
    fn keychain_round_trip_when_available() {
        let store = KeyringStore::new(format!("{SERVICE}.test"));
        if !store.usable() {
            eprintln!("no usable keychain here; skipped");
            return;
        }
        let id = CredentialId::new("test", format!("round-trip-{}", std::process::id()));
        store.set(&id, &SecretString::from("value-1")).unwrap();
        assert_eq!(store.get(&id).unwrap().unwrap().expose_secret(), "value-1");
        store.delete(&id).unwrap();
        assert!(store.get(&id).unwrap().is_none());
    }
}
