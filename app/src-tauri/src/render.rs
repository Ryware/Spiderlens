//! The browser JavaScript rendering uses (spec 014 FR-045, plan D12,
//! contracts/desktop-commands.md "Browser").
//!
//! The browser is downloaded on first use, after the person agreed in the
//! window's dialog. These commands are thin wrappers over
//! `spiderlens_render`: status, the pinned download with progress and
//! cancel, the machine preference for a browser the person chose, and a
//! reachability watch for the dialog.
//!
//! Events: `render-browser-progress` (`job`, `bytes_done`, `bytes_total`,
//! `stage`, `waiting`), `render-browser-finished` (`job`, `status`, `error`)
//! and `render-browser-network` (`online`) while the watch runs.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use spiderlens_config::RenderSettings;
use spiderlens_engine::crawl::{CrawlError, Crawler};
use spiderlens_render::{
    BrowserInfo, BrowserSource, CancellationFlag, DownloadProgress, DownloadStage, FindOptions, NotUsable,
    data_dir, download, download_host_reachable, download_plan, find_browser,
};
use tauri::{AppHandle, Emitter, Manager};

type Cmd<T> = Result<T, String>;

/// How often the open dialog checks that the download host can be reached.
const WATCH_EVERY: Duration = Duration::from_secs(5);

/// Where the person's browser choice is kept: a machine preference, never
/// stored in a crawl file.
const PREFERENCE_FILE: &str = "render-browser.json";

#[derive(Default)]
pub struct State {
    next: AtomicU64,
    jobs: Mutex<HashMap<u64, CancellationFlag>>,
    /// Bumped to start or end the network watch; a watch ends when it sees
    /// another number than its own.
    watch: AtomicU64,
}

