//! Crawl data management (spec 019): what a crawl costs on disk, removing
//! its stored HTML, and which files automatic cleanup may remove.
//!
//! The file as a document is `file`'s business (spec 006); this module adds
//! the end of its life. Trash and scheduling belong to the shell (plan D4,
//! D8), so nothing here deletes a crawl: [`plan_cleanup`] only decides.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use jiff::{SignedDuration, Timestamp};
use rusqlite::{Connection, OptionalExtension};
use serde::Serialize;

use super::file::{self, FileError, WriterLock};
use super::store::{CrawlStore, StoreError, connect_read_only};

/// The crawl file extension.
pub const EXT: &str = "spiderlens";

/// Helper files older than this with no crawl are leftovers (FR-019).
pub const LEFTOVER_AGE: SignedDuration = SignedDuration::from_hours(24);

/// Room kept free on top of the compacted file's size (plan D2).
pub const COMPACT_MARGIN: u64 = 64 * 1024 * 1024;

const HELPER_SUFFIXES: [&str; 3] = ["-wal", "-shm", ".lock"];

// ---------------------------------------------------------------------------
// Files that make up a crawl

/// The crawl file and every helper file next to it that exists: SQLite's
/// `-wal` and `-shm`, the writer lock and an interrupted copy (plan D4).
pub fn crawl_files(path: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if path.exists() {
        out.push(path.to_path_buf());
    }
    for suffix in HELPER_SUFFIXES {
        let p = with_suffix(path, suffix);
        if p.exists() {
            out.push(p);
        }
    }
    let partial = file::partial_path(path);
    if partial.exists() {
        out.push(partial);
    }
    out
}

/// Size of the crawl and its helper files.
pub fn crawl_files_size(path: &Path) -> u64 {
    crawl_files(path).iter().map(|p| size_of(p)).sum()
}

fn with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(suffix);
    path.with_file_name(name)
}

fn size_of(p: &Path) -> u64 {
    std::fs::metadata(p).map(|m| m.len()).unwrap_or(0)
}

/// A copy 006 keeps before upgrading a crawl to a newer format.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Backup {
    pub path: PathBuf,
    pub format_version: i64,
    pub modified: Option<String>,
    pub size_bytes: u64,
}

/// `Some((stem, version))` when `path` is named like a 006 backup:
/// `<stem> (format N backup).spiderlens` or `<stem> (format N backup K).spiderlens`.
pub fn backup_name(path: &Path) -> Option<(String, i64)> {
    if path.extension()? != EXT {
        return None;
    }
    let stem = path.file_stem()?.to_str()?;
    let open = stem.rfind(" (format ")?;
    let inner = stem[open + " (format ".len()..].strip_suffix(')')?;
    let (version, rest) = inner.split_once(" backup")?;
    if !(rest.is_empty() || rest.trim_start().parse::<u32>().is_ok()) {
        return None;
    }
    Some((stem[..open].to_string(), version.parse().ok()?))
}

/// Backups of the crawl at `path`, oldest format first (FR-024).
pub fn backups_of(path: &Path) -> Vec<Backup> {
    let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
        return Vec::new();
    };
    let dir = parent_dir(path);
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out: Vec<Backup> = entries
        .flatten()
        .map(|e| e.path())
        .filter_map(|p| {
            let (s, version) = backup_name(&p)?;
            (s == stem).then(|| Backup {
                size_bytes: size_of(&p),
                modified: modified(&p).map(|t| t.to_string()),
                format_version: version,
                path: p,
            })
        })
        .collect();
    out.sort_by(|a, b| (a.format_version, &a.path).cmp(&(b.format_version, &b.path)));
    out
}

fn parent_dir(path: &Path) -> &Path {
    path.parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."))
}

fn modified(p: &Path) -> Option<Timestamp> {
    std::fs::metadata(p).ok()?.modified().ok().and_then(to_timestamp)
}

fn to_timestamp(t: SystemTime) -> Option<Timestamp> {
    Timestamp::try_from(t).ok()
}

// ---------------------------------------------------------------------------
// Stored HTML (plan D1 to D3)

/// How much stored page HTML a crawl holds.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct HtmlSize {
    /// Bytes on disk (compressed).
    pub bytes: u64,
    pub pages: u64,
}

