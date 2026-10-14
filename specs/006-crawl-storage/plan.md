# Implementation Plan: Crawl Storage and Crawl Files

**Branch**: `006-crawl-storage` | **Date**: 2026-10-13 | **Spec**: [spec.md](./spec.md)

## Summary

Much of this spec already exists because earlier specs needed it: every
crawl is a SQLite file written in 200 ms transactions (spec 008), the queue
lives in the file (spec 003), pause, stop and resume work from the CLI and
the app, and the results table reads 1M-URL files without loading them
(spec 007). What is missing is everything around the file as a *document*:
who may write it, what happens after a crash, saving it somewhere else,
finding it again, failing safely when the disk fills, and keeping old files
openable as the format grows.

This plan adds, in order of the spec's priorities:

1. A writer lock and crash recovery (US1, US3.5).
2. Save As that moves a live crawl without stopping it, and quit-while-
   crawling handling (US2, US3).
3. Format versioning with migration steps, an `events` table for version
   and history records, read-only and damaged-file handling (US6).
4. Disk reserve and write-failure pauses that leave the file resumable (US5).
5. Recent crawls, unsaved crawls, file association and drag-and-drop in the
   app; `spiderlens info` in the CLI (US4, FR-039).

The hot path (one coordinator thread writing batches) does not change, so
crawl speed must stay where it is. Baseline measured before any change on
main `f3a4d3d`, 4-core container, release build, bench site 100,000 pages,
Fast: **31.4 to 32.9 s, peak RSS 32 MB, file 320 MB.**

## Technical Context

- Rust 2024, rusqlite (bundled SQLite) in WAL mode with
  `synchronous=NORMAL`, background passive checkpoints every 250 ms.
- Tauri 2 shell, React UI, Vitest + Testing Library (UI tests run against a
  fake transport, `ui/src/testing/fakeTransport.ts`).
- New dependency: `fs4` (free disk space on all three OSes; D6). File locks
  use `std::fs::File::try_lock`, stable since Rust 1.89, so no lock crate.
- No new crate. File-level operations go in `crates/engine/src/crawl/file.rs`
  and `migrate.rs`; the recent list goes in `crates/config/src/recent.rs`
  next to the profile store; the app only wires them (principle III).

## Constitution Check

| Principle | How this plan complies |
|---|---|
| I. Spec before code | Spec approved 2026-10-11; this plan and tasks are a separate PR. |
| II. Local-first | Files stay on the user's disk; no sync, no service. |
| III. Engine independent of shell | Locking, recovery, Save As, migration, disk checks and info live in the engine; the CLI gets them too (`info`, `--disk-reserve`, exit code 6). The app adds only windows, menus and file association. |
| IV. Respectful crawling | Resume restores per-host robots and crawl-delay state from the file (exists). |
| V. Correct and complete data | Every write is a transaction; a failed batch is rolled back so no URL is ever marked crawled without its result (FR-028). |
| VI. Scales by design | Nothing new is loaded into memory; opening reads only `meta`; recovery is one indexed UPDATE. Speed checked against the baseline above. |
| VII. Test-first | Each acceptance scenario maps to a Rust or Vitest test (tasks.md), including a real kill -9 test and a disk-full test. |
| VIII. Simplicity | One small dependency; SQLite features instead of a custom format. |

No violations.

## Design decisions

### D1. The crawl file stays one SQLite database

SQLite is already the format, and it covers the spec's hard parts for free:
atomic batches (FR-004), a portable byte format that is identical on every
OS and CPU (FR-019), random access so a 1M-URL file opens in milliseconds
(FR-022). The extension stays `.spiderlens`.

While a crawl is open, SQLite keeps two helper files next to it (`-wal`,
`-shm`); when the last connection closes cleanly they are folded back and
deleted, so a closed crawl is exactly one file. Save As and the backup copy
(D5, D4) always produce a single, fully checkpointed file.

Analogy: think of it like a `.docx`, which is a ZIP, except our "ZIP" can
be appended to in place, safely, thousands of times a second.

### D2. One writer at a time: an OS lock on a sidecar file

FR-020 says two crawls must never write the same file. Options:

