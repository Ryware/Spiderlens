//! SpiderLens desktop shell. It only wires the engine to the UI; there is no
//! crawl logic here (constitution principle III).

// Release builds on Windows run without a console window.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod crawl;
mod files;
mod integrations;
mod results;
mod settings;

use spiderlens_engine::BuildInfo;
use tauri::webview::PageLoadEvent;
use tauri::{Emitter, Manager};

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
            let unsaved = app.path().app_data_dir()?.join("crawls");
            app.manage(files::State::new(unsaved.clone(), &app.path().app_config_dir()?)?);
            app.manage(crawl::State::new(unsaved));
            app.manage(results::State::default());
            app.manage(integrations::State::open()?);
            // Windows and Linux pass a double-clicked crawl file as an
            // argument (spec 006 FR-017).
            for arg in std::env::args_os().skip(1) {
                let p = std::path::PathBuf::from(arg);
                if p.extension().is_some_and(|x| x == "spiderlens") {
                    app.state::<files::State>().add_pending(p);
                }
            }
            files::recover_at_launch(app.handle());
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
            files::crawl_open,
            files::crawl_info,
            files::crawl_save_as,
            files::recent_list,
            files::recent_remove,
            files::recent_locate,
            files::unsaved_list,
            files::unsaved_delete,
            files::crawl_salvage,
            files::storage_status,
            files::pending_open,
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
            integrations::integrations_status,
            integrations::integrations_settings_save,
            integrations::integrations_google_client,
            integrations::integrations_connect_google,
            integrations::integrations_connect_cancel,
            integrations::integrations_save_key,
            integrations::integrations_forget,
            integrations::integrations_properties,
            integrations::integrations_default_request,
            integrations::integrations_run_search_console,
            integrations::integrations_run_resume,
            integrations::integrations_run_pause,
            integrations::integrations_run_cancel,
            integrations::integrations_runs,
            integrations::integrations_queries,
            integrations::integrations_orphans,
            integrations::integrations_remove,
            integrations::integrations_cache_info,
            integrations::integrations_cache_clear,
            integrations::integrations_open_terms,
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
        .on_window_event(|window, event| {
            // Closing the window with a crawl running pauses it first, so it
            // can be resumed after relaunch (spec 006 FR-012).
            if let tauri::WindowEvent::CloseRequested { api, .. } = event
                && window.app_handle().state::<crawl::State>().pause_for_quit()
            {
                api.prevent_close();
                let _ = window
                    .app_handle()
                    .emit("app-quitting", serde_json::json!({ "pausing": true }));
            }
        })
        .build(tauri::generate_context!())
        .expect("SpiderLens failed to start")
        .run(|app, event| match event {
            // Code None is ⌘Q or the last window closing; app.exit(0) after
            // the pause comes back with Some(0).
            tauri::RunEvent::ExitRequested { api, code: None, .. }
                if app.state::<crawl::State>().pause_for_quit() =>
            {
                api.prevent_exit();
                let _ = app.emit("app-quitting", serde_json::json!({ "pausing": true }));
            }
            // macOS hands over files opened from Finder this way (FR-017).
            #[cfg(target_os = "macos")]
            tauri::RunEvent::Opened { urls } => {
                for u in urls {
                    if let Ok(p) = u.to_file_path() {
                        files::open_from_os(app, p);
                    }
                }
            }
            _ => {}
        });
}
