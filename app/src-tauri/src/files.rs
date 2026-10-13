//! Commands for crawl files as documents (spec 006,
//! specs/006-crawl-storage/contracts/desktop-commands.md): open, information,
//! Save As, the recent list, unsaved crawls, damaged files and launch-time
//! recovery. Every rule lives in the engine (`crawl::file`); this file only
//! knows where the app keeps things.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::Serialize;
use spiderlens_config::{RecentEntry, RecentStore};
use spiderlens_engine::crawl::file::{self, CrawlInfo, DEFAULT_DISK_RESERVE};
use spiderlens_engine::crawl::store::StoreError;
use tauri::{AppHandle, Emitter, Manager};

type Cmd<T> = Result<T, String>;

pub struct State {
    /// Where crawls go until the user saves them somewhere (FR-007).
    pub unsaved_dir: PathBuf,
    recent: RecentStore,
    pub disk_reserve: u64,
    /// Files the OS asked us to open before the window listened (D10).
    pending: Mutex<Vec<PathBuf>>,
}

impl State {
    pub fn new(unsaved_dir: PathBuf, config_dir: &Path) -> std::io::Result<Self> {
        std::fs::create_dir_all(&unsaved_dir)?;
        Ok(Self {
            unsaved_dir,
            recent: RecentStore::open(config_dir)?,
            disk_reserve: DEFAULT_DISK_RESERVE,
            pending: Mutex::new(Vec::new()),
        })
    }

    pub fn is_unsaved(&self, path: &Path) -> bool {
        match (
            path.parent().map(Path::canonicalize),
            self.unsaved_dir.canonicalize(),
        ) {
            (Some(Ok(p)), Ok(d)) => p == d,
            _ => path.parent() == Some(self.unsaved_dir.as_path()),
        }
    }

    /// Records that a crawl was used now, with what the list shows.
    pub fn touch(&self, path: &Path, urls: Option<u64>) {
        let previous = self.recent.list().into_iter().find(|e| e.path == path);
        if let Some(e) = entry_from_file(path, urls.or(previous.map(|p| p.urls))) {
            let _ = self.recent.touch(e);
        }
    }

    /// After Save As: the entry follows the file.
    pub fn moved(&self, from: &Path, to: &Path) {
        let _ = self.recent.relocate(from, to);
        self.touch(to, None);
    }

    pub fn add_pending(&self, path: PathBuf) {
        self.pending.lock().expect("pending lock").push(path);
    }
}

/// What the list shows for a crawl file, read from its `meta` only (quick
/// on any size).
fn entry_from_file(path: &Path, urls: Option<u64>) -> Option<RecentEntry> {
    let status = file::read_meta(path, "status")?;
    let urls = match urls {
        Some(n) => n,
        None => file::info(path).map(|i| i.counts.discovered).unwrap_or(0),
    };
    Some(RecentEntry {
        path: path.to_path_buf(),
        name: file::read_meta(path, "name")
            .unwrap_or_else(|| path.file_stem().unwrap_or_default().to_string_lossy().to_string()),
        start_url: file::read_meta(path, "start_url"),
        urls,
        status,
        recovered: file::read_meta(path, "recovered").as_deref() == Some("1"),
        last_used: now(),
    })
}

fn now() -> String {
    file::now_string()
}

#[derive(Serialize, Clone)]
pub struct RecentRow {
    #[serde(flatten)]
    entry: RecentEntry,
    available: bool,
    unsaved: bool,
}

fn row(state: &State, mut e: RecentEntry) -> RecentRow {
    let available = e.path.exists();
    if available {
        // The status may have changed outside the app (CLI, a crash).
        if let Some(s) = file::read_meta(&e.path, "status") {
            e.status = s;
        }
        e.recovered = file::read_meta(&e.path, "recovered").as_deref() == Some("1");
    }
    RecentRow {
        unsaved: state.is_unsaved(&e.path),
        entry: e,
        available,
    }
}

/// The information panel's data, with the configuration in force now.
fn info_value(state: &State, info: CrawlInfo) -> serde_json::Value {
    let effective = info.config.as_ref().map(|c| c.effective());
    let unsaved = state.is_unsaved(&info.path);
    let mut v = serde_json::to_value(&info).expect("info serializes");
    v["config"] = serde_json::to_value(effective).expect("config serializes");
    v["unsaved"] = serde_json::Value::Bool(unsaved);
    v
}