| Option | Problem |
|---|---|
| SQLite `locking_mode=EXCLUSIVE` | Blocks readers too, but the results table reads the file live while the crawl writes. |
| A "locked by pid X" row in `meta` with a heartbeat | Needs timeouts and pid checks to tell a crash from a slow writer; wrong after sleep or on another machine. |
| **OS advisory lock on `<file>.lock`** | Released by the kernel when the process dies, crash or not. |

We take the lock with `File::try_lock()` on `<name>.spiderlens.lock` and
hold it for as long as the crawl may write (running, or open with a
`Crawler`). The lock file holds a small JSON note (app version, pid, host
name, since) used only for the message "in use by SpiderLens 0.2 on
mac-studio since 14:02". The file's existence means nothing; only the
kernel lock counts, so a lock file left by a crash never blocks anyone.
It is deleted on clean close.

Analogy: like a `flock()` in Unix daemons or `FileShare.None` in .NET, but
on a sidecar so SQLite's own byte-range locks on the database stay
untouched (on Windows a whole-file lock on the database would block
SQLite's readers).

Readers never take the lock. A second app instance opening a file in use
gets it read-only with a banner (FR-020 allows either).

### D3. Crash detection and recovery fall out of D2

`meta.status` says `running` while a crawl runs. If we can take the lock on
a file whose status is `running`, the process that wrote it is gone:
that is the definition of "not closed cleanly" (FR-013). Recovery is:

1. `UPDATE urls SET state='queued' WHERE state='in_flight'` (exists as
   `requeue_for_resume`, indexed by `urls_state`).
2. `status = 'paused'`, `recovered = 1`, an `events` row `recovered`.

Uncommitted work was never in the file (transactions), so nothing is half
written and nothing is duplicated (FR-004, US1.5). The app runs this check
at launch over the unsaved-crawls folder and the recent list; the CLI does
it on `--resume` and `info`.

How much can be lost (FR-014, 5 s): a process kill loses only the open
batch, at most 200 ms. A power cut can lose the WAL frames not yet synced;
with `synchronous=NORMAL` the WAL is synced at every checkpoint, which our
background thread runs every 250 ms, so about half a second. We keep
`NORMAL` rather than `FULL` (fsync on every commit), which would cost speed
for a margin the spec does not ask for.

### D4. Format versions as a list of migration steps

Today `meta.format_version` is a number and readers refuse anything newer.
That breaks FR-037 (open newer files read-only when safe) and gives no
path for 004 and later specs to add columns. New rule:

```rust
pub struct Step { pub version: i64, pub additive: bool, pub sql: &'static str }
pub const STEPS: &[Step] = &[
    Step { version: 3, additive: true, sql: "CREATE TABLE events (...)" },
    // spec 004 adds step 4, and so on
];
```

- The format-2 schema (spec 005) stays the base. A new file is base plus
  every step, so new and upgraded files are identical by construction.
- `meta.format_version` = last applied step. `meta.read_compat` = the
  oldest format whose reader can still read the file: unchanged by
  additive steps (new tables, new nullable columns), raised by a breaking
  step. This is the same idea as SQLite's own header having separate
  "write version" and "read version" bytes.
- Opening for writing: allowed only if `format_version <= LATEST`.
  Opening for viewing: allowed if `read_compat <= LATEST`, otherwise refused
  with "made by SpiderLens X or later" (from the `events` table).
- Upgrading (resume of an older file): the app asks first and keeps an
  untouched copy `<name> (format 2 backup).spiderlens` unless the user
  declines (FR-036). The CLI keeps the copy by default; `--no-backup`
  declines. Viewing never writes: viewers open with `SQLITE_OPEN_READ_ONLY`.
- Format-1 files stay view-only (spec 005 decision).

Analogy: database migrations in Rails or Flyway, applied lazily per file
instead of once per server, plus a compatibility marker like protobuf's
"unknown fields are fine" for additive changes.

### D5. Save As moves a live crawl between batches

FR-015 wants Save As at any time, with a running crawl continuing into the
new file. The crawl loop owns the only write connection, so Save As is a
message to it (a new `mpsc` command channel next to the pause/stop
`watch`): at the next batch boundary the loop commits, then:

1. `PRAGMA wal_checkpoint(TRUNCATE)` so the database file alone is complete
   (falls back to SQLite's online backup API if a reader holds the log).
2. Copy to `.<name>.spiderlens.partial` in the target folder, `fsync`,
   then rename over the target. A rename within one folder is atomic on all
   three OSes, so an existing file is replaced only by a complete one, and
   a crash mid-copy leaves only a `.partial` that is never listed (FR-016,
   edge case "closed during Save As").
3. Reopen the write connection and the lock at the new path; delete the
   old copy if it was an unsaved crawl (it "moved"), keep it otherwise
   (classic Save As semantics).

The crawl pauses for the copy only. `std::fs::copy` uses `clonefile` on
macOS APFS, so on the owner's Mac a same-volume Save As is near-instant
regardless of size; elsewhere it is disk speed (about 1 s per GB on SSD).
Fetches keep running meanwhile; their results queue in the channel.

When no crawl is running, Save As is the same copy under the lock.
Overwrite confirmation is the native save dialog's job (FR-016).

### D6. Disk reserve and write failures pause, never crash

- **Reserve (FR-027)**: every 2 s the loop asks the OS for free space on the
  crawl file's volume (`fs4::available_space`, one `statvfs` /
  `GetDiskFreeSpaceExW` call). Below the reserve (default 1 GB, a machine
  setting, `--disk-reserve` in the CLI) it pauses with a `disk_low` reason.
- **Write failure (FR-028)**: any SQLite error from the write connection
  ends the run as `CrawlError::Local(LocalError)`. The open batch is rolled
  back, so URLs in it go back to their last committed state (queued or in
  flight, both requeued on resume) and none is marked crawled. The loop
  then tries, best effort, to write `status = paused`, `pause_reason` and an
  `events` row `local_error`; if even that fails the file still says
  `running` and the next open treats it as a crash (D3), which is also
  correct.
- `LocalError { kind, message, path, free_bytes }`, where `kind` is
  `disk_low`, `disk_full` (`SQLITE_FULL`), `drive_gone` (`SQLITE_IOERR`,
  `SQLITE_CANTOPEN`, file missing), `permission` (`SQLITE_READONLY`,
  `SQLITE_PERM`) or `other`. Messages are plain language (FR-029). The CLI
  exits with code **6** (FR-040). Local errors are never URL errors
  (FR-030): they live in `events`, not `urls.error_class`.
- Testing disk full without a full disk: `PRAGMA max_page_count` makes
  SQLite return `SQLITE_FULL` at a chosen size, the same error a full disk
  gives.

### D7. History and versions in one `events` table (format step 3)

```sql
CREATE TABLE events (id INTEGER PRIMARY KEY, at INTEGER NOT NULL,
  kind TEXT NOT NULL, app_version TEXT NOT NULL, os TEXT NOT NULL, detail TEXT);
```

Kinds: `created`, `resumed`, `paused`, `stopped`, `completed`, `recovered`,
`saved`, `upgraded`, `config_changed`, `local_error`. The version records
of FR-032 are the distinct `app_version`/`os` pairs of `created`,
`resumed` and `upgraded` rows; no separate table. A few rows per run, so no
speed cost. Files without the table (format 2) show the creating version
from `meta.config.app_version`.

### D8. Resume may change speed and limits only

`StoredCrawlConfig` already records mid-crawl changes and can only express
speed and retry changes. We add `Change::MaxUrls(Option<u64>)`; depth
limits are scope (FR-011), so they stay fixed. The app's resume sheet shows
speed and the URL limit; the CLI accepts `--speed`, `--concurrency`,
`--per-host`, `--rate` and `--max-urls` with `--resume` and refuses the
rest as today. Each change adds a `config_changed` event.

Secrets (FR-034) are already never written (`Secret(None)` on load). On
resume, if a header's secret is missing, the app asks for it and the CLI
takes `--header-secret NAME=VALUE` (or refuses with the header names).

### D9. Recent crawls and unsaved crawls in the app

- Unsaved crawls live in `<app data>/crawls/` (exists). "Unsaved" means
  "in that folder". The app shows their total size and can delete one with
  confirmation (FR-023). There is no automatic deletion.
- `recent.json` in the app config folder: up to 20 entries
  `{path, last_used, name, start_url, urls, status, recovered}`. The cached
  fields are refreshed from the file's `meta` (a read-only open, a few
  rows) when the list is shown; a missing file shows as unavailable with
  Locate and Remove (FR-026). Removing an entry never deletes the file.
