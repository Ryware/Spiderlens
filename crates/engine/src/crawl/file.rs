//! The crawl file as a document (spec 006): who may write it, recovery after
//! a crash, Save As, information about it, damaged files and disk problems.
//!
//! The data inside is `store`'s business; this module deals with the file.

use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use spiderlens_config::{Mode, ModeInput, StoredCrawlConfig};

use super::migrate;
use super::reason::UrlState;
use super::store::{CrawlStore, FORMAT_VERSION, StoreError, connect_read_only};

/// Default free space kept on the crawl file's drive (FR-027).
pub const DEFAULT_DISK_RESERVE: u64 = 1024 * 1024 * 1024;

// ---------------------------------------------------------------------------
// Writer lock (plan D2)

/// Who holds a crawl file, for the "in use" message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LockNote {
    pub app_version: String,
    pub pid: u32,
    pub host: String,
    pub since: String,
}

impl LockNote {
    fn mine() -> Self {
        Self {
            app_version: super::store::APP_VERSION.into(),
            pid: std::process::id(),
            host: host_name(),
            since: jiff::Timestamp::now().to_string(),
        }
    }

    /// Placeholder when the holder's note cannot be read (Windows keeps a
    /// locked file unreadable to other processes).
    fn unknown() -> Self {
        Self {
            app_version: String::new(),
            pid: 0,
            host: String::new(),
            since: String::new(),
        }
    }

    pub fn describe(&self) -> String {
        if self.pid == 0 {
            "another SpiderLens window or process".into()
        } else {
            format!(
                "SpiderLens {} (process {}) on {} since {}",
                self.app_version, self.pid, self.host, self.since
            )
        }
    }
}

fn host_name() -> String {
    std::env::var("HOSTNAME")
        .or_else(|_| std::env::var("COMPUTERNAME"))
        .ok()
        .or_else(|| {
            std::fs::read_to_string("/etc/hostname")
                .ok()
                .map(|s| s.trim().to_string())
        })
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "this computer".into())
}

/// `<file>.lock` next to the crawl file.
pub fn lock_path(crawl: &Path) -> PathBuf {
    let mut name = crawl.file_name().unwrap_or_default().to_os_string();
    name.push(".lock");
    crawl.with_file_name(name)
}

/// The right to write one crawl file. The operating system releases it when
/// the process ends, crash or not, so a lock file left behind never blocks
/// anyone; only the kernel lock counts.
#[derive(Debug)]
pub struct WriterLock {
    file: Option<File>,
    path: PathBuf,
}

#[derive(Debug, thiserror::Error)]
pub enum LockError {
    #[error("this crawl is in use by {}", .0.describe())]
    InUse(LockNote),
    #[error("could not lock the crawl file: {0}")]
    Io(#[from] std::io::Error),
}

impl WriterLock {
    pub fn acquire(crawl: &Path) -> Result<Self, LockError> {
        let path = lock_path(crawl);
        for _ in 0..3 {
            let mut file = match OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .truncate(false)
                .open(&path)
            {
                Ok(f) => f,
                // Windows refuses to open a lock file that is being deleted.
                Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied && cfg!(windows) => {
                    std::thread::sleep(std::time::Duration::from_millis(50));
                    continue;
                }
                Err(e) => return Err(e.into()),
            };
            match file.try_lock() {
                Ok(()) => {}
                Err(std::fs::TryLockError::WouldBlock) => {
                    let mut s = String::new();
                    let note = file
                        .read_to_string(&mut s)
                        .ok()
                        .and_then(|_| serde_json::from_str(&s).ok())
                        .unwrap_or_else(LockNote::unknown);
                    return Err(LockError::InUse(note));
                }
                Err(std::fs::TryLockError::Error(e)) => return Err(e.into()),
            }
            // The previous holder deletes its lock file on release; if it did
            // so between our open and our lock, we hold a lock on a file that
            // no longer has this name. Try again with the current one.
            if !same_file(&file, &path) {
                continue;
            }
            file.set_len(0)?;
            file.write_all(
                serde_json::to_string(&LockNote::mine())
                    .expect("note serializes")
                    .as_bytes(),
            )?;
            file.flush()?;
            return Ok(Self {
                file: Some(file),
                path,
            });
        }
        Err(LockError::Io(std::io::Error::other(
            "the lock file keeps changing",
        )))
    }

