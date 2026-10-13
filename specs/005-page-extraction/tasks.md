# Tasks: Page Data Extraction

**Input**: [plan.md](./plan.md), [data-model.md](./data-model.md), [spec.md](./spec.md)

`[P]` = can run in parallel (different files). `[USn]` = user story.

## Phase 1: Setup

- [x] T001 Add `zstd` and `xxhash-rust` to the workspace; record the 100k bench baseline in `benchmarks.md` (plan D9)

## Phase 2: Pure extraction (`crates/engine/src/extract/`, plan D1, D2)

- [x] T002 [US1] `extract/text.rs`: whitespace collapse (FR-009), word count (FR-014, R8), xxh3 hashes (D5), unit tests
- [x] T003 [US3] `extract/directives.rs`: meta robots, crawler-specific meta and X-Robots-Tag into directive rows, known/unknown, page noindex/nofollow for a robots token (FR-016, FR-017), unit tests incl. US3.1, US3.2
- [x] T004 [US3] `extract/link_header.rs`: Link header canonical, hreflang, next/prev (moved from `crawl/links.rs`), tests
- [x] T005 [US1][US2][US3][US5] `extract/walk.rs` + `extract/mod.rs`: one parse (scripting off), one walk: titles, descriptions with head/body location, headings, lang, base, anchors/areas with text, alt, rel, noscript, canonical/hreflang/next/prev elements, meta refresh, images with srcset/size/alt state/in_link, JSON-LD, visible text; `PageData` (FR-006 to FR-029)
- [x] T006 [P] [US5] `extract/microdata.rs`: item trees with type, id, nested items (FR-029), tests incl. US5.4
- [x] T007 [US4] Encoding, partial and failure flags: `encoding_errors`, `partial`, `no_content`, caught panics (FR-007, FR-008, FR-033)
- [x] T008 [P] [US6] `extract/body.rs`: zstd compress/decompress with a reused per-thread context, byte-identical round trip (FR-030)
- [x] T009 Unit tests mapping every US1-US5 acceptance scenario and edge case to `extract_page` (spec SC-001)

## Phase 3: Fetch and storage

- [x] T010 [US1][US4] `fetch.rs`: response headers kept, `Content-Length` for unread bodies, partial body kept on size limit and dropped connection with the content problem still recorded (plan D7)
- [x] T011 [US1] `crawl/store.rs`: schema v2 (data-model.md), `extraction_version` meta, format-1 files open read-only and refuse resume, `record_page()` with cached statements, `page_html()`, `page_data()` read-back
- [x] T012 [US2] `links` rows with context columns; non-crawlable links stored with `skip_reason` and no target id (FR-024); `LinkKind` gains `meta_refresh`, `next`, `prev`
- [x] T013 [US1] `crawl/runner.rs`: response time measured before extraction; extraction and compression in `spawn_blocking` under a 64 MB byte budget (plan D3); header facts for every response; meta refresh as redirect kind and discovery (FR-002, FR-025)
- [x] T014 [P] [US6] `config`: `extraction.store_html` (default true, old configs read as default) (FR-031)

## Phase 4: Command line (contracts/cli.md)

- [x] T015 [US6] `crawl --no-store-html`
- [x] T016 [US1] `spiderlens page <FILE> <URL> [--json]`
- [x] T017 [US6] `spiderlens html <FILE> <URL> [--out PATH]`

## Phase 5: Fixtures and end-to-end tests

- [x] T018 [US1-US6] `test-fixtures`: extraction site serving one page per acceptance scenario (headers included, a PDF with X-Robots-Tag and Link canonical, a meta refresh, a truncated body, windows-1252 without charset); `tests/extraction.rs` crawls it and checks stored values through `page_data()`
- [x] T019 [US4] Malformed corpus of at least 50 pages + `scripts/browser-golden.mjs` (Chromium via Playwright) + committed `golden.json`; test asserts at least 98% agreement (SC-002, plan D8)
- [x] T020 [US6] Raw HTML byte-identical for sampled pages and average stored size at most 30% (SC-005)

## Phase 6: Performance and finish

- [x] T021 SC-004: a 50 MB page with 100,000 links extracted under 10 s (ignored test, run in the benchmark)
- [x] T022 Benchmark after on 100k pages: URLs/s, peak memory, file size; target at least 3,100 URLs/s here (plan D9); results in `benchmarks.md`
- [x] T023 `spiderlens check` uses `extract_page` for its title; clippy, fmt, full workspace tests; PR

## Deferred

- Transferred size of compressed responses (plan owner point 1)
- Per-crawl zstd dictionary (plan D6)
- Settings sheet toggle for raw HTML (results table thread)