- Start screen and command palette (the app has no menu bar yet) show the
  list; one click opens.

### D10. Opening files: four doors, one command

Open dialog (⌘O), recent list, double-click in Finder or Explorer, and
drop on the window all call one `crawl_open(path)` command that returns the
file's info, status and whether it can be resumed, viewed only, or needs an
upgrade, Save As (read-only media) or recovery (damaged).

- File association: `bundle.fileAssociations` in `tauri.conf.json`
  (`.spiderlens`, role Editor). macOS delivers the path as
  `RunEvent::Opened`; Windows and Linux pass it as an argument at launch.
  The shell forwards both to the UI as an `open-file` event.
- Drag and drop: Tauri's webview drag-drop event, handled in the UI.
- Opening while a crawl runs: the UI asks to pause it first (edge case).

### D11. Read-only and damaged files

- Read-only (permissions, read-only media): viewers already open read-only;
  on media where SQLite cannot create its `-shm` file we retry with
  `immutable=1`. `crawl_open` reports `writable: false`, and resume offers
  Save As first (FR-021).
- Damaged (FR-038): we do not run `PRAGMA integrity_check` on open (it reads
  the whole file, too slow for FR-022). Corruption shows up as
  `SQLITE_CORRUPT`/`SQLITE_NOTADB` from the first read; then the app shows
  "This crawl file is damaged" with **Recover what can be read**: the engine
  copies table by table in rowid order into a new unsaved crawl, skipping
  ranges it cannot read, and opens that copy marked recovered.

