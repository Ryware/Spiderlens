# Implementation Plan: JavaScript Rendering

**Branch**: `014-javascript-rendering` | **Date**: 2026-10-13 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `specs/014-javascript-rendering/spec.md` (approved 2026-10-13 with the recommended defaults) and [research.md](./research.md).

## Summary

Add an opt-in JavaScript rendering mode. When it is on, every eligible internal HTML page is fetched once by the existing crawl, extracted from its raw HTML as today, then rendered in headless Chromium and extracted again from the rendered DOM with the same `extract_page` function. Both versions are stored, compared, and shown in a new JavaScript tab, in the existing tabs (rendered values by default for rendered pages) and in the detail pane.

The approach, from research R1 to R3 and the planning additions R6 to R15:

- A new library crate, `crates/render`, finds or downloads a Chromium-family browser and drives it over the Chrome DevTools Protocol with `chromiumoxide`. It knows nothing about crawls; the engine hands it a page and a network to load resources through.
- The crawl becomes two stages. The fetch stage is unchanged. Eligible pages go into an on-disk render queue in the crawl file. A render stage inside the same coordinator pulls from that queue with its own concurrency (default 4, adjusted by a memory guard).
- The browser never touches the network. The engine answers every request it makes: the main document from the response already received, every other request through the spec 008 host limiter, robots.txt and a shared resource cache.
- With rendering off, the render stage does not exist: one `Option` check per page, no browser, no queue, no new rows.

Crawl speed is the first constraint. The plan's gate for that is SC-001: with rendering off, the 100,000-page benchmark stays within 2% of throughput and 5% of peak memory of the build without this feature.

## Technical Context

**Language/Version**: Rust 2024 (toolchain from `rust-toolchain.toml`, workspace `rust-version` 1.97), TypeScript 5 with React 18 for the UI.

**Primary Dependencies**: existing: Tokio, reqwest 0.12, rusqlite 0.40, scraper/html5ever, zstd, Tauri 2. New (justified in research R15): `chromiumoxide` 0.9 (fallback `chromey`), `sysinfo`, `zip`, `sha2`, `similar`.

**Storage**: the crawl file (SQLite). New tables and columns arrive as one additive migration step (see "Crawl file changes" below).

**Testing**: `cargo test --workspace`; render integration tests against a new JavaScript fixture site in `crates/test-fixtures`; a Playwright golden file for SC-004 (same pattern as spec 005 D8); Vitest for every UI change; the 100,000-page benchmark from spec 008 for SC-001.

**Target Platform**: Windows 10+, macOS (Intel and Apple Silicon), mainstream Linux desktops; desktop app and CLI.

**Project Type**: desktop app (Tauri) plus CLI over shared Rust libraries.

**Performance Goals**: rendering off: within 2% throughput and 5% peak memory of the baseline (SC-001). Rendering on: at least 10 pages per second on the 10,000-page client-rendered fixture with defaults on the reference machine (SC-002); raw fetching not slowed until the queue high-water mark.

**Constraints**: total memory below 3 GB in a 100,000-page rendered crawl, growth at most 20% from 10,000 to 100,000 pages (SC-003); a page that never settles ends within the cap plus 2 s (SC-007); no browser process left 10 s after SpiderLens is closed or killed (SC-009).

**Scale/Scope**: up to 100,000 rendered pages per crawl; render concurrency 1 to 16.

## Constitution Check

*Gate before research, re-checked after design.*

