//! Commands for crawl data management (spec 019,
//! specs/019-crawl-data-management/contracts/desktop-commands.md): the crawl
//! manager, delete to the trash, removing stored HTML, storage settings and
//! automatic cleanup. What may be removed is decided in the engine
//! (`crawl::manage::plan_cleanup`); this file moves files to the trash and
//! decides when cleanup runs (plan D4, D8).

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use jiff::{SignedDuration, Timestamp};
use serde::Serialize;
use spiderlens_config::{CleanupReason, CleanupRecord, StoragePrefs};
use spiderlens_engine::crawl::file::{self, WriterLock};
use spiderlens_engine::crawl::manage::{
    self, CleanupRule, CrawlRow, HtmlSize, Kind, PurgeCheck, PurgeReport, ScanContext,
};
use spiderlens_schedule::retention;
use tauri::{AppHandle, Emitter, Manager};

type Cmd<T> = Result<T, String>;

/// Set while a cleanup run is going, so the timer never overlaps one.
static CLEANING: AtomicBool = AtomicBool::new(false);

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> Cmd<T> + Send + 'static) -> Cmd<T> {
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|e| e.to_string())?
}

/// Now, or shifted by `SPIDERLENS_CLEANUP_NOW=+10d` for manual testing
/// (quickstart.md).
fn now() -> Timestamp {
    let now = Timestamp::now();
    std::env::var("SPIDERLENS_CLEANUP_NOW")
        .ok()
        .and_then(|v| v.strip_prefix('+')?.strip_suffix('d')?.parse::<i64>().ok())
        .map_or(now, |d| now + SignedDuration::from_hours(24 * d))
}

/// Crawls open in this app: the running or paused crawl and the one the
/// results window reads.
fn open_paths(app: &AppHandle) -> HashSet<PathBuf> {
    let mut out = HashSet::new();
    if let Some(p) = app.state::<crate::crawl::State>().active_path() {
        out.insert(p);
    }
    if let Some(p) = app.state::<crate::results::State>().open_path() {
        out.insert(p);
    }
    out
}

// ---------------------------------------------------------------------------
// The manager list (US1)

#[derive(Serialize, Clone)]
pub struct ManagedRow {
    #[serde(flatten)]
    row: CrawlRow,
    unsaved: bool,
    running: bool,
    in_recent: bool,
}

#[derive(Serialize, Clone)]
pub struct OtherFile {
    path: PathBuf,
    size_bytes: u64,
}

#[derive(Serialize, Clone, Default)]
pub struct Totals {
    all: u64,
    unsaved: u64,
    leftovers: u64,
}

#[derive(Serialize, Clone)]
pub struct ManageList {
    crawls: Vec<ManagedRow>,
    other_files: Vec<OtherFile>,
    totals: Totals,
    free_bytes: Option<u64>,
    unsaved_dir: PathBuf,
}

fn list(app: &AppHandle) -> ManageList {
    let files = app.state::<crate::files::State>();
    let recent = files.recent_entries();
    let active = app.state::<crate::crawl::State>().active_path();
    let now = now();
    let mut paths: Vec<PathBuf> = manage::crawls_in(&files.unsaved_dir);
    for e in &recent {
        if !paths.iter().any(|p| p == &e.path) {
            paths.push(e.path.clone());
        }
    }
    let mut totals = Totals::default();
    let crawls: Vec<ManagedRow> = paths
        .into_iter()
        .map(|p| {
            let entry = recent.iter().find(|e| e.path == p);
            let mut row = manage::describe(
                &p,
                entry.map(|e| e.last_used.as_str()),
                entry.map(|e| e.urls),
                now,
            );
            if !row.available
                && let Some(e) = entry
            {
                row.name = e.name.clone();
                row.start_url = e.start_url.clone();
                row.status = Some(e.status.clone());
            }
            let unsaved = files.is_unsaved(&p);
            let running = active.as_deref() == Some(p.as_path());
            let backups: u64 = row.backups.iter().map(|b| b.size_bytes).sum();
            totals.all += row.size_bytes + backups;
            if unsaved {
                totals.unsaved += row.size_bytes;
            }
            ManagedRow {
                in_recent: entry.is_some(),
                row,
                unsaved,
                running,
            }
        })
        .collect();
    let candidates = scan(app, now);
    let other_files = candidates
        .iter()
        .filter(|c| c.kind == Kind::Other && c.in_unsaved_dir)
        .map(|c| OtherFile {
            path: c.path.clone(),
            size_bytes: c.size_bytes,
        })
        .collect();
    let leftovers = manage::plan_cleanup(
        &candidates,
        CleanupRule {
            days: None,
            announced: false,
        },
        now,
    )
    .leftovers;
    totals.leftovers = candidates
        .iter()
        .filter(|c| leftovers.contains(&c.path))
        .map(|c| c.size_bytes)
        .sum();
    ManageList {
        crawls,
        other_files,
        totals,
        free_bytes: file::free_space(&files.unsaved_dir),
        unsaved_dir: files.unsaved_dir.clone(),
    }
}

