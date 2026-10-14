# Tasks: Results Interface

**Input**: [plan.md](./plan.md), [research.md](./research.md), [data-model.md](./data-model.md), [contracts/](./contracts/), [quickstart.md](./quickstart.md)

Each task ships with its tests. `[P]` = can run in parallel with the other
`[P]` tasks of the same phase. Tasks marked **after 005** are written as
ignored tests now and enabled when spec 005 lands.

## Phase 0: Spec alignment

- [x] T000 Amend spec FR-003: rolling speed window 5 s (as spec 008), on Ryware's approval (applied to spec.md 2026-10-13)

## Phase 1: Setup

- [x] T001 New crate `crates/results` (`spiderlens-results`), rusqlite with `functions` and `collation`; wire into workspace, CLI and app
- [x] T002 [P] UI: add `@tanstack/react-table`; `ui/src/results/` folder
- [x] T003 [P] Fixtures: `gen-crawl` binary writing a synthetic crawl file (N URLs, links, titles with known duplicates, statuses, external share) in the real schema (as `crates/results/examples/gen_crawl.rs`)
- [x] T004 [P] Fixtures: `issues-site` (pages with missing, duplicate, long and short titles; 404s, 5xx, redirect chain and loop; long, uppercase, underscore and query URLs; robots-blocked and excluded URLs; content issues for 005 seeded now) (as a site built inside `crates/results/tests/issues.rs`)

## Phase 2: Foundation (blocks all stories)

- [x] T005 `sql.rs`: `ci_contains`, `natural` collation, `norm_ws`; unit and property tests (Hebrew, Greek, accented, digits) (a `natkey()` sort-key function replaced the collation (2.5x faster))
- [x] T006 `tabs.rs`: all eleven tabs, columns and filters from FR-009 with `needs`; ts-rs export; test that every tab × filter × sort column executes on a base file and on a gen-crawl file
- [x] T007 Availability: `Features` from schema introspection and meta; unavailable columns and filters with reasons; test with an old-format file (detected from meta `data_features`; spec 005 names to add when it lands)
- [x] T008 `view.rs`: open with first-screen top-k, background materialise into temp table, rows by position, locate, close; tests for order, empty-last, ties by id
- [x] T009 `counts.rs`: one-pass filter counts and duplicate sets; exact counts equal row totals for every filter (FR-006 style property)
- [x] T010 App: `results.rs` reader thread (one read-only connection per file, WAL), commands of contracts/desktop-commands.md except export; transport methods and fake transport

## Phase 3: US1 watch a crawl and browse (P1) — MVP

- [x] T011 [US1] Windowing component with scaled scrolling (cap 2^24 px), wheel and keys in row units when scaled; tests at 1k, 1.5M and 5M rows (position ↔ index round-trip, last row reachable)
- [ ] T012 [US1] `usePagedRows`: 100-row pages, 30-page LRU, version-tagged requests, skeleton rows; tests for stale replies and eviction (done: cache keeps the 30 pages nearest the last one loaded, stale replies dropped by version; open: dedicated tests)
- [x] T013 [US1] Grid: TanStack Table column model, 28 px rows, cell renderers per kind (status badge, state, URL with readable form, numbers tabular), truncation with tooltip
- [x] T014 [US1] ResultsView in the main area; sidebar result tabs live with counts; empty states ("results will appear…", "No URLs match this filter") (sidebar counts: `tab_counts`, one pass over `urls` for every tab, cached per data version; ~35 ms on 100k URLs)
- [x] T015 [US1] Live refresh: `view_refresh` driven by progress `version`, adaptive interval, anchor and selection kept; Rust and UI tests
- [x] T016 [US1] Engine: `active_ms` in meta and progress events; status bar and overview use it (FR-004); `stopping` state shown
- [x] T017 [US1] Engine + overview: failures by error class in the completeness summary (FR-005); counts equal stored states (FR-006 test)
- [x] T018 [US1] Open crawl… (⌘O, palette): read-only view of a file, Resume when paused; reopen gives identical views test (US1-4)
- [x] T019 [US1] Screenshots of the table (dark and light, live and finished) for Ryware's design review before merging Phase 3 (screenshots in /mnt/project-files/specs-drafts/007/screenshots)

## Phase 4: US2 filters, sort, search, columns (P1)

