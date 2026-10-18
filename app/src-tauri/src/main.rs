//! SpiderLens desktop shell. It only wires the engine to the UI; there is no
//! crawl logic here (constitution principle III).

// Release builds on Windows run without a console window.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod accessibility;
mod aeo;
mod assistants;
mod compare;
mod crawl;
mod custom;
mod files;
mod integrations;
mod login;
mod manage;
mod near_dup;
mod render;
mod report;
mod results;
mod schedules;
mod settings;
mod spelling;

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
    // The browser guard of a rendering crawl is this executable too (R12).
    spiderlens_engine::run_render_guard_if_asked();
    // AI assistants start the app's executable with `mcp` (spec 016 plan
    // D8): serve MCP on standard input and output, with no window.
    let mut args = std::env::args_os().skip(1);
    if args.next().is_some_and(|a| a == "mcp") {
        let code = match spiderlens_mcp::McpArgs::parse(args) {
            Ok(a) => spiderlens_mcp::run(a),
            Err(e) => {
                eprintln!("spiderlens mcp: {e}");
                2
            }
        };
        std::process::exit(code);
    }
    // The system scheduler starts the app with these flags; no window opens
    // (spec 021 plan D1).
    let args: Vec<String> = std::env::args().collect();
    if let Some(code) = schedules::headless(&args) {
        std::process::exit(code);
    }
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
            app.manage(near_dup::State::default());
            app.manage(assistants::State::new(spiderlens_config::AppDirs {
                config: app.path().app_config_dir()?,
                data: app.path().app_data_dir()?,
            }));
            app.manage(spelling::State::default());
            app.manage(compare::State::default());
            app.manage(custom::State::default());
            app.manage(integrations::State::open()?);
            app.manage(aeo::State::default());
            // Windows and Linux pass a double-clicked crawl file as an
            // argument (spec 006 FR-017).
            for arg in std::env::args_os().skip(1) {
                let p = std::path::PathBuf::from(arg);
                if p.extension().is_some_and(|x| x == "spiderlens") {
                    app.state::<files::State>().add_pending(p);
                }
            }
            files::recover_at_launch(app.handle());
            if let Some(s) = schedules::State::new() {
                app.manage(s);
            }
            app.manage(report::State::default());
            app.manage(render::State::default());
            app.manage(login::State::default());
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
            settings::list_load_sitemap,
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
            render::render_browser_status,
            render::render_browser_install,
            render::render_browser_cancel,
            render::render_browser_set_path,
            render::render_browser_network_watch,
            crawl::crawl_pause,
            crawl::crawl_stop,
            crawl::crawl_summary,
            crawl::crawl_resume_host,
            crawl::crawl_apply_settings,
            crawl::retry_preview,
            crawl::retry_start,
            crawl::crawl_log,
            aeo::aeo_run,
            aeo::aeo_cancel,
            aeo::aeo_summary,
            aeo::aeo_page,
            aeo::aeo_llms_txt_draft,
            aeo::ai_bots_list,
            aeo::aeo_rules,
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
            manage::crawls_manage_list,
            manage::crawls_html_size,
            manage::reveal_path,
            manage::crawls_delete,
            manage::crawls_delete_permanently,
            manage::crawls_purge_html_check,
            manage::crawls_purge_html,
            manage::storage_prefs_get,
            manage::storage_prefs_set,
            manage::cleanup_history,
            manage::cleanup_run_now,
            results::results_tabs,
            results::results_open,
            results::view_open,
            results::view_rows,
            results::view_locate,
            results::view_refresh,
            results::view_counts,
            results::tab_counts,
            results::segments_set,
            results::segments_counts,
            results::view_close,
            results::url_detail,
            results::url_list,
            results::agents_page,
            results::agents_summary,
            results::agents_entry_points,
            assistants::assistants_get,
            assistants::assistants_set,
            assistants::assistants_test,
            assistants::assistants_activity,
            assistants::assistants_activity_clear,
            results::viz_scene,
            results::viz_term_links,
            results::viz_export,
            results::cookies_list,
            login::login_open,
            login::login_done,
            login::login_cancel,
            login::login_clear,
            login::login_status,
            login::cookies_import,
            crawl::crawl_add_sign_in,
            crawl::proxy_test,
            results::cookie_urls,
            results::url_html,
            results::url_html_diff,
            results::url_screenshot,
            results::url_tabs,
            results::copy_text,
            results::copy_rows,
            results::export_start,
            results::export_cancel,
            near_dup::near_dup_status,
            near_dup::near_dup_clusters,
            near_dup::near_dup_members,
            near_dup::near_dup_pairs,
            near_dup::compare_pages,
            near_dup::run_near_dup,
            near_dup::cancel_near_dup,
            near_dup::export_near_dup,
            accessibility::accessibility_run,
            spelling::spell_dictionaries,
            spelling::spell_download,
            spelling::spell_add,
            spelling::spell_remove,
            spelling::spell_start,
            spelling::spell_cancel,
            spelling::spell_summary,
            compare::compare_suggest,
            compare::compare_setup,
            compare::compare_test_mapping,
            compare::compare_start,
            compare::compare_cancel,
            compare::compare_summary,
            compare::compare_settings,
            compare::compare_set_elements,
            compare::compare_open_group,
            compare::compare_rows,
            compare::compare_close_group,
            compare::compare_detail,
            compare::compare_report,
            compare::compare_export,
            compare::compare_close,
            compare::compare_settings_load,
            compare::compare_settings_save,
            results::rule_catalogue,
            results::issues_apply,
            results::passes_run,
            results::issues_summary,
            results::url_issues,
            results::url_custom,
            custom::custom_validate,
            custom::custom_test_page,
            custom::custom_test_url,
            custom::custom_rules_of_crawl,
            custom::custom_run,
            custom::custom_cancel,
            custom::custom_import,
            custom::custom_export,
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
            results::sitemap_files,
            results::orphan_summary,
            results::sitemap_entries_for_url,
            results::sitemap_preview,
            results::sitemap_generate,
            results::analysis_run,
            results::analysis_start,
            results::analysis_cancel,
            results::analysis_settings,
            results::positions_test,
            schedules::schedules_list,
            schedules::schedule_get,
            schedules::schedule_preview,
            schedules::schedule_validate,
            schedules::schedule_save,
            schedules::schedule_set_enabled,
            schedules::schedule_duplicate,
            schedules::schedule_delete,
            schedules::schedule_run_now,
            schedules::schedule_stop,
            schedules::schedule_history,
            schedules::compare_schedule_baseline,
            schedules::schedule_update_from_profile,
            schedules::schedules_repair,
            schedules::schedules_remove_all,
            schedules::schedule_default_output,
            report::report,
            report::export_report_html,
            report::print_report,
            report::bulk_export,
            report::bulk_export_cancel,
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
