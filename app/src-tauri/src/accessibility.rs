//! The accessibility command (spec 026 FR-031): run the static checks on a
//! finished crawl from its stored HTML. Nothing is fetched.

use spiderlens_config::AccessibilitySettings;
use spiderlens_engine::accessibility::{RerunSummary, rerun_static};

/// Checks every page of the crawl file with stored HTML and replaces its
/// static results. Fails with a message when the file is still being
/// crawled, is open in another process or has no stored HTML.
#[tauri::command]
pub async fn accessibility_run(path: String) -> Result<RerunSummary, String> {
    tauri::async_runtime::spawn_blocking(move || {
        rerun_static(std::path::Path::new(&path), &AccessibilitySettings::default(), |_, _| {})
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| format!("The check stopped unexpectedly: {e}"))?
}
