# Tasks: Crawl Speed and Performance

**Input**: [plan.md](./plan.md), [spec.md](./spec.md)

`[P]` = can run in parallel (different files). `[USn]` = user story.

## Phase 1: Measure

- [x] T001 [US5] `test-fixtures/src/bin/bench-site.rs`: synthetic N-page site with optional delay and server-side `/__stats` (plan D7)
- [x] T002 [US5] Record the baseline on the 100,000-page site with the Fast preset

## Phase 2: Coordinator cost per URL (US1)

- [x] T003 [US1] `crawl/window.rs`: per-host index; `dispatch()` walks hosts and skips saturated ones in O(1) (plan D1), unit tests for order and fairness
- [x] T004 [US1] `crawl/runner.rs`: group commits every 200 ms / 500 responses / idle (plan D2)
- [x] T005 [US1] In-memory queued and not-crawled counters replace the per-tick `GROUP BY` (plan D3), test against the file's counts
- [x] T006 [US1] Fix: with robots.txt ignored, the first dispatch pass could leave the crawl waiting forever
- [x] T007 [P] [US1] Enable gzip and brotli decoding in reqwest (plan D5)

## Phase 3: Adaptive per-site allowance (US2)

- [x] T008 [US2] `crawl/speed.rs`: per-host AIMD allowance, response-time EWMA and baseline, unit tests (plan D4)
- [x] T009 [US2] 429/503 halve the allowance; Retry-After (seconds or date) holds the host; tests on the fixture site
- [x] T010 [US2] Adaptive off uses the fixed per-site cap but still honours 429/503 and Retry-After (FR-018)

## Phase 4: Speed figures (US3)

- [x] T011 [US3] `Progress`: current and average URLs per second, ETA (plan D6)
- [x] T012 [US3] CLI progress line shows rate and ETA; end-of-crawl speed summary (FR-030, FR-031); desktop status bar shows time left

## Phase 5: Verify

- [x] T013 Benchmark after: 100,000 pages Fast, 100 ms delay with 50 at once and adaptive off (SC-002), caps checked from `/__stats` (SC-003)

## Later rounds

- [ ] T014 [US1] Live cap changes during a crawl (FR-005)
- [ ] T015 [US2] Global slowdown detection (FR-019)
- [ ] T016 [US3] Limiting-factor reasons and per-site breakdown (FR-028, FR-029), desktop status bar
- [ ] T017 [US2] Speed events recorded in the crawl file (FR-021)
- [x] T018 [US4] Saving backpressure (FR-023) and the 1,000,000 URL memory run (SC-007); fixed the write-ahead log growing without limit
- [x] T019 [US5] Reduced benchmark in CI (FR-034): `bench-check`, against a build of the base branch
- [ ] T020 [US2] Retry the URL that got 429/503 (needs spec 004 retries)