| Principle | How this plan complies |
|---|---|
| I. Spec before code | Spec approved 2026-10-13 (Ryware). This plan, research and tasks go to the owner before any code. Three spec points that need the owner's nod are listed below. |
| II. Local-first, no servers | The browser runs on the user's machine. The only network use beyond the crawl itself is the optional one-time `chrome-headless-shell` download from Google's static Chrome for Testing storage, with consent and a pinned SHA-256 (R14). No SpiderLens server. No user browser profile is ever used (FR-011). |
| III. Engine independent of the shell | `crates/render` and all render logic in `crates/engine` have no Tauri dependency; the CI check that guards `spiderlens-engine` is extended to `spiderlens-render`. **CLI parity**: every setting is a `spiderlens crawl` flag with the same default, and every new view has a CLI command (`browser status/install`, `render`, `html --rendered/--diff`, `screenshot`, `export --tab javascript`), see [contracts/cli.md](./contracts/cli.md). |
| IV. Respectful crawling | Each page's own URL is requested once (D3). Every resource request to a crawled host goes through the same per-host allowance, global caps, Retry-After and crawl-delay as page requests (D4); robots.txt applies to resources (FR-016); third-party requests have their own small cap. The browser identifies itself with the crawl's user agent. |
| V. Correct and complete data | Every eligible page ends in exactly one rendering state with a reason (D8). Raw data is never changed by rendering (FR-020, FR-042). The renderer name, version and source and every rendering setting are stored with the crawl (FR-003, FR-034). The summary reports rendering completeness (FR-041). |
| VI. Scales by design | The render queue lives in the crawl file (D2); input bodies are read from disk per page; the resource cache and every per-page buffer are byte-bounded; rendered rows go straight to disk. Performance impact is stated below and measured (D10). |
| VII. Test-first | Every acceptance scenario maps to a fixture page and test (tasks.md). Rendering is tested against the local fixture only. CI fails, not skips, if no browser is available on a runner (R13). **UI tests are mandatory**: every task that touches `ui/` includes Vitest tests in the same task. |
| VIII. Simplicity | One new crate, justified: the CDP client and browser management are large and unrelated to crawling, and keeping them out of `crates/engine` keeps the engine's compile time and its no-UI check simple. Five new dependencies, each justified (R15). The JavaScript tab is data in `tabs.rs` like every other tab; existing tabs show rendered values through temporary views, not a second set of definitions (D7). No setting beyond those the spec lists. |

**Speed first, explicitly**: rendering off costs one `Option::is_some()` check per finished page in the coordinator and one in `finish_page` (whether to keep the compressed body for rendering). No browser code runs, no thread is started, no memory is reserved, no table row is written. Task T080 compares the 100,000-page benchmark against the stored baseline and blocks the merge if throughput drops by more than 2% or peak memory grows by more than 5% (median of 7 alternating runs, because single runs vary by up to 7% on the reference container: `specs/005-page-extraction/benchmarks.md:14`).

**Desktop-first, no servers**: the desktop app is the primary surface (settings category, browser install consent, JavaScript tab, detail sections). The CLI mirrors it. Nothing listens on a network port: the CDP connection is a local pipe or a loopback port owned by the launched browser (R12), never a SpiderLens server.

Result: no violations. Complexity Tracking below records the one new crate.

## Design decisions

### D1. `crates/render`: browser management and one-page rendering

`spiderlens-render` exposes three things (full API in [contracts/render-crate.md](./contracts/render-crate.md)):

1. **Finding a browser** (`find_browser`): in order, a path the user gave (`--browser` or the app preference), then a `chrome-headless-shell` that SpiderLens downloaded earlier, then installed Chrome, Chromium and Edge in the standard places for each OS. A browser older than the pinned major version minus 4 is reported as too old (FR-003 "comparable version").
2. **Downloading one** (`download`): the pinned `chrome-headless-shell` for the current platform from Chrome for Testing, with size shown before consent, progress, cancel, SHA-256 check (R14), unpacked into the shared SpiderLens data folder (`browsers/`) so the desktop app and the CLI share one copy.
3. **Rendering** (`Browser::launch`, `Browser::render`): one browser process, one fresh browser context per page (FR-011), closed afterwards. The caller passes a `RenderJob` (URL, the main response, user agent, window, limits, blocking choices, screenshot flag) and a `Network` handle that answers every request.

Launch flags: headless, no first run, no default apps, no background networking, no component updates, no sync, a temporary user data directory deleted on close, WebSockets refused, `--js-flags=--max-old-space-size=<per-page heap limit>`. Locale `en-US` and timezone `UTC` are fixed so results do not depend on the user's machine (listed as a known difference from Google in the docs).