    /// True when another process holds the crawl.
    pub fn held_by_other(crawl: &Path) -> Option<LockNote> {
        let path = lock_path(crawl);
        let mut file = OpenOptions::new().read(true).write(true).open(&path).ok()?;
        match file.try_lock() {
            Ok(()) => None,
            Err(std::fs::TryLockError::WouldBlock) => {
                let mut s = String::new();
                Some(
                    file.read_to_string(&mut s)
                        .ok()
                        .and_then(|_| serde_json::from_str(&s).ok())
                        .unwrap_or_else(LockNote::unknown),
                )
            }
            Err(_) => None,
        }
    }
}

impl Drop for WriterLock {
    fn drop(&mut self) {
        // Deleted while still locked, so nobody can lock this name and then
        // see it removed under them.
        let _ = std::fs::remove_file(&self.path);
        if let Some(f) = self.file.take() {
            let _ = f.unlock();
        }
    }
}

#[cfg(unix)]
fn same_file(file: &File, path: &Path) -> bool {
    use std::os::unix::fs::MetadataExt;
    match (file.metadata(), std::fs::metadata(path)) {
        (Ok(a), Ok(b)) => a.dev() == b.dev() && a.ino() == b.ino(),
        _ => false,
    }
}

#[cfg(not(unix))]
fn same_file(_file: &File, path: &Path) -> bool {
    path.exists()
}

// ---------------------------------------------------------------------------
// Recovery (plan D3)

/// Recovers a crawl that was not closed cleanly: rows left in flight go back
/// to the queue, the crawl becomes paused and is marked recovered. The
/// caller holds the writer lock. Returns how many URLs were requeued, or
/// None when the crawl was closed cleanly.
pub fn recover(store: &CrawlStore) -> Result<Option<usize>, StoreError> {
    if store.meta("status")?.as_deref() != Some("running") {
        return Ok(None);
    }
    store.begin()?;
    let n = store.requeue_in_flight()?;
    store.set_meta("status", "paused")?;
    store.set_meta("recovered", "1")?;
    store.set_meta("pause_reason", "recovered")?;
    store.record_event("recovered", Some(&format!(r#"{{"requeued":{n}}}"#)))?;
    store.commit()?;
    Ok(Some(n))
}

/// Opens, locks and recovers a crawl file if it needs it, then releases it.
/// Used at app launch and when a file is opened for viewing. Returns the
/// number of URLs requeued when it was recovered; None when nothing was done
/// (closed cleanly, in use, or not writable here).
pub fn recover_path(path: &Path) -> Result<Option<usize>, StoreError> {
    {
        let ro = CrawlStore::open_read_only(path)?;
        if ro.meta("status")?.as_deref() != Some("running") || ro.format_version()? > FORMAT_VERSION {
            return Ok(None);
        }
    }
    if !writable(path) {
        return Ok(None);
    }
    let Ok(_lock) = WriterLock::acquire(path) else {
        return Ok(None);
    };
    let store = CrawlStore::open(path)?;
    recover(&store)
}

/// Whether this process may write the file (not read-only media, not
/// read-only permissions).
pub fn writable(path: &Path) -> bool {
    OpenOptions::new().append(true).open(path).is_ok()
}

// ---------------------------------------------------------------------------
// Copies: Save As and backups (plan D5)

/// Writes `dest` through a temporary file in the same folder that is synced
/// and then renamed over `dest`, so `dest` is only ever replaced by a
/// complete file and an interrupted copy leaves only a hidden `.partial`.
pub fn write_atomically(
    dest: &Path,
    write: impl FnOnce(&Path) -> Result<(), StoreError>,
) -> Result<(), StoreError> {
    let partial = partial_path(dest);
    let _ = std::fs::remove_file(&partial);
    let result = write(&partial).and_then(|()| {
        // Windows flushes only through a handle opened for writing.
        std::fs::OpenOptions::new()
            .write(true)
            .open(&partial)?
            .sync_all()?;
        std::fs::rename(&partial, dest)?;
        Ok(())
    });
    if result.is_err() {
        let _ = std::fs::remove_file(&partial);
        return result;
    }
    #[cfg(unix)]
    if let Some(dir) = dest.parent().filter(|d| !d.as_os_str().is_empty()) {
        let _ = File::open(dir).and_then(|d| d.sync_all());
    }
    Ok(())
}

/// `.<name>.partial` in the same folder.
pub fn partial_path(dest: &Path) -> PathBuf {
    let mut name = std::ffi::OsString::from(".");
    name.push(dest.file_name().unwrap_or_default());
    name.push(".partial");
    dest.with_file_name(name)
}

/// Copies the committed contents of the crawl open on `conn` (at `src`) to
/// `to` as one self-contained file. Empties the write-ahead log first and
/// copies the file (an APFS clone on macOS); if a reader holds the log, uses
/// SQLite's online backup instead.
pub(crate) fn copy_database(store: &CrawlStore, src: &Path, to: &Path) -> Result<(), StoreError> {
    if store.checkpoint_truncate().unwrap_or(false) {
        std::fs::copy(src, to)?;
        return Ok(());
    }
    backup_into(store.conn(), to)
}

fn backup_into(src: &Connection, to: &Path) -> Result<(), StoreError> {
    let mut dst = Connection::open(to)?;
    {
        let backup = rusqlite::backup::Backup::new(src, &mut dst)?;
        backup.run_to_completion(4096, std::time::Duration::ZERO, None)?;
    }
    // A copy is one file: fold any log back in before it is renamed.
    dst.pragma_update(None, "journal_mode", "DELETE")?;
    dst.pragma_update(None, "journal_mode", "WAL")?;
    drop(dst);
    Ok(())
}

/// Save As for a crawl that is not running (FR-015, FR-016). The original is
/// left as it is; the caller deletes it when it was an unsaved crawl.
pub fn save_as(src: &Path, dest: &Path) -> Result<(), FileError> {
    if same_path(src, dest) {
        return Ok(());
    }
    let _dest_lock = WriterLock::acquire(dest)?;
    if writable(src) {
        let _lock = WriterLock::acquire(src)?;
        let store = CrawlStore::open(src)?;
        write_atomically(dest, |p| copy_database(&store, src, p))?;
    } else {
        let conn = connect_read_only(src)?;
        write_atomically(dest, |p| backup_into(&conn, p))?;
    }
    if writable(dest) {
        let store = CrawlStore::open(dest)?;
        store.record_event("saved", Some(&json_path(dest)))?;
    }
    Ok(())
}

pub(crate) fn json_path(p: &Path) -> String {
    serde_json::json!({ "path": p }).to_string()
}

pub fn same_path(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    }
}

/// Deletes a crawl file and its helper files. Only for crawls the user asked
/// to delete or that were moved by Save As.
pub fn remove_crawl_file(path: &Path) -> std::io::Result<()> {
    std::fs::remove_file(path)?;
    for suffix in ["-wal", "-shm"] {
        let mut name = path.file_name().unwrap_or_default().to_os_string();
        name.push(suffix);
        let _ = std::fs::remove_file(path.with_file_name(name));
    }
    Ok(())
}

/// `<stem> (format N backup).spiderlens`, numbered if taken (FR-036).
pub fn backup_path(path: &Path, version: i64) -> PathBuf {
    let stem = path.file_stem().unwrap_or_default().to_string_lossy().to_string();
    let first = path.with_file_name(format!("{stem} (format {version} backup).spiderlens"));
    if !first.exists() {
        return first;
    }
    (2..)
        .map(|n| path.with_file_name(format!("{stem} (format {version} backup {n}).spiderlens")))
        .find(|p| !p.exists())
        .expect("an unused name exists")
}

// ---------------------------------------------------------------------------
// Disk and write problems (plan D6)

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LocalErrorKind {
    DiskLow,
    DiskFull,
    DriveGone,
    Permission,
    Other,
}

impl LocalErrorKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::DiskLow => "disk_low",
            Self::DiskFull => "disk_full",
            Self::DriveGone => "drive_gone",
            Self::Permission => "permission",
            Self::Other => "other",
        }
    }
}

