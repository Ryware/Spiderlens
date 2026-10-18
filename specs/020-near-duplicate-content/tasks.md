# Tasks: Near Duplicate Content

Input: [plan.md](plan.md), [data-model.md](data-model.md), [contracts/](contracts/), [research.md](research.md). `[P]` = can run in parallel with the previous task. Each task ships with its tests.

## Phase 1: Engine, pure functions (plan D1, D2)

- [ ] T001 `extract/shingle.rs`: tokenizer (005 word rules, lowercase, single-character tokens for Han, Hiragana, Katakana, Hangul, Thai), 5-token shingles, xxh3-64. Tests: token cases, no-space scripts, short texts.
- [ ] T002 [P] `extract/shingle.rs`: one-permutation MinHash, K = 960, optimal densification with a fixed seed table, 8-bit bins (b-bit corrected), `similarity(a, b)`. Tests: golden signature bytes for a fixed text (cross-platform), identical texts = 100, disjoint ≈ 0, accuracy against exact Jaccard on 500 seeded pairs at 50 to 100% (SC-003: 95% within 3 points from 100 words up).
- [ ] T003 `extract/main_content.rs`: `MainCollector` buffers, boilerplate rules from research R5, paragraph breaks, `main_source`. Tests: US1.2 and US1.3 pages (different chrome, same content; same chrome, different content), header inside article kept, role attributes, nested main.
- [ ] T004 Selectors in `MainCollector`: content selector (first match), exclude selectors; `selector_missing` fallback (US4.2). Tests: related-products exclusion (US4.1), invalid selector never reaches the walk.
- [ ] T005 Wire the collector into `walk.rs`; `PageData.main: MainContent { text_hash, words, source, signature }`. Tests: existing 005 suite unchanged; main word count on the malformed corpus never exceeds the full word count.

## Phase 2: Settings and storage (plan D3, D6)

- [ ] T006 `config`: `ContentSettings` with defaults and validation (ranges, floor ≤ threshold, selectors parse, at most 50). Tests: old profiles load, round trip, error messages.
- [ ] T007 Migration step (number at merge): `pages.main_*`, `content_sig`; store insert in the group commit; `data_features += near_dup`. Tests: new file schema, older file upgrade on resume, read-only view of a file with the step by a reader without it.
- [ ] T008 Crawl-speed check: 100k benchmark before and after, recorded in `benchmarks.md` (SC-004: at least 98%).

## Phase 3: Analysis (plan D4, D5)

- [ ] T009 `results/near_dup/lsh.rs`: band parameters from the floor (research R3 table as a test), band keys, radix sort, buckets, dense groups for buckets over 256.
- [ ] T010 [P] `results/near_dup/score.rs`: pair scoring, top 50 per page, counts, union-find clusters, stable numbering, representative, sample for groups over 4,000 with `near_count_is_min`.
- [ ] T011 `results/near_dup/pass.rs`: eligibility query and statuses (FR-001, FR-004), outputs replaced in one transaction, settings in `meta.analysis`, cancellation and progress. Runs through the spec 009 runner when it is on main; until then called from the crawl-end hook and the CLI with the same signature.
- [ ] T012 Fixture site `test-fixtures/src/near_dup.rs`: groups with known similarity (100, 97, 92, 85, 70%), shared chrome, related-products block, short pages, noindex and canonicalised copies, a 300-page template group, Japanese pages. End-to-end tests for US1.1 to US1.5, US4.3, US4.4, US5.3 (scaled), FR-014.
- [ ] T013 Benchmarks: 100k fixture analysis (SC-001), synthetic 1M signature file with a 20k group (SC-002, SC-005), threshold change (SC-006); numbers in `benchmarks.md`.

## Phase 4: Results crate and CLI (plan D7, D8, D10)

- [ ] T014 Content tab `TabDef` with columns and filters, `Need::ContentSig`, `Need::NearDupAnalysis`, "Not analysed yet" state (US1.6). Tests: filter URL sets on the fixture crawl.
- [ ] T015 [P] Queries: status, clusters, pairs; exports (pairs, cluster). Tests on the fixture crawl.
- [ ] T016 [P] `near_dup/diff.rs` and `compare`: re-extraction from `bodies`, paragraph alignment, word diff, full-text mode, `html_not_stored`. Tests: exactly the changed words are marked (US2.3), excluded parts marked (US2.4).
- [ ] T017 Selector change rerun from stored HTML (D5), parallel, writable vs read-only files (SC-007, US4.1, US5.1).
- [ ] T018 CLI `duplicates` per contracts/cli.md. Tests: each view and format, overrides do not change the file, exit codes, parity with the results API (SC-008).

## Phase 5: Desktop app and UI (plan D11)

- [ ] T019 Tauri commands per contracts/desktop-commands.md.
- [ ] T020 UI: Content tab in the sidebar, Pages/Clusters switch, Clusters grid, cluster drill-down. Vitest + Playwright with the fake transport.
- [ ] T021 [P] UI: inspector Duplicates section with "50 of N" and Compare buttons. Vitest.
- [ ] T022 [P] UI: CompareSheet (two synchronised columns, `[` `]` navigation, full-text toggle, unavailable state); `--diff-added`, `--diff-removed` tokens in `tokens.css` passing the contrast test. Vitest + Playwright, light and dark.
- [ ] T023 UI: Settings › Content section with validation. Vitest + Playwright.

## Phase 6: Issues and finish

- [ ] T024 `content.near_duplicate` rule in the spec 009 catalogue (after 009 code is on main), evidence, "available when the crawl ends" state (US3). Tests: rule URL set equals the Near duplicates filter; CLI `issues --fail-on medium` exit code.
- [ ] T025 Older files: pre-feature file with HTML is offered analysis and matches a new crawl (US5.1); without HTML shows unavailable (US5.2). Fixture files.
- [ ] T026 `/speckit-converge`; CI green on Windows, macOS and Linux.