Per page:
- `Fetch.enable` for every request. The main document is fulfilled from `RenderJob.main` (D3). Every other request goes to `Network::fetch` (D4). Blocked types fail with `BlockedByClient` without a network call (FR-014).
- Dialogs are dismissed, permission requests denied, downloads denied, new windows and pop-ups closed as soon as they are created, file choosers cancelled (FR-012). No input events are sent.
- Window: the chosen width and height until `load`, then the height grows to the document height, capped at 12,000 CSS pixels, so lazy content in view loads (FR-009). Mobile emulation (touch, device scale factor 2.625 for the Googlebot smartphone size) when the window is mobile.
- **Settle rule** (FR-010): after `load`, the page is settled when no engine-answered request has been open for 500 ms and an injected `MutationObserver` saw no DOM change for 500 ms. Requests of type `Ping` and `CSPViolationReport`, and third-party requests marked as beacons, do not hold the page open. The optional minimum wait is applied first. The cap (default 10 s, 1 to 60) stops rendering whatever happens; the DOM at that moment is kept and the page is marked "Render timed out".
- **Output**: `document.documentElement.outerHTML` after flattening open shadow roots and same-origin iframes into the serialisation (as Google and Screaming Frog do), with the doctype prepended. Console errors and warnings and uncaught exceptions with source location (bounded to 50 per page plus a total count, FR-043). The JavaScript redirect target if the main frame navigated elsewhere (that navigation request is failed, not followed: the target goes to the frontier instead, FR-013). An optional screenshot (D9).
- **Failure**: a renderer crash (`Inspector.targetCrashed`) or a CDP error returns `Failed { reason, retryable: true }`; the engine retries once in a fresh context (FR-042). A crash after the page's heap passed the limit returns `PageTooLarge`, not retried.

*Analogy*: like a print driver. The engine owns the document and the paper supply; the renderer only turns one document into output and asks the engine for every resource it needs.

### D2. Two stages and the on-disk render queue

`Run` (the coordinator, `crates/engine/src/crawl/runner.rs`) gains `render: Option<RenderStage>`, `None` unless the crawl's rendering mode is JavaScript.

- **Enqueue** (`on_page`): an internal, non-check, HTML page with a 2xx status (or 4xx/5xx when "render error pages" is on, FR-004) gets a `renders` row. If it matches the rendering include and exclude patterns (same pattern rules as spec 003, compiled once), the row is `queued` with a queue sequence number; otherwise it is `not_rendered` with reason `outside_rules` (FR-005). With "Store page HTML" off, the compressed body is also written to `render_inputs` (research R8), so `finish_page` keeps the compressed body when rendering is on.
- **Dispatch** (new `crawl/render/stage.rs`): while fewer renders are open than the current limit, take the next queued row (a small in-memory window refilled from the partial index, like `QueueWindow`), load its body (from `bodies` or `render_inputs`), headers (`headers`), status and depth, mark it `rendering`, and spawn a render task. The task calls `Browser::render`, then runs `extract_page` on the rendered HTML and on the raw HTML (with `keep_text`, R10) in `spawn_blocking` under the existing extraction byte budget, computes the comparison (D5), and sends `Msg::Rendered` back.
- **Record** (`on_rendered`): writes the rendered rows (D6), marks the `renders` row final, deletes its `render_inputs` row, and hands rendered-only references to `discover()` (D5).
- **Back-pressure** (FR-035): when the number of queued renders passes 50,000, the fetch stage stops starting new page fetches (external checks and robots.txt continue) until it falls below 45,000. This bounds `render_inputs` on disk; the coordinator's memory does not depend on the queue length at all. The progress display names this as the limiting factor (FR-040).
- **End of crawl**: the crawl completes only when the fetch stage is done, the render queue is empty and no render is open. The raw stage finishing earlier is recorded (`meta.fetch_completed_at`) and shown.
- **Pause**: no new renders start; open renders finish (each is bounded by the cap) or, on a second Ctrl-C or window close, are abandoned and requeued on resume. On resume, `rendering` rows go back to `queued` (FR-036, US2.5), like `requeue_for_resume` does for URLs. **Stop**: open renders finish; every row still `queued` becomes `not_rendered` with reason `crawl_stopped`.

