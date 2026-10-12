# Tasks: Crawl Configuration

**Input**: [plan.md](./plan.md), [research.md](./research.md), [data-model.md](./data-model.md), [contracts/](./contracts/)

## Phase 1: Setup

- [x] T001 Create `crates/config` crate, add dependencies to the workspace

## Phase 2: Foundational

- [x] T002 `model.rs`, `defaults.rs`: CrawlConfig and groups with defaults; test that defaults match the spec
- [x] T003 `presets.rs`: user agent presets and tokens, speed presets, derived preset name
- [x] T004 `secret.rs`: HeaderValue with secret handling; tests that secrets never serialize or print
- [x] T005 `url_norm.rs`, `site.rs`: normalization and same-site rules with `psl`

## Phase 3: US1 + US2 start a crawl, List mode (P1)

- [x] T006 [US2] `list_input.rs`: text and CSV parsing, encodings, counts, invalid sample, progress callback; 1M-line benchmark
- [x] T007 [US1] `validate.rs`: issue model; start URL, list, limits, network ranges

## Phase 4: US3 + US4 scope and identity (P2)

- [x] T008 [US3] `patterns.rs`: compile, exclude-beats-include, tester; validation of syntax with look-around explanation
- [x] T009 [US4] Validation for user agent, headers, speed, robots, retry, including all warnings

## Phase 5: US5 profiles (P3) and US6 stored configuration (P2)

- [x] T010 [US5] `profile.rs`: file format, import with unknown-setting report, refuse foreign files
- [x] T011 [US5] `store.rs`: ProfileStore with atomic writes, default profile, name conflicts
- [x] T012 [US6] `stored.rs`: StoredCrawlConfig, change log, effective(), missing-field report

## Phase 6: Engine and CLI

- [x] T013 Engine: build request options from CrawlConfig; robots token from config
- [x] T014 CLI: shared crawl options, `config show|validate|defaults|test-patterns`, contract tests

## Phase 7: Desktop

- [x] T015 ts-rs generation into `ui/src/generated`, CI staleness check
- [x] T016 Tauri commands per contracts/desktop-commands.md
- [x] T017 Settings screen: start field and mode, List input with counts, scope with pattern tester, identity and headers, speed with presets and notices, robots, retry
- [x] T018 Profile bar: save, save as, revert, modified indicator, default, import and export, reset
- [x] T019 UI tests for warnings (SC-009), Custom label, profile bar

## Phase 8: Polish

- [x] T020 fmt, clippy, full test suite; quickstart map updated
