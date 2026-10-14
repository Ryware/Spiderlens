# Tasks: Link Score and Internal Link Analysis

**Input**: [plan.md](./plan.md), [data-model.md](./data-model.md), [contracts/](./contracts/), [spec.md](./spec.md)

`[P]` = can run in parallel (different files). `[USn]` = user story. Stage 1 = no file change (plan D6); stage 2 = after the 009 runner and 006 migrations are on `main`.

## Phase 1: Setup

- [ ] T001 Fixture graphs and pages in `crates/test-fixtures/src/links.rs` (hub, deep page, nofollow-only, redirect chain and loop, 404 target, image link, self link, duplicates, sitemap-only, single page, List mode without links, landmark pages for positions) and a `links` site in the fixture server
- [ ] T002 [P] `synth` option for links per page and a redirect share; example `bench_links` printing time and peak RSS

## Phase 2: Foundations

- [ ] T003 [US1] `analysis/link_metrics.rs`: graph scan (D2) into CSR, redirect substitution, loop handling; unit tests on the fixture graphs
- [ ] T004 [US1] Power iteration, scaling and ranks (D3); reference dense PageRank in tests (SC-001); determinism hash test (SC-008)
- [ ] T005 [US1] Counts (D4) and the `link_metrics` writer into a given schema; tests
- [ ] T006 [US2] `Reader::ensure_analysis()` (stage 1, D6): status check, temp tables, data-version staleness, progress callback, cancel flag; tests incl. cancel keeps earlier results and running crawl gives `Need::Analysis` running

## Phase 3: US1 columns and detail (P1)

- [ ] T007 [US1] `Need::Analysis`, columns in `tabs.rs` (Link Score default on Internal), `UrlDetail.link_metrics`; tests on the fixture crawl
- [ ] T008 [US1] Tauri: `analysis_state`, `run_analysis`, `cancel_analysis`, `analysis_progress` / `analysis_done` events; regenerate TS types
- [ ] T009 [US1] UI: pending and not-calculated cells with Run action, inspector Links group, Vitest tests

## Phase 4: US2 runs on its own (P1)

- [ ] T010 [US2] App runs the analysis when a crawl finishes (if `auto_analyse`) and when a finished file without results is opened; status bar progress and cancel; Vitest tests
- [ ] T011 [US2] `LinkAnalysisSettings.auto_analyse` in config with Settings switch; CLI `--no-analyse`; tests
- [ ] T012 [US2] Benchmarks on `synth` 100k / 1M: record in `benchmarks.md`; check SC-002 to SC-005 (crawl speed via the 008 benchmark job)

## Phase 5: US3 Links tab and rules (P2)

- [ ] T013 [US3] `links` tab with filters (plan D10); filter tests on the fixture crawl (SC-007)
- [ ] T014 [US3] Sidebar entry, tab wiring, Playwright e2e opening Links and applying a filter
- [ ] T015 [US3] Expose inputs to the 009 rule catalogue (column ids and `Need::Analysis`), agreed with the 009 thread

## Phase 6: US4 link positions (P2)

- [ ] T016 [US4] `PositionRule`, `PositionWeights`, validation (selector parse errors) in `spiderlens-config`; tests
- [ ] T017 [US4] `analysis/link_positions.rs`: decompress, parse, classify (plan D8), parallel chunks with ordered output; `pos` alignment test on malformed pages; tester function
- [ ] T018 [US4] Weighted edges in `link_metrics` when weighting is on; tests (footer-only page drops)
- [ ] T019 [US4] Position on inlink/outlink rows and filter; Links tab "Linked only from navigation, header or footer"; tests
- [ ] T020 [US4] Settings › Link analysis: rules editor (reorder, validation), tester, weighting sliders; Vitest tests; Playwright e2e for the tester
- [ ] T021 [US4] Positions benchmark on 100k pages (SC-009)

## Phase 7: US5 command line (P3)

- [ ] T022 [US5] `spiderlens links analyse`, `links positions`, export of the `links` tab; CLI tests comparing with the reader results

## Phase 8: Stage 2 (after 009 runner and 006 migrations land)

- [ ] T023 Migration step creating `link_metrics` and `link_positions` (number at merge); passes registered in `PASSES`; `meta.link_analysis_run`; read-only files compute into `temp`
- [ ] T024 Remove the stage 1 trigger in favour of the runner; `spiderlens links analyse` becomes an alias of `spiderlens analyse`; tests that an analysed file reopens with scores and no recomputation

## Phase 9: Polish

- [ ] T025 `/speckit-converge` and quickstart walk-through
