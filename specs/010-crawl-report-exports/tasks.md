# Tasks: Crawl Report and Exports

Each task ships with its tests. [P] = can run in parallel with the other [P] tasks of the same phase. Code is stacked on spec 009 (PR 52).

## Phase 1: Foundations

- [ ] T001 Add `rust_xlsxwriter` to the workspace and `spiderlens-results`; `calamine` as a dev-dependency (research R1).
- [ ] T002 Move `neutralise` and `csv_field` to `crates/results/src/csvfmt.rs`; 007 export uses it unchanged (existing export tests pass).
- [ ] T003 `crates/results/build.rs`: light-theme tokens from `ui/src/styles/tokens.css` into a CSS constant; test that it is non-empty and has no raw colour outside the token set (research R5).
- [ ] T004 Report fixture site in `crates/test-fixtures/src/report.rs`: known pages per priority, known codes, depths, one redirect chain, one loop, a page with a Hebrew title, and expected values.

## Phase 2: Report model and score (US1, US2) 🎯 MVP

- [ ] T005 `report/mod.rs`: `Report`, `Section`, `Block`, `Target`, `SectionState`, ts-rs export; `Reader::report(opts)`.
- [ ] T006 `report/score.rs`: streaming walk, weights, value, by-priority losses, shares, gains, N = 0, partial, `SCORE_VERSION`. Tests: formula, shares sum to 100 − score within 1 point (SC-008), gain equals the score change after turning the rule off (US2-3), N = 0 is unavailable (edge case), unavailable rules make it partial (FR-012).
- [ ] T007 `report/sections.rs`: SECTIONS with summary, completeness, issues by priority, top issues, response codes, site structure, content; each figure's `Target`. Tests: every figure equals the 007 count or 009 issue count on the fixture (FR-005, SC-001); unavailable on old fixture files (US6-1); provisional while running (US1-4); incomplete crawl banner (US1-3, FR-007); site-level issues excluded from the score (FR-010, plan D4).
- [ ] T008 Settings change: report after `set_issue_settings` reflects new priorities and states custom settings (US1-5, US2-4).

## Phase 3: Report file (US3)

- [ ] T009 `report/html.rs`: renderer, escaping, inline SVG bars, print CSS, `dir="auto"`, URL limit with remainder line, title and note, meta line. Tests: no external references; hostile strings escaped; all sections and figures present; limit 0 and 10,000; provisional stamp (US3-1, US3-3, US3-4, US3-5, FR-014 to FR-017).
- [ ] T010 Size and time on the 100k synthetic file: under 10 MB, under 10 s (SC-003).

## Phase 4: Bulk exports (US4)

- [ ] T011 `bulk/sink.rs`: `Sink`, `CsvFolderSink`, `XlsxSink` (overview, frozen header, number cells, continuation sheets, cell cap, unique sheet names), atomic partial paths, overwrite refusal, cancel cleanup. Tests with an injected small row limit (US4-3, FR-022, FR-024, FR-025).
- [ ] T012 `bulk/mod.rs`: BULK_TABLES (tabs, issues, links, images, redirect_chains, headers, summary) over one snapshot; progress and cancel. Tests: each tab and issue equals its single export row for row (SC-007, FR-020); links filtered by source and target equal the detail pane Outlinks and Inlinks (US4-4); xlsx read back with calamine (US4-2).
- [ ] T013 Non-Latin, commas, quotes, line breaks and formula-like values survive in CSV and xlsx (FR-023, SC-007).

## Phase 5: CLI (US5)

- [ ] T014 `report --audit [--json]` and `report --html` with title, note, limit, force, issue settings. Tests: JSON equals the app command's value; exit codes; no partial file on failure (US5-1, US5-3).
- [ ] T015 `export --bulk --format --out --force`. Tests: same files as the app for the same choices (US5-2); exit codes.

## Phase 6: Desktop app and UI (US1, US3, US4)

- [ ] T016 `app/src-tauri/src/report.rs`: `report`, `export_report_html`, `print_report` (hidden web view, `print()`), `bulk_export` with `export_progress`, `cancel_export`; register in `main.rs`; fake transport handlers in `ui/src/testing/fakeTransport.ts`.
- [ ] T017 Tokens `--score-good`, `--score-fair`, `--score-poor` in `tokens.css` (light and dark, every palette), contrast test entries.
- [ ] T018 `ui/src/report/ReportView.tsx`, `ScoreCard.tsx`, `SectionView.tsx`: blocks, badges (provisional, partial, incomplete), targets open the results tab, filter or issue; 5 s refresh while running. Vitest: rendering every block kind, link targets, badges, keyboard and screen-reader names (FR-004, FR-006, FR-032).
- [ ] T019 Sidebar Report entry above results tabs; "Crawl finished" toast with View report; `openReportOnFinish` setting in Settings › General. Vitest (FR-031).
- [ ] T020 `ExportReportDialog.tsx` (HTML or PDF, title, note, limit 0 to 10,000 validated) and `BulkExportDialog.tsx` (tables, format, destination, overwrite confirm, progress, cancel); menu and command palette entries. Vitest.
- [ ] T021 Playwright: finish a fake crawl, open the report, click the top issue, see its rows; export HTML and open the written file offline in Chromium; run a bulk export with progress and cancel.

## Phase 7: Polish

- [ ] T022 Benchmarks in `benchmarks.md`: SC-002 (100k and 1M), SC-004 (bulk Excel 100k, links 10M), SC-005 (memory 1M vs 100k), SC-006 (crawl speed with report and export running).
- [ ] T023 `/speckit-converge` until Converged; CI green on Windows, macOS and Linux.

## Dependencies

Phase 1 → Phase 2 → (Phase 3 [P] Phase 4) → Phase 5 → Phase 6 → Phase 7. Phase 2 needs spec 009's issues module (PR 52).

## Acceptance scenario map

| Scenario | Tasks |
|---|---|
| US1-1..5 | T007, T008, T018, T021 |
| US2-1..4 | T006, T008 |
| US3-1..5 | T009, T010, T016, T020, T021 |
| US4-1..5 | T011, T012, T013, T020, T021 |
| US5-1..3 | T014, T015 |
| US6-1..2 | T007, T014 |
