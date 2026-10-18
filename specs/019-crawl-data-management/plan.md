# Implementation Plan: Crawl Data Management and Cleanup

**Branch**: `019-crawl-data-management` | **Date**: 2026-10-13 | **Spec**: [spec.md](./spec.md)

## Summary

Spec 006 made the crawl file a safe document. This plan adds the rest of
the document's life: seeing every crawl and what it costs, deleting to the
trash, shrinking a crawl by dropping its stored HTML, and a narrow automatic
cleanup for data the user never chose to keep.

Most of it is file-system work around files 006 already defines, so the
plan is mainly about where each decision lives:

1. **Engine** (`crates/engine/src/crawl/manage.rs`, new): helper-file
   discovery, stored-HTML size, HTML purge with compaction, leftover
   detection, and the cleanup decision as a pure function.
2. **Config** (`crates/config/src/storage.rs`, new): storage preferences
   (cleanup age, safety reserve, one-time notice) and the cleanup history.
3. **App** (`app/src-tauri/src/manage.rs`, new): trash, the startup and
   daily cleanup run, and the commands the UI calls.
4. **UI**: a Manage crawls sheet, Settings › Storage, and the cleanup
   notices. **CLI**: `spiderlens files` and `spiderlens purge-html`.

**No file format change.** Stored HTML lives in the `bodies` table with a
`pages.html_stored` flag (spec 005); purging is a `DELETE` and an `UPDATE`
inside one transaction plus an `events` row of a new kind, all within the
current schema. So 019 takes **no migration step** and does not collide with
004 (Step 4) or JS rendering (next step).

The crawl hot path does not change. The only crawl-time change is that the
disk reserve comes from a setting instead of a constant.

## Technical Context

- Builds on 006 (PR #27): `file.rs` (writer lock, `write_atomically`,
  `free_space`, `remove_crawl_file`, backups), `config/src/recent.rs`,
  `app/src-tauri/src/files.rs` (`unsaved_list`, `unsaved_delete`,
  `storage_status`). Code starts from main after #27 merges.
- New dependency: `trash` (moves files to the macOS Trash, Windows Recycle
  Bin and the freedesktop trash on Linux). App crate only; the engine and
  CLI stay free of it (principle III, D4).
- Rust 2024, rusqlite bundled; Tauri 2; React, Vitest, Playwright.

## Constitution Check

| Principle | How this plan complies |
|---|---|
| I. Spec before code | Spec 019 approved 2026-10-13; plan and tasks are this separate docs PR. |
| II. Local-first | Everything is local files; trash is the OS trash. |
| III. Engine independent of shell | Size, purge, leftovers and the cleanup decision live in the engine and are used by the CLI; trash and scheduling are the shell's. |
| IV. Respectful crawling | Not affected. |
| V. Correct and complete data | Purge is one transaction; results are compared byte for byte before and after in tests (FR-013). Saved crawls are never removed automatically, enforced in one pure function with exhaustive tests (SC-003). |
| VI. Scales by design | Listing reads file metadata and `meta` only; HTML size is one aggregate query loaded lazily. |
| VII. Test-first | Every acceptance scenario maps to a Rust, Vitest or Playwright test in tasks.md. |
| VIII. Simplicity | No schema change, one small dependency, no background service. |

No violations.

## Design decisions

### D1. Purging stored HTML needs no format change

The bytes are in `bodies (url_id, codec, size, body)`; `pages.html_stored`
says whether a page has them. A purge is:

```sql
BEGIN IMMEDIATE;
DELETE FROM bodies;
UPDATE pages SET html_stored = 0 WHERE html_stored = 1;
INSERT INTO events (kind = 'html_purged', detail = '{"bytes":…,"pages":…}');
COMMIT;
```

SQLite's transaction makes it all-or-nothing (FR-014): a crash before
`COMMIT` leaves the HTML, a crash after leaves none. Every other table is
untouched, so results and exports are identical (FR-013). When JS rendering
adds its rendered-HTML table, it adds one `DELETE` to this list.