/// A problem with this computer, not with the website: the crawl pauses and
/// stays resumable (FR-028 to FR-030).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalError {
    pub kind: LocalErrorKind,
    pub message: String,
    pub path: PathBuf,
    pub free_bytes: Option<u64>,
}

impl std::fmt::Display for LocalError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} Crawl file: {}", self.message, self.path.display())
    }
}

/// Free space for the user on the drive holding `path` (or its folder).
pub fn free_space(path: &Path) -> Option<u64> {
    let dir = if path.is_dir() {
        path
    } else {
        path.parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."))
    };
    fs4::available_space(dir).ok()
}

/// Now, as an RFC 3339 timestamp.
pub fn now_string() -> String {
    jiff::Timestamp::now().to_string()
}

pub fn format_bytes(n: u64) -> String {
    const UNITS: [&str; 5] = ["bytes", "KB", "MB", "GB", "TB"];
    let mut v = n as f64;
    let mut u = 0;
    while v >= 1024.0 && u < UNITS.len() - 1 {
        v /= 1024.0;
        u += 1;
    }
    if u == 0 {
        format!("{n} bytes")
    } else {
        format!("{v:.1} {}", UNITS[u])
    }
}

impl LocalError {
    pub fn disk_low(path: &Path, free: u64, reserve: u64) -> Self {
        Self {
            kind: LocalErrorKind::DiskLow,
            message: format!(
                "The disk is nearly full: {} free, and SpiderLens keeps at least {} free. \
                 The crawl was paused before the disk filled up. Free some space or save the crawl \
                 to another drive, then resume.",
                format_bytes(free),
                format_bytes(reserve)
            ),
            path: path.to_path_buf(),
            free_bytes: Some(free),
        }
    }

