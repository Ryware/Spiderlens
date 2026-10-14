# Tasks: Scheduled Crawls

**Input**: [spec.md](./spec.md), [plan.md](./plan.md), [data-model.md](./data-model.md), [contracts/](./contracts/)

Tests come first in each task (constitution VII). `[P]` = can run in parallel with the other `[P]` tasks of its phase.

## Phase 1: Setup

- [ ] T001 New crate `crates/schedule` in the workspace (default member); deps `spiderlens-config`, `spiderlens-engine`, `spiderlens-results`, `jiff`, `serde`, `serde_json`, `thiserror`, `tokio`; optional `notify-rust` behind feature `notify`.
- [ ] T002 [P] Engine: `Crawler::set_meta(key, value)`; test that `meta.schedule` round-trips and old readers ignore it.

## Phase 2: Foundation (pure logic)

- [ ] T003 [P] `cron.rs`: parse and validate (single minute value, ranges, steps, lists, `L`, Sunday 0/7, OR rule); error messages per field. Property test: `next_after` is strictly increasing and every result matches the expression.
- [ ] T004 [P] `cron.rs` DST: gap goes to the first valid minute after it, fold runs once (tests in `America/New_York` and `Europe/London` on the 2026 change dates); "never runs" for `0 0 31 2 *`.
- [ ] T005 [P] Frequency presets to cron and back (`hours`, `daily`, `weekly`, `monthly` with last day) and the display text.
- [ ] T006 [P] `due.rs`: Nothing / Scheduled / CaughtUp / Missed with a fake clock; several missed slots give one result; Run now does not move `last_slot` (FR-012, US1-5, US2-3).
- [ ] T007 [P] `naming.rs`: `<name> YYYY-MM-DD HHmm.spiderlens`, invalid characters replaced, ` (2)` suffix (FR-015).
- [ ] T008 [P] `store.rs`: load/save with lock + atomic rename; unique names; damaged file is an error that names the file, never silently emptied; ids stable.
- [ ] T009 [P] `history.rs`: append, newest-N read, last record per run wins; `running` with a free lock reads as Interrupted (D4).
- [ ] T010 Validation: invalid config, secret headers, missing output folder at save time (warning), frequency more often than hourly (FR-005, FR-006).

## Phase 3: US1 Run without the app (P1)

- [ ] T011 `runner.rs`: tick flow (D1 steps 1 to 4) and run flow (D5) against the fixture site; crawl file named and complete, `meta.schedule` set (US1-2, US1-4, SC-005).
- [ ] T012 Overlap: second tick while the lock is held records Skipped, exit 7 (FR-013, SC-004).
- [ ] T013 Output folder not writable: Failed before any request, no crawl file (FR-014).
- [ ] T014 Stop file ends the run as Stopped by user (FR-017).
- [ ] T015 [P] `os/mod.rs` trait `Scheduler { register, remove, inspect, list_ours }` and a fake; `os/launchd.rs` plist text, golden test, `launchctl` calls.
- [ ] T016 [P] `os/taskschd.rs` task XML (UTF-16 file as schtasks expects), golden test, `schtasks` calls; real register/inspect/remove test on the Windows CI runner.
- [ ] T017 [P] `os/systemd.rs` unit text, golden test, `systemctl --user` calls; crontab fallback with tagged lines that never touches other lines (test on a temp crontab via `SPIDERLENS_CRONTAB` override).
- [ ] T018 Real registration tests that skip with a printed reason when the runner has no user session (Linux, macOS).

## Phase 4: US4 Command line (P2) — before the UI so the UI and tests can use it

- [ ] T019 `spiderlens schedule` subcommands per contracts/cli.md, `--json` everywhere; config dir from `dirs` or `SPIDERLENS_CONFIG_DIR`.
- [ ] T020 CLI tests: add, list, show (5 next runs), edit, duplicate, enable, disable, remove, history, remove-all; app and CLI read the same file (FR-029).
- [ ] T021 `run-now` foreground with progress and exit codes (FR-030).
- [ ] T022 Tick benchmark: nothing due, under 50 ms.

## Phase 5: US2 History and notifications (P1)

- [ ] T023 Run record counts and step results; reason text for each failure class (FR-018, SC-007).
- [ ] T024 [P] Notifications via `notify-rust` per level; a test hook records instead of showing (FR-019).

## Phase 6: US3 After-run steps (P2)

- [ ] T025 CSV exports into `<run name> exports/`, one file per tab and filter, same bytes as `spiderlens export` (FR-020).
- [ ] T026 Step failure does not change the crawl status and later steps still run (FR-023, US3-4).
- [ ] T027 Report (010) and comparison (012) steps: offered only when those crates exist; tracked as follow-ups in those specs' threads (FR-021, FR-022, US3-5).

## Phase 7: Desktop app and UI

- [ ] T028 `app/src-tauri/src/schedules.rs` commands per contracts/desktop-commands.md; `--schedule-tick <id>` handled before Tauri starts (no window); startup repair (FR-025).
- [ ] T029 [P] UI `ui/src/schedules/`: Sidebar item, list with next run and last status, detail with history, editor sheet with presets and live next-5 preview, enable switch, Run now, Stop, delete with confirmation, Update from profile; fake transport support. Vitest for each acceptance scenario of US1, US2, US3 that has UI.
- [ ] T030 [P] "Schedule this crawl" from the toolbar more-menu and command palette prefills the editor (FR-002, SC-006: 5 interactions or fewer, counted in a Vitest test).
- [ ] T031 Playwright: create a schedule in the editor, see it in the list with its next run, open history, disable it.

## Phase 8: After spec 006 (#27) is on main

- [ ] T032 Disk reserve check before the first request, using 006's `free_space` (FR-014); `paused_disk` status and exit 6.
- [ ] T033 Scheduled runs in the recent crawls list marked as scheduled (FR-016).

## Phase 9: Polish

- [ ] T034 US5: registration repair after moving the program; Not registered with the OS reason; Remove all removes orphans (FR-024 to FR-026).
- [ ] T035 Docs: README section and `docs/scheduled-crawls.md` (what runs when, how to remove everything by hand).
- [ ] T036 `/speckit-converge` reports Converged; CI green on Windows, macOS, Linux.

## Coverage

| Spec item | Tasks |
|---|---|
| US1 | T011, T014, T015-T018, T028, T029 |
| US2 | T006, T009, T012, T023, T024, T029 |
| US3 | T025-T027, T029 |
| US4 | T019-T021 |
| US5 | T034 |
| FR-001-008 | T005, T008, T010, T029, T030 |
| FR-009-017 | T011-T018, T032, T033 |
| FR-018-019 | T009, T023, T024 |
| FR-020-023 | T025-T027 |
| FR-024-027 | T015-T018, T034 |
| FR-028-030 | T019-T021 |
| SC-001-008 | T011, T018, T006, T012, T011, T030, T023, T034 |
