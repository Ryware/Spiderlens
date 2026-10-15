//! The app's own folders, shared by the desktop app and the CLI.
//!
//! Tauri derives its folders from the bundle identifier and the platform's
//! base folders; these functions do the same, so the CLI reads and writes
//! the same files (spec 024 plan D2, D5, D6). The desktop app checks in a
//! test that both agree.

use std::path::PathBuf;

/// The Tauri bundle identifier (`app/src-tauri/tauri.conf.json`).
pub const APP_ID: &str = "com.ryware.spiderlens";

/// Where settings live (Tauri's `app_config_dir`).
pub fn app_config_dir() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join(APP_ID))
}

/// Where app data lives (Tauri's `app_data_dir`).
pub fn app_data_dir() -> Option<PathBuf> {
    dirs::data_dir().map(|d| d.join(APP_ID))
}

/// Where caches live (Tauri's `app_cache_dir`).
pub fn app_cache_dir() -> Option<PathBuf> {
    dirs::cache_dir().map(|d| d.join(APP_ID))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folders_end_with_the_app_id() {
        for d in [app_config_dir(), app_data_dir(), app_cache_dir()]
            .into_iter()
            .flatten()
        {
            assert!(d.ends_with(APP_ID));
        }
    }
}
