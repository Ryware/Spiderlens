# Tasks: Content Checks (spec 027)

**Input**: [spec.md](./spec.md), [plan.md](./plan.md), [research.md](./research.md), [data-model.md](./data-model.md), [contracts/](./contracts/)

Tests come first in each task (constitution VII). `[P]` = can run in parallel with other `[P]` tasks of its phase. One code PR per part; parts are independent (FR-000a). A and B start from main after 006 (#27) and 009 (#52) merge; until #52 merges, rule tasks wait and the rest proceeds.

## Part A: SERP preview (PR 1)

- [ ] T001 [P] `tools/serp-widths`: generator reading Liberation Sans, Noto Sans Hebrew and Noto Sans Arabic with `ttf-parser`, writing `crates/results/src/serp/widths.rs` and `ui/src/generated/serpWidths.json`; CI job re-runs it and fails on a diff.
- [ ] T002 [P] Fixture `crates/test-fixtures/data/serp/reference.csv`: 200 titles and 200 descriptions (Latin, Hebrew, Arabic, CJK, emoji, spaces) with reference widths measured in Chromium with Arial at 20 px and 14 px.
- [ ] T003 Tests then code: `serp::px_width`, `serp::truncate` (whitespace collapse, per-script fallback, RTL logical cut, 1,000-char early stop). SC-A1 test on T002.
- [ ] T004 Register `px_width` and `px_trunc` in `sql::register`; columns and filters in the Page titles and Meta description tabs; test on a pre-027 reference crawl file (A1.3, FR-A04).
- [ ] T005 Rules `rules/serp.rs` (5 rules, unit `Px`), `RULES_VERSION` bump; test that each filter and its rule list the same URLs (A1.4, 009 FR-005).
- [ ] T006 Benchmark: sort 100k pages by Title px under 1 s (SC-A2); fall back to a `Derived` table if it misses.
- [ ] T007 Schema step `serp_proposals`; commands `serp_proposal_get/set`, `serp_proposals_export`; tests (A2.2, A2.3).
- [ ] T008 [P] UI `SearchPreview.tsx` in the Inspector: desktop and mobile previews, editable title and description, local measuring from `serpWidths.json`, "no description" note (A2.1, A2.4). Vitest including a Rust/TS parity test on 50 fixture strings; Playwright: select row, edit title, see width change.
- [ ] T009 SERP list mode: `serp_list_open` building an in-memory view with the same tab definitions; start-screen entry; CLI `spiderlens serp`; tests for 500 rows under 2 s and no network (A3.1, A3.2). Vitest + Playwright.

## Part B: Segments (PR 2)

- [ ] T010 [P] Tests then code: `config::segments` types, validation (op/field match, pattern limits, unique names, max 100), serde in `CrawlConfig`, profile round trip (FR-B07, B3).
- [ ] T011 Tests then code: `results::segments::classify` and `build(conn, segments)` into `temp.url_segment`, first match wins, incremental extend for new ids (B1.2, B2.1).
- [ ] T012 Benchmark: rebuild with 100 segments on 100k and 1M URLs within SC-B1.
- [ ] T013 Segment column and filter parameter in `view_query`, `counts`, `export_tab`, `issues_summary` (FR-B04, FR-B05, FR-B09); CLI `--segment` and `spiderlens segments`.
- [ ] T014 Commands `segments_validate`, `segments_set`, `segments_counts`, `segment_of`; scope tester shows the segment (FR-B08).
- [ ] T015 [P] UI `SegmentsSettings.tsx` (list, reorder, colour, clauses, inline errors) and `SegmentFilter.tsx` in the FilterStrip, filter persists across tabs (B1.4). Vitest; Playwright: define two segments, filter, switch tab.
- [ ] T016 Coordinate the FilterStrip change with the results follow-ups thread before opening the PR.

## Part C: PDF (PR 3)

- [ ] T017 [P] Fixture PDFs in `crates/test-fixtures/data/pdf/`: titled 12-page, scanned image-only, encrypted, damaged, 25 MB, one linking to `/only-from-pdf`, one with `X-Robots-Tag: noindex`; `expected.json`.
- [ ] T018 Config: `crawl_pdfs`, `follow_pdf_links`, `pdf_max_bytes`, `pdf_time_limit_ms` with validation and CLI flags.
- [ ] T019 Fetch: keep PDF bodies (type or `%PDF-` sniff) up to the PDF cap when on; unchanged when off (C1.5, C1.6).
- [ ] T020 Tests then code: `engine::pdf::reader` with `lopdf` + `pdf-extract`, per-page deadline, `catch_unwind`, watchdog, statuses (C1.1 to C1.4, C3.2).
- [ ] T021 Bounded reader pool with spill to `bodies` when full; runner integration; schema step `pdfs`; links into `links` with `source = 'pdf'`, discovery per setting (C2.1, C2.2, FR-C05).
- [ ] T022 PDFs tab, detail section, rules `rules/pdf.rs` (FR-C06, FR-C07); CLI export.
- [ ] T023 Benchmark SC-C2 (10k pages + 1k PDFs of 5 MB) and memory SC-C3.
- [ ] T024 [P] UI: crawl settings fields, PDFs tab view, PDF section in the Inspector. Vitest + Playwright.

## Part D: Spelling and grammar (PR 4)

- [ ] T025 New crate `crates/spell` with `spellbook` and `harper-core`; CI check that `spiderlens-engine` and `spiderlens-results` do not depend on it.
- [ ] T026 [P] Fixture pages: English and Hebrew with planted errors, prefixed Hebrew words, niqqud text, mixed-script text, wrong `lang`, code blocks, URLs, all-caps, hyphenated words, a footer typo repeated on every page; `expected.json`.
- [ ] T027 Tests then code: tokenizer (scripts, skips, hyphen parts, niqqud stripping), language choice (FR-D02), checker with ignore list (D1.1, D1.2, D1.4, D1.6, FR-D05, FR-D06).
- [ ] T028 Tests then code: dictionary manager: bundled English, manifest, download with SHA-256 check, add from files, remove; offline use (D2.1 to D2.3). Release CI job builds `dictionaries.json` from a pinned commit and uploads assets.
- [ ] T029 Schema step `spell_pages`, `spell_findings`; `SpellRun` over stored text in batches, low-priority thread during a crawl, on demand after (FR-D07); site-wide repeats (FR-D09).
- [ ] T030 Grammar for English pages via `harper-core` (D3.1, D3.2).
- [ ] T031 Rules `rules/spelling.rs`; Spelling tab; detail section; CLI `spiderlens spell` and `spell dictionaries` (FR-D11 to FR-D13).
- [ ] T032 Benchmarks SC-D2 and SC-D3; accuracy SC-D1.
- [ ] T033 [P] UI: Settings › Spelling (dictionary list with size and licence before download, add from file, ignore list), Spelling tab with progress, Inspector findings with ignore actions. Vitest + Playwright.

## Part E: AMP (PR 5)

- [ ] T034 [P] Fixture: valid AMP page, one page per failing check, mismatched pair, cross-host amphtml.
- [ ] T035 Config `check_amp`; `amphtml` as a discovery source when on (FR-E01).
- [ ] T036 Tests then code: `extract::amp` checks on the parsed DOM; schema step `amp_checks` (E1.2, FR-E02, FR-E03).
- [ ] T037 Rules `rules/amp.rs` including the site-wide pairing rule (E1.3, FR-E05); AMP tab with the core-subset note (FR-E04); CLI export.
- [ ] T038 [P] UI: AMP tab and Inspector section, crawl setting. Vitest + Playwright.

## Shared

- [ ] T039 SC-000: 008 speed benchmark with all defaults, before and after each part's PR, result in the PR description.
- [ ] T040 `/speckit-converge` after each part's PR merges; spec status and the feature-gap report rows updated.