Old readers see a crawl with no stored HTML, which is already a valid state
(Store page HTML off), and an `events` kind they don't know, which the info
window shows as plain text. So no `read_compat` change either.

### D2. Compaction with `VACUUM INTO` and the 006 atomic write

`DELETE` frees pages inside the file but does not shrink it. Two ways to
give the space back:

| Option | Extra disk needed | Crash safety |
|---|---|---|
| `VACUUM` in place | up to 2 × the new size (temp copy, then written back through the WAL) | SQLite's own |
| `VACUUM INTO '<hidden partial>'`, then rename over the original | 1 × the new size | 006 `write_atomically`: until the rename, the original is the valid file |

We take `VACUUM INTO` through `write_atomically`, the same path Save As
uses, so there is one crash-safe "replace a crawl file" in the codebase.
Before starting, the app checks `free_space >= new_size + 64 MB` where
`new_size = file_size − html_bytes` (FR-015); if not, it offers the purge
without compaction (the freed pages are reused by later writes of the same
crawl).

Order: purge transaction first, then compaction. A crash between them
leaves a valid, purged, uncompacted file, which is acceptance scenario
US3.3's "without the stored HTML".

The writer lock is held throughout; the results view's read connection for
that file is closed first (as `unsaved_delete` does), because Windows
cannot rename over an open file.

Analogy: like `git gc` after deleting branches. The objects are already
unreachable after the delete; `gc` is what returns the bytes to the disk.

### D3. Stored-HTML size without reading the HTML

`SELECT COUNT(*), COALESCE(SUM(length(body)), 0) FROM bodies`. For a BLOB
column SQLite reads the length from the record header and does not load
overflow pages, so this walks only the table's b-tree leaf pages. Task T004
measures it on a 100k-page crawl with HTML stored; target under 300 ms. If
it is slower, fallback: `SUM(size)` on a covering index is not available
(size is the uncompressed size), so we would cache the total in `meta` at
the end of each run; we expect not to need it.

The manager loads sizes from file metadata first (instant) and HTML sizes
afterwards, one file at a time (FR-004).

### D4. Trash belongs to the shell

Options for "delete to the trash":

| Option | Problem |
|---|---|
| Implement per OS (NSFileManager, IFileOperation, freedesktop spec) | Three code paths, unsafe FFI, freedesktop edge cases (per-drive trash dirs) |
| `trash` crate | One small, widely used dependency covering all three |

`trash` goes in the app crate only. The engine provides
`crawl_files(path) -> Vec<PathBuf>` (the crawl plus its `-wal`, `-shm`,
`.lock` and `.partial` files) and the app trashes that list together
(FR-008).

When trashing fails (network drive, no trash on that volume), the command
returns `TrashUnavailable { paths }` without deleting anything; the UI then
shows a second confirmation that says the delete is permanent, and only on
that confirmation calls `crawls_delete_permanently`. So the user is always
told before a permanent delete (US2.3), without guessing in advance which
volumes have a trash.

The existing start-screen delete (006 `unsaved_delete`) switches to the same
path (FR-011).

### D5. The cleanup decision is a pure function

The riskiest rule in this spec is "never remove a saved crawl" (SC-003). It
lives in one function with no I/O:

```rust
pub fn plan_cleanup(candidates: &[Candidate], rule: &CleanupRule, now: Timestamp) -> CleanupPlan
```

`Candidate` carries what the scan found (path, `in_unsaved_dir`, is a crawl,
last used, locked, open, running, previously auto-removed). `CleanupPlan`
lists crawls to trash, leftovers to remove, and crawls to announce (the
first-run notice, FR-020). The scanner builds candidates and the app
executes the plan; neither decides anything. Tests enumerate every
combination of the flags (a small truth table, ~200 cases) and assert no
saved, outside, locked, open or running file is ever in the plan.

Like a Kubernetes reconcile loop split into "compute desired actions" and
"apply them": the compute half is deterministic and fully testable.

