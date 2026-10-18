---
description: "Task list for spec 011, sitemaps and orphan pages"
---

# Tasks: Sitemaps and Orphan Pages

**Input**: [plan.md](./plan.md), [spec.md](./spec.md), [research.md](./research.md), [data-model.md](./data-model.md), [contracts/](./contracts/), [quickstart.md](./quickstart.md)

**Tests**: required (constitution VII). Every acceptance scenario maps to a test against the sitemap fixture; every task that touches `ui/` includes its Vitest tests in the same task.

**Format**: `- [ ] T### [P] [USn] Description`. `[P]` = parallel with other `[P]` tasks of the same phase.

**PRs**: Phases 1 to 5 are code PR **011a** (no dependency on spec 009). Phases 6 to 9 are code PR **011b**, started when the spec 009 code (catalogue and analysis phase) is on main.

## Phase 1: Setup

- [ ] T001 Baseline: on the merge base, run the spec 008 100,000-page bench 5 times with sitemaps off and record median time, URLs/s and peak RSS in `specs/011-sitemaps-orphan-pages/benchmarks.md` (SC-002 baseline).
- [ ] T002 [P] Move `quick-xml` and `flate2` to `[workspace.dependencies]`; use them from `spiderlens-engine` and `spiderlens-results`.
- [ ] T003 [P] Bundle the protocol XSDs under `specs/011-sitemaps-orphan-pages/xsd/` with their source URLs and licences in a README (research R6).
- [ ] T004 [P] Sitemap fixture site `crates/test-fixtures/src/sitemaps.rs` and bin `sitemap-site` (port 8711): robots.txt with an index of two sitemaps (one gzip), the seeded pages and files of US1 to US4 and US6 (redirect, 404, 500, noindex, canonicalised, robots-blocked, excluded, linked-but-unlisted, orphan, nofollow-only, redirect-reached, self-linked, oversized file, HTML file, 404 file, relative and cross-host `loc`, duplicates, bad and future `lastmod`, identical-`lastmod` file, `Last-Modified` headers, images and hreflang in entries), plus a generator for N entries in M files (benchmark).

## Phase 2: Foundational (engine and storage)

- [ ] T005 Migration step in `crates/engine/src/crawl/migrate.rs`: `sitemap_files`, `sitemap_entries`, `sitemap_refs`, `sitemap_problems`, `orphans`, `url_sources` and indexes from data-model.md; version = main's `LATEST + 1` at merge. Test: a format-2 file upgrades, gap test passes, older-reader `read_compat` unchanged.
- [ ] T006 Streaming parser in `crates/engine/src/crawl/sitemap.rs`: `read(bytes, on_entry) -> Result<FileFacts, FileError>` with `lastmod`, `changefreq`, `priority`, `image:image/image:loc`, `xhtml:link` (namespace by URI, not prefix), error line from the reader position, stop after 50,000 entries with `over_limit`. Keep the existing tests; add property tests (any generated urlset round-trips) and the edge cases of the spec (whitespace, entities, CDATA, news and video namespaces ignored).
- [ ] T007 [P] W3C datetime parser (`sitemap::parse_lastmod`, research R3) with unit and property tests (all six forms, invalid months and days, leap years, offsets).
- [ ] T008 Store writes in `crates/engine/src/crawl/store.rs`: `upsert_sitemap_file`, `insert_sitemap_entries(batch)`, `insert_sitemap_refs`; keep writing the base `sitemaps` row; add `sitemap_entries` to `data_features`.
- [ ] T009 `NotCrawledReason::SitemapOnly` in `reason.rs` with `can_upgrade() = true`, label "Found only in sitemap"; update the FR-038 order test and the 007 reason labels.

## Phase 3: Sources and settings (US6, US3 sources)

