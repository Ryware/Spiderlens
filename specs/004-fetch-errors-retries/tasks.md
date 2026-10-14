# Tasks: Fetching Pages, Errors and Retries

**Input**: [plan.md](./plan.md), [data-model.md](./data-model.md), [spec.md](./spec.md)

`[P]` = can run in parallel (different files). `[USn]` = user story.

## Phase 1: Setup

- [x] T001 Record the bench-site baseline on main (100k pages, 3 runs) in `benchmarks.md` (plan D10)
- [x] T002 Add `fastrand` to the engine; `RetrySettings` gains `base_wait_secs`, `host_pause_after`, `host_pause_secs`, `host_pause_max_secs`, `host_give_up_after` with defaults, validation and `unavailable_settings` coverage (data-model.md, FR-016, FR-024 to FR-026); regenerate TS bindings

## Phase 2: Classification (US1)

- [x] T003 [US1] `error.rs`: `ErrorClass::{RedirectProblem, HostUnavailable}`, `ErrorReason`, `CheckError.reason`; `FinalClass` derived from status or class with `is_transient()` (data-model.md tables); unit tests for every row
- [x] T004 [US1] `fetch.rs`: connect vs read timeout (R1), refused vs reset, TLS reasons (R2), truncated vs over-size vs decompression (R3); `spiderlens check` uses `redirect_problem` with sub-reasons
- [x] T005 [US1] `store.rs`: new columns, `waiting` state, `crawl_log` table, `redirect_limit` reason, upgrade of older format-2 files on resume, `data_features` gains `attempts`
- [x] T006 [US3] Redirects in the crawler: invalid target recorded on the redirecting URL; hop count via `source_id`, target over the limit recorded "Redirect chain over limit" (plan D8)

## Phase 3: Retries (US2)

- [x] T007 [US2] `crawl/retry.rs`: pure decision (plan D1, D3), backoff with jitter, Retry-After max, cap and "capped" flag; tests for US2.1-US2.5, US2.7, FR-016 bounds, attempts = 1
- [x] T008 [US2] Runner: waiting state and timer heap (plan D4), back of the queue (R9), Retry-After applied to the host and capped, intermediate attempts not recorded, attempts and round attempts written, log rows (FR-035)
- [x] T009 [US2] Resume and stop with waiting URLs (FR-023, SC-009): waiting rows reload with their remaining wait; stop turns them into "Crawl stopped" and resume brings them back with attempts kept

## Phase 4: Host pausing and local problems (US4)

- [x] T010 [US4] `crawl/health.rs`: breaker state machine (plan D6) with unit tests for US4.1-US4.3, US4.5
- [x] T011 [US4] Runner: paused hosts skipped, single probe, in-flight results of a paused host re-queued without an attempt (US4.6), given-up host's URLs recorded "Host unavailable", progress carries paused hosts and "waiting for host"
- [x] T012 [US4] Queue window grows when every host in it is blocked (plan D11)
- [x] T013 [US4] Local network detection pauses the crawl (plan D7, FR-029); disk full ends the run as a local problem with nothing marked failed (FR-015)
- [x] T014 [US4] Run commands: `ResumeHost`, `Apply(Change)` for live speed and retry changes (plan D13), recorded in the stored configuration

## Phase 5: Manual retry and summary (US5, FR-036)

- [x] T015 [US5] `CrawlStore::transient_failures()`, `Crawler::retry_failed()` (plan D12): re-queue, reset round, mark manual, clear old derived rows, log; refuse while running
- [x] T016 Summary: by final class, transient/permanent, retries, recovered, hosts paused, manual retries, pause reason; `report --log`

## Phase 6: Fixtures and end-to-end tests

- [x] T017 [P] `test-fixtures`: scripted failures (fail N times then succeed, always fail, 429/503 with Retry-After, refused, reset, hang, truncated, over size, bad gzip), a host that goes down and comes back, request log with times
- [x] T018 [US1] `tests/errors.rs`: US1.1-US1.8, SC-001, SC-008
- [x] T019 [US2] `tests/retries.rs`: US2.1-US2.7, SC-002, SC-003, SC-004
- [x] T020 [US4] `tests/host_pause.rs`: US4.1-US4.6, SC-005, local network pause
- [x] T021 [US5] `tests/manual_retry.rs`: US5.1-US5.3, SC-006, SC-009
- [x] T022 [US6] Headers and user agent on every request kind including retries and probes (US6.1-US6.3)

## Phase 7: Command line (contracts/cli.md)

- [x] T023 New `crawl` retry options; host pause notices on stderr; exit code 5 on network pause
- [x] T024 [US5] `spiderlens retry FILE [--yes] [--json]` with tests (US5.5)
- [x] T025 `report --summary` new counts and `report --log`

## Phase 8: Desktop app and UI (contracts/desktop-commands.md)

- [x] T026 Tauri commands `crawl_resume_host`, `crawl_apply_settings`, `retry_preview`, `retry_start`, `crawl_log`; progress and summary payloads extended
- [x] T027 [P] [US4] Status bar: waiting count, "Waiting for host" countdown, paused-hosts popover with "Resume now"; Vitest tests
- [x] T028 [P] [US5] "Retry failed URLs" in the toolbar and command palette, confirmation with counts by class, disabled with reason; Vitest tests
- [x] T029 [P] Settings › Network and retries: new fields; Speed and Retry editable during a crawl with "Apply to running crawl"; Vitest tests
- [x] T030 [P] Crawl summary panel: errors by class, transient/permanent, retries, recovered, hosts paused, network pause message; Vitest tests
- [x] T031 Results (with the spec 007 owner): Attempts column, class labels incl. sub-reasons and HTTP classes, "Transient errors" / "Permanent errors" / per-class filters in Response codes, detail pane fields, "Waiting to retry" state; Rust and Vitest tests. *Done, including one filter per class (listed only when it matches). Sub-reasons show in the error message.*

## Phase 9: Polish

- [x] T032 Bench after: healthy 100k crawl within 3% of T001; 2% flaky pages run; record in `benchmarks.md` (SC-011)
- [ ] T033 Update contracts of specs 001/003 where 004 changed them (`redirect_problem`, events); `/speckit-converge`. *Done: spec 001 page-check schema. Open: converge pass.*
