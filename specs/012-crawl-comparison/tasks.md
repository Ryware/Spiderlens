# Tasks: Crawl Comparison and Change Detection

**Input**: [plan.md](./plan.md), [research.md](./research.md), [data-model.md](./data-model.md), [contracts/](./contracts/), [quickstart.md](./quickstart.md)

Each task ships with its tests. `[P]` = can run in parallel with the other
`[P]` tasks of the same phase.

## Phase 1: Setup

- [ ] T001 New crate `crates/compare` (`spiderlens-compare`), wired into the workspace, CLI and app; expose from `spiderlens-results` what the comparison reuses (`sql::register`, connection setup, `build_derived`, tab constants)
- [ ] T002 [P] Fixtures: site v1/v2, migration pair, limit pair (quickstart.md) in `crates/test-fixtures/src/compare.rs`, crawled into crawl files by a test helper
- [ ] T003 [P] `CompareSettings`, `MapRule`, `ElementSetting` in `crates/config/src/compare.rs` with validation and serde round-trip tests

## Phase 2: Foundation (blocks all stories)

- [ ] T004 Workspace: temp file, side connections on `mode=ro` + `ATTACH … AS cmp`, cleanup on close, cancel and next start; test that writes to `main` fail
- [ ] T005 Filter registry: bit layout from `results::tabs()`; side fill with `(eligible AND cond) << k`, derived tables built first, `Need` availability per side
- [ ] T006 Tracked elements list with expressions, kinds, defaults, needs; side fill of element values
- [ ] T007 Batched side fill (50,000 ids), progress events, interrupt and cancel
- [ ] T008 Matching: `map_url()`, `cmp.matches`, groups New / Missing / Not reached / collisions
- [ ] T009 Open and validate: same file refused, newer format read-only or refused, running crawl snapshot (read transaction), configuration differences (D10)

## Phase 3: US1 URLs that came and went (P1) — MVP

- [ ] T010 [US1] Summary and filter deltas in one pass (FR-011, FR-014, FR-015); fixture tests US1-1..3
- [ ] T011 [US1] Group views: open, rows, search, sort, export through the 007 export writer (FR-013)
- [ ] T012 [US1] Desktop: `compare.rs` commands and progress events; transport methods; fake transport data
- [ ] T013 [US1] UI: setup sheet (pickers from the recent list, swap, info, config differences), entry points (start screen, palette, File menu, Compare with…)
- [ ] T014 [US1] UI: comparison mode sidebar, Overview with sort by change and hide unchanged, group lists in the Grid with paired columns, inspector side by side; Vitest and Playwright

## Phase 4: US2 change detection (P1)

- [ ] T015 [US2] Element comparison SQL, `cthr()`, changed counts and lists with previous, current and differences
- [ ] T016 [US2] Element settings update without rescan (FR-019), remembered in the app settings store
- [ ] T017 [US2] UI: Change detection view and settings popover; Vitest and Playwright

## Phase 5: US4 mapping and migration (P2)

- [ ] T018 [US4] Host mapping suggestion, mapping tester command
- [ ] T019 [US4] Migration lists (FR-027) with redirect chains via the current side's redirect derived table
- [ ] T020 [US4] UI: mapping editor and tester in the setup sheet, Migration view; Vitest and Playwright

## Phase 6: US5 command line (P2)

- [ ] T021 [US5] `spiderlens compare`: text summary, `--json` report, `--map`, `--map-regex`, `--settings`
- [ ] T022 [US5] `--export` folder with the contract's file names
- [ ] T023 [US5] `--fail-on-missing`, `--fail-on-lost-indexable`, `COMPARE_FAILED` exit code; parity test CLI vs desktop numbers (SC-005)

## Phase 7: US6 scale and safety (P2)

- [ ] T024 [US6] SC-004 test: bytes and modification time of both files unchanged after completed and cancelled comparisons; another process can open the current file for crawling during a comparison
- [ ] T025 [US6] Edge cases: old format without extraction ("not available"), rendering on one side, low overlap warning, file deleted mid-run, cancel leaves nothing behind

## Phase 8: With other features

- [ ] T026 [US3] (needs 009 code) issue deltas via 009's evaluator, analysis versions (FR-024a), Issues view, `--fail-on-new-issues`
- [ ] T027 (needs 021 code) schedule baseline in app and CLI (FR-002a)
- [ ] T028 (with 010) `ComparisonReport` handed to the report renderer (FR-033)

## Phase 9: Performance and polish

- [ ] T029 Benchmark: gen_crawl 100k and 1M pairs, summary and full times, peak memory; record in research.md (SC-002, SC-003)
- [ ] T030 Crawl speed while comparing (SC-008)
- [ ] T031 Free-space warning for the workspace
- [ ] T032 Docs: README section, CLI help
- [ ] T033 `/speckit-analyze`, then `/speckit-converge`
