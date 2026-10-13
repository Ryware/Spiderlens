# Tasks: Crawl Storage and Crawl Files

**Input**: [plan.md](./plan.md), [data-model.md](./data-model.md), [contracts/](./contracts/), [spec.md](./spec.md)

`[P]` = can run in parallel (different files). `[USn]` = user story.

## Phase 1: Setup

- [ ] T001 Record the bench baseline in `benchmarks.md` (main f3a4d3d: 31.4-32.9 s, 32 MB, 320 MB); commit reference files `crates/engine/tests/reference/v2-completed.spiderlens` and `v2-paused.spiderlens` made by main, with `expected.json` (counts, sample titles)
- [ ] T002 Add `fs4` to the engine

## Phase 2: Foundations (engine, blocks everything)

- [ ] T003 [US6] `crawl/migrate.rs`: `Step`, `STEPS` (step 3 `events`), `LATEST`, `apply(conn, from)`, `read_compat`; `CrawlStore::create` = base + all steps; unit tests (new file equals migrated v2 file schema)
- [ ] T004 [US6] `CrawlStore::open_read_only` (never writes; `immutable=1` fallback) and `open` checks `format_version`/`read_compat` (plan D4); `CrawlSummary::read` uses the read-only open
- [ ] T005 [US1] `crawl/file.rs`: `WriterLock` on `<file>.lock` (`File::try_lock`, JSON note, removed on drop); `Crawler::create/open` hold it; second writer gets `InUse`; tests
- [ ] T006 [US6] `events` writes: created, resumed, paused, stopped, completed, saved, upgraded, config_changed, local_error, with app version and `os-arch`

## Phase 3: US1 crash recovery (P1)

- [ ] T007 [US1] `file::recover(path)`: status running + lock free -> in-flight requeued, paused, `recovered=1`, event; `Crawler::open` recovers first; tests
- [ ] T008 [US1] Kill test `crates/engine/tests/storage.rs`: start the CLI on the fixture site, kill it hard mid-crawl, recover, check no duplicate URLs, every URL final or queued, open in results reader; repeat 5 times (SC-001)

## Phase 4: US2 pause, stop, resume (P1)

- [ ] T009 [US2] Resume equals uninterrupted: pause at ~30%, drop the crawler, reopen, resume; compare URL set and states with a straight crawl (SC-003)
- [ ] T010 [US2] `Change::MaxUrls` in `config/stored.rs`; `Crawler::open_with(changes)`; CLI `--resume` accepts speed and `--max-urls`; event `config_changed` (plan D8)
- [ ] T011 [US2] Secrets on resume: `CrawlInfo.missing_secrets`, `Crawler::with_secrets`, CLI `--header-secret NAME=VALUE`

## Phase 5: US3 Save As and open (P1)

- [ ] T012 [US3] `file::save_as(src, dest)` for an idle crawl: checkpoint, copy to `.partial`, fsync, atomic rename; backup-API fallback; tests incl. overwrite and interrupted copy
- [ ] T013 [US3] Command channel in `Crawler::run` (`RunCommand::SaveAs`) moving a running crawl between batches (`CrawlStore::relocate`); test: crawl keeps going into the new file, old unsaved copy deleted
- [ ] T014 [US3] `file::info(path) -> CrawlInfo` (read-only, no full scan) incl. `Access`; `spiderlens info` (contracts/cli.md)

## Phase 6: US5 disk and write failures (P2)

- [ ] T015 [US5] `LocalError` + kinds from SQLite codes; run loop rolls back the batch, writes paused + `pause_reason` + event best effort, returns `CrawlError::Local`
- [ ] T016 [US5] Free-space check every 2 s against `with_disk_reserve(bytes)`; CLI `--disk-reserve`, exit 6
- [ ] T017 [US5] Tests: `max_page_count` disk full mid-crawl -> paused, file opens, nothing marked crawled without its row, resume completes equal to straight crawl (SC-006); huge reserve -> immediate `disk_low`

## Phase 7: US6 versions and damaged files (P2)

- [ ] T018 [US6] Upgrade on resume with backup copy `<stem> (format N backup).spiderlens`; CLI `--no-backup`; tests on the committed v2 reference files
- [ ] T019 [US6] Newer-format file (format 99, additive) opens read-only for viewing and is refused for resume with the version needed; non-additive newer refused
- [ ] T020 [US6] `file::salvage(src, dest)`: table-by-table copy skipping unreadable rows; test with a file whose pages are overwritten with garbage
- [ ] T021 [P] [US6] Reference-file test reading `expected.json` on all OSes (SC-007); CI portability job: each OS writes a crawl, the others open it (SC-008)
- [ ] T022 [P] [US6] `results` reader: `immutable=1` fallback for read-only media; test on a read-only copy

## Phase 8: US4 recent crawls and the app (P2)

- [ ] T023 [P] [US4] `config/recent.rs`: `RecentStore` (20 entries, MRU, remove, locate, refresh from file meta), tests
- [ ] T024 [US4] `app/src-tauri/src/files.rs`: `crawl_open`, `crawl_info`, `crawl_save_as`, `recent_*`, `unsaved_*`, `crawl_salvage`, `storage_status`, `pending_open`; recovery scan at launch (`crawls-recovered`)
- [ ] T025 [US2] `crawl.rs`: resume with changes/secrets/upgrade, Save As while running (`crawl-moved`), `local` in `crawl-finished`, disk reserve
- [ ] T026 [US2] Quit while crawling: intercept close/exit, `app-quitting`, pause, exit when paused (plan D12)
- [ ] T027 [US3] File association in `tauri.conf.json`; `RunEvent::Opened` and launch arguments -> `open-file`
- [ ] T028 [US4] UI `ui/src/files/`: `RecentCrawls` on the start screen (status, URLs, date, unavailable with Locate/Remove, unsaved badge), unsaved crawls size and delete with confirmation, low-space warning
- [ ] T029 [US3][US6] UI: open flow (`useCrawlFile`): Open, recent, drop, `open-file`; prompts for running crawl, upgrade (keep copy), read-only (Save As first), damaged (Recover), secrets
- [ ] T030 [US6] UI: Crawl info panel (configuration, versions, events, size) and "New crawl with these settings"
- [ ] T031 [US3][US5] UI: Save As command (⌘⇧S), disk/write error notice with cause, path, free space and actions, quit notice
- [ ] T032 Vitest tests for T028-T031 against the fake transport

## Phase 9: Polish

- [ ] T033 Benchmarks after (3 runs alternated), memory at 10k vs 100k (SC-002), record in `benchmarks.md`
- [ ] T034 Update `specs/003-frontier-scope/contracts/desktop-commands.md` pointer and README crawl-file section