#[derive(Debug, Clone, Serialize)]
pub struct BrowserView {
    pub name: String,
    pub version: String,
    /// `downloaded`, `installed` or `user_path`.
    pub source: &'static str,
    pub path: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct DownloadView {
    pub version: String,
    pub bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct BrowserStatus {
    pub usable: Option<BrowserView>,
    pub problem: Option<String>,
    /// The pinned download, `None` where this platform has none.
    pub download: Option<DownloadView>,
    /// The browser path the person chose, if any.
    pub user_path: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct ProgressEvent {
    job: u64,
    bytes_done: u64,
    bytes_total: u64,
    stage: &'static str,
    /// The connection was lost; the download resumes when it returns.
    waiting: bool,
}

#[derive(Debug, Clone, Serialize)]
struct FinishedEvent {
    job: u64,
    status: BrowserStatus,
    error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct NetworkEvent {
    online: bool,
}

#[derive(Serialize, Deserialize, Default)]
struct Preference {
    path: Option<PathBuf>,
}

fn preference_file(app: &AppHandle) -> Option<PathBuf> {
    app.path().app_config_dir().ok().map(|d| d.join(PREFERENCE_FILE))
}

/// The browser path the person chose with "Change browser", if any.
pub fn saved_browser_path(app: &AppHandle) -> Option<PathBuf> {
    let text = std::fs::read_to_string(preference_file(app)?).ok()?;
    serde_json::from_str::<Preference>(&text).ok()?.path
}

fn save_browser_path(app: &AppHandle, path: Option<PathBuf>) -> Cmd<()> {
    let file = preference_file(app).ok_or("The settings folder is not available.")?;
    if path.is_none() {
        return match std::fs::remove_file(&file) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.to_string()),
            _ => Ok(()),
        };
    }
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let text = serde_json::to_string(&Preference { path }).map_err(|e| e.to_string())?;
    std::fs::write(file, text).map_err(|e| e.to_string())
}

fn view(b: &BrowserInfo) -> BrowserView {
    BrowserView {
        name: b.name.label().to_owned(),
        version: b.version.clone(),
        source: match b.source {
            BrowserSource::Downloaded => "downloaded",
            BrowserSource::Installed => "installed",
            BrowserSource::UserPath => "user_path",
        },
        path: b.path.display().to_string(),
    }
}

/// Why no browser is usable, in the window's words.
pub fn problem_text(e: &NotUsable) -> String {
    const FIX: &str =
        "Download the browser, choose an installed Chrome or Edge, or turn JavaScript rendering off.";
    match e {
        NotUsable::NotFound => {
            format!("JavaScript rendering needs a Chromium-based browser and none was found. {FIX}")
        }
        NotUsable::TooOld { found, min_major } => format!(
            "JavaScript rendering needs a Chromium-based browser version {min_major} or newer; {} {} is too old. {FIX}",
            found.name.label(),
            found.version
        ),
        NotUsable::Broken { path, message } => {
            format!(
                "The browser at {} cannot be used: {message} {FIX}",
                path.display()
            )
        }
    }
}

/// The browser rendering would use now, with the person's saved choice.
pub fn find_for(app: &AppHandle) -> Result<BrowserInfo, NotUsable> {
    find_browser(&FindOptions {
        user_path: saved_browser_path(app),
        data_dir: data_dir().unwrap_or_else(std::env::temp_dir),
    })
}

fn status_of(app: &AppHandle) -> BrowserStatus {
    let (usable, problem) = match find_for(app) {
        Ok(b) => (Some(view(&b)), None),
        Err(e) => (None, Some(problem_text(&e))),
    };
    BrowserStatus {
        usable,
        problem,
        download: download_plan().map(|p| DownloadView {
            version: p.version.to_owned(),
            bytes: p.bytes,
        }),
        user_path: saved_browser_path(app).map(|p| p.display().to_string()),
    }
}

/// What rendering would use, why nothing is usable, and the download on offer.
#[tauri::command]
pub async fn render_browser_status(app: AppHandle) -> Cmd<BrowserStatus> {
    tauri::async_runtime::spawn_blocking(move || status_of(&app))
        .await
        .map_err(|e| e.to_string())
}

/// Starts the pinned download. The window asked for consent before calling.
/// Progress and the end arrive as events; the job number cancels it.
#[tauri::command]
pub fn render_browser_install(app: AppHandle, state: tauri::State<'_, State>) -> Cmd<u64> {
    let plan = download_plan().ok_or("No browser download is available for this platform.")?;
    let job = state.next.fetch_add(1, Ordering::Relaxed) + 1;
    let cancel = CancellationFlag::new();
    state
        .jobs
        .lock()
        .expect("render lock")
        .insert(job, cancel.clone());
    let dir = data_dir().unwrap_or_else(std::env::temp_dir);
    tauri::async_runtime::spawn(async move {
        let events = app.clone();
        let progress = move |p: DownloadProgress| {
            let _ = events.emit(
                "render-browser-progress",
                ProgressEvent {
                    job,
                    bytes_done: p.done_bytes,
                    bytes_total: p.total_bytes,
                    stage: match p.stage {
                        DownloadStage::Download => "download",
                        DownloadStage::Verify => "verify",
                        DownloadStage::Unpack => "unpack",
                    },
                    waiting: p.waiting,
                },
            );
        };
        // Waits for a lost connection and resumes; only cancel ends the wait.
        let result = download(&plan, &dir, &progress, &cancel).await;
        app.state::<State>()
            .jobs
            .lock()
            .expect("render lock")
            .remove(&job);
        let probe = app.clone();
        let status = tauri::async_runtime::spawn_blocking(move || status_of(&probe))
            .await
            .unwrap_or_else(|_| status_of(&app));
        let _ = app.emit(
            "render-browser-finished",
            FinishedEvent {
                job,
                status,
                error: result.err().map(|e| e.to_string()),
            },
        );
    });
    Ok(job)
}

/// Cancels a download; its partial files are removed.
#[tauri::command]
pub fn render_browser_cancel(state: tauri::State<'_, State>, job: u64) {
    if let Some(flag) = state.jobs.lock().expect("render lock").get(&job) {
        flag.cancel();
    }
}

/// Saves a browser the person chose (`None`: back to detection) and returns
/// the status. A path that cannot be used is not saved and is explained in
/// the returned status.
#[tauri::command]
pub async fn render_browser_set_path(app: AppHandle, path: Option<String>) -> Cmd<BrowserStatus> {
    tauri::async_runtime::spawn_blocking(move || {
        match path
            .map(|p| PathBuf::from(p.trim()))
            .filter(|p| !p.as_os_str().is_empty())
        {
            None => {
                save_browser_path(&app, None)?;
                Ok(status_of(&app))
            }
            Some(path) => {
                let checked = find_browser(&FindOptions {
                    user_path: Some(path.clone()),
                    data_dir: data_dir().unwrap_or_else(std::env::temp_dir),
                });
                match checked {
                    Ok(_) => {
                        save_browser_path(&app, Some(path))?;
                        Ok(status_of(&app))
                    }
                    Err(e) => {
                        let mut status = status_of(&app);
                        status.usable = None;
                        status.problem = Some(problem_text(&e));
                        Ok(status)
                    }
                }
            }
        }
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Starts or ends the reachability watch of the open browser dialog: a
/// cheap HEAD request every 5 s, answered with `render-browser-network`
/// (the first right away). `navigator.onLine` alone is not reliable.
#[tauri::command]
pub fn render_browser_network_watch(app: AppHandle, state: tauri::State<'_, State>, on: bool) {
    let mine = state.watch.fetch_add(1, Ordering::Relaxed) + 1;
    if !on {
        return;
    }
    tauri::async_runtime::spawn(async move {
        let mut last = None;
        while app.state::<State>().watch.load(Ordering::Relaxed) == mine {
            let online = download_host_reachable().await;
            // Only changes go out, and the first answer.
            if last != Some(online) {
                last = Some(online);
                let _ = app.emit("render-browser-network", NetworkEvent { online });
            }
            tokio::time::sleep(WATCH_EVERY).await;
        }
    });
}

/// Renders an existing crawl from its stored HTML (spec 014 FR-006, T073).
/// The main documents are not requested again. Refused with a message when
/// the crawl is not finished, was already rendered or stored no HTML, before
/// anything changes; `browser_unavailable: ...` when no browser is usable
/// (the window answers with the download dialog and calls again). Progress
/// and the end arrive as `crawl-progress` and `crawl-finished`, as for any
/// crawl; a paused render continues when this is called again.
#[tauri::command]
pub async fn crawl_render(
    app: AppHandle,
    path: PathBuf,
    rendering: RenderSettings,
) -> Cmd<crate::crawl::Started> {
    let check = path.clone();
    tauri::async_runtime::spawn_blocking(move || Crawler::check_renderable(&check))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?;
    // Before the file changes: no browser, nothing recorded.
    let browser = find_for(&app).map_err(|e| format!("browser_unavailable: {}", problem_text(&e)))?;
    let file = path.clone();
    let crawler = tauri::async_runtime::spawn_blocking(move || Crawler::open_for_render(&file, rendering))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| match e {
            CrawlError::NeedsUpgrade { .. } => format!("needs_upgrade: {e}"),
            CrawlError::ReadOnly(_) => format!("read_only: {e}"),
            CrawlError::Lock(l) => format!("in_use: {l}"),
            other => other.to_string(),
        })?;
    crate::crawl::launch_prepared(app, path, crawler.with_browser(browser))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn problems_name_the_way_out() {
        let none = problem_text(&NotUsable::NotFound);
        assert!(none.starts_with("JavaScript rendering needs a Chromium-based browser and none was found."));
        assert!(none.contains("Download the browser"));
    }

    #[test]
    fn the_preference_round_trips() {
        let text = serde_json::to_string(&Preference {
            path: Some(PathBuf::from("/x/chrome")),
        })
        .unwrap();
        let back: Preference = serde_json::from_str(&text).unwrap();
        assert_eq!(back.path, Some(PathBuf::from("/x/chrome")));
        let none: Preference = serde_json::from_str("{}").unwrap();
        assert!(none.path.is_none());
    }
}