*Analogy*: the raw crawl is the press, rendering is the bindery. The press does not wait for the bindery; finished sheets go onto a pallet on the floor (the queue in the file), and the press only stops if the pallet gets too high.

### D3. One request per page: the main document comes from our fetch

The render task passes the stored status, headers (from `headers`) and decompressed body to Chrome with `Fetch.fulfillRequest` for the first main-frame request (FR-007, US2.3, SC-006). `Content-Encoding` and `Content-Length` are removed from the headers, because the stored body is already decoded. The render-later command (D11) uses the same path. Fixture tests count requests per path from the fixture's own log.

### D4. Every other request goes through the engine (research R6, R7)

`crawl/render/net.rs` implements the render crate's `Network` trait:

1. **Blocked by settings** (images, media, fonts, stylesheets, third-party, per FR-014): fail at once, recorded `blocked_setting`.
2. **Cache** (FR-017): a byte-bounded in-memory LRU (64 MB) keyed by URL, shared by all render tasks. Responses are stored only when `Cache-Control` and `Expires` allow it (no `no-store`, not `private`, freshness from `max-age` or `Expires`, heuristic freshness not used). A hit is answered without a permit and recorded `cached`. *Alternative rejected*: an on-disk cache; the bounded memory cache covers the "same script on thousands of pages" case, and a disk cache would either grow the crawl file or add a second file next to it.
3. **robots.txt** (FR-016): the host's policy from the coordinator. A host seen for the first time (a third-party CDN, for example) has its robots.txt fetched once first, as for pages. Under `respect`, a disallowed resource is failed and recorded `blocked_robots` with the rule; under `ignore_report` it is fetched and the rule recorded; under `ignore` robots.txt is not read.
4. **Permit** (FR-015): `Msg::ResourcePermit { host_key, third_party }` to the coordinator, which grants it when the host is under its allowance, outside crawl-delay and Retry-After, and under the global `max_concurrent` and `max_urls_per_sec` caps. Third-party hosts use a separate cap (8 open in total, 2 per host) and do not count against the crawled site. Waiting permits are served before new page fetches.
5. **Fetch** with the crawl's client, headers and timeouts, then `Msg::ResourceDone` so the host's AIMD allowance and Retry-After learn from it (spec 008 D4).
6. **Record**: every request becomes a resource record (URL, type, host, internal or third-party, outcome, status or error, bytes, from cache) returned with the render outcome and stored in `render_resources` (FR-018).

### D5. Comparing raw and rendered (pure functions in `crates/engine/src/extract/compare.rs`)

`compare(raw: &PageData, rendered: &PageData) -> Comparison`, no I/O:

- **Origins** (FR-023): each item gets `raw`, `rendered` or `both` by multiset matching on a key: titles and descriptions by text; headings by level and text; directives by source, crawler, name and value; structured data by xxh3 of the normalised block; links by kind, resolved target and anchor text; images by resolved source. Duplicates match one for one, so a link that appears twice raw and three times rendered gives two `both` and one `rendered`.
- **Comparison fields** (FR-024): word counts and change, link counts and rendered-only and raw-only link counts, and for title, meta description, first H1, canonical, noindex, nofollow and the hreflang set a state of `same`, `added` (missing raw, present rendered), `removed` or `changed`.
- **JavaScript dependence** (FR-029): rendered-only share of visible text (word multiset, R10), headings, internal links, structured data blocks and images, plus raw presence of title, description, canonical and directives, and the overall percentage (the text share).
- **Bot challenge** (FR-044, `extract/challenge.rs`): a short list of known challenge markers (title patterns such as "Just a moment...", "Attention Required!", known challenge script and form markers), combined with a rendered word count far below the raw one.
- **Discovery** (FR-022): rendered-only references go through the existing `discover()` with the page's depth plus one and the same scope, follow and nofollow rules, using the rendered page's own nofollow directive. Their link rows go to `rendered_links`. A JavaScript redirect becomes a reference of the new kind `js_redirect` and is discovered like a redirect.