- [ ] T010 [US6] `SitemapSettings` and `LocalSitemap` in `crates/config/src/model.rs` with old-key mapping (research R10) and validation in `validate.rs`; tests: old profile on/off maps to both flags, new defaults, invalid URLs and missing files reported.
- [ ] T011 [US6] `SitemapJob` loop in the runner replacing `maybe_start_sitemaps`: sources robots.txt, defaults (`/sitemap.xml`, `/sitemap_index.xml` when robots lists none, research R9), user URLs, local files (read with `spawn_blocking`); `Msg::SitemapEntries` batches of 1,000; per-file facts; index depth and loop detection; per-host rate limit and robots.txt applied to sitemap requests. Tests US3 scenarios 1, 7, 8 and US6 scenarios 1, 2.
- [ ] T012 [US6] `crawl_sitemap_only = false` path: sitemap-only URLs recorded with `SitemapOnly`, upgraded when linked. Tests US6 scenario 3 and US2 scenario 7.
- [ ] T013 [US6] Image and hreflang refs from entries: discovered under scope rules (images as status checks per spec 003 FR-033, alternates with source `sitemap`); refs stored with `url_id`. Test: FR-008.
- [x] T014 [US6] List mode sitemap source (`ListInput::Sitemap`): the runner reads the sitemap (and children) and uses the page URLs as the list. Test US6 scenario 5.
- [x] T015 [P] [US6] CLI crawl flags from contracts/cli.md in `crates/cli/src/options.rs`, with tests.
- [ ] T016 [P] [US6] Settings › Scope › Sitemaps group `ui/src/settings/SitemapSources.tsx` (two switches, URL list, local files with base URL and file picker), wired into `categories.tsx`; Vitest tests for every control, validation messages, keyboard order; Playwright: toggle, add URL, add file, save profile.

## Phase 4: Sitemap files view (US3)

- [ ] T017 [US3] `crates/results/src/sitemaps/mod.rs`: `Need::SitemapEntries`, `sitemap_files()` tree with problem counts (ingestion-time problems now, pass problems once phase 6 lands), fallback to the base `sitemaps` table for older files; tests including US6 scenario 4.
- [ ] T018 [US3] Tauri command `sitemap_files` in `app/src-tauri/src/results.rs` and `view` support for `sitemap_file`; fake transport entries in `ui/src/testing/fakeTransport.ts`.
- [ ] T019 [US3] `ui/src/sitemaps/SitemapFiles.tsx`: virtualised tree panel from the sidebar, source badges, status, entries, problems, selection filters the Sitemaps tab; Vitest (tree keyboard navigation, `aria-expanded`, older-file notice, every palette via the token test) and Playwright (open, expand, select a file).
- [ ] T020 [P] [US3] CLI `spiderlens sitemaps` (plain tree and `--json`) in `crates/cli/src/sitemap_cmd.rs` with tests.

## Phase 5: Generation (US5)

- [ ] T021 [US5] `crates/results/src/sitemaps/generate.rs`: `GenerateOptions`, selection query (FR-020), pattern filter in Rust, `FileSplitter` (count and byte limits), index file, `lastmod`, `changefreq`, `priority`, deterministic gzip (research R7), temp-file-and-rename sink, dry-run sink. Tests US5 scenarios 1, 2, 5, 6.
- [ ] T022 [US5] Images (inline and separate, 1,000 per page cap, other hosts opt-in) and hreflang alternates restricted to selected pages, in `generate.rs`. Tests US5 scenarios 3, 4 and canonical-pair edge case.
- [ ] T023 [US5] Structural validator (`sitemaps/xml.rs` test helper) on all OSes and `xmllint --schema` in the Linux CI job (`.github/workflows/ci.yml`, install `libxml2-utils` if missing); round-trip test (generated files read back by the T006 parser give exactly the selected pages) and byte-identical rerun test (SC-006).
- [ ] T024 [P] [US5] CLI `spiderlens sitemap generate` with every option, `--dry-run`, `--force`, exit codes; test that CLI and library output match (US5 scenario 8).
- [ ] T025 [US5] Tauri commands `sitemap_preview` and `sitemap_generate` (refused while running), options saved per profile.
- [ ] T026 [US5] `ui/src/sitemaps/GenerateSheet.tsx`: option groups, live counts with debounce, base URL and folder picker, replace confirmation, result summary with the equivalent CLI line, unavailable state while running; command palette and crawl menu entries. Vitest for every option, counts, confirm, running state; Playwright: open from ⌘K, change options, generate with the fake transport.
- [ ] T027 [US5] Generation benchmark: 1,000,000 pages with images and hreflang on the generated fixture file, record in benchmarks.md (SC-005).

