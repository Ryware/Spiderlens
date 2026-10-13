//! SpiderLens desktop shell. It only wires the engine to the UI; there is no
//! crawl logic here (constitution principle III).

// Release builds on Windows run without a console window.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod crawl;
mod results;
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
        .plugin(tauri_plugin_clipboard_manager::init())
        .setup(|app| {
            let dir = app.path().app_config_dir()?.join("profiles");
            app.manage(settings::State::open(dir)?);
            app.manage(crawl::State::new(app.path().app_data_dir()?.join("crawls")));
            app.manage(results::State::default());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            build_info,
            settings::config_defaults,
            settings::config_presets,
            settings::config_validate,
            settings::patterns_test,
            settings::scope_test,
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
            crawl::crawl_start,
            crawl::crawl_resume,
            crawl::crawl_pause,
            crawl::crawl_stop,
            crawl::crawl_summary,
            crawl::crawl_resume_host,
            crawl::crawl_apply_settings,
            crawl::retry_preview,
            crawl::retry_start,
            crawl::crawl_log,
            results::results_tabs,
            results::results_open,
            results::view_open,
            results::view_rows,
            results::view_locate,
            results::view_refresh,
            results::view_counts,
            results::view_close,
            results::url_detail,
            results::url_list,
            results::copy_rows,
            results::export_start,
            results::export_cancel,
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