**Last used** = the recent list's `last_used` if present, else the newest
`events.at` in the file, else the file's modified time; a time in the future
counts as now (edge case: clock jumps).

### D6. Leftovers

A helper file is a leftover when the crawl it belongs to does not exist and
it is older than 24 hours:

- `<name>.spiderlens.lock` with no `<name>.spiderlens`, and the lock is not
  held (006 `WriterLock::held_by_other` is `None`);
- `<name>.spiderlens-wal` / `-shm` with no `<name>.spiderlens`;
- `.<name>.spiderlens.partial` (a finished copy is renamed away; one older
  than 24 h is from a crash).

Leftovers are looked for only in the unsaved-crawls folder and in folders
of crawls in the recent list (never a full disk scan). They are removed
permanently, not trashed: they hold no user data a person could open.

The root causes (006 `remove_crawl_file` not removing `.lock`; `.partial`
not cleaned after a crash) are reported to the Storage thread and fixed in
006's code; 019 cleans what older builds already left behind.

### D7. Storage preferences and history live with the config crate

App-level, not crawl-profile, settings (like appearance, spec 018, but read
by Rust at startup and by the crawl runner, so not in the UI's local
storage):

`<config dir>/storage.json`

```json
{ "version": 1, "cleanup_days": 30, "reserve_bytes": 1073741824, "first_cleanup_announced": null }
```

`cleanup_days: null` means off. Written beside and renamed, like
`recent.json`. The history is `cleanup-history.jsonl`, one record per line,
trimmed to the last 500 on write (FR-022).

The reserve is passed to the runner with the existing
`with_disk_reserve(Some(bytes))`; 006's start-up and pre-crawl warnings
read the same value.

### D8. When cleanup runs

At launch, after 006's `recover_at_launch` (so recovered crawls count as
used now), and then on a one-hour timer that runs it at most once per
calendar day. A lock file `cleanup.lock` in the config folder (006
`WriterLock` reused) keeps two app instances from running it at once.
Cleanup runs on a blocking thread; the window is never held.

### D9. UI

- **Manage crawls** is a sheet like Settings (spec 017 pattern), opened
  from File, the command palette (`Manage crawls…`) and a link next to the
  start screen's unsaved-crawls total. A table: name, location, last used,
  created, URLs, status, size, HTML size; sort, filter chips (All /
  Unsaved / Saved / Unavailable), checkbox selection, a bulk action bar
  (Delete, Remove stored HTML) and a row menu (Open, Show in folder, Crawl
  information, Save As, Remove stored HTML, Remove from list, Delete).
  Backups appear as indented child rows. Totals in the header.
- **Settings › Storage** (an `app: true` category, like Appearance):
  cleanup switch and age select, safety reserve input with range check,
  unsaved-folder path with Show in folder, and Cleanup history (a small
  table).
- **Notices**: after a cleanup that removed something, and the one-time
  first-run notice, reuse the toast/notice pattern of 006's
  `LocalErrorNotice`.
- The 006 low-space warning gets an "Open Manage crawls" button (FR-023).

UI tests are mandatory (owner rule): Vitest for every component against the
fake transport, Playwright for the sheet's keyboard and selection flows.

### D10. CLI

- `spiderlens files [DIR]`: lists `.spiderlens` files in DIR (default: the
  current folder) with size, HTML size, URL count, status, last used; `--json`.
- `spiderlens purge-html FILE [--no-compact] [--yes]`: same engine call as
  the app; without `--yes` it prints what would be freed and asks. Exit
  code 6 (006: local error) when the disk is too full to compact.
- No automatic cleanup in the CLI (FR-026).

### D11. Keep the last N runs of a schedule

Spec 021 saves one crawl file per scheduled run and asked this spec for
retention. The owner chose: per schedule, default 10, configurable 1 to 999
or keep all (US6, FR-027 to FR-033).

- **Field.** `Schedule.keep_last: Option<u32>` in `crates/schedule`
  (`None` = keep all). `Schedule::new` and the app's new-schedule form set
  `Some(10)`. The field is `#[serde(default)]`, so a schedule file written
  before this change has no field and reads as `None`. Nothing existing is
  deleted unless the user chooses a limit. Validation: 1 to 999 (`validate.rs`).