fn html_size_on(conn: &Connection) -> Result<HtmlSize, StoreError> {
    let has: bool = conn.query_row(
        "SELECT EXISTS (SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'bodies')",
        [],
        |r| r.get(0),
    )?;
    if !has {
        return Ok(HtmlSize::default());
    }
    // length() of a BLOB comes from the record header: the HTML itself is
    // not read (plan D3).
    let (pages, bytes): (i64, i64) = conn.query_row(
        "SELECT COUNT(*), COALESCE(SUM(length(body)), 0) FROM bodies",
        [],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    Ok(HtmlSize {
        bytes: bytes.max(0) as u64,
        pages: pages.max(0) as u64,
    })
}

/// Stored HTML size without changing the file.
pub fn html_size(path: &Path) -> Result<HtmlSize, StoreError> {
    html_size_on(&connect_read_only(path)?)
}

/// What removing the stored HTML would do, for the confirmation (US3.1,
/// US3.4).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PurgeCheck {
    pub html_bytes: u64,
    pub html_pages: u64,
    pub file_bytes: u64,
    /// Expected file size after compaction.
    pub new_size: u64,
    pub free_bytes: Option<u64>,
    /// Enough free space to compact (FR-015).
    pub can_compact: bool,
}

pub fn purge_check(path: &Path) -> Result<PurgeCheck, StoreError> {
    let html = html_size(path)?;
    let file_bytes = file::file_size(path);
    let new_size = file_bytes.saturating_sub(html.bytes);
    let free_bytes = file::free_space(path);
    Ok(PurgeCheck {
        html_bytes: html.bytes,
        html_pages: html.pages,
        file_bytes,
        new_size,
        free_bytes,
        can_compact: free_bytes.is_none_or(|f| f >= new_size + COMPACT_MARGIN),
    })
}

/// What a purge did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PurgeReport {
    pub html_pages: u64,
    pub html_bytes: u64,
    pub freed_bytes: u64,
    pub compacted: bool,
}

/// Removes all stored page HTML from the crawl at `path`, then compacts it
/// when `compact` is set (plan D1, D2). Takes the writer lock, so it fails
/// with [`FileError::Lock`] while the crawl runs or another process has it.
/// Every other table is left exactly as it was (FR-013). The caller closes
/// its own read connections first: Windows cannot replace an open file.
pub fn purge_html(path: &Path, compact: bool) -> Result<PurgeReport, FileError> {
    let _lock = WriterLock::acquire(path)?;
    let before = file::file_size(path);
    let html = {
        let store = CrawlStore::open(path)?;
        let conn = store.conn();
        let html = html_size_on(conn)?;
        conn.execute_batch("BEGIN IMMEDIATE")?;
        let removed = (|| -> Result<(), StoreError> {
            if html.pages > 0 {
                conn.execute_batch(
                    "DELETE FROM bodies;
                     UPDATE pages SET html_stored = 0 WHERE html_stored = 1;",
                )?;
            }
            store.record_event(
                "html_purged",
                Some(
                    &serde_json::json!({
                        "bytes": html.bytes,
                        "pages": html.pages,
                        "compacted": compact,
                    })
                    .to_string(),
                ),
            )?;
            Ok(())
        })();
        match removed {
            Ok(()) => conn.execute_batch("COMMIT")?,
            Err(e) => {
                let _ = conn.execute_batch("ROLLBACK");
                return Err(e.into());
            }
        }
        html
        // The store closes here: the log is folded back into the file.
    };
    if compact {
        compact_file(path)?;
    }
    Ok(PurgeReport {
        html_pages: html.pages,
        html_bytes: html.bytes,
        freed_bytes: before.saturating_sub(file::file_size(path)),
        compacted: compact,
    })
}