### D6. Crawl file changes: one additive migration step

All new data is in new tables (full columns in [data-model.md](./data-model.md)), so the raw tables mean exactly what they meant before and readers that do not know about rendering see an unchanged raw crawl (FR-020, FR-034):

`renders` (one row per eligible page, also the render queue), `render_inputs` (temporary input bodies), `rendered_pages` (the rendered counterpart of `pages` plus the four `urls` fields that rendering can change), `rendered_text`, `rendered_directives`, `rendered_structured_data` (full rendered sets plus raw-only rows, each marked with its origin), `rendered_links` (rendered-only link rows) and `raw_only_links` (positions of raw links missing after rendering), `render_comparisons` (FR-024 and FR-029 values), `render_resources`, `console_entries`, `rendered_bodies`, `screenshots`. Meta keys: `renderer` (name, version, source), `render_version`, and the `data_features` list gains `rendering`, `rendered_html` and `screenshots` as applicable.

**Schema step**: these tables and indexes are one numbered additive step in `crates/engine/src/crawl/migrate.rs` `STEPS` (`Step { version, additive: true, sql }`), the mechanism spec 006 introduces in PR #27. It is **step 6**: step 3 (events, 006), step 4 (retries and the crawl log, 004 PR #29) and step 5 (Set-Cookie values removed, PR #51) are on `main` (checked at 3ccf82c). If another step lands on `main` first, renumber to the next free number. `store.rs` `SCHEMA` stays the format-2 base and `FORMAT_VERSION` follows `migrate::LATEST`. The storage task (T012) depends on PR #27 being merged.

Tables are created by the migration in every new file, whether or not rendering is on (an empty table costs a few hundred bytes and no write time). Rows are written only when rendering is on.

### D7. Results: rendered values, the JavaScript tab, and the detail pane

- **Feature detection**: `Need::Rendering` (and `RenderedHtml`, `Screenshots`), detected from `data_features` like the existing needs (`crates/results/src/features.rs:54`). Files without them show "This crawl was not rendered" in the JavaScript tab (FR-034).
- **Rendered values in existing tabs** (FR-021, research R11): `ViewQuery` gains `values: raw | rendered` (default rendered when the file has rendering). In rendered mode the reader creates temporary views that shadow `pages`, `page_text`, `directives`, `structured_data`, `links` and overlay the rendered `urls` fields, picking rendered rows for rendered pages. A hidden-by-default "Values" column shows `Rendered` or `Raw` per row, and the toolbar switch changes the whole view.
- **JavaScript tab**: one `TabDef` in `tabs.rs` with `eligible = EXISTS renders`, the FR-025 columns and the 26 FR-026 filters as SQL conditions over `renders` and `render_comparisons`. Thresholds (FR-027, FR-030) come from the stored configuration, like `redirect_limit` today, and appear in the filter labels.
- **Detail pane** (FR-028): new sections Rendered HTML, Raw vs rendered, Resources, JavaScript console, Screenshot; existing Headings, Outlinks and Images lists gain an origin badge. The diff is computed in Rust (`crates/results/src/html_diff.rs` with `similar`): both sides are normalised (one tag per line, attributes sorted, whitespace collapsed, `<script>` bodies folded) so only real differences are highlighted. The UI gets hunks, not two megabyte strings.

### D8. Rendering states and reasons