### D12. Quitting with a crawl running

The shell intercepts window close and app exit while a crawl runs: it
prevents the close, tells the UI ("Pausing the crawl so you can resume it
later…"), sends Pause, and exits when the crawl reports paused (FR-012).
A second quit while waiting exits at once; the file is still consistent
(D3).

### D13. Command line

- `spiderlens info FILE [--json]`: configuration, status, counts, versions,
  events, file size, lock state (FR-039).
- `crawl --resume FILE`: recovery and upgrade as above; speed and URL-limit
  options allowed (D8); `--no-backup`; `--disk-reserve MB`.
- Exit code 6: paused by a disk or write problem; the file stays resumable.

## Performance plan

No change on the per-page write path. Added per run: one lock file, one
`events` row per state change, a free-space syscall every 2 s. Check after
implementation with the same bench (3 runs before and after, alternated)
and record in `benchmarks.md`. Memory flatness (SC-002) is measured as RSS
at 10k vs 100k crawled.

## Testing plan

- Rust unit tests per module (`file.rs`, `migrate.rs`, `recent.rs`).
- Integration tests in `crates/engine/tests/storage.rs` against the local
  fixture site: kill -9 a running CLI crawl and recover (SC-001), pause /
  reopen / resume equals uninterrupted (SC-003), Save As while running,
  disk full via `max_page_count` then resume (SC-006), lock refusal, newer
  format read-only, damaged file salvage.
- Reference files `crates/engine/tests/reference/*.spiderlens` (a completed
  and a paused format-2 crawl made by `main` before this change) with
  expected counts, opened by CI on all three OSes (SC-007).
- CI portability job: each OS writes a crawl file, every OS opens the other
  two (SC-008).
- Vitest: recent list, unavailable entries, unsaved crawls and delete,
  crawl info panel, Save As, upgrade prompt, disk/write error notice, quit
  notice, drag-and-drop, open-file event.

## Project structure (changes)

```text
crates/engine/src/crawl/
  file.rs        lock, recovery, save_as, info, salvage, disk check
  migrate.rs     STEPS, apply, compat checks
  store.rs       open_read_only, events, relocate
  runner.rs      command channel (Save As), disk check, LocalError pause
crates/engine/tests/storage.rs, tests/reference/
crates/config/src/recent.rs, stored.rs (Change::MaxUrls)
crates/results/src/reader.rs   immutable=1 fallback
crates/cli/src/main.rs, crawl_cmd.rs   info, --disk-reserve, exit 6
app/src-tauri/src/files.rs     recent, open, save_as, unsaved, recovery at launch
app/src-tauri/src/crawl.rs     save-as while running, quit handling, local errors
app/src-tauri/tauri.conf.json  fileAssociations
ui/src/files/                  RecentCrawls, CrawlInfo, notices, hooks, tests
```