/// A damaged or foreign file, as the UI's OpenFailure (FR-038).
fn open_failure(path: &Path, e: &StoreError) -> String {
    serde_json::json!({
        "message": e.to_string(),
        "salvageable": matches!(e, StoreError::Damaged(_)) && file::salvageable(path),
    })
    .to_string()
}

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> Cmd<T> + Send + 'static) -> Cmd<T> {
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|e| e.to_string())?
}

/// Opens a crawl for viewing: recovers it first when it was not closed
/// cleanly, then adds it to the recent list (FR-013, FR-018).
#[tauri::command]
pub async fn crawl_open(app: AppHandle, path: PathBuf) -> Cmd<serde_json::Value> {
    blocking(move || {
        let state = app.state::<State>();
        let active = app.state::<crate::crawl::State>().active_path();
        if active.as_deref() != Some(path.as_path()) {
            file::recover_path(&path).map_err(|e| open_failure(&path, &e))?;
        }
        let info = file::info(&path).map_err(|e| open_failure(&path, &e))?;
        state.touch(&path, Some(info.counts.discovered));
        Ok(info_value(&state, info))
    })
    .await
}

#[tauri::command]
pub async fn crawl_info(app: AppHandle, path: PathBuf) -> Cmd<serde_json::Value> {
    blocking(move || {
        let state = app.state::<State>();
        let info = file::info(&path).map_err(|e| e.to_string())?;
        Ok(info_value(&state, info))
    })
    .await
}

/// Save As (FR-015): a running crawl moves and keeps going; otherwise the
/// file is copied. An unsaved crawl is moved, not copied: its old file is
/// deleted once the new one is complete.
#[tauri::command]
pub async fn crawl_save_as(app: AppHandle, path: PathBuf, dest: PathBuf) -> Cmd<PathBuf> {
    let running = app.state::<crate::crawl::State>().save_as_running(&path, &dest);
    let saved = match running {
        Some(wait) => wait
            .await
            .map_err(|_| "The crawl stopped before it could be saved.".to_string())??,
        None => {
            let (p, d) = (path.clone(), dest.clone());
            blocking(move || file::save_as(&p, &d).map(|()| d).map_err(|e| e.to_string())).await?
        }
    };
    let state = app.state::<State>();
    if state.is_unsaved(&path) && !file::same_path(&path, &saved) {
        app.state::<crate::results::State>().close(&path);
        let _ = file::remove_crawl_file(&path);
    }
    state.moved(&path, &saved);
    Ok(saved)
}

#[tauri::command]
pub fn recent_list(state: tauri::State<'_, State>) -> Vec<RecentRow> {
    state.recent.list().into_iter().map(|e| row(&state, e)).collect()
}