/// Rewrites the crawl without free pages through 006's atomic write: until
/// the rename, the original stays the valid file (plan D2).
fn compact_file(path: &Path) -> Result<(), StoreError> {
    file::write_atomically(path, |partial| {
        let conn = Connection::open(path)?;
        conn.execute("VACUUM INTO ?1", [partial.to_string_lossy()])?;
        drop(conn);
        // Same journal mode as every crawl file (006 D1).
        let out = Connection::open(partial)?;
        out.pragma_update(None, "journal_mode", "WAL")?;
        drop(out);
        Ok(())
    })?;
    // The old file's log and index belonged to the replaced inode.
    for suffix in ["-wal", "-shm"] {
        let p = with_suffix(path, suffix);
        if p.exists() && size_of(&p) == 0 {
            let _ = std::fs::remove_file(p);
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Last use (plan D5)

/// When the crawl was last used: the recent list's date when the app knows
/// it, else the newest event in the file, else the file's modified time. A
/// time after `now` counts as `now` (clock changes).
pub fn last_used(path: &Path, recent: Option<&str>, now: Timestamp) -> Option<Timestamp> {
    let t = recent
        .and_then(|s| s.parse::<Timestamp>().ok())
        .or_else(|| newest_event(path))
        .or_else(|| modified(path))?;
    Some(t.min(now))
}

fn newest_event(path: &Path) -> Option<Timestamp> {
    let conn = connect_read_only(path).ok()?;
    let ms: Option<i64> = conn
        .query_row("SELECT MAX(at) FROM events", [], |r| r.get(0))
        .optional()
        .ok()
        .flatten()
        .flatten();
    Timestamp::from_millisecond(ms?).ok()
}

// ---------------------------------------------------------------------------
// Automatic cleanup (plan D5, D6)

/// What a file found by the cleanup scan is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Crawl,
    /// A 006 migration backup: never removed automatically (FR-024).
    Backup,
    /// `-wal`, `-shm`, `.lock` or `.partial`. `orphan`: its crawl file does
    /// not exist (always true for `.partial`, whose age decides).
    Helper {
        orphan: bool,
    },
    /// Anything else: never touched (edge case "a user's own file").
    Other,
}

/// One file the scan found, with everything the decision needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    pub path: PathBuf,
    pub kind: Kind,
    /// In the app's folder for unsaved crawls.
    pub in_unsaved_dir: bool,
    /// Crawls only: see [`last_used`].
    pub last_used: Option<Timestamp>,
    pub modified: Option<Timestamp>,
    /// Another process (or this one) holds the writer lock.
    pub locked: bool,
    /// Open in the app window, running or paused there.
    pub open: bool,
    /// Automatic cleanup removed this path before and the user brought it
    /// back (edge case: restored from the trash).
    pub auto_removed_before: bool,
    pub size_bytes: u64,
}

/// The user's cleanup setting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CleanupRule {
    /// Unsaved crawls not used for more days than this are removed; None:
    /// automatic removal of crawls is off (leftovers are still removed).
    pub days: Option<u32>,
    /// The one-time notice was shown (FR-020).
    pub announced: bool,
}

/// What cleanup will do. Built by [`plan_cleanup`] alone.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CleanupPlan {
    /// Unsaved crawls to move to the trash with their helper files.
    pub trash: Vec<PathBuf>,
    /// Leftover helper files to remove.
    pub leftovers: Vec<PathBuf>,
    /// Crawls that would be removed if the one-time notice had been shown:
    /// show it now, remove them on a later run.
    pub announce: Vec<PathBuf>,
}

impl CleanupPlan {
    pub fn is_empty(&self) -> bool {
        self.trash.is_empty() && self.leftovers.is_empty() && self.announce.is_empty()
    }
}

fn older_than(t: Option<Timestamp>, now: Timestamp, age: SignedDuration) -> bool {
    // An unknown or future time is never old.
    t.is_some_and(|t| t <= now && now.duration_since(t) > age)
}

/// The one place that decides what automatic cleanup removes (SC-003). A
/// crawl is removed only when it is an unsaved crawl in the app folder, not
/// locked, not open, never auto-removed before, and unused for longer than
/// the rule's days; and only once the one-time notice was shown. Saved
/// crawls, files outside the app folder, backups and other files are never
/// in the plan. Leftovers are helper files with no crawl, unlocked, older
/// than 24 hours.
pub fn plan_cleanup(candidates: &[Candidate], rule: CleanupRule, now: Timestamp) -> CleanupPlan {
    let mut plan = CleanupPlan::default();
    for c in candidates {
        if c.locked || c.open {
            continue;
        }
        match c.kind {
            Kind::Crawl => {
                let Some(days) = rule.days else { continue };
                if !c.in_unsaved_dir || c.auto_removed_before {
                    continue;
                }
                let age = SignedDuration::from_hours(24 * i64::from(days));
                if !older_than(c.last_used, now, age) {
                    continue;
                }
                if rule.announced {
                    plan.trash.push(c.path.clone());
                } else {
                    plan.announce.push(c.path.clone());
                }
            }
            Kind::Helper { orphan: true } => {
                if older_than(c.modified, now, LEFTOVER_AGE) {
                    plan.leftovers.push(c.path.clone());
                }
            }
            Kind::Helper { orphan: false } | Kind::Backup | Kind::Other => {}
        }
    }
    plan
}

/// What the scan needs from the app.
pub struct ScanContext<'a> {
    pub unsaved_dir: &'a Path,
    /// Folders of crawls in the recent list (searched for leftovers only).
    pub other_dirs: &'a [PathBuf],
    /// The recent list's last-used date per crawl.
    pub recent_last_used: &'a dyn Fn(&Path) -> Option<String>,
    /// Crawls open, running or paused in the app.
    pub open: &'a HashSet<PathBuf>,
    /// Paths automatic cleanup removed before.
    pub auto_removed: &'a HashSet<PathBuf>,
    pub now: Timestamp,
}

