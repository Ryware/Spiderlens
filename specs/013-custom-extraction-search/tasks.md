# Tasks: Custom Extraction and Custom Search

**Input**: [plan.md](./plan.md), [data-model.md](./data-model.md), [spec.md](./spec.md)

`[P]` = can run in parallel (different files). `[USn]` = user story.

## Phase 1: Setup and baseline

- [ ] T001 Measure bench-site 100k on the merge base (3 runs, URLs/s and peak RSS) into `benchmarks.md` (plan, Performance plan)
- [ ] T002 Add `sxd-document`, `sxd-xpath`, `serde_json_path` to the workspace (research R1, R2)
- [ ] T003 Prototype the scraper → sxd tree copy and time it against parse on the bench pages; record in `benchmarks.md`; keep or switch XPath option (research R1)

## Phase 2: Configuration (`crates/config`)

- [ ] T004 [US1][US2][US5] `model.rs`: `CustomRules`, `Extractor`, `Search`, `ElementLimit` with serde defaults and ts-rs export (data-model.md, FR-001, FR-006, FR-010)
- [ ] T005 [US5] Rule file read/write with format and version; import into a profile; tests incl. US5.1, US5.4
- [ ] T006 [US4] Validation hook: names unique, limits (FR-009), empty-match regex warning; engine compile errors surfaced through `validate` (FR-008)

## Phase 3: Engine evaluator (`crates/engine/src/custom/`)

- [ ] T007 [US1] `walk.rs`: keep the parsed document and visible text for a hook (`DocView`); no-rules path unchanged (plan D1, FR-027, FR-028)
- [ ] T008 [US1][US4] `custom/compile.rs`: CSS, regex, text-as-regex, XPath, JSONPath compilation with error positions (plan D2, FR-008)
- [ ] T009 [US1] `custom/css.rs`: Text, Inner HTML, Outer HTML, Attribute modes; US1.1, US1.3, US1.4, US1.5, US1.9
- [ ] T010 [US1] `custom/xpath.rs`: tree copy, node-set and value results, Inner/Outer HTML via NodeId map; US1.2, US1.8
- [ ] T011 [P] [US1] `custom/regex.rs`: first group or whole match on source; US1.6
- [ ] T012 [P] [US1] `custom/json_ld.rs`: item path with `@type` and `@graph`, JSONPath form, bad JSON flag; US1.7
- [ ] T013 [US2][US6] `custom/search.rs`: HTML or visible text, ignore case, non-overlapping count, 10 contexts, element limits, element not found; US2.1-US2.6, US6.1, US6.2
- [ ] T014 [US1] Limits and flags: 100 values, 32,000 chars, more-than-stored, per-page budget with timed out (plan D6, FR-017, FR-019, FR-020)
- [ ] T015 Fixture pages under `crates/test-fixtures` `/custom/` covering every scenario; unit tests per scenario (SC-004)

## Phase 4: Storage and crawl

- [ ] T016 [US1][US2] `migrate.rs` step (number taken at merge time): `custom_rules`, `custom_results`, `custom_values`; `data_features` gains `custom` (data-model.md)
- [ ] T017 [US1][US2] `store.rs`: write rule rows (with slots) at crawl start, one packed results row and optional values row with the page in `record_page` (cached statements)
- [ ] T018 [US1] `runner.rs`: compile at start, `Arc<CompiledRules>` into `finish_page`; pause-edit-resume applies new rules and marks old ones stale (FR-011, FR-016)

## Phase 5: Reruns and tester

- [ ] T019 [US3] `custom/rerun.rs`: stream bodies in id order, worker pool, batches of 2,000, progress callback, cancel, not available, partial state (plan D8, FR-012-FR-015)
- [ ] T020 [US3] Equality test: crawl-with-rules values == crawl-then-rerun values on the fixture site (US3.1, FR-015); rerun bench on 100k into `benchmarks.md` (SC-003)
- [ ] T021 [US4] `custom::test_page` (stored HTML) and `test_url` (one fetch with crawl identity and robots) (FR-018, US4.1-US4.4)

## Phase 6: Results (`crates/results`)

- [ ] T022 [US1][US2] `TabDef`/`ColumnDef`/`FilterDef` text fields to `Cow<'static, str>`; `Reader::tabs()` adds per-file custom tabs (plan D9); existing tests unchanged
- [ ] T023 [US1][US2] Custom extraction and Custom search columns and filters with counts (FR-022, FR-023); numeric sort when all values are numbers
- [ ] T024 [US1][US2] Detail: all values and search contexts (FR-024)
- [ ] T025 [P] [US1] Export `--all-values SEP` (FR-025)

## Phase 7: Command line

- [ ] T026 [US5] `crawl --rules FILE`; `rules validate|test|run` (contracts/cli.md); CLI tests incl. US3.7, US5.3

## Phase 8: Desktop and UI

- [ ] T027 [US1][US2][US4][US5] `app/src-tauri/src/custom.rs` commands (contracts/desktop-commands.md)
- [ ] T028 [US1][US2][US5] Settings › Custom rules: lists, editor rows, inline errors, import/export, read-only while running; Vitest tests
- [ ] T029 [US4] Test panel (crawled page or URL); Vitest tests (SC-006)
- [ ] T030 [US3] Results: Run on this crawl with progress, cancel and disabled reasons; detail pane Custom section; Vitest tests
- [ ] T031 Fake transport support and Playwright end-to-end: add rule, test, crawl, filter, rerun

## Phase 9: Performance and converge

- [ ] T032 Bench with no rules and with the typical set; record in `benchmarks.md` (SC-001, SC-002); tune per plan levers if needed
- [ ] T033 CSS/XPath vs Chromium on a 50-page corpus with a golden file from `scripts/browser-golden.mjs` (SC-005)
- [ ] T034 Pathological rule fixture: page marked timed out, crawl continues (SC-007)
- [ ] T035 `/speckit-converge`