- **Pure decision.** `crates/schedule/src/retention.rs`:
  `plan(runs: &[Run], keep_last: Option<u32>, exists: impl Fn(&Path) -> bool, in_dir: impl Fn(&Path) -> bool) -> Vec<PathBuf>`.
  It returns the files to remove, nothing else. A file counts only if it is
  `Run.file`, exists, and is inside the schedule's output folder; each path
  once. Sorted by run number, newest first; the newest N are kept and the rest
  are returned. `None` or too few files returns empty. Runs still in progress
  are never returned. The checks are closures so the truth table needs no disk.
- **Where it runs.** In the desktop app's existing cleanup pass
  (`run_cleanup`, D8): after launch recovery, then hourly (at most once a
  day). After the age and leftover steps, the app loads each schedule, calls
  `retention::plan`, then drops any path that is open, running, or whose
  `WriterLock` is held (the same `scan` facts as D5). The rest go through the
  existing `remove_crawl(path, Removal::Trash(..))`, which takes the crawl and
  its `-wal`, `-shm`, `.lock` and `.partial` helpers.
- **Trash unavailable.** `remove_crawl` returns the error, the file stays,
  and the next pass plans it again. There is no permanent-delete fallback.
- **History and notice.** `CleanupReason::ScheduleRetention`
  (`schedule_retention`) is a new record reason. `CleanupReport` gains
  `retention_removed: u32`; the notice adds "N older scheduled runs were
  moved to the Trash (each schedule keeps its last runs)."
- **Engine and CLI.** `spiderlens-engine` and `spiderlens-cli` still do not
  depend on `trash` (T001). The CLI only stores and shows the rule:
  `spiderlens schedule add/edit --keep-last N|all`, shown in `schedule show`.
  It never removes run files, like FR-026.
- **No format change.** Crawl files are untouched, so no migrate step. The
  schedule record gains one optional field. Nothing runs on the crawl hot path.

Run files of a deleted schedule become ordinary saved crawls (spec edge case).

## Performance

- Crawl speed: unchanged code path; the 100k bench is run once to confirm
  33 to 35 s (006 number).
- Manager listing: 200 files in under 2 s (SC-006), measured in an engine
  test with 200 small files and a 2 GB sparse file.
- HTML size query: under 300 ms on a 100k-page crawl (D3).
- Purge: 100k pages with HTML, measured; expected dominated by
  `VACUUM INTO` (roughly the time to write the new file).

## Project structure

```text
crates/engine/src/crawl/manage.rs     # crawl_files, html_size, purge_html, leftovers, plan_cleanup
crates/engine/tests/manage.rs         # purge identity, crash points, leftovers, truth table
crates/config/src/storage.rs          # StoragePrefs, CleanupHistory
crates/cli/src/files_cmd.rs           # files, purge-html
crates/schedule/src/retention.rs      # keep-last decision (pure, D11)
app/src-tauri/src/manage.rs           # trash, cleanup run, commands
ui/src/files/ManageCrawls.tsx         # sheet + tests
ui/src/files/useManagedCrawls.ts
ui/src/settings/StorageSettings.tsx   # + tests
ui/src/files/CleanupNotice.tsx        # + tests
ui/e2e/app.spec.ts                    # manage crawls flows
```

## Risks

- **Trash on Linux CI**: the `trash` crate uses `$XDG_DATA_HOME/Trash`; CI
  sets `XDG_DATA_HOME` to a temp folder in the test. On Windows and macOS
  runners the real trash is used by one ignored-by-default test plus one run
  in CI with a unique file name.
- **Windows rename over an open file** (D2): every handle on the file is
  closed before compaction; a test holds a read handle and expects a clear
  "in use" error, not a half state.
- **Overlap with 006**: 006 owns `file.rs`; 019 adds `manage.rs` next to it
  and changes `unsaved_delete` only to call the trash path.