`renders.state`: `queued`, `rendering` (both transient), then exactly one of `rendered`, `timed_out`, `failed`, `not_rendered` (FR-039). Reasons: `failed`: `crash`, `renderer_error`, `page_too_large`, `navigation_error`; `not_rendered`: `outside_rules`, `crawl_stopped`, `browser_unavailable`, `html_not_stored` (render-later only, owner point 1). `attempts` counts tries (at most 2, FR-042). A debug assertion at the end of each run checks that no row is left `queued` or `rendering` when the crawl status is `completed` or `stopped` (SC-008).

### D9. Screenshots (FR-033)

Off by default. When on, `Page.captureScreenshot` in WebP at quality 60 at the rendering window width, full page up to 8,192 CSS pixels tall, stored in `screenshots`. Captured after the settle rule, so it adds no wait; it adds encoding time and roughly 50 to 300 KB per page *(estimate, measured in T081)*.

### D10. Memory guard and default concurrency (FR-037, FR-038)

- **Default concurrency**: total memory in GB divided by 4, clamped to 1 to 16 (16 GB gives 4). **Default budget**: one eighth of total memory (16 GB gives 2 GB). Both can be set by the user.
- **Guard**: every second, read system free memory and the browser process tree's resident memory (`sysinfo`). If free memory is under 10% of total or the browser is over budget, lower the render limit by one (never below 1) and report `memory_guard` as the limiting reason. Raise it by one after 10 s with free memory above 20% and the browser under 80% of budget, up to the configured concurrency. If the browser stays over budget at a limit of 1, restart it between pages. The browser is also restarted every 1,000 pages to cap leaks.

### D11. Rendering an existing crawl (FR-006)

`spiderlens render <FILE>` and the desktop command `crawl_render` open a finished crawl, record the rendering settings as a configuration change (`Change::Rendering`, owner point 2), enqueue every eligible page as in D2, and run the render stage alone with the fetch stage idle (subresources are still fetched politely through D4; the main documents are not requested again). Pages whose raw HTML is not stored get `not_rendered` with `html_not_stored`; if the crawl stored no HTML at all, the command is refused before starting with that explanation.

### D12. Browser availability, consent and cleanup (FR-045, FR-046)