    /// What a failed write means, in plain language.
    pub fn from_store(e: &StoreError, path: &Path) -> Self {
        use rusqlite::ErrorCode as C;
        let free = free_space(path);
        let code = match e {
            StoreError::Sql(s) => s.sqlite_error_code(),
            _ => None,
        };
        let io_kind = match e {
            StoreError::Io(io) => Some(io.kind()),
            _ => None,
        };
        let kind = match (code, io_kind) {
            (Some(C::DiskFull), _) | (_, Some(std::io::ErrorKind::StorageFull)) => LocalErrorKind::DiskFull,
            (Some(C::ReadOnly | C::PermissionDenied), _)
            | (_, Some(std::io::ErrorKind::PermissionDenied)) => LocalErrorKind::Permission,
            (Some(C::SystemIoFailure | C::CannotOpen | C::NotFound), _)
            | (_, Some(std::io::ErrorKind::NotFound)) => LocalErrorKind::DriveGone,
            _ if !path.exists() => LocalErrorKind::DriveGone,
            _ => LocalErrorKind::Other,
        };
        let message = match kind {
            LocalErrorKind::DiskFull => {
                "The disk is full. The crawl was paused and everything written so far is kept. \
                 Free some space or save the crawl to another drive, then resume."
                    .to_string()
            }
            LocalErrorKind::Permission => "SpiderLens is not allowed to write the crawl file (permission \
                 denied). The crawl was paused. Save it to another folder to continue."
                .to_string(),
            LocalErrorKind::DriveGone => "The crawl file can no longer be reached: the drive may have been \
                 disconnected or the network share lost. The crawl was paused. Reconnect the drive and \
                 resume, or save the crawl somewhere else."
                .to_string(),
            _ => format!("Writing the crawl file failed ({e}). The crawl was paused and can be resumed."),
        };
        Self {
            kind,
            message,
            path: path.to_path_buf(),
            free_bytes: free,
        }
    }
}

// ---------------------------------------------------------------------------
// Errors for file operations

