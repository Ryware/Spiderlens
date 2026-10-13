# Tasks: Crawl Frontier and Scope

**Input**: [plan.md](./plan.md), [research.md](./research.md), [data-model.md](./data-model.md), [contracts/](./contracts/)

## Phase 1: Setup

- [x] T001 Add `rusqlite` (bundled), `quick-xml`, `flate2` to the workspace; tokio `sync` and `signal`; remove `texting_robots`

## Phase 2: Foundational

- [x] T002 Config: new scope and limit fields with defaults, the legacy `"keep"` query setting still read (`follow_nofollow` renamed without an alias: no files were released with it); validation for new ranges; regenerate TS types
- [x] T003 `url_norm.rs`: `normalize_link` with percent-encoding pass, query rules, length limit; table and property tests (FR-001..FR-006)
- [x] T004 `robots.rs`: RFC 9309 parser and matcher with matched rule, crawl-delay, sitemaps, HTML detection; RFC example tests; single-page check switched over
- [x] T005 `crawl/reason.rs`, `crawl/scope.rs`: reasons, sources, `classify` in FR-038 order; www equivalence in `InternalHosts`; unit tests per reason
- [x] T006 `crawl/store.rs` + `crawl/window.rs`: schema, insert-if-new, upgrade rules (FR-039), queue window; bounded-window tests at small scale (large-scale run is open, see quickstart)
- [x] T007 Fixture `site.rs`: multi-host link-graph site, configurable robots.txt and sitemaps, request log; engine DNS override for tests

## Phase 3: US1 + US2 crawl once, breadth-first, robots.txt (P1)

- [x] T008 [US1] `fetch_once` and `crawl/links.rs` (anchors, base, canonical, hreflang, images, meta robots, X-Robots-Tag, Link header)
- [x] T009 [US1] `crawl/runner.rs` (host state lives in the runner): coordinator loop, BFS dispatch, per-host limits, rate cap, redirects as discoveries, start-redirect adoption
- [x] T010 [US1] Integration tests US1-1..6
- [x] T011 [US2] robots per host with retries, unavailable handling, crawl-delay; integration tests US2-1..6

## Phase 4: US3 + US4 scope rules and reasons (P2)

- [x] T012 [US3] Integration tests US3-1..8 (rules, limits, query params, folder, redirect to excluded)
- [x] T013 [US4] Summary and per-reason counts; stop and resume; reason fixture test (every reason once, SC-004)

## Phase 5: US5 + US6 nofollow and sitemaps (P3)

- [x] T014 [US5] Nofollow handling and upgrades; integration tests US5-1..4
- [x] T015 [US6] `crawl/sitemap.rs` with gzip and index levels; integration tests US6-1..4

## Phase 6: CLI

- [x] T016 New shared flags; `crawl` (new, resume, Ctrl-C pause), `report`, `scope test`
- [x] T017 CLI contract tests including JSON schema validation and US4-5 parity

## Phase 7: Polish

- [x] T018 2,000-page guard test; fmt, clippy, full test suite; quickstart map; spec 002 T020

## Phase 8: Desktop (after spec 017's shell)

- [x] T019 Engine: progress events carry queued, not crawled and errors for the status bar; `default_file_name` shared by CLI and desktop
- [x] T020 Desktop commands `crawl_start`, `crawl_pause`, `crawl_stop`, `crawl_resume`, `crawl_summary` (app/src-tauri/src/crawl.rs); one crawl at a time on its own thread; files go to the app data folder under `crawls/`
- [x] T021 UI: Start, Pause, Resume and Stop in the toolbar; live figures in the status bar; Overview shows figures, not-crawled reasons, robots.txt per site and sitemaps
- [x] T022 Settings sheet: list depth, stay in folder, sitemaps from robots.txt, query parameters (keep, remove some, remove all, sort), max URL length
- [x] T023 `crawl_urls` and `scope_test` commands: `scope_test` done in spec 007 T041; `crawl_urls` superseded by spec 007's view commands
