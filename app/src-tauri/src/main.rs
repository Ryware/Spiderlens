//! SpiderLens desktop shell. It only wires the engine to the UI; there is no
//! crawl logic here (constitution principle III).

// Release builds on Windows run without a console window.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod settings;

use spiderlens_engine::BuildInfo;
use tauri::Manager;
use tauri::webview::PageLoadEvent;

/// Product, engine and build identity, shown on the start screen (FR-008).
#[tauri::command]
fn build_info() -> BuildInfo {
    BuildInfo::current()
}

/// When set, the app writes its build info to this file once the window has
/// loaded, then exits. CI uses it as the launch check (FR-027).
const SMOKE_TEST_ENV: &str = "SPIDERLENS_SMOKE_TEST";

fn main() {
    let smoke_file = std::env::var_os(SMOKE_TEST_ENV);

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let dir = app.path().app_config_dir()?.join("profiles");
            app.manage(settings::State::open(dir)?);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            build_info,
            settings::config_defaults,
            settings::config_presets,
            settings::config_validate,
            settings::patterns_test,
            settings::list_parse_text,
            settings::list_parse_file,
            settings::profiles_list,
            settings::profile_load,
            settings::profile_save,
            settings::profile_rename,
            settings::profile_duplicate,
            settings::profile_delete,
            settings::profile_set_default,
            settings::profile_export,
            settings::profile_import,
        ])
        .on_page_load(move |webview, payload| {
            if let (Some(path), PageLoadEvent::Finished) = (&smoke_file, payload.event()) {
                let json = serde_json::to_string(&BuildInfo::current()).expect("BuildInfo serializes");
                let code = match std::fs::write(path, json) {
                    Ok(()) => 0,
                    Err(_) => 1,
                };
                webview.app_handle().exit(code);
            }
        })
        .run(tauri::generate_context!())
        .expect("SpiderLens failed to start");
}
