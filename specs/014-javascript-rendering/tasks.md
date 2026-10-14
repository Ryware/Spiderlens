---
description: "Task list for spec 014, JavaScript rendering"
---

# Tasks: JavaScript Rendering

**Input**: [plan.md](./plan.md), [spec.md](./spec.md), [research.md](./research.md), [data-model.md](./data-model.md), [contracts/](./contracts/), [quickstart.md](./quickstart.md)

**Tests**: required. Constitution principle VII maps every acceptance scenario to an automated test against the local fixture, and every task that touches `ui/` includes its Vitest tests in the same task.

**Format**: `- [ ] T### [P] [USn] Description`. `[P]` = can run in parallel with the other `[P]` tasks of the same phase (different files, no dependency on an unfinished task). `[USn]` = user story from spec.md. "Depends on" names tasks that must be finished first when it is not simply the previous task.

**Browser in tests**: tests that need a browser call a shared helper that skips with a message locally when none is found and fails when `SPIDERLENS_REQUIRE_BROWSER=1` (set in CI, research R13).

## Phase 1: Setup and spike

**Purpose**: answer the open technical questions before building on them, and record the baselines.

- [x] T001 Spike (research R1, R6, R12): in `crates/render/examples/spike.rs`, drive headless Chromium with `chromiumoxide` 0.9 and, separately, `chromey`, at 4, 8 and 16 tabs, with `Fetch` interception on every request and every request fulfilled from Rust (main document from memory, scripts, stylesheets and JSON from the local fixture through `reqwest`). Fixture: 2,000 client-rendered pages served by `spiderlens-test-fixtures` (3 scripts, 1 stylesheet, 2 JSON requests each). Measure pages per second, browser process tree memory (peak and steady), the number of paused requests waiting for an answer over time, and any request paused longer than 5 s (back-pressure or deadlock). Also check: pipe transport (`--remote-debugging-pipe`) support in each crate, base64 fulfil overhead versus `continueRequest`, and the CLI binary size added. Record results and the chosen crate as "R16. Spike results" in `specs/014-javascript-rendering/research.md`. Blocks T003, T009, T010, T016.
- [ ] T002 [P] CI browser availability check: add a step to the `test` job of `.github/workflows/ci.yml` on all three runners that runs a new `scripts/find-browser.sh` (bash, also under Git Bash on Windows; looks in the standard Chrome, Chromium and Edge locations and prints the path and `--version`) and writes the result to the job summary. It does not fail the job yet; T074 makes it binding. Record what each runner has in research R13. *(2026-10-14: step and script done; recording in R13 waits for the branch's first CI run, so the task stays open.)*
- [x] T003 Create the `spiderlens-render` crate: `crates/render/Cargo.toml`, `crates/render/src/lib.rs`, `crates/render/src/types.rs` with the public types from `contracts/render-crate.md` (bodies `todo!()` free: return `NotUsable::NotFound` until T009); add it to `members` and `default-members` in `Cargo.toml`; add the crate chosen in T001 plus `sysinfo`, `zip`, `sha2` to `[workspace.dependencies]`; extend the "Engine has no UI dependencies" step in `.github/workflows/ci.yml` to also check `spiderlens-render`. Depends on T001.
- [x] T004 [P] Baseline benchmark: create `specs/014-javascript-rendering/benchmarks.md`; on the merge base of this branch, run the 100,000-page bench (`bench-site --pages 100000`, `--speed fast`) 7 times alternating with an idle run, and record median and spread of time, URLs per second and peak RSS, plus the release sizes of the `spiderlens` and `spiderlens-desktop` binaries. This is the SC-001 baseline used by T080.
- [x] T005 [P] JavaScript fixture site skeleton: `crates/test-fixtures/src/js_site.rs` with a builder for pages that ship scripts, JSON endpoints, stylesheets, images, fonts and media; a second host `cdn.test` for third-party resources; per-path request counts, peak open requests per host and a request log at `/__stats`; `Cache-Control` per resource. Export it from `crates/test-fixtures/src/lib.rs`. Add `crates/test-fixtures/src/bin/js-site.rs` serving it on 127.0.0.1:8710 for manual checks (quickstart step 2).

**Checkpoint**: crate choice made, CI knows which runners have a browser, baselines recorded.

---

## Phase 2: Foundational (blocking prerequisites)

**Purpose**: settings, storage, the render crate's core, the comparison functions and the render stage that every story uses.

**No user story work starts before this phase is complete.**

- [x] T006 [P] Settings model: `RenderSettings` in `crates/config/src/model.rs` as field `rendering` of `CrawlConfig`, with every field and default from data-model.md ("`mode` default `raw`", "`max_render_secs` default `10`, 1 to 60", "`window.preset` default `auto`", "`block.images`, `block.media`, `block.fonts` default `true`", "`block.stylesheets`, `block.third_party` default `false`", "`concurrency` `null` = auto, 1 to 16", "`screenshots` default `false`", thresholds `20`, `50`, `50`), `#[serde(default)]`, ts-rs export. Extend `all_defaults_match_spec`, `serializes_every_field_including_untouched_defaults` and `missing_fields_take_defaults` (a config without `rendering` loads with mode `raw`). Commit the regenerated `ui/src/generated/*.ts`.
- [x] T007 Validation in `crates/config/src/validate.rs`: ranges from data-model.md ("`max_render_secs` 1 to 60", "`min_wait_secs` 0 to `max_render_secs`", "custom width 320 to 3,840, height 480 to 4,320", "`concurrency` 1 to 16", "`memory_budget_mb` 256 to 65,536", thresholds), include and exclude pattern syntax with the spec 002 codes (`rendering.include.syntax`), a warning when rendering is on and robots mode is `ignore`. Unit tests in the same file. Depends on T006.
- [x] T008 `Change::Rendering(RenderSettings)` in `crates/config/src/stored.rs`, applied by `effective()`, with a test (plan owner point 2; hold if the owner declines and render-later then records its settings in meta instead). Depends on T006.
- [x] T009 [P] Browser discovery in `crates/render/src/find.rs`: `find_browser` order (user path, previous download, Chrome, Chromium, Edge in the standard per-OS locations), version read and `TooOld` below the pinned major minus 4, `data_dir()` matching Tauri's `app_data_dir` for `com.ryware.spiderlens`. Unit tests with fake executables in a temp folder. Depends on T003.
- [x] T010 Launch and cleanup in `crates/render/src/launch.rs` and `crates/render/src/guard.rs`: launch flags from plan D1, temporary profile deleted on close and on `Drop`, pipe transport or the `--render-guard <pid>` process per T001's result (research R12), a startup sweep that deletes stale SpiderLens temporary profiles, `memory_bytes()` over the process tree with `sysinfo`, a test-only launch counter. Integration test: launch, render `about:blank`, close, no process left. Depends on T003.
- [x] T011 [P] Settle rule in `crates/render/src/settle.rs` as a pure state machine fed with events (load, request open and close with type and third-party flag, DOM mutation, clock): settles after 500 ms with no open request and no mutation, ignores `Ping`, `CSPViolationReport` and third-party beacons, applies the minimum wait first, stops at the cap. Unit tests with a fake clock, no browser. Depends on T003.
- [x] T012 Crawl file step: the new tables and indexes from data-model.md as one additive step (`Step { version, additive: true, sql }`) in `crates/engine/src/crawl/migrate.rs`; **step 5** (after 006's step 3 and 004's step 4, main at a6af84f); `SCHEMA` in `crates/engine/src/crawl/store.rs` unchanged. In `store.rs`: `enqueue_render`, `renders_after` (queue window refill), `mark_rendering`, `record_render` (final state and every rendered row of one page), `requeue_rendering`, `stop_renders`, `render_input`, `rendered_html`, `screenshot`, meta `renderer` and `render_version`, `data_features` entries. Tests: round trip of every table; a format-2 file from before this step opens and migrates; a raw crawl writes no rows in any new table. **Depends on spec 006 PR #27 being merged.**
- [x] T013 [P] `PageInput.keep_text` and `PageData.visible_text: Option<String>` in `crates/engine/src/extract/mod.rs` and `crates/engine/src/extract/walk.rs` (research R10). The raw crawl passes `false`; a test checks `visible_text` is `None` then and that the other fields are identical with either value.
- [x] T014 `crates/engine/src/extract/compare.rs`: multiset origin matching for titles, descriptions, headings, directives, structured data, links and images (keys from plan D5); comparison fields from data-model.md `render_comparisons`; dependence per type and overall (research R10). Unit tests from hand-written raw and rendered HTML pairs covering US3 scenarios 1, 2, 4, 5, 7 and US7 scenarios 1, 2 (no browser). Depends on T013.
- [x] T015 [P] `crates/engine/src/extract/challenge.rs`: bot challenge markers (FR-044) with unit tests on saved challenge pages and on normal pages that mention "captcha".
- [x] T016 One-page render in `crates/render/src/page.rs` and `crates/render/src/flatten.js` (plan D1): fresh browser context, `Fetch` on every request, main document fulfilled from `RenderJob.main` with `Content-Encoding` and `Content-Length` removed, every other request through `Network::fetch`, blocked types failed with `BlockedByClient`, dialogs dismissed, permissions denied, downloads denied, pop-ups closed, window and height extension to 12,000 px, settle rule from T011, open shadow roots and same-origin iframes flattened, `outerHTML` with doctype, console and exception capture (50 kept plus totals), main-frame navigation recorded as `js_redirect` and failed, crash and heap limit mapped to `Failed`/`PageTooLarge`, the `max_time + 2 s` hard stop. Tests against `js_site` pages with a test `Network` that proxies to the fixture. Depends on T005, T010, T011.
- [x] T017 Resource permits and the engine `Network` in `crates/engine/src/crawl/render/net.rs` and `crates/engine/src/crawl/runner.rs` (plan D4, research R7): `Msg::ResourcePermit` with a one-shot reply and `Msg::ResourceDone`; the coordinator grants permits under the host allowance, crawl-delay, Retry-After and the global caps, serves waiting permits before new page fetches, fetches robots.txt for hosts seen first through a resource, applies the three robots modes, keeps third-party requests under their own cap (8 open, 2 per host), and feeds response times and 429/503 into `SiteSpeed`. Unit tests on permit accounting: a host never exceeds its allowance counting pages and resources together; Retry-After holds resources too. Depends on T003.
- [x] T018 [P] Resource cache in `crates/engine/src/crawl/render/cache.rs`: byte-bounded LRU (64 MB), stores only responses that `Cache-Control`/`Expires` allow (no `no-store`, not `private`), freshness from `max-age` or `Expires`, no heuristic freshness. Unit tests.
- [x] T019 Render stage in `crates/engine/src/crawl/render/mod.rs` and `crates/engine/src/crawl/runner.rs` (plan D2, D8): `Run.render: Option<RenderStage>`, `None` unless mode is `javascript`; queue window refilled from `renders_queue`; dispatch up to the current limit; render task runs `Browser::render`, then `extract_page` on rendered and raw HTML with `keep_text` in `spawn_blocking` under the existing extraction budget, then `compare`; `Msg::Rendered` handled by `on_rendered`; retry once on a retryable failure; the crawl completes only when fetch and render are both done; `meta.fetch_completed_at`; end-of-run debug assertion that no row is left `queued` or `rendering` after `completed` or `stopped`. Depends on T012, T013, T014, T016, T017, T018.

**Checkpoint**: a crawl with rendering on can render queued pages and store results; stories can start.

---

## Phase 3: User Story 1, see what a search engine sees after JavaScript runs (P1) MVP

**Goal**: rendered values for every eligible page, rendered-only links crawled, CLI and desktop both able to run it.

**Independent Test**: crawl the US1 fixture pages with rendering off and on; off stores raw values only and does not find JavaScript-only pages; on stores rendered values and crawls every JavaScript-only page.

- [x] T020 [P] [US1] Fixture pages in `crates/test-fixtures/src/js_site.rs`: `/title-changed` (raw `Loading`, script sets `Red Shoes | Shop`), `/js-nav` (all navigation links inserted by script, pointing to pages linked from nowhere else), `/jsonld-added`, `/noindex-added`, `/js-redirect` (`location.replace` to `/redirect-target`), a fully server-rendered page.
- [x] T021 [US1] Enqueue in `crates/engine/src/crawl/runner.rs` `on_page` and `finish_page` (plan D2): eligible = internal, not check-only, HTML, 2xx (or 4xx/5xx with `render_error_pages`); include/exclude compiled once, non-matching pages inserted as `not_rendered`/`outside_rules`; with rendering on, `finish_page` keeps the compressed body, which goes to `render_inputs` when "Store page HTML" is off.
- [x] T022 [US1] Record rendered results in `crates/engine/src/crawl/runner.rs` `on_rendered` and `crates/engine/src/crawl/reason.rs`: rendered rows with origins, comparison row, rendered-only references through `discover()` with the page depth plus one and the rendered page's nofollow (FR-022), `rendered_links` and `raw_only_links`, new `LinkKind::JsRedirect` discovered like a redirect and stored in `renders.js_redirect` and `js_redirect_id` (FR-013), rendered noindex, nofollow and canonical in `rendered_pages` (FR-021). Raw rows are never updated (FR-020).
- [x] T023 [US1] CLI basics in `crates/cli/src/options.rs` and `crates/cli/src/crawl_cmd.rs`: `--render`, `--browser`, `--allow-browser-download`, the browser check before creating the file with exit code 7 (6 is taken by spec 006; `exit::NO_BROWSER` in `crates/cli/src/main.rs`) and the messages from `contracts/cli.md`, the renderer notice, and the summary's Rendering block (counts by state).
- [x] T024 [US1] Engine integration tests in `crates/engine/tests/rendering.rs` for US1 scenarios 1 to 6 (rendered and raw title both stored; JavaScript links stored as rendered-only and their targets crawled; JSON-LD marked rendered-only; "noindex only in rendered HTML" and rendered indexability; JavaScript redirect recorded and target discovered) and scenario 3 (rendering off: launch counter stays 0, no JavaScript-only link rows, targets not crawled).
- [x] T025 [US1] CLI parity test for US1 scenario 7 in `crates/cli/tests/render_cli.rs`: `spiderlens crawl --render` on the fixture and the engine API with the same settings produce identical `spiderlens page --json` output for every page (ignoring timings), with no window.
- [x] T026 [US1] `spiderlens page --json` and `CrawlStore::page_report` in `crates/engine/src/crawl/store.rs` and `crates/cli/src/crawl_cmd.rs`: `render`, `rendered` (items with `origin`), `comparison`, `dependence`, `resources`, `console` per `contracts/cli.md`; text output gets a short Rendering section. Tests in `crates/cli/tests/render_cli.rs`.
- [x] T027 [US1] Desktop crawl start in `app/src-tauri/src/crawl.rs` and `ui/src/settings/categories.tsx`: `crawl_start` and `crawl_resume` check the browser first and return `browser_unavailable: ...` without creating a file; new settings category "JavaScript rendering" with the mode switch only (full fields in T054). Vitest in `ui/src/settings/SettingsSheet.test.tsx`: the category renders, the switch writes `rendering.mode`, issues for `rendering.*` count in the category.
- [x] T028 [US1] Rendered values in results (plan D7, research R11) in `crates/results/src/features.rs`, `crates/results/src/reader.rs`, `crates/results/src/view.rs`: `Need::Rendering`, `Need::RenderedHtml`, `Need::Screenshots` detected from `data_features`; `ViewQuery.values` (`raw` or `rendered`, default `rendered` when available); temporary views `pages`, `page_text`, `directives`, `structured_data`, `links` and the `urls` overlay created in rendered mode and dropped in raw mode; a hidden "Values" column. Tests in `crates/results/tests/rendering.rs`: Page titles tab shows "Red Shoes | Shop" in rendered mode and "Loading" in raw mode; indexability uses the rendered noindex; Outlinks counts include rendered-only links only in rendered mode. (Done with views for `urls` and `pages` only; the list tables use per-expression rendered variants, see research R11 "Found during implementation".)
- [x] T029 [US1] UI values switch in `ui/src/results/ResultsView.tsx`, `ui/src/results/useView.ts` and `ui/src/transport.ts`: a toolbar switch "Rendered values / Raw values" shown only when the crawl has rendering, passed as `values` to `view_open` and `view_counts`, remembered per crawl in UI state. Vitest in `ui/src/results/Results.test.tsx`: hidden without rendering, toggling re-opens the view with the new `values`.
- [x] T030 [US1] Results performance with the temporary views: run `crates/results/examples/bench.rs` on a 100,000-page synthetic rendered file with and without the views and record the numbers in `specs/014-javascript-rendering/benchmarks.md`; if a tab is more than 20% slower, rewrite only its affected expressions with a `{pages}` placeholder (research R11 fallback). (Done: link counts, the first screen and counts rewritten; Inlinks/Outlinks sorts and title sorts stay 40 to 70% slower, options in benchmarks.md.)

**Checkpoint**: US1 works from the CLI and the desktop app. This is the MVP.

---

## Phase 4: User Story 2, rendering never slows the raw crawl (P1)

**Goal**: rendering off costs nothing; rendering on never holds up fetching except at the queue limit; pause and resume are exact.

**Independent Test**: 100,000-page benchmark with rendering off against the baseline; mixed fixture with rendering on, raw fetch rate read from the fixture log.

- [ ] T031 [P] [US2] Bench and fixture support: `--spa` mode in `crates/test-fixtures/src/bin/bench-site.rs` (each page an application shell whose script fetches its content and links as JSON, about 200 KB of script shared by all pages, cacheable) and a slow-render page set in `crates/test-fixtures/src/js_site.rs` (each page settles after 1 s).
- [ ] T032 [US2] Back-pressure in `crates/engine/src/crawl/runner.rs` `dispatch()`: stop starting page fetches while more than 50,000 renders are queued, resume below 45,000 (constants, overridable in tests); robots.txt and external checks continue; limiting reason `queue_full`. Test in `crates/engine/tests/rendering.rs` with the limits lowered to 20/15 and slow renders: fetching pauses at the mark and resumes, memory is not involved (queue lives in the file).
- [ ] T033 [US2] Pause, resume and stop for renders in `crates/engine/src/crawl/render/mod.rs` and `crates/engine/src/crawl/store.rs`: pause lets open renders finish and keeps the queue; resume requeues `rendering` rows; stop marks queued rows `not_rendered`/`crawl_stopped`. Test for US2 scenario 5: pause mid-render, resume, every eligible page rendered exactly once (one `rendered_at`, fixture main-document count unchanged by the resume).
- [ ] T034 [US2] One request per page (US2 scenario 3, part of SC-006): test in `crates/engine/tests/rendering.rs` that every rendered page's own path appears once in the fixture's request log, including after a crash retry.
- [ ] T035 [US2] Progress (plan D13) in `crates/engine/src/crawl/runner.rs` and `crates/cli/src/crawl_cmd.rs`: `Progress.render: Option<RenderProgress>` with rendered, queued, rendering, current and average pages per second, limit and limiting reason; the CLI's second progress line and "fetching done; rendering continues". Tests: the fields add up; `render` is `None` for raw crawls (US2 scenario 4).
- [ ] T036 [US2] Desktop progress in `app/src-tauri/src/crawl.rs`, `ui/src/transport.ts`, `ui/src/crawl/useCrawlRun.ts` and the status area in `ui/src/shell/panels.tsx`: a rendering row (rendered, waiting, rendering, pages per second, limiting reason in words) shown only when `render` is present. Vitest in `ui/src/crawl/CrawlRun.test.tsx`: row hidden for raw progress events, figures and the limiting reason shown for render events.
- [ ] T037 [US2] Rendering-off guarantee tests in `crates/engine/tests/rendering.rs`: with mode `raw`, the launch counter is 0, no row exists in any table from the T012 step, `Progress.render` and `CrawlSummary.render` are `None`, and the summary JSON has exactly the pre-feature keys (snapshot). Covers US2 scenario 1 functionally and FR-002; T080 covers the numbers.
- [ ] T038 [US2] Raw fetch rate while rendering (US2 scenario 2, SC-002 second half): an ignored test plus a `benchmarks.md` entry comparing the fixture-side fetch rate on a 10,000-page `--spa` site with rendering on and off until the queue mark; target: at least equal.

**Checkpoint**: speed guarantees hold; US1 and US2 both pass.

---

## Phase 5: User Story 3, compare raw and rendered HTML (P1)

**Goal**: the JavaScript tab with every FR-026 filter, and the side-by-side diff in the detail pane.

**Independent Test**: crawl the difference fixture set; each page appears under exactly the expected filters with the expected raw and rendered values.

- [ ] T039 [P] [US3] Difference fixture set in `crates/test-fixtures/src/js_site.rs`: title changed, title added, description added, description changed, H1 removed, H1 added, canonical changed, canonical added, noindex added, noindex removed, nofollow added, nofollow removed, 40 links of which 12 added, text grown from 50 to 900 words, JSON-LD added, hreflang changed, and an unchanged page.
- [ ] T040 [US3] JavaScript tab in `crates/results/src/tabs.rs`: `eligible` = has a `renders` row; FR-025 default columns (Address, Rendering status, Render time, Word count raw, rendered, change, Links raw, rendered, JavaScript-only links, JavaScript dependence, JavaScript errors, Blocked resources); the 26 FR-026 filters as SQL over `renders`, `render_comparisons` and the rendered tables; labels of "Contains JavaScript content" and "Content hidden from non-rendering crawlers" filled with the stored thresholds (FR-027, FR-030), which means `FilterDef.label` gains a template form filled by the reader. Extend the existing tab-definition tests (unique ids, `all` first, every filter compiles).
- [ ] T041 [US3] Detail data in `crates/results/src/detail.rs`: `UrlDetail.render: Option<RenderDetail>` (state, reason, attempts, time, window, JavaScript redirect, counts, comparison with both values of each changed element, dependence, stored flags); `url_list` takes `values` and returns `origin` for headings, outlinks and images; `ListKind::Resources` and `ListKind::Console`. Tests in `crates/results/tests/rendering.rs`.
- [ ] T042 [US3] Diff in `crates/results/src/html_diff.rs` (add `similar` to `crates/results/Cargo.toml`): normalise both sides (one tag per line, attributes sorted, whitespace collapsed, long `<script>` and `<style>` bodies folded to a hash line) and return hunks; `spiderlens html --diff` in `crates/cli/src/crawl_cmd.rs` prints them as a unified diff. Tests: identical pages give zero hunks; attribute order alone gives zero hunks; one inserted `<a>` gives one hunk.
- [ ] T043 [US3] Desktop commands in `app/src-tauri/src/results.rs` and `app/src-tauri/src/main.rs`: `url_html` with `version` (registering the command if spec 007 has not), `url_html_diff` per `contracts/desktop-commands.md`.
- [ ] T044 [US3] UI in `ui/src/results/Inspector.tsx`, new `ui/src/results/HtmlDiff.tsx` and `ui/src/transport.ts`: "Raw vs rendered" section with side-by-side panes, highlighted differences, next and previous difference, copy each side (US3 scenario 6); origin badges ("raw only", "rendered only") in the Headings, Outlinks and Images lists; the explanation for "Noindex only in original HTML" (US3 scenario 3); the JavaScript tab's "This crawl was not rendered" notice (FR-034). Vitest in new `ui/src/results/HtmlDiff.test.tsx` and in `ui/src/results/Results.test.tsx`: navigation between hunks, copy actions, badges, notice.
- [ ] T045 [US3] End-to-end filter test for SC-005 and US3 scenarios 1 to 7 in `crates/results/tests/rendering.rs`: crawl the T039 set with rendering on, then for every page assert the exact set of JavaScript filters it appears in and the raw and rendered values shown.

**Checkpoint**: all P1 stories complete.

---

## Phase 6: User Story 4, control how pages are rendered (P2)

**Goal**: every rendering setting works as documented, from the desktop app and the CLI, and is stored with the crawl.

**Independent Test**: crawl pages with known delays, resource types and robots rules under different settings; fixture logs and stored data show exactly the documented effect.

- [ ] T046 [P] [US4] Fixture pages in `crates/test-fixtures/src/js_site.rs`: settles after 2 s, polls forever, loads an image, a font, a video and a third-party script from `cdn.test`, loads `/private/app.js` (disallowed for SpiderLens by the default fixture robots.txt), a `/products/` section, a page that writes `window.innerWidth` into the DOM, a script with `Retry-After` on 429, a shared script with `Cache-Control: max-age=3600` and one with `no-store`.
- [ ] T047 [US4] Window sizes in `crates/render/src/page.rs` and `crates/engine/src/crawl/render/mod.rs`: `auto` from the user agent (mobile for Googlebot smartphone, Chrome mobile and custom strings containing `Mobile`), presets 411 × 731 and 1024 × 768, custom sizes, mobile emulation, height extension. Test for US4 scenario 7 via the `innerWidth` page.
- [ ] T048 [US4] Resource blocking tests for US4 scenarios 3 and 4 and FR-014 third-party blocking in `crates/engine/tests/rendering.rs`, read from the fixture log; `render_resources` rows show `blocked_setting`.
- [ ] T049 [US4] Politeness for resources (US4 scenario 5, FR-015, FR-016, SC-006) in `crates/engine/tests/rendering.rs`: `/private/app.js` never requested under `respect` and listed with its rule, page flagged "Has blocked resources"; requested and listed under `ignore_report`; the fixture's peak open requests per host never exceeds the per-site cap counting pages and resources; the 429 script's `Retry-After` holds further requests to that host.
- [ ] T050 [US4] Settle, timeout and minimum wait tests for US4 scenarios 1 and 2 and SC-007: the 2 s page ends shortly after settling, not at the cap; the polling page ends within `max_render_secs + 2 s` as `timed_out` with its DOM stored; `--render-min-wait 3` holds a fast page for 3 s.
- [ ] T051 [US4] Concurrency (US4 scenario 6, FR-037): default from total memory (GB / 4, clamped 1 to 16) in `crates/engine/src/crawl/render/mod.rs`; a test with `concurrency = 2` asserting the stage's peak open renders is 2.
- [ ] T052 [US4] Include and exclude test for US4 scenario 8: with include `/products/`, only those pages are rendered and every other eligible page is `not_rendered`/`outside_rules`.
- [ ] T053 [US4] All CLI flags from `contracts/cli.md` in `crates/cli/src/options.rs`, including the error for rendering flags without `--render`, and in `spiderlens config show`; tests in `crates/cli/tests/config.rs` that each flag maps to its field and that CLI defaults equal `RenderSettings::default()` (US4 scenario 9, FR-047).
- [ ] T054 [US4] Full settings category in `ui/src/settings/categories.tsx`: maximum render time, minimum wait, window (auto, mobile, desktop, custom size), "Load all resources" and the individual blocks, render error pages, include and exclude with the existing pattern tester, concurrency (auto or number), memory budget, screenshots, the three thresholds, and the browser status line with "Change browser" (calls `render_browser_set_path`). The stored rendering settings also appear in the crawl summary report for a reopened crawl (US4 scenario 9). Vitest in `ui/src/settings/SettingsSheet.test.tsx`: each control writes its field, validation issues show on the right control, auto values display as "Auto".
- [ ] T055 [US4] Cache test for FR-017 in `crates/engine/tests/rendering.rs`: across 50 pages the `max-age` script is requested once and the `no-store` script 50 times; `render_resources` marks hits `cached`.

**Checkpoint**: US4 settings are complete in both interfaces.

---

## Phase 7: User Story 5, know when rendering went wrong (P2)

**Goal**: every rendering problem is recorded against its page, the crawl always finishes, and missing browsers are handled before the crawl starts.

**Independent Test**: crawl pages that throw, log errors, request missing resources, never settle and crash the renderer; every page ends in exactly one final state with its problems stored.

- [ ] T056 [P] [US5] Fixture pages in `crates/test-fixtures/src/js_site.rs`: uncaught exception, `console.error` and `console.warn`, a script returning 404, a page that allocates past the heap limit, a page whose test hook crashes its renderer (CDP `Page.crash` sent by the test), a bot challenge page ("Just a moment..." with a challenge form).
- [ ] T057 [US5] Console, exceptions and failed resources stored (FR-043, FR-018) in `crates/engine/src/crawl/runner.rs` `on_rendered`; tests for US5 scenarios 1 and 2: message and source location stored, "JavaScript errors" and "Has failed resources" filters list the pages.
- [ ] T058 [US5] Crash handling (FR-042) in `crates/engine/src/crawl/render/mod.rs`: retry once in a fresh context, then `failed` with reason; `page_too_large` not retried; raw rows and `urls` unchanged. Test for US5 scenario 3.
- [ ] T059 [US5] Memory guard in `crates/engine/src/crawl/render/guard.rs` (plan D10): default budget total / 8, lower and raise rules, browser restart when over budget at limit 1 and every 1,000 pages, limiting reason `memory_guard`. Unit tests with a fake memory source for US5 scenario 4.
- [ ] T060 [US5] Browser download (FR-045, research R14) in `crates/render/src/fetcher/mod.rs`, `crates/render/src/fetcher/pins.rs` and new `scripts/pin-headless-shell.mjs`: plan, download with progress and cancel, SHA-256 check, unzip into `<data_dir>/browsers/`, `remove_download`; CLI `spiderlens browser status`, `install`, `remove` in `crates/cli/src/main.rs`. Tests with a local fixture serving a small fake archive: success, hash mismatch removes everything, cancel removes partial files.
- [ ] T061 [US5] No-browser flow on the desktop (US5 scenario 5): `render_browser_status`, `render_browser_install`, `render_browser_cancel`, `render_browser_set_path` and the two events in new `app/src-tauri/src/render.rs` registered in `app/src-tauri/src/main.rs`; new `ui/src/crawl/BrowserDialog.tsx` showing the download size with "Download and crawl" and "Crawl raw HTML instead", progress and errors, opened when `crawl_start` returns `browser_unavailable`. Vitest in new `ui/src/crawl/BrowserDialog.test.tsx`: consent required before install, raw fallback switches the mode off and starts the crawl, errors shown. CLI test in `crates/cli/tests/render_cli.rs`: `--render --browser /nonexistent` exits 7 and creates no file.
- [ ] T062 [US5] Summary totals (FR-041, US5 scenario 6): `CrawlSummary.render: Option<RenderSummary>` in `crates/engine/src/crawl/runner.rs`, the CLI text and JSON summary, `render_reasons` labels in `app/src-tauri/src/crawl.rs`, and the desktop summary view in `ui/src/crawl/`. Test that the counts add up to the eligible pages. Vitest for the summary block in `ui/src/crawl/CrawlRun.test.tsx`.
- [ ] T063 [US5] Fault injection and cleanup (SC-008, SC-009, FR-046) in `crates/engine/tests/rendering.rs` and `crates/cli/tests/render_cli.rs`: kill the browser process repeatedly during a crawl, the crawl completes with every eligible page in one final state and no raw data lost; a browser that cannot be relaunched turns the rest into `browser_unavailable` with a warning; kill the `spiderlens` process (SIGKILL on Unix, `taskkill /F` on Windows) during a rendered crawl and assert no browser process from it remains after 10 s.
- [ ] T064 [US5] Bot challenge wiring: `renders.bot_challenge` from `challenge.rs` in `on_rendered`, "Possible bot challenge" filter test.

**Checkpoint**: failures are never silent; P2 stories complete.

---

## Phase 8: User Story 6, keep rendered pages for later checks (P3)

**Goal**: rendered HTML and optional screenshots stored in the crawl file and retrievable from both interfaces.

**Independent Test**: crawl with screenshots on and off; retrieve rendered HTML and screenshots for sample pages and compare with what was produced at crawl time.

- [ ] T065 [US6] Rendered HTML storage (FR-032) in `crates/engine/src/crawl/runner.rs` and `crates/engine/src/crawl/store.rs`: compressed into `rendered_bodies` when "Store page HTML" is on; `CrawlStore::rendered_html`; `spiderlens html --rendered`. Tests for US6 scenarios 1 and 4: bytes identical to the serialised DOM returned by the render (captured in the test), nothing stored with `--no-store-html` while all extracted rendered data still is.
- [ ] T066 [US6] Screenshots (FR-033, plan D9) in `crates/render/src/page.rs`, `crates/engine/src/crawl/store.rs` and `crates/cli/src/main.rs`: WebP capture after settling, height capped at 8,192 px, stored in `screenshots`, `spiderlens screenshot --out`. Tests for US6 scenarios 2 and 3: an image of the window width exists only with `--screenshots`.
- [ ] T067 [US6] Desktop sections in `app/src-tauri/src/results.rs` (`url_screenshot`) and `ui/src/results/Inspector.tsx`: "Rendered HTML" (chunked viewer as for raw HTML) and "Screenshot" (shown when stored). Vitest in `ui/src/results/Results.test.tsx`: sections appear only when stored, unavailable reasons shown otherwise.

---

## Phase 9: User Story 7, see how much of each page depends on JavaScript (P3)

**Goal**: per-page JavaScript dependence values stored, shown, filtered and exported.

**Independent Test**: crawl pages from fully server-rendered to fully client-rendered and check the stored dependence values against the expected shares.

- [ ] T068 [P] [US7] Dependence ladder fixture in `crates/test-fixtures/src/js_site.rs`: pages whose rendered text is 0%, 25%, 50%, 75% and 100% script-added (exact word counts), plus an empty application shell with title and H1 added by script.
- [ ] T069 [US7] Dependence in results and export in `crates/results/src/tabs.rs` and `crates/results/src/export.rs`: the "JavaScript dependence" column, per-type columns (hidden by default) and the "Content hidden from non-rendering crawlers" filter (overall above the threshold, or title or H1 only in the rendered page). Tests for US7 scenarios 1 to 3 and SC-011 (each value within 2 percentage points) in `crates/results/tests/rendering.rs`, including `spiderlens export --tab javascript` and `spiderlens page --json`.
- [ ] T070 [US7] UI: a "JavaScript dependence" block in the render section of `ui/src/results/Inspector.tsx` (overall percentage and per-type shares, raw presence of title, description, canonical and directives). Vitest in `ui/src/results/Results.test.tsx`.

---

## Phase 10: Render an existing crawl (FR-006, cross-story)

**Purpose**: render a finished raw crawl later from its stored HTML (plan D11).

- [ ] T071 Engine in `crates/engine/src/crawl/runner.rs` and `crates/engine/src/crawl/render/mod.rs`: `Crawler::open_for_render(path, RenderSettings)` refuses running crawls, already rendered crawls and crawls with no stored HTML (with the explanation); records `Change::Rendering`; enqueues eligible pages (`html_not_stored` where needed); runs the render stage with the fetch stage idle. Test: fixture page paths requested zero more times, subresources requested politely. Depends on T008.
- [ ] T072 CLI `spiderlens render` in `crates/cli/src/main.rs` and `crates/cli/src/crawl_cmd.rs` per `contracts/cli.md`, with pause and resume; tests in `crates/cli/tests/render_cli.rs`.
- [ ] T073 Desktop `crawl_render` in `app/src-tauri/src/render.rs` and a "Render this crawl" action in `ui/src/shell/commands.ts` and the results toolbar, using the same browser dialog and progress. Vitest in `ui/src/shell/AppShell.test.tsx`: action hidden for rendered crawls and crawls without HTML, shown otherwise.

---

## Phase 11: Polish and cross-cutting checks

- [ ] T074 [P] CI binding in `.github/workflows/ci.yml`: replace `scripts/find-browser.sh` with `spiderlens browser status --json`; set `SPIDERLENS_REQUIRE_BROWSER=1` for `cargo test --workspace` on all three runners; on a runner without a browser, run `spiderlens browser install --yes` first (also exercising the download path).
- [ ] T075 [P] SC-004 accuracy: at least 50 JavaScript-dependent pages in `crates/test-fixtures/js-golden/`, new `scripts/render-golden.mjs` (Playwright Chromium, JavaScript on, same text rules as `browser-golden.mjs`) writing a committed `golden.json`, and a test in `crates/engine/tests/rendering.rs` asserting at least 98% of pages match on title, description, headings, canonical, robots directives, link list and structured data.
- [ ] T076 [P] Edge case tests in `crates/engine/tests/rendering.rs`: clean state (a cookie and `localStorage` value set by one page are not visible to the next, FR-011); dialogs, pop-ups, downloads and permission requests do not block (FR-012); a service worker page; fragment routes (`#!/products`) treated as one URL with the note; non-HTML, external, redirect and blocked URLs never rendered; a raw crawl file from before this feature opens with the JavaScript tab notice (FR-034).
- [ ] T077 [P] User documentation in `docs/javascript-rendering.md`: what rendering does, settings, the browser source and download, known differences from Google's renderer (fixed locale and timezone, cookies within one render, settle rule), and the AEO angle of JavaScript dependence. Linked from `README.md`.
- [ ] T078 [P] Spec follow-up for the owner: record the decisions on the three "Spec points that need the owner's nod" in `specs/014-javascript-rendering/spec.md` (FR-039 reason, `Change::Rendering`, SC-001 method) once given.
- [ ] T079 SC-010 usability check: three people who have not used rendering turn it on, crawl the fixture and find the pages with JavaScript-only links; record times and friction in `specs/014-javascript-rendering/benchmarks.md`; fix wording issues found.
- [ ] T080 SC-001 gate: rerun the T004 benchmark on this branch with rendering off (median of 7 alternating runs), record in `benchmarks.md`; merge is blocked if throughput is more than 2% lower or peak memory more than 5% higher than the baseline.
- [ ] T081 Render benchmark (SC-002, SC-003): 10,000-page `--spa` site, rendering on, defaults; pages per second, SpiderLens plus browser memory over time, file size with and without screenshots; then the 100,000-page memory run (memory at 100,000 at most 20% above 10,000). Record in `benchmarks.md`; adjust the default concurrency formula if the data says so.
- [ ] T082 Binary size: release sizes of `spiderlens` and `spiderlens-desktop` against T004; budget 8 MB added per binary; record in `benchmarks.md`.
- [ ] T083 Finish: run every step of `quickstart.md`, `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, `pnpm test`, then `/speckit-converge`.

---

## Dependencies and execution order

### Phases

- **Phase 1**: T001 first; T002, T004, T005 in parallel with it. T003 after T001.
- **Phase 2**: after Phase 1. T012 also waits for spec 006 PR #27. T016 needs T005, T010, T011. T019 needs T012 to T018. Blocks every story.
- **Phase 3 (US1)**: after Phase 2. MVP.
- **Phase 4 (US2)**, **Phase 5 (US3)**: after Phase 3 (they need rendered pages in the file). They can run in parallel with each other.
- **Phase 6 (US4)**, **Phase 7 (US5)**: after Phase 3; independent of US2 and US3 except T049 (uses the T017 permits) and T061 (uses T060).
- **Phase 8 (US6)**, **Phase 9 (US7)**: after Phase 3; US7's UI task T070 after T044 (same Inspector render section).
- **Phase 10**: after Phase 3 and T008.
- **Phase 11**: after the stories it checks; T080 and T083 last.

### Story dependencies

| Story | Depends on | Notes |
|---|---|---|
| US1 | Foundational | MVP |
| US2 | US1 | Measures and bounds the stage US1 built |
| US3 | US1 | Needs rendered rows and comparisons |
| US4 | US1 | Settings act on the stage |
| US5 | US1 | T061 depends on T060 |
| US6 | US1 | |
| US7 | US1, US3 (T040 tab, T044 Inspector section) | |

### Parallel examples

- Phase 1: T002, T004 and T005 alongside the T001 spike.
- Phase 2: T006, T009, T011, T013, T015 and T018 touch different files and can run together once T003 is in; T007 and T008 follow T006, T014 follows T013; T010, T016, T017 and T019 come after.
- US1: T020 (fixture) in parallel with T021 and T023; T028 and T029 can proceed while T024 to T026 are written.
- US2: T031 in parallel with T032 to T035; T036 (UI) after T035.
- US3: T039 (fixture) in parallel with T040 to T042; T043 then T044.
- US4: T046 (fixture) in parallel with T047 and T053; T054 (UI) in parallel with T048 to T052.
- US5: T056 (fixture) in parallel with T059 and T060.

## Implementation strategy

1. **MVP**: Phases 1 to 3. A rendered crawl from the CLI and the desktop app with rendered values in the existing tabs and JavaScript links crawled. Stop and review with the owner here, including the spike results.
2. **P1 complete**: Phases 4 and 5. Speed guarantees proven and the JavaScript tab with every filter. Run T080 at this point too, so a speed regression is caught early, not at the end.
3. **P2**: Phases 6 and 7.
4. **P3 and render-later**: Phases 8 to 10.
5. **Polish**: Phase 11, ending with the SC-001 gate, the render benchmark and `/speckit-converge`.

Each task is one reviewable change with its tests (constitution, Development Workflow).

## Task count

| Phase | Tasks |
|---|---|
| 1 Setup and spike | 5 (T001 to T005) |
| 2 Foundational | 14 (T006 to T019) |
| 3 US1 (P1) | 11 (T020 to T030) |
| 4 US2 (P1) | 8 (T031 to T038) |
| 5 US3 (P1) | 7 (T039 to T045) |
| 6 US4 (P2) | 10 (T046 to T055) |
| 7 US5 (P2) | 9 (T056 to T064) |
| 8 US6 (P3) | 3 (T065 to T067) |
| 9 US7 (P3) | 3 (T068 to T070) |
| 10 Render an existing crawl | 3 (T071 to T073) |
| 11 Polish | 10 (T074 to T083) |
| **Total** | **83** |