#[tauri::command]
pub async fn crawls_manage_list(app: AppHandle) -> Cmd<ManageList> {
    blocking(move || Ok(list(&app))).await
}

/// Stored-HTML size of one crawl, asked for after the list (FR-004).
#[tauri::command]
pub async fn crawls_html_size(path: PathBuf) -> Cmd<HtmlSize> {
    blocking(move || manage::html_size(&path).map_err(|e| e.to_string())).await
}

/// Shows a file in the system file manager.
#[tauri::command]
pub fn reveal_path(path: PathBuf) -> Cmd<()> {
    use std::process::Command;
    let result = if cfg!(target_os = "macos") {
        Command::new("open").arg("-R").arg(&path).spawn()
    } else if cfg!(windows) {
        Command::new("explorer")
            .arg(format!("/select,{}", path.display()))
            .spawn()
    } else {
        let dir = if path.is_dir() {
            path.clone()
        } else {
            path.parent().map(Path::to_path_buf).unwrap_or_default()
        };
        Command::new("xdg-open").arg(dir).spawn()
    };
    result.map(|_| ()).map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// Deleting (US2, plan D4)

#[derive(Serialize, Clone)]
pub struct Skipped {
    path: PathBuf,
    reason: String,
}

#[derive(Serialize, Clone, Default)]
pub struct DeleteReport {
    deleted: Vec<PathBuf>,
    skipped: Vec<Skipped>,
    /// Not deleted: the trash is not available there. The UI asks again
    /// before a permanent delete (US2.3).
    trash_unavailable: Vec<PathBuf>,
    freed_bytes: u64,
}

impl DeleteReport {
    pub fn first_skipped(&self) -> Option<&str> {
        self.skipped.first().map(|s| s.reason.as_str())
    }

    pub fn trash_unavailable_any(&self) -> bool {
        !self.trash_unavailable.is_empty()
    }
}

/// How a crawl's files leave the disk.
pub enum Removal<'a> {
    Trash(&'a dyn Fn(&[PathBuf]) -> Result<(), String>),
    Permanent,
}

/// Why removing a crawl did not happen.
#[derive(Debug, PartialEq, Eq)]
pub enum RemoveError {
    NoTrash,
    Failed(String),
}

/// Removes the crawl at `path` and every helper file of it. With the trash,
/// the crawl file goes first; if the trash refuses it, nothing was removed.
/// Returns the bytes freed.
pub fn remove_crawl(path: &Path, how: &Removal<'_>) -> Result<u64, RemoveError> {
    let files = manage::crawl_files(path);
    let size: u64 = files
        .iter()
        .map(|p| std::fs::metadata(p).map(|m| m.len()).unwrap_or(0))
        .sum();
    let Some((main, helpers)) = files.split_first() else {
        return Err(RemoveError::Failed("The crawl file no longer exists.".into()));
    };
    match how {
        Removal::Trash(trash) => {
            trash(std::slice::from_ref(main)).map_err(|_| RemoveError::NoTrash)?;
            // Helper files go with it; one the trash will not take is no
            // user data, so it is removed.
            for h in helpers {
                if trash(std::slice::from_ref(h)).is_err() {
                    let _ = std::fs::remove_file(h);
                }
            }
        }
        Removal::Permanent => {
            std::fs::remove_file(main).map_err(|e| RemoveError::Failed(e.to_string()))?;
            for h in helpers {
                let _ = std::fs::remove_file(h);
            }
        }
    }
    Ok(size)
}

fn os_trash(paths: &[PathBuf]) -> Result<(), String> {
    trash::delete_all(paths).map_err(|e| e.to_string())
}

/// Deletes crawls the user chose, skipping any in use (FR-007 to FR-010).
pub fn delete(app: &AppHandle, paths: &[PathBuf], permanently: bool) -> DeleteReport {
    let files = app.state::<crate::files::State>();
    let active = app.state::<crate::crawl::State>().active_path();
    let mut report = DeleteReport::default();
    let mut records = Vec::new();
    for path in paths {
        if active.as_deref() == Some(path.as_path()) {
            report.skipped.push(Skipped {
                path: path.clone(),
                reason: "Stop the crawl before deleting it.".into(),
            });
            continue;
        }
        if let Some(by) = WriterLock::held_by_other(path) {
            report.skipped.push(Skipped {
                path: path.clone(),
                reason: format!("In use by {}.", by.describe()),
            });
            continue;
        }
        app.state::<crate::results::State>().close(path);
        let how = if permanently {
            Removal::Permanent
        } else {
            Removal::Trash(&os_trash)
        };
        match remove_crawl(path, &how) {
            Ok(bytes) => {
                report.deleted.push(path.clone());
                report.freed_bytes += bytes;
                files.forget(path);
                records.push(CleanupRecord {
                    at: Timestamp::now().to_string(),
                    path: path.clone(),
                    bytes,
                    reason: if permanently {
                        CleanupReason::ManualDeletePermanent
                    } else {
                        CleanupReason::ManualDelete
                    },
                });
            }
            Err(RemoveError::NoTrash) => report.trash_unavailable.push(path.clone()),
            Err(RemoveError::Failed(reason)) => report.skipped.push(Skipped {
                path: path.clone(),
                reason,
            }),
        }
    }
    let _ = files.storage.record(&records);
    if !report.deleted.is_empty() {
        let _ = app.emit("crawls-changed", ());
    }
    report
}

#[tauri::command]
pub async fn crawls_delete(app: AppHandle, paths: Vec<PathBuf>) -> Cmd<DeleteReport> {
    blocking(move || Ok(delete(&app, &paths, false))).await
}

/// Only after the UI's second confirmation for `trash_unavailable`.
#[tauri::command]
pub async fn crawls_delete_permanently(app: AppHandle, paths: Vec<PathBuf>) -> Cmd<DeleteReport> {
    blocking(move || Ok(delete(&app, &paths, true))).await
}

// ---------------------------------------------------------------------------
// Removing stored HTML (US3)

#[tauri::command]
pub async fn crawls_purge_html_check(path: PathBuf) -> Cmd<PurgeCheck> {
    blocking(move || manage::purge_check(&path).map_err(|e| e.to_string())).await
}

#[tauri::command]
pub async fn crawls_purge_html(app: AppHandle, path: PathBuf, compact: bool) -> Cmd<PurgeReport> {
    blocking(move || {
        if app.state::<crate::crawl::State>().active_path().as_deref() == Some(path.as_path()) {
            return Err("Pause or stop the crawl before removing its stored HTML.".into());
        }
        // Windows cannot replace a file someone has open (plan D2).
        app.state::<crate::results::State>().close(&path);
        let report = manage::purge_html(&path, compact).map_err(|e| e.to_string())?;
        let _ = app
            .state::<crate::files::State>()
            .storage
            .record(&[CleanupRecord {
                at: Timestamp::now().to_string(),
                path: path.clone(),
                bytes: report.freed_bytes,
                reason: CleanupReason::HtmlPurge,
            }]);
        let _ = app.emit("crawls-changed", ());
        Ok(report)
    })
    .await
}

// ---------------------------------------------------------------------------
// Settings › Storage (US5, FR-025)

#[tauri::command]
pub fn storage_prefs_get(app: AppHandle) -> StoragePrefs {
    app.state::<crate::files::State>().storage.prefs()
}

/// Saves the user-editable fields; errors are the validation issues as JSON.
#[tauri::command]
pub fn storage_prefs_set(app: AppHandle, prefs: StoragePrefs) -> Cmd<StoragePrefs> {
    app.state::<crate::files::State>()
        .storage
        .update(|p| {
            p.cleanup_days = prefs.cleanup_days;
            p.reserve_bytes = prefs.reserve_bytes;
        })
        .map_err(|issues| serde_json::to_string(&issues).expect("issues serialize"))
}

#[tauri::command]
pub fn cleanup_history(app: AppHandle) -> Vec<CleanupRecord> {
    app.state::<crate::files::State>().storage.history()
}

// ---------------------------------------------------------------------------
// Automatic cleanup (US4, plan D5, D8)

#[derive(Serialize, Clone, Default)]
pub struct CleanupReport {
    removed: Vec<PathBuf>,
    /// Older runs of schedules that keep only their last N runs.
    schedule_removed: Vec<PathBuf>,
    leftovers: usize,
    freed_bytes: u64,
    /// Unsaved crawls the one-time notice names: removed at a later start.
    announced: Vec<PathBuf>,
}

fn scan(app: &AppHandle, now: Timestamp) -> Vec<manage::Candidate> {
    let files = app.state::<crate::files::State>();
    let recent = files.recent_entries();
    let mut dirs: Vec<PathBuf> = recent
        .iter()
        .filter_map(|e| e.path.parent().map(Path::to_path_buf))
        .collect();
    dirs.sort();
    dirs.dedup();
    let auto_removed: HashSet<PathBuf> = files
        .storage
        .history()
        .into_iter()
        .filter(|r| r.reason == CleanupReason::AutoAge)
        .map(|r| r.path)
        .collect();
    let open = open_paths(app);
    let last_used = |p: &Path| recent.iter().find(|e| e.path == p).map(|e| e.last_used.clone());
    manage::scan(&ScanContext {
        unsaved_dir: &files.unsaved_dir,
        other_dirs: &dirs,
        recent_last_used: &last_used,
        open: &open,
        auto_removed: &auto_removed,
        now,
    })
}

/// One cleanup run: plan in the engine, then trash and remove. Unsaved
/// crawls only ever go to the trash; when the trash refuses one, it stays.
pub fn run_cleanup(app: &AppHandle) -> CleanupReport {
    if CLEANING.swap(true, Ordering::SeqCst) {
        return CleanupReport::default();
    }
    let report = (|| {
        let files = app.state::<crate::files::State>();
        // Two app instances never clean at once.
        let Ok(_guard) = WriterLock::acquire(&files.config_dir.join("cleanup")) else {
            return CleanupReport::default();
        };
        let now = now();
        let prefs = files.storage.prefs();
        let candidates = scan(app, now);
        let plan = manage::plan_cleanup(
            &candidates,
            CleanupRule {
                days: prefs.cleanup_days,
                announced: prefs.first_cleanup_announced.is_some(),
            },
            now,
        );
        let mut report = CleanupReport::default();
        let mut records = Vec::new();
        for path in &plan.trash {
            if let Ok(bytes) = remove_crawl(path, &Removal::Trash(&os_trash)) {
                files.forget(path);
                report.removed.push(path.clone());
                report.freed_bytes += bytes;
                records.push(CleanupRecord {
                    at: Timestamp::now().to_string(),
                    path: path.clone(),
                    bytes,
                    reason: CleanupReason::AutoAge,
                });
            }
        }
        // Scheduled runs beyond a schedule's keep-last limit. Never the crawl
        // open here or one another process is writing; with no trash, they stay.
        let open = open_paths(app);
        for (schedule, runs) in crate::schedules::retention_sets() {
            let exists = |p: &Path| p.is_file();
            let doomed = retention::plan(&runs, schedule.keep_last, &schedule.output_dir, &exists);
            for path in doomed {
                if open.contains(&path) || WriterLock::held_by_other(&path).is_some() {
                    continue;
                }
                if let Ok(bytes) = remove_crawl(&path, &Removal::Trash(&os_trash)) {
                    files.forget(&path);
                    report.freed_bytes += bytes;
                    records.push(CleanupRecord {
                        at: Timestamp::now().to_string(),
                        path: path.clone(),
                        bytes,
                        reason: CleanupReason::ScheduleRetention,
                    });
                    report.schedule_removed.push(path);
                }
            }
        }
        for path in &plan.leftovers {
            let bytes = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
            if std::fs::remove_file(path).is_ok() {
                report.leftovers += 1;
                report.freed_bytes += bytes;
                records.push(CleanupRecord {
                    at: Timestamp::now().to_string(),
                    path: path.clone(),
                    bytes,
                    reason: CleanupReason::Leftover,
                });
            }
        }
        if !plan.announce.is_empty() {
            report.announced = plan.announce.clone();
        }
        let _ = files.storage.record(&records);
        let today = now.to_string().get(..10).map(str::to_string);
        let announce_now = !plan.announce.is_empty();
        let _ = files.storage.update(|p| {
            p.last_cleanup = today;
            if announce_now && p.first_cleanup_announced.is_none() {
                p.first_cleanup_announced = Some(now.to_string());
            }
        });
        report
    })();
    CLEANING.store(false, Ordering::SeqCst);
    if !report.removed.is_empty()
        || !report.schedule_removed.is_empty()
        || report.leftovers > 0
        || !report.announced.is_empty()
    {
        let _ = app.emit("cleanup-done", report.clone());
        let _ = app.emit("crawls-changed", ());
    }
    report
}

/// Runs cleanup now (tests, and the palette's hidden command).
#[tauri::command]
pub async fn cleanup_run_now(app: AppHandle) -> Cmd<CleanupReport> {
    blocking(move || Ok(run_cleanup(&app))).await
}

/// After launch recovery: cleanup once, then at most once per day while the
/// app runs (plan D8). The window may still be loading at launch, so the
/// first run waits a moment for it to listen.
pub fn after_recovery(app: &AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_secs(2));
        run_cleanup(&app);
        loop {
            std::thread::sleep(std::time::Duration::from_secs(3600));
            let today = now().to_string().get(..10).map(str::to_string);
            let last = app.state::<crate::files::State>().storage.prefs().last_cleanup;
            if last != today {
                run_cleanup(&app);
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    fn crawl_with_helpers(dir: &Path) -> PathBuf {
        let path = dir.join("a.spiderlens");
        for p in [
            path.clone(),
            dir.join("a.spiderlens-wal"),
            dir.join("a.spiderlens.lock"),
        ] {
            std::fs::write(p, b"xx").unwrap();
        }
        path
    }

    #[test]
    fn us2_2_trash_takes_the_crawl_and_its_helpers() {
        let dir = tempfile::tempdir().unwrap();
        let path = crawl_with_helpers(dir.path());
        let taken = RefCell::new(Vec::new());
        let trash = |p: &[PathBuf]| {
            for f in p {
                std::fs::remove_file(f).unwrap();
                taken.borrow_mut().push(f.clone());
            }
            Ok(())
        };
        assert_eq!(remove_crawl(&path, &Removal::Trash(&trash)), Ok(6));
        assert_eq!(taken.borrow().len(), 3);
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
    }

    #[test]
    fn us2_3_no_trash_removes_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let path = crawl_with_helpers(dir.path());
        let trash = |_: &[PathBuf]| Err("no trash on this drive".to_string());
        assert_eq!(
            remove_crawl(&path, &Removal::Trash(&trash)),
            Err(RemoveError::NoTrash)
        );
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 3);
        // The user confirmed a permanent delete.
        assert_eq!(remove_crawl(&path, &Removal::Permanent), Ok(6));
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
    }

    #[test]
    fn a_missing_crawl_is_reported() {
        let dir = tempfile::tempdir().unwrap();
        let r = remove_crawl(&dir.path().join("gone.spiderlens"), &Removal::Permanent);
        assert!(matches!(r, Err(RemoveError::Failed(_))));
    }
}