/// Looks at the unsaved-crawls folder and the folders of recent crawls (not
/// below them, never a whole disk) and describes every file there.
pub fn scan(ctx: &ScanContext<'_>) -> Vec<Candidate> {
    let mut dirs = vec![ctx.unsaved_dir.to_path_buf()];
    for d in ctx.other_dirs {
        if !dirs.iter().any(|x| file::same_path(x, d)) {
            dirs.push(d.clone());
        }
    }
    let mut out = Vec::new();
    for (i, dir) in dirs.iter().enumerate() {
        let in_unsaved_dir = i == 0;
        let Ok(entries) = std::fs::read_dir(dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !entry.file_type().is_ok_and(|t| t.is_file()) {
                continue;
            }
            let kind = classify(&path);
            // Outside the app folder only helper files are of interest.
            if !in_unsaved_dir && !matches!(kind, Kind::Helper { .. }) {
                continue;
            }
            let locked = match kind {
                Kind::Crawl => file::WriterLock::held_by_other(&path).is_some(),
                Kind::Helper { .. } => {
                    owner_of(&path).is_some_and(|c| file::WriterLock::held_by_other(&c).is_some())
                }
                _ => false,
            };
            let open = match kind {
                Kind::Crawl => ctx.open.iter().any(|o| file::same_path(o, &path)),
                Kind::Helper { .. } => {
                    owner_of(&path).is_some_and(|c| ctx.open.iter().any(|o| file::same_path(o, &c)))
                }
                _ => false,
            };
            out.push(Candidate {
                last_used: match kind {
                    Kind::Crawl => last_used(&path, (ctx.recent_last_used)(&path).as_deref(), ctx.now),
                    _ => None,
                },
                modified: modified(&path),
                auto_removed_before: ctx.auto_removed.contains(&path),
                size_bytes: size_of(&path),
                in_unsaved_dir,
                kind,
                locked,
                open,
                path,
            });
        }
    }
    out
}

/// The crawl file a helper file belongs to.
pub fn owner_of(helper: &Path) -> Option<PathBuf> {
    let name = helper.file_name()?.to_str()?;
    if let Some(inner) = name.strip_prefix('.').and_then(|n| n.strip_suffix(".partial")) {
        return Some(helper.with_file_name(inner));
    }
    HELPER_SUFFIXES
        .iter()
        .find_map(|s| name.strip_suffix(s))
        .filter(|base| base.ends_with(&format!(".{EXT}")))
        .map(|base| helper.with_file_name(base))
}

fn classify(path: &Path) -> Kind {
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or_default();
    if let Some(owner) = owner_of(path) {
        let partial = name.starts_with('.') && name.ends_with(".partial");
        return Kind::Helper {
            orphan: partial || !owner.exists(),
        };
    }
    if path.extension().is_some_and(|x| x == EXT) && !name.starts_with('.') {
        if backup_name(path).is_some() {
            return Kind::Backup;
        }
        return Kind::Crawl;
    }
    Kind::Other
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backup_names() {
        assert_eq!(
            backup_name(Path::new("/d/site (format 2 backup).spiderlens")),
            Some(("site".into(), 2))
        );
        assert_eq!(
            backup_name(Path::new("/d/site (format 3 backup 4).spiderlens")),
            Some(("site".into(), 3))
        );
        assert_eq!(
            backup_name(Path::new("/d/site (format x backup).spiderlens")),
            None
        );
        assert_eq!(backup_name(Path::new("/d/site.spiderlens")), None);
        assert_eq!(backup_name(Path::new("/d/site (format 2 backup).txt")), None);
    }

    #[test]
    fn helper_owners() {
        let d = Path::new("/d");
        assert_eq!(
            owner_of(&d.join("a.spiderlens-wal")),
            Some(d.join("a.spiderlens"))
        );
        assert_eq!(
            owner_of(&d.join("a.spiderlens-shm")),
            Some(d.join("a.spiderlens"))
        );
        assert_eq!(
            owner_of(&d.join("a.spiderlens.lock")),
            Some(d.join("a.spiderlens"))
        );
        assert_eq!(
            owner_of(&d.join(".a.spiderlens.partial")),
            Some(d.join("a.spiderlens"))
        );
        assert_eq!(owner_of(&d.join("notes.txt.lock")), None);
        assert_eq!(owner_of(&d.join("a.spiderlens")), None);
    }
}