- [x] T020 [US2] Filter strip: chips with counts (refresh ≤ 5 s live, exact at end), disabled with reason when unavailable; Response codes scope switch (all, internal, external)
- [x] T021 [US2] Sort by header click asc/desc with indicator; natural text, numeric numbers, empty last
- [x] T022 [US2] Search field (debounced) on URL + main column; count updates
- [x] T023 [US2] Column menu: show, hide, reorder (drag and keyboard: Alt/⌥+↑/↓ on a shown column, announced), resize, reset; persisted per tab across restarts
- [x] T024 [US2] Per-tab session memory of filter, search, sort (FR-014); keyboard operation of the table: arrows, Shift range, Home/End, Page keys, Enter to inspector (Escape back), ⌘1..⌘9 tabs (FR-018)
- [x] T025 [US2] Issues-site acceptance tests for every base filter (SC-002 now); content filters **after 005**

## Phase 5: US3 inspector (P2)

- [x] T026 [US3] `detail.rs`: response details, redirect chain with loop and over-limit flags, counts; inlinks and outlinks paged (anchor text and rel **after 005**)
- [x] T027 [US3] Inspector UI replacing the placeholder: sections, "does not apply" and "not collected yet" states, copy and "show in table" on list rows; follows keyboard selection (link, image and redirect rows: Copy URL and Show in table on hover, focus and right-click; show in table picks a tab from `url_tabs` and clears a filter or search that hides the URL)
- [ ] T028 [US3] Paged lists in the inspector reuse the windowing component; 100,000-inlink test (FR-025, SC-006) (open: lists page with "Show more", not the windowing component; the lists live inside the scrolling inspector with rows of varying height, so windowing them needs their own fixed-height scroller first)
- [x] T029 [US3] Headers, images, headings, raw HTML sections: UI and queries behind `needs` (**after 005** for data). Raw HTML: `Reader::page_html` (decoded through the engine's one `bodies` reader, `extract::body::unpack`, in the encoding extraction used) and `url_html`; the detail pane's Page HTML section opens a windowed, line-numbered, searchable source viewer with Copy, and explains the Store page HTML setting when the crawl did not keep it

## Phase 6: US4 copy and export (P2)

- [x] T030 [US4] CSV and TSV writer: BOM, RFC 4180 quoting, CRLF, formula neutralising; property tests
- [x] T031 [US4] Export job: snapshot read transaction, `.part` then rename, cancel and failure cleanup, progress events; tests for cancel, disk error (read-only dir), snapshot during writes
- [x] T032 [US4] Export UI: button in the filter strip, save dialog, progress in the status bar, cancel; export of a selection (progress shows in the results footer)
- [x] T033 [US4] Copy: cell, rows as TSV with header (`copy_rows`), over 10,000 rows redirected to export with explanation
- [x] T034 [US4] CLI `spiderlens export` and `--list-tabs`; parity test: CLI CSV equals desktop export for the same view

## Phase 7: US5 scale (P2)

- [ ] T035 [US5] Perf harness (Rust): open, sort every column, search, filter counts, deep jumps on a 1M gen-crawl file; thresholds from SC-003..SC-006, SC-009 (partly: `examples/bench.rs` times the queries; thresholds not asserted yet)
- [ ] T036 [US5] UI scroll test in the real webview (Playwright on Linux CI; manual run on Ryware's Mac): drag top to bottom, frame times, row fill ≤ 200 ms
- [ ] T037 [US5] Memory check 100k vs 1M (SC-008)
- [ ] T038 [US5] Live: crawl the 100k bench site with the app open and a sorted view, compare to CLI speed (SC-007 ≥ 95%)
- [ ] T039 [US5] Fix what T035–T038 find (indexes on temp tables, page size, refresh budget)
- [ ] T040 [US5] Record the numbers in research.md, including a run on Ryware's Mac

## Phase 8: Polish

- [x] T041 `scope_test` desktop command (spec 003 T023 carry-over) and its place in Settings › Scope
- [x] T042 Docs: README section, CLI help; update spec 003 contract (crawl_urls superseded)
- [ ] T043 `/speckit-analyze`, then `/speckit-converge` for what can converge before 005

## After spec 005 (tracked here, done with 005)

- [x] T044 Content tabs and filters wired to spec 005's tables (SC-002 complete: `crates/results/tests/content.rs`), indexability, detail sections (size, response time, indexability, response headers, headings, anchor and alt text). Engine writes `data_features`; files from before it fall back to `extraction_version`. Settings › Page data › Store page HTML. Not yet: a viewer for the stored HTML (`spiderlens html` reads it today)
- [ ] T045 Final `/speckit-converge` for 007

## Dependencies

Phase 1 → Phase 2 → Phase 3 (MVP). Phases 4, 5, 6 can proceed in parallel
after Phase 3; Phase 7 after 4–6. T019 gates merging Phase 3.