#[derive(Debug, thiserror::Error)]
pub enum FileError {
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error(transparent)]
    Lock(#[from] LockError),
    #[error("{0}")]
    Io(#[from] std::io::Error),
}

impl From<rusqlite::Error> for FileError {
    fn from(e: rusqlite::Error) -> Self {
        Self::Store(e.into())
    }
}

// ---------------------------------------------------------------------------
// Crawl information (FR-032, FR-033, plan D13)

/// What may be done with a crawl file right now.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Access {
    Writable,
    InUse { by: LockNote },
    ReadOnlyMedia,
    NewerFormat { needs: String },
    NeedsUpgrade { from: i64 },
    ViewOnly { reason: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VersionRecord {
    pub kind: String,
    pub app_version: String,
    pub os: String,
    pub at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EventRow {
    pub at: String,
    pub kind: String,
    pub app_version: String,
    pub os: String,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct InfoCounts {
    pub discovered: u64,
    pub crawled: u64,
    /// Crawled URLs that failed or answered 4xx/5xx.
    pub failed: u64,
    pub not_crawled: u64,
    pub queued: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct CrawlInfo {
    pub path: PathBuf,
    pub name: String,
    pub size_bytes: u64,
    pub format_version: i64,
    pub status: String,
    pub pause_reason: Option<String>,
    pub recovered: bool,
    pub mode: Mode,
    pub start_url: Option<String>,
    pub config: Option<StoredCrawlConfig>,
    pub started_at: Option<String>,
    pub ended_at: Option<String>,
    pub counts: InfoCounts,
    pub versions: Vec<VersionRecord>,
    pub events: Vec<EventRow>,
    pub access: Access,
    pub missing_secrets: Vec<String>,
    /// Set by the app: the file is in its folder for unsaved crawls.
    pub unsaved: bool,
}

fn ms_to_string(ms: i64) -> String {
    jiff::Timestamp::from_millisecond(ms)
        .map(|t| t.to_string())
        .unwrap_or_default()
}

/// Everything about a crawl file without changing it. Reads only `meta`,
/// `events` and the per-state counts (an index), so it is quick on large
/// files.
pub fn info(path: &Path) -> Result<CrawlInfo, StoreError> {
    let store = CrawlStore::open_read_only(path)?;
    let conn = store.conn();
    let version = store.format_version()?;
    let status = store.meta("status")?.unwrap_or_else(|| "running".into());
    let config: Option<StoredCrawlConfig> = store.meta("config")?.and_then(|c| serde_json::from_str(&c).ok());
    let start_url = store.meta("start_url")?;
    let mode = config.as_ref().map_or(
        if start_url.is_some() {
            Mode::Spider
        } else {
            Mode::List
        },
        |c| match c.input {
            ModeInput::Spider { .. } => Mode::Spider,
            ModeInput::List(_) => Mode::List,
        },
    );
    let mut counts = InfoCounts::default();
    for (state, n) in store.state_counts()? {
        counts.discovered += n;
        match state {
            UrlState::Crawled => counts.crawled = n,
            UrlState::NotCrawled => counts.not_crawled = n,
            UrlState::Queued | UrlState::InFlight => counts.queued += n,
        }
    }
    counts.failed = store.page_errors()?;

    let has_events: bool = conn.query_row(
        "SELECT EXISTS (SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'events')",
        [],
        |r| r.get(0),
    )?;
    let mut events = Vec::new();
    if has_events {
        let mut stmt = conn.prepare("SELECT at, kind, app_version, os, detail FROM events ORDER BY id")?;
        let rows = stmt.query_map([], |r| {
            Ok(EventRow {
                at: ms_to_string(r.get(0)?),
                kind: r.get(1)?,
                app_version: r.get(2)?,
                os: r.get(3)?,
                detail: r.get(4)?,
            })
        })?;
        for r in rows {
            events.push(r?);
        }
    }
    let mut versions: Vec<VersionRecord> = events
        .iter()
        .filter(|e| matches!(e.kind.as_str(), "created" | "resumed" | "upgraded"))
        .map(|e| VersionRecord {
            kind: e.kind.clone(),
            app_version: e.app_version.clone(),
            os: e.os.clone(),
            at: e.at.clone(),
        })
        .collect();
    // Format 2 files have no history; the configuration names the version
    // that created them.
    if !versions.iter().any(|v| v.kind == "created")
        && let Some(c) = &config
    {
        versions.insert(
            0,
            VersionRecord {
                kind: "created".into(),
                app_version: c.app_version.clone(),
                os: String::new(),
                at: c.started_at.to_string(),
            },
        );
    }
    versions.dedup_by(|b, a| b.kind == a.kind && a.app_version == b.app_version && a.os == b.os);

    let ended_at = if status == "completed" || status == "stopped" {
        events
            .iter()
            .rev()
            .find(|e| e.kind == "completed" || e.kind == "stopped")
            .map(|e| e.at.clone())
    } else {
        None
    };
    let missing_secrets = config
        .as_ref()
        .map(|c| {
            c.config
                .identity
                .headers
                .iter()
                .filter(|h| h.value.is_secret() && h.value.expose().is_none())
                .map(|h| h.name.clone())
                .collect()
        })
        .unwrap_or_default();

    let access = if version > FORMAT_VERSION {
        Access::NewerFormat {
            needs: events
                .last()
                .map(|e| format!("SpiderLens {} or later", e.app_version))
                .unwrap_or_else(|| "a newer SpiderLens".into()),
        }
    } else if version < migrate::BASE {
        Access::ViewOnly {
            reason: "This crawl was made by an early SpiderLens build, before page data extraction. \
                     It can be viewed but not resumed."
                .into(),
        }
    } else if !writable(path) {
        Access::ReadOnlyMedia
    } else if let Some(by) = WriterLock::held_by_other(path) {
        Access::InUse { by }
    } else if version < FORMAT_VERSION {
        Access::NeedsUpgrade { from: version }
    } else {
        Access::Writable
    };

    Ok(CrawlInfo {
        path: path.to_path_buf(),
        name: store
            .meta("name")?
            .unwrap_or_else(|| path.file_stem().unwrap_or_default().to_string_lossy().to_string()),
        size_bytes: file_size(path),
        format_version: version,
        status,
        pause_reason: store.meta("pause_reason")?,
        recovered: store.meta("recovered")?.as_deref() == Some("1"),
        mode,
        start_url,
        started_at: config.as_ref().map(|c| c.started_at.to_string()),
        config,
        ended_at,
        counts,
        versions,
        events,
        access,
        missing_secrets,
        unsaved: false,
    })
}

/// The file's size including an unfolded write-ahead log.
pub fn file_size(path: &Path) -> u64 {
    let base = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    let mut wal = path.file_name().unwrap_or_default().to_os_string();
    wal.push("-wal");
    base + std::fs::metadata(path.with_file_name(wal))
        .map(|m| m.len())
        .unwrap_or(0)
}

// ---------------------------------------------------------------------------
// Damaged files (plan D11, FR-038)

/// Rows copied and rows lost per table by [`salvage`].
#[derive(Debug, Clone, Default, Serialize)]
pub struct SalvageReport {
    pub tables: Vec<(String, u64, bool)>,
}

/// Whether part of a damaged file can be read: the database header and the
/// list of tables are intact.
pub fn salvageable(path: &Path) -> bool {
    Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
        .and_then(|c| c.query_row("SELECT COUNT(*) FROM sqlite_master", [], |r| r.get::<_, i64>(0)))
        .is_ok_and(|n| n > 0)
}

/// Copies whatever can be read from a damaged crawl file into a new crawl
/// file at `dest`, table by table in row order, skipping rows it cannot
/// read. The copy is paused and marked recovered.
pub fn salvage(src: &Path, dest: &Path) -> Result<SalvageReport, FileError> {
    let from = Connection::open_with_flags(src, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let store = CrawlStore::create(dest)?;
    let to = store.conn();
    let tables: Vec<String> = {
        let mut stmt =
            to.prepare("SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%'")?;
        stmt.query_map([], |r| r.get(0))?.collect::<Result<_, _>>()?
    };
    let mut report = SalvageReport::default();
    to.execute_batch("BEGIN")?;
    for table in tables {
        let Ok(cols) = columns(&from, &table) else {
            report.tables.push((table, 0, false));
            continue;
        };
        let dest_cols = columns(to, &table)?;
        let cols: Vec<String> = cols.into_iter().filter(|c| dest_cols.contains(c)).collect();
        if cols.is_empty() {
            continue;
        }
        let list = cols
            .iter()
            .map(|c| format!("\"{c}\""))
            .collect::<Vec<_>>()
            .join(", ");
        let marks = (1..=cols.len())
            .map(|i| format!("?{i}"))
            .collect::<Vec<_>>()
            .join(", ");
        let verb = if table == "meta" {
            "INSERT OR REPLACE"
        } else {
            "INSERT OR IGNORE"
        };
        let mut insert = to.prepare(&format!("{verb} INTO \"{table}\" ({list}) VALUES ({marks})"))?;
        let mut copied = 0u64;
        let mut complete = true;
        let mut after = i64::MIN;
        let mut skip = 1i64;
        'chunks: loop {
            let sql =
                format!("SELECT rowid, {list} FROM \"{table}\" WHERE rowid > ?1 ORDER BY rowid LIMIT 512");
            // Rows are copied as they are read, so the rows before an
            // unreadable part are kept.
            let mut read = 0usize;
            let outcome: rusqlite::Result<()> = (|| {
                let mut stmt = from.prepare(&sql)?;
                let mut rows = stmt.query([after])?;
                while let Some(r) = rows.next()? {
                    let rowid: i64 = r.get(0)?;
                    let values: Vec<rusqlite::types::Value> = (1..=cols.len())
                        .map(|i| r.get(i))
                        .collect::<rusqlite::Result<_>>()?;
                    after = rowid;
                    read += 1;
                    if table == "meta" {
                        let key = values.first().and_then(|v| match v {
                            rusqlite::types::Value::Text(t) => Some(t.clone()),
                            _ => None,
                        });
                        if matches!(key.as_deref(), Some("format_version" | "read_compat" | "format")) {
                            continue;
                        }
                    }
                    if insert.execute(rusqlite::params_from_iter(values)).is_ok() {
                        copied += 1;
                    }
                }
                Ok(())
            })();
            match outcome {
                Ok(()) if read == 0 => break 'chunks,
                Ok(()) => skip = 1,
                Err(_) => {
                    complete = false;
                    // Jump past the unreadable part, further each time.
                    if read == 0 {
                        after = after.saturating_add(skip);
                        skip = skip.saturating_mul(8);
                    }
                    if skip > 1 << 40 || after == i64::MAX {
                        break 'chunks;
                    }
                }
            }
        }
        report.tables.push((table, copied, complete));
    }
    to.execute_batch("COMMIT")?;
    store.begin()?;
    store.requeue_in_flight()?;
    let status = store.meta("status")?;
    if status.as_deref() != Some("completed") {
        store.set_meta("status", "paused")?;
    }
    store.set_meta("recovered", "1")?;
    store.set_meta("pause_reason", "recovered")?;
    store.record_event(
        "recovered",
        Some(&serde_json::json!({ "salvaged_from": src, "tables": report.tables }).to_string()),
    )?;
    store.commit()?;
    Ok(report)
}

fn columns(conn: &Connection, table: &str) -> rusqlite::Result<Vec<String>> {
    let mut stmt = conn.prepare(&format!("PRAGMA table_info(\"{table}\")"))?;
    let cols: Vec<String> = stmt.query_map([], |r| r.get(1))?.collect::<Result<_, _>>()?;
    if cols.is_empty() {
        return Err(rusqlite::Error::QueryReturnedNoRows);
    }
    Ok(cols)
}

/// The value of one `meta` key without opening the file as a crawl.
pub fn read_meta(path: &Path, key: &str) -> Option<String> {
    let conn = connect_read_only(path).ok()?;
    conn.query_row("SELECT value FROM meta WHERE key = ?1", params![key], |r| {
        r.get(0)
    })
    .optional()
    .ok()
    .flatten()
}