- **Before a crawl starts** with rendering on, the CLI and the desktop app call `find_browser`. If none is usable: the CLI exits with code 7 (6 is spec 006's disk-problem pause) and a message naming `spiderlens browser install` and `--allow-browser-download`; the desktop app shows a dialog with the download size, "Download and crawl" and "Crawl raw HTML instead". The crawl never starts in rendering mode without a browser. If the browser disappears mid-crawl and cannot be relaunched, the remaining pages become `not_rendered` with `browser_unavailable`, and a warning is shown.
- **Cleanup**: normal exits close the browser from Rust and delete its temporary profile. Abnormal exits rely on the pipe transport or the guard process (research R12). A startup sweep removes stale temporary profiles left by a killed run.

### D13. Progress and summary (FR-040, FR-041)

`Progress` gains `render: Option<RenderProgress>` (rendered, queued, rendering, pages per second current and average, current limit, limiting reason: `concurrency`, `memory_guard`, `slow_pages`, `site_limits`, `queue_full`). `CrawlSummary` gains `render: Option<RenderSummary>` (counts by state and reason, average and slowest render time, totals of JavaScript errors, failed and blocked resources). Both are `None` when rendering is off, so the event payload and summary JSON are unchanged for raw crawls. The CLI progress line gains a second line; the desktop status area gains a rendering row.

## Performance impact (constitution principle VI)

| Situation | Cost | How it is checked |
|---|---|---|
| Rendering off | One `Option` check per page in `on_page` and one in `finish_page`; no allocation, task, thread, table write or process. New empty tables in each new file (a few KB). | T080: 100,000-page benchmark, median of 7 alternating runs, at most 2% slower and 5% more peak memory than the stored baseline (SC-001). T037 asserts no browser process is started and no render row is written. |
| Rendering on, fetch stage | Each eligible page adds one `renders` insert, plus one `render_inputs` insert when HTML is not stored. Resource permits add coordinator messages. | T038: raw fetch rate from the fixture log, rendering on versus off, until the high-water mark (SC-002). |
| Rendering on, render stage | Rendering is about three orders of magnitude slower than raw fetching (research R3); crawl wall time becomes render time. Memory: browser plus 4 tabs about 1 to 2 GB *(estimate)*, held under budget by the guard. | T001 spike, T081 render benchmark (SC-002, SC-003). |
| File size | Rendered rows roughly double the per-page content rows for rendered pages; rendered HTML about 15% of its size compressed (as raw HTML in spec 005); screenshots only when on. | T081 records file size with and without screenshots. |
| Binary size | `chromiumoxide`'s generated CDP types. | T082 records CLI and desktop binary sizes; budget 8 MB per binary *(target)*. |

## Spec points that need the owner's nod

1. **A fourth "Not rendered" reason**: FR-039 lists outside rendering rules, crawl stopped and browser unavailable. Rendering an existing crawl (FR-006) needs one more for pages whose raw HTML was not stored: `Not rendered: raw HTML not stored`. Proposed as an addition to FR-039.
2. **Rendering settings as a recorded change**: `StoredCrawlConfig` says only speed and retry may change during a crawl (`crates/config/src/stored.rs:12`). Rendering an existing crawl later changes the rendering settings of a finished crawl. Proposed: a new `Change::Rendering` entry, so the reopened crawl shows both the original settings and the later rendering run (FR-034).
3. **The SC-001 measurement method**: single runs of the 100,000-page benchmark vary by about 7% on the reference container (`specs/005-page-extraction/benchmarks.md:14`), more than the 2% budget. The plan measures the median of 7 alternating before and after runs. If the owner wants a single-run gate, the budget has to be stated as "within run-to-run variation" (which is how US2 scenario 1 words it) rather than 2%.

## Open risks

1. **CDP interception under load.** Historical reports describe stalls when every request is intercepted at high tab counts. The spike (T001) measures 4, 8 and 16 tabs with fulfil-all interception; if `chromiumoxide` stalls, switch to `chromey`; if both stall above 8 tabs, cap concurrency at the highest stable value and say so in the docs.
2. **Fulfil-all overhead.** Base64 bodies over the DevTools connection could cost more than expected on script-heavy pages. Fallback in research R6 (continue third-party requests).
3. **Orphan browsers after a crash** (SC-009). Depends on pipe transport support or the guard process (research R12); neither is proven yet. The fault-injection test (T063) kills SpiderLens with SIGKILL and `taskkill /F`.
4. **Browsers on CI runners.** Render tests need a browser on all three runners (R13). If one image lacks it, CI installs `chrome-headless-shell` with the same pinned download the app uses, which also tests that path.
5. **Binary size and build time** of the generated CDP types (T082). If over budget, check `chromiumoxide`'s feature flags for trimming unused domains.
6. **Temporary views over `urls`** may slow some results queries (research R11). Measured in T030 with the spec 007 results benchmark; fallback described there.
7. **Cookies within one render.** Requests answered by the engine do not carry cookies Chrome would have attached, and `Set-Cookie` in fulfilled responses depends on Chrome's handling. Sites whose content depends on a cookie set by an earlier request in the same page may render differently. The fixture covers the common case (a cookie set by the main document and read by a script); differences are listed in the docs.
8. **Schema step ordering** with specs 006 and 004 (D6): T012 waits for PR #27.
9. **Settle rule on real sites**: 500 ms quiet may end some pages too early (content after a timer) or too late (constant analytics). The minimum wait setting covers the first; the beacon exclusion covers the second. Tuned against the fixture and a short list of public JavaScript sites recorded as HAR-like fixtures, never crawled live in tests.

## Project Structure

### Documentation (this feature)

```text
specs/014-javascript-rendering/
├── spec.md                     # approved
├── research.md                 # R1-R5 (spec phase), R6-R15 (this plan)
├── plan.md                     # this file
├── data-model.md               # crawl file tables, settings, states
├── quickstart.md               # end-to-end validation with the local fixture
├── contracts/
│   ├── cli.md                  # flags, commands, exit codes, progress line
│   ├── desktop-commands.md     # Tauri commands and events
│   └── render-crate.md         # spiderlens-render public API
├── benchmarks.md               # created by T004, filled by T080-T082
└── tasks.md
```

### Source Code (repository root)

```text
crates/render/                      # NEW crate spiderlens-render (D1)
├── Cargo.toml
└── src/
    ├── lib.rs                      # public API (contracts/render-crate.md)
    ├── find.rs                     # browser discovery per OS, version check
    ├── fetcher/mod.rs              # chrome-headless-shell download, unzip, verify
    ├── fetcher/pins.rs             # pinned version and SHA-256 per platform (R14)
    ├── launch.rs                   # flags, temp profile, pipe or guard (R12)
    ├── guard.rs                    # --render-guard mode
    ├── page.rs                     # one render: context, interception, settle, output
    ├── settle.rs                   # settle rule state machine (unit-tested without a browser)
    ├── flatten.js                  # shadow DOM and iframe flattening, injected
    └── types.rs                    # RenderJob, RenderOutcome, Network, records

crates/engine/src/
├── extract/compare.rs              # NEW: origins, comparison, dependence (D5)
├── extract/challenge.rs            # NEW: bot challenge markers (FR-044)
├── extract/mod.rs                  # PageInput.keep_text, PageData.visible_text (R10)
├── crawl/render/mod.rs             # NEW: RenderStage, queue window, states (D2, D8)
├── crawl/render/net.rs             # NEW: Network impl, permits, robots (D4)
├── crawl/render/cache.rs           # NEW: byte-bounded resource cache (FR-017)
├── crawl/render/guard.rs           # NEW: memory guard (D10)
├── crawl/runner.rs                 # Option<RenderStage>, new Msg variants, back-pressure, progress
├── crawl/store.rs                  # render writes and reads; SCHEMA unchanged
├── crawl/migrate.rs                # (from spec 006) one additive step for D6
└── crawl/reason.rs                 # LinkKind::JsRedirect, RenderState, RenderReason

crates/config/src/model.rs          # RenderSettings in CrawlConfig
crates/config/src/validate.rs       # rendering ranges and patterns
crates/config/src/stored.rs         # Change::Rendering (owner point 2)
crates/results/src/                 # Need::Rendering, JavaScript tab, temp views, html_diff.rs, detail sections
crates/cli/src/                     # flags, browser/render/screenshot commands, progress, summary
app/src-tauri/src/render.rs         # NEW: browser status/install, crawl_render, html, screenshot
app/src-tauri/src/crawl.rs          # browser check before start, render progress in events
ui/src/settings/                    # "JavaScript rendering" category + tests
ui/src/crawl/                       # browser dialog, render progress + tests
ui/src/results/                     # values switch, JavaScript tab bits, new Inspector sections, HtmlDiff + tests
crates/test-fixtures/src/js_site.rs # NEW: JavaScript fixture site (one page per scenario)
crates/test-fixtures/src/bin/bench-site.rs  # --spa mode for SC-002/SC-003
scripts/render-golden.mjs           # NEW: Playwright golden for SC-004
scripts/pin-headless-shell.mjs      # NEW: writes fetcher/pins.rs at release time
.github/workflows/ci.yml            # browser check step, render tests, no-UI check for spiderlens-render
```

**Structure Decision**: one new library crate (`crates/render`) beside the existing ones; everything else extends existing modules. The engine depends on `spiderlens-render`; `spiderlens-render` depends on nothing in this workspace, so it can be tested and replaced on its own.

## Complexity Tracking

| Addition | Why needed | Simpler alternative rejected because |
|---|---|---|
| New crate `crates/render` | Browser discovery, download and the CDP client are a large, separate concern; isolating them keeps `crates/engine` free of the CDP types and lets the render tests run on their own. | Putting it in `crates/engine` would pull the generated CDP code into every engine build and test run and mix browser process management with the crawl loop. |