**Checkpoint (PR 011a)**: entries stored, sources and settings work, Sitemap files view and generation shipped, CLI `sitemaps` and `sitemap generate`; crawl bench within SC-002 and SC-003 (T028).

- [ ] T028 Crawl benchmarks: sitemaps off versus baseline (T001), sitemaps on with only linked URLs (2% target), 1,000,000 entries in 20 files (30 s and 50 MB targets); record in benchmarks.md.

## Phase 6: Analysis passes (011b, needs spec 009 code)

- [x] T029 [US3] [US4] `sitemap_audit` pass in `crates/results/src/sitemaps/audit.rs` registered in `PASSES`: location rule (research R4), duplicates, `lastmod` invalid, future, identical, older than the page, index loops and depth, empty files, robots listing, no sitemap found; writes `sitemap_problems`. Tests US3 scenarios 2 to 7, US4 scenarios 1 to 5, read-only file into temp. _(Part b: written without compiling; tests to add when CI runs.)_
- [x] T030 [US2] `orphans` pass in `crates/results/src/sitemaps/orphans.rs`: linked set, redirect and meta refresh fixpoint, nofollow-only class, `url_sources`, List mode skipped with reason, completeness note. Tests US2 scenarios 1 to 6 and 8. _(Uses spec 024 `orphan_candidates` in place of `url_sources`.)_
- [ ] T031 [US2] [US3] Pass budgets at 1,000,000 URLs and entries (SC-004), recorded in benchmarks.md.

## Phase 7: Rules (US1, US2, US3, US4)

- [x] T032 [US1] `crates/results/src/issues/rules/sitemaps.rs`: the 27 rules of the spec table with ids, priorities, thresholds, scopes, `need`, evidence (sitemap files per URL), why and fix texts; per-URL live rules for response, indexability, blocked, not crawled, not in sitemap; pass-backed rules for the rest; site-wide rules with the `detail` view (research R8, agreed with 009). Tests: each rule has a positive and a negative fixture case; US1 scenarios 1 to 8; FR-017 "not checked".
- [x] T033 [US1] `sitemaps` TabDef in `crates/results/src/tabs.rs` with the FR-026 filters pointing at rules (spec 009 D2) and the Sitemaps, Lastmod and Orphan class columns; the filter-equals-rule test covers them.

## Phase 8: Desktop and CLI read side (011b)

- [x] T034 [US2] Tauri commands `orphan_summary`, `sitemap_entries_for_url`; fake transport entries.
- [x] T035 [US1] [US2] Sitemaps tab UI: orphan header with completeness and List mode notices, provisional label while running; inspector "Sitemaps" section `ui/src/sitemaps/InspectorSitemaps.tsx`. Vitest and Playwright (filter to orphans, select a URL, see its sitemaps).
- [x] T036 [P] [US2] CLI `spiderlens orphans` (plain, CSV, JSON, `--nofollow-only`, completeness note) with tests.

## Phase 9: Polish

- [ ] T037 Update spec 003 docs (FR-034 note pointing at spec 011) and the CLI docs in `README.md`.
- [ ] T038 Run quickstart end to end on Linux; `/speckit-converge` until Converged; CI green on all three OSes.
