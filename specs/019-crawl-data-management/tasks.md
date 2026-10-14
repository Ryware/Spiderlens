# Tasks: Crawl Data Management and Cleanup

**Input**: [spec.md](./spec.md), [plan.md](./plan.md), [data-model.md](./data-model.md), [contracts/](./contracts/)

Tests come first in each phase (constitution VII). `[P]` = can run in parallel with the other `[P]` tasks of its phase. Code starts from main after spec 006 (#27) merges.

## Phase 1: Setup

- [ ] T001 Add `trash` to `app/src-tauri/Cargo.toml` only; CI check that `spiderlens-engine` and `spiderlens-cli` do not depend on it.
- [ ] T002 [P] Test helper in `crates/test-fixtures`: build a crawl file with N pages and stored HTML, plus helper files with chosen ages (`filetime`, dev-dependency).

## Phase 2: Foundation (engine and config)

- [ ] T003 [P] Tests then code: `manage::crawl_files(path)` returns the crawl and its existing `-wal`, `-shm`, `.lock`, `.partial` files; `backups_of(path)` returns migration backups with their format version (FR-024).
- [ ] T004 [P] Tests then code: `manage::html_size(path)` via `SUM(length(body))` on a read-only connection; benchmark on 100k pages, target under 300 ms (plan D3).
- [ ] T005 [P] Tests then code: `config::storage::StoragePrefs` load/save (atomic rename, damaged file reads as defaults) and range validation (256 MiB to 50 GiB, days in {7, 14, 30, 90, off}).
- [ ] T006 [P] Tests then code: `config::storage::CleanupHistory` append and trim to 500, newest first.
- [ ] T007 Tests then code: `manage::last_used(path, recent)`: recent entry, else newest `events.at`, else mtime; future times count as now.

## Phase 3: US1 See all crawl data (P1)

- [ ] T008 Tests then code: `manage::list(unsaved_dir, recent, added)` builds `ManagedCrawl` rows from metadata and `meta` only; unavailable entries keep last known size; non-crawl files in the unsaved folder go to `other_files`. Test: 200 files listed in under 2 s (SC-006).
- [ ] T009 App commands `crawls_manage_list`, `crawls_html_size`, `crawls_add`; the active crawl reports `running` and its live size.
- [ ] T010 [P] Vitest: `ManageCrawls` sheet renders rows, totals, lazy HTML size placeholder, sort, filter chips, unavailable rows with Locate / Remove from list (US1.1 to US1.5).
- [ ] T011 [P] Open the sheet from File menu, command palette and the start screen's unsaved total; Vitest for each entry point.

## Phase 4: US2 Delete safely (P1)

- [ ] T012 Tests then code (app): `crawls_delete` trashes each crawl with all its helper files; skips running, locked and other-window crawls with reasons; returns `trash_unavailable` without deleting (plan D4). Linux test with a temp `XDG_DATA_HOME`.
- [ ] T013 `crawls_delete_permanently`; both write cleanup history records.
- [ ] T014 `unsaved_delete` (006) switches to the trash path (FR-011); its 006 tests updated, not removed.
- [ ] T015 [P] Vitest: confirmation shows count, saved files marked, space freed; second confirmation for permanent delete; skipped reasons shown; deleting the open crawl closes it first (US2.1 to US2.6).
- [ ] T016 [P] Playwright: select several rows with keyboard and pointer, delete, confirm, rows disappear.

## Phase 5: US3 Remove stored HTML (P2)

- [ ] T017 Tests then code: `manage::purge_html(path, compact)` per plan D1/D2 under the writer lock; test that every results tab export is byte-identical before and after (FR-013, SC-002).
- [ ] T018 Crash tests: kill the process (as 006's kill -9 test) inside the purge transaction and between purge and compaction; the file opens complete each time (FR-014, US3.3).
- [ ] T019 Free-space check and `--no-compact` path (FR-015, US3.4); Windows test that an open read handle gives a clear "in use" error.
- [ ] T020 `html_purged` event shown in the crawl information window (FR-016); a resumed crawl stores HTML again (US3.6).
- [ ] T021 App commands `crawls_purge_html_check`, `crawls_purge_html`; refuse while running (US3.5); not offered for view-only or no-HTML crawls.
- [ ] T022 [P] Vitest: confirmation text, progress, result, unavailable states.
- [ ] T023 [P] CLI `spiderlens purge-html` and `spiderlens files` with tests in `crates/cli/tests/` (FR-026, contracts/cli.md).

## Phase 6: US4 Automatic cleanup (P2)

- [ ] T024 Tests then code: `manage::plan_cleanup` pure function; exhaustive truth table over location, kind, locked, open, running, age, previously removed, announced; asserts no saved, outside, locked, open or running file is ever planned (FR-018, SC-003).
- [ ] T025 Tests then code: `manage::scan_leftovers(dirs)` per plan D6 (FR-019); SC-004 test: 100 simulated crash leftovers, all gone after one run, no live helper touched.
- [ ] T026 App: run at launch after `recover_at_launch`, then hourly at most once per day, guarded by `cleanup.lock` (plan D8); first-run announce flow (FR-020); history records.
- [ ] T027 [P] Vitest: `CleanupNotice` for the one-time notice and the "removed N, freed X" notice with links (FR-021).

## Phase 7: US5 Backups and settings (P3)

- [ ] T028 Backups as child rows in the manager, open view-only, delete (US5.1).
- [ ] T029 Settings › Storage category (`app: true`): cleanup switch and age, reserve with range errors, folder path, cleanup history table (FR-025, US5.3); Vitest.
- [ ] T030 Reserve from `StoragePrefs` passed to the runner (`with_disk_reserve`) and to 006's warnings; test a crawl pausing at a 5 GB reserve with a faked free-space source (US5.2).
- [ ] T031 [P] 006 low-space warning gets "Open Manage crawls" (FR-023); Vitest.

## Phase 8: Polish

- [ ] T032 Playwright: Settings › Storage keyboard flow; manager filter and sort.
- [ ] T033 100k bench once to confirm crawl speed unchanged (33 to 35 s).
- [ ] T034 Update `docs/` user notes: where crawls live, what cleanup does and never does.
- [ ] T035 Run the quickstart on the Mac build.

## Dependencies

Phase 2 before all stories. US1 (T008, T009) before US2 and US3 UI tasks. US4 needs T007 and T012 (trash path). US5 is independent after Phase 2.