/// Removes an entry; the crawl file stays where it is (FR-026).
#[tauri::command]
pub fn recent_remove(state: tauri::State<'_, State>, path: PathBuf) -> Cmd<()> {
    state.recent.remove(&path).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn recent_locate(state: tauri::State<'_, State>, old: PathBuf, new: PathBuf) -> Cmd<RecentRow> {
    file::info(&new).map_err(|e| e.to_string())?;
    state.recent.relocate(&old, &new).map_err(|e| e.to_string())?;
    state.touch(&new, None);
    let e = state
        .recent
        .list()
        .into_iter()
        .find(|e| e.path == new)
        .ok_or("The crawl could not be added to the list.")?;
    Ok(row(&state, e))
}

#[derive(Serialize)]
pub struct Unsaved {
    entries: Vec<RecentRow>,
    total_bytes: u64,
    size_bytes: HashMap<PathBuf, u64>,
}

/// Crawls not yet saved to a chosen place, and the space they use (FR-023).
#[tauri::command]
pub async fn unsaved_list(app: AppHandle) -> Cmd<Unsaved> {
    blocking(move || {
        let state = app.state::<State>();
        let recent = state.recent.list();
        let mut entries = Vec::new();
        let mut size_bytes = HashMap::new();
        let dir = std::fs::read_dir(&state.unsaved_dir).map_err(|e| e.to_string())?;
        for f in dir.flatten() {
            let path = f.path();
            let name = path.file_name().unwrap_or_default().to_string_lossy();
            if path.extension().is_none_or(|x| x != "spiderlens") || name.starts_with('.') {
                continue;
            }
            let urls = recent.iter().find(|e| e.path == path).map(|e| e.urls);
            let Some(e) = entry_from_file(&path, urls) else {
                continue;
            };
            size_bytes.insert(path.clone(), file::file_size(&path));
            entries.push(row(&state, e));
        }
        entries.sort_by(|a, b| b.entry.path.cmp(&a.entry.path));
        Ok(Unsaved {
            total_bytes: size_bytes.values().sum(),
            entries,
            size_bytes,
        })
    })
    .await
}

/// Deletes an unsaved crawl after the UI asked for confirmation. Only files
/// in the unsaved folder can be deleted this way.
#[tauri::command]
pub fn unsaved_delete(app: AppHandle, path: PathBuf) -> Cmd<()> {
    let state = app.state::<State>();
    if !state.is_unsaved(&path) {
        return Err("Only unsaved crawls can be deleted here.".into());
    }
    if app.state::<crate::crawl::State>().active_path().as_deref() == Some(path.as_path()) {
        return Err("Stop the crawl before deleting it.".into());
    }
    if let Some(by) = file::WriterLock::held_by_other(&path) {
        return Err(format!("This crawl is in use by {}.", by.describe()));
    }
    app.state::<crate::results::State>().close(&path);
    file::remove_crawl_file(&path).map_err(|e| e.to_string())?;
    let _ = state.recent.remove(&path);
    Ok(())
}

/// Copies what can be read from a damaged file into a new unsaved crawl and
/// opens that (FR-038).
#[tauri::command]
pub async fn crawl_salvage(app: AppHandle, path: PathBuf) -> Cmd<serde_json::Value> {
    blocking(move || {
        let state = app.state::<State>();
        let stem = path.file_stem().unwrap_or_default().to_string_lossy().to_string();
        let dest = crate::crawl::free_path(&state.unsaved_dir, &format!("{stem} (recovered).spiderlens"));
        file::salvage(&path, &dest).map_err(|e| e.to_string())?;
        let info = file::info(&dest).map_err(|e| e.to_string())?;
        state.touch(&dest, Some(info.counts.discovered));
        Ok(info_value(&state, info))
    })
    .await
}

#[derive(Serialize)]
pub struct StorageStatus {
    unsaved_dir: PathBuf,
    free_bytes: Option<u64>,
    reserve_bytes: u64,
    low: bool,
}

/// Free space where unsaved crawls go, for the start-up warning (FR-031).
#[tauri::command]
pub fn storage_status(state: tauri::State<'_, State>) -> StorageStatus {
    let free = file::free_space(&state.unsaved_dir);
    StorageStatus {
        unsaved_dir: state.unsaved_dir.clone(),
        free_bytes: free,
        reserve_bytes: state.disk_reserve,
        low: free.is_some_and(|f| f < state.disk_reserve),
    }
}

/// Files the OS asked us to open before the window was listening.
#[tauri::command]
pub fn pending_open(state: tauri::State<'_, State>) -> Vec<PathBuf> {
    std::mem::take(&mut *state.pending.lock().expect("pending lock"))
}

/// A file to open from the OS (double-click, file association): sent to the
/// window, and kept for `pending_open` in case it is not listening yet.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub fn open_from_os(app: &AppHandle, path: PathBuf) {
    if app.emit("open-file", path.clone()).is_err() {
        app.state::<State>().add_pending(path);
    }
}

/// Crawls a previous run left "running" (a crash or a forced quit) are
/// recovered at launch and reported to the window (FR-013).
pub fn recover_at_launch(app: &AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || {
        let state = app.state::<State>();
        let mut candidates: Vec<PathBuf> = state.recent.list().into_iter().map(|e| e.path).collect();
        if let Ok(dir) = std::fs::read_dir(&state.unsaved_dir) {
            candidates.extend(
                dir.flatten()
                    .map(|f| f.path())
                    .filter(|p| p.extension().is_some_and(|x| x == "spiderlens")),
            );
        }
        candidates.sort();
        candidates.dedup();
        // Copies cut off by a crash during Save As.
        let mut dirs: Vec<PathBuf> = candidates
            .iter()
            .filter_map(|p| p.parent().map(Path::to_path_buf))
            .collect();
        dirs.push(state.unsaved_dir.clone());
        dirs.sort();
        dirs.dedup();
        for dir in &dirs {
            file::remove_stale_partials(dir);
        }
        let mut recovered = Vec::new();
        for path in candidates {
            if let Ok(Some(_)) = file::recover_path(&path) {
                state.touch(&path, None);
                if let Some(e) = state.recent.list().into_iter().find(|e| e.path == path) {
                    recovered.push(row(&state, e));
                }
            }
        }
        if !recovered.is_empty() {
            // The window may still be loading; give it a moment.
            std::thread::sleep(std::time::Duration::from_secs(1));
            let _ = app.emit("crawls-recovered", recovered);
        }
    });
}
