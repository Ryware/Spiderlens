//! Commands for spelling checks and dictionaries (spec 027 part D). The
//! check runs on its own thread over the crawl file and reports progress as
//! `spell-progress` events; the results reader picks the results up when
//! the results are reopened.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use serde::Serialize;
use spiderlens_config::SpellingSettings;
use spiderlens_spell::{Dictionaries, Installed, SpellSummary, Store};
use tauri::{AppHandle, Emitter, Manager};

type Cmd<T> = Result<T, String>;

#[derive(Default)]
pub struct State {
    /// The running check's cancel flag.
    running: Mutex<Option<Arc<AtomicBool>>>,
}

fn store(app: &AppHandle) -> Cmd<Store> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("No folder for dictionaries: {e}"))?;
    Ok(Store::new(dir.join("dictionaries")))
}

#[tauri::command]
pub fn spell_dictionaries(app: AppHandle) -> Cmd<Dictionaries> {
    Ok(store(&app)?.dictionaries())
}

#[tauri::command]
pub async fn spell_download(app: AppHandle, lang: String) -> Cmd<Installed> {
    let store = store(&app)?;
    let client = reqwest::Client::new();
    store.download(&lang, &client).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub fn spell_add(app: AppHandle, lang: String, aff: PathBuf, dic: PathBuf) -> Cmd<Installed> {
    store(&app)?
        .add_files(&lang, &aff, &dic)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn spell_remove(app: AppHandle, lang: String) -> Cmd<()> {
    store(&app)?.remove(&lang).map_err(|e| e.to_string())
}

#[derive(Serialize, Clone)]
struct Progress {
    checked: u64,
    total: u64,
    state: &'static str,
    summary: Option<SpellSummary>,
    error: Option<String>,
}

/// Starts checking a crawl file. Answers at once; progress and the result
/// come as `spell-progress` events.
#[tauri::command]
pub fn spell_start(
    app: AppHandle,
    state: tauri::State<'_, State>,
    path: PathBuf,
    settings: SpellingSettings,
) -> Cmd<()> {
    let store = store(&app)?;
    let cancel = Arc::new(AtomicBool::new(false));
    {
        let mut running = state.running.lock().expect("spell lock");
        if running.is_some() {
            return Err("Spelling is already being checked.".into());
        }
        *running = Some(cancel.clone());
    }
    std::thread::Builder::new()
        .name("spelling".into())
        .spawn(move || {
            let send = |p: Progress| {
                let _ = app.emit("spell-progress", p);
            };
            let result = spiderlens_spell::run(&path, &settings, &store, &cancel, |p| {
                send(Progress {
                    checked: p.checked,
                    total: p.total,
                    state: "running",
                    summary: None,
                    error: None,
                });
            });
            match result {
                Ok(s) => send(Progress {
                    checked: s.pages,
                    total: s.pages,
                    state: if s.cancelled { "cancelled" } else { "done" },
                    summary: Some(s),
                    error: None,
                }),
                Err(e) => send(Progress {
                    checked: 0,
                    total: 0,
                    state: "failed",
                    summary: None,
                    error: Some(e.to_string()),
                }),
            }
            *app.state::<State>().running.lock().expect("spell lock") = None;
        })
        .map_err(|e| format!("Could not start the spelling check: {e}"))?;
    Ok(())
}

#[tauri::command]
pub fn spell_cancel(state: tauri::State<'_, State>) {
    if let Some(c) = state.running.lock().expect("spell lock").as_ref() {
        c.store(true, Ordering::Relaxed);
    }
}

/// The last check of a crawl file, if it was checked.
#[tauri::command]
pub fn spell_summary(path: PathBuf) -> Cmd<Option<SpellSummary>> {
    let store = spiderlens_engine::crawl::CrawlStore::open(&path).map_err(|e| e.to_string())?;
    Ok(store
        .spelling_summary()
        .map_err(|e| e.to_string())?
        .and_then(|s| serde_json::from_str(&s).ok()))
}
