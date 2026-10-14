# Research: JavaScript Rendering (spec 014)

**Date**: 2026-10-13. Sources were read on that date; items marked *(estimate)* or *(unverified)* still need checking during `/speckit-plan`.

This note feeds `/speckit-plan`. The spec stays technology-neutral; the choices below are recommendations for the plan, not approved decisions.

## R1. Which renderer: headless Chromium over CDP, or the Tauri webview

**Recommendation: headless Chromium driven over the Chrome DevTools Protocol (CDP) from a new Rust crate, `crates/render`. The Tauri webview is not used for rendering.**

| | Headless Chromium over CDP | Tauri webview (WebView2 / WKWebView / WebKitGTK) |
|---|---|---|
| Same engine as Google | Yes, on every OS (Google renders with evergreen Chromium) | Only on Windows. macOS and Linux use WebKit, so results would differ from Google and from Windows |
| Command line, no window | Yes | No. All three need a GUI event loop; Linux needs a display (Xvfb on servers) |
| Block or intercept requests | Yes: the `Fetch` domain pauses every request, so robots.txt, host caps and resource types can be checked per request | No portable API in wry. WebView2 can via raw COM; WKWebView only by URL pattern lists |
| Serve the main document from our own fetch (FR-007) | Yes: `Fetch.fulfillRequest` with the bytes the engine already has | No portable way |
| Console errors, exceptions, lifecycle events, screenshots | Yes, as typed CDP events | Partial, differs by OS |
| Concurrency | One browser process, many tabs, fresh browser context per page | One full webview per page; macOS webviews are tied to the main thread |
| Install size | Zero if the user's Chrome or Edge is used; about 100 MB download otherwise *(estimate)* | Zero |
| Memory | About 100–300 MB for the browser plus 50–150 MB per open page *(secondary source)* | Similar per webview, plus window overhead |

The webview fails the hard requirements: CLI parity (constitution principle III, FR-047), the same results on all three operating systems, and per-request control for politeness (FR-015, FR-016). Its only advantage is no extra download, and the "installed Chrome or Edge first" option below removes most of that advantage (Edge is on every Windows 10 and 11 machine).

Rust CDP clients (crates.io, read 2026-10-13):

- **chromiumoxide** 0.9.1 (Feb 2026), MIT/Apache, about 5M downloads, async on tokio (the engine already uses tokio). Typed CDP events, `enable_request_intercept`, `content()` for the rendered DOM, screenshots, browser detection, optional fetcher. Maintained again after a quiet 2024. **Recommended.**
- **chromey** 2.58.x (Sep 2026), the spider-rs fork of chromiumoxide (formerly `spider_chrome`). Released most often, used by the `spider` crawler. **Fallback** if chromiumoxide's interception does not hold up under load.
- **headless_chrome** 1.0.22: maintained but synchronous, a poor fit for the async engine.
- **fantoccini**: classic WebDriver, needs a separate driver process, no CDP-level interception. Rejected.

Spike to run first in the plan: 4, 8 and 16 tabs with `Fetch` interception on every request against the local fixture, to measure throughput, memory and whether the interception handler causes back-pressure or deadlocks *(historical reports, unverified)*.

## R2. Where the browser comes from

Options:

1. **Installed Chrome or Edge first, else a one-time download of `chrome-headless-shell` with consent** (recommended). Chrome for Testing publishes `chrome-headless-shell` for linux64, linux-arm64, mac-arm64, mac-x64, win32 and win64 (current Stable 155.0.8059.39). It has fewer system dependencies and is lighter than full Chrome. Playwright and Quarto (April 2026) use this approach. The installer does not grow, Windows users never download anything (Edge), and the version is stored with each crawl (FR-003).
2. **Always download a pinned headless shell.** Every user gets the same engine version, so results are more reproducible, but every user downloads about 100 MB *(estimate)* and SpiderLens has to keep the pin current.
3. **Bundle Chromium in the installer** (what Screaming Frog does). Works offline, but adds about 100 MB to the installer and 250–300 MB on disk for every user, including those who never render *(estimate)*. SpiderLens would also have to ship Chromium security updates and sign nested binaries on macOS. This conflicts with the speed and size goals.

Licensing: Chromium is BSD-3-Clause; downloading or driving it is fine for an MIT app. If we ever bundle it, we ship its licence and credits files.

## R3. How rendering fits the crawl pipeline without slowing it

- **Two stages.** The fetch stage (specs 004, 005, 008) stays exactly as it is. When rendering is on, each eligible page's `url_id` is appended to an on-disk render queue in the crawl file. A separate render stage pulls from that queue with its own concurrency (default 4). The fetch stage never waits for rendering, except when the render queue reaches a high-water mark, so the queue cannot grow without limit. With rendering off, the render stage does not exist: no browser is started and no queue is written (FR-002, SC-001).
- **One fetch per page.** The renderer intercepts the main document request and fulfils it with the stored response: status, headers and body, decompressed from `bodies` or kept in memory while hot. This halves the load on the site, keeps raw and rendered input identical, and makes "render an existing crawl later" (FR-006) the same code path.
- **Subresources go through the engine's politeness.** Each intercepted request to an internal host first asks the existing per-host limiter from spec 008 (AIMD allowance, Retry-After, crawl-delay) and robots.txt from spec 003. Third-party hosts use a separate small cap. Blocked types (image, media, font by default) fail immediately with `BlockedByClient`.
- **Shared resource cache.** Scripts and stylesheets are cached per crawl in a bounded on-disk cache keyed by URL, respecting cache headers. Google is known to cache aggressively, so this is both faster and closer to Google.
- **Clean state per page.** Each page gets a fresh browser context (incognito-like) and is closed afterwards. The browser is restarted every N pages, or when its memory passes the budget, to cap leaks.
- **Settle rule.** Wait for `load`, then until there are no requests in flight and no DOM mutations for 500 ms, capped by the maximum render time (default 10 s). Puppeteer's `networkidle0` alone hangs on analytics beacons and long polling, so beacons to third parties are excluded from the idle check. Google has no hard 5 s limit (it uses a virtual clock with observed cut-offs of 30–50 s). Screaming Frog uses a fixed 5 s wait and Sitebulb 1 s. Settling as soon as the page is idle is faster than a fixed wait on most pages.
- **Rendered extraction reuses spec 005.** `content()` returns the serialised DOM. Shadow DOM and same-origin iframes are flattened first, as Google and Screaming Frog do. The serialised DOM then goes through the same `extract_page` function as raw HTML, so raw and rendered values are comparable by construction (FR-019). Extraction runs in `spawn_blocking`, as it does now.
- **Storage.** New tables for rendered pages, element origins (raw, rendered or both), the comparison, resources, console entries and screenshots, all keyed by `url_id`. Rendered HTML goes into `bodies` with a version column, zstd-compressed like raw HTML. The `meta` table records the renderer version and settings. Older files simply lack the tables, and the results crate detects that the same way it detects extraction (spec 007).

### Performance impact (constitution principle VI)

- Rendering off: one `if` per page and no new allocations. Target: within benchmark noise (SC-001).
- Rendering on: rendering is about three orders of magnitude slower than raw fetching (raw about 3,300 URLs/s on the fixture; rendered estimated at 2–8 pages/s with 4–8 tabs on real sites, faster on the local fixture) *(estimate, to be benchmarked)*. The crawl's wall time becomes the render time. The raw stage finishes early, so the user already has the complete raw crawl while rendering continues. The progress bar shows both stages (FR-040).
- Memory: one browser process plus 4 tabs fits about 1–2 GB *(estimate)*. The memory guard (FR-038) lowers concurrency before the laptop swaps.

## R4. What Google and AI crawlers do (why raw versus rendered matters)

- **Googlebot**: evergreen Chromium. Stateless (cookies and storage cleared). Declines permission prompts. Never scrolls or clicks; it makes the viewport tall instead. Does not fetch resources blocked by robots.txt. Caches resources aggressively. May skip rendering when the raw HTML has `noindex`, so JavaScript that removes `noindex` cannot be relied on. Gemini uses the same rendering service.
- **AI crawlers mostly do not run JavaScript.** Vercel and MERJ (Dec 2024) found that GPTBot, OAI-SearchBot, ChatGPT-User, ClaudeBot, PerplexityBot, Meta-ExternalAgent and Bytespider fetch JavaScript files but do not execute them; only Gemini (via Google) and AppleBot render. A controlled test reported by Advanced Web Ranking (Aug 2026) found that ChatGPT, Claude, Gemini live fetch, Perplexity, Meta AI, Copilot and Grok all returned only what was in the raw HTML.
- **For SpiderLens**: Screaming Frog's JavaScript filters are about Google. SpiderLens can also answer "what can AI answer engines not see on this page?" That is the JavaScript dependence measure (FR-029, FR-030), which spec 015 turns into AEO and GEO issues and spec 016 exposes to agents. No mainstream crawler frames it this way yet.

## R5. Parity with Screaming Frog and Sitebulb

| Screaming Frog / Sitebulb | SpiderLens spec 014 |
|---|---|
| Rendering mode Text only / JavaScript (+ old AJAX scheme) | Raw HTML / JavaScript rendering (the AJAX scheme is deprecated by Google and is left out) |
| AJAX timeout, fixed 5 s (SF); render timeout 1 s (Sitebulb) | Settle rule plus a 10 s maximum and an optional minimum wait (faster on most pages) |
| Window size presets, height extended to 8,192 px | Mobile and desktop presets that follow the user agent, extended height |
| 15 JavaScript tab filters | All of SF's current filters except the two AJAX scheme ones, plus timeouts, failures, errors, JS redirects, bot challenge, structured data only in rendered HTML, and "hidden from non-rendering crawlers" |
| View source with "Show differences" | Raw vs rendered pane with normalised diff |
| Screenshots on by default | Off by default (speed and file size), one switch |
| Console log tab and bulk export | JavaScript console section, counts in the tab, export via spec 010 |
| Blocked resources (robots.txt) | Same, plus host caps and Retry-After applied to resources |
| Bundled Chromium | Installed Chrome or Edge, else a download on first use |
| Sitebulb "Response vs Render" report | The JavaScript tab plus the comparison table |

## Decisions for the owner

**Resolved 2026-10-13**: Ryware approved the spec with the recommended option (a) for all five items below.

1. **Browser source**: (a) installed Chrome or Edge first, else download with consent *(recommended)*; (b) always download a pinned version; (c) bundle in the installer.
2. **What to render**: (a) every eligible internal HTML page, with optional include and exclude patterns *(recommended, matches SF)*; (b) a sample first (for example 100 pages per URL pattern), then only the patterns where rendering changes something. (b) is much faster on big sites but more complex; it could come later as a "Smart" mode.
3. **Default resource blocking**: (a) skip images, media and fonts *(recommended, faster, rarely changes the DOM)*; (b) load everything like Google.
4. **Links found only after rendering**: (a) crawl them *(recommended, as Google does)*; (b) record them only.
5. **Screenshots in M4**: (a) included, off by default *(recommended)*; (b) postponed.

## Planning additions (2026-10-13, `/speckit-plan`)

The sections below were added while writing `plan.md`, after reading the code on `main` (3c62a4e). They refine R1 to R3 where the code works differently from what R3 assumed. Each one is referenced from the plan as a design decision.

## R6. The engine answers every request the browser makes

**Decision**: With `Fetch.enable` on all requests, the engine fetches every HTTP(S) request the page makes with its own `reqwest` client and answers it with `Fetch.fulfillRequest`. Chrome never talks to the network itself. Requests that are blocked (by setting or robots.txt) are answered with `Fetch.failRequest` (`BlockedByClient`).

**Rationale**:
- The per-host limiter needs to know when a request starts and when it ends. With `Fetch.continueRequest` the engine only sees the start; the end would have to be matched from `Network.loadingFinished` events, a second event stream per tab.
- The cache (FR-017), the resource records (FR-018), robots.txt (FR-016) and the host caps (FR-015) then sit in one place, in Rust, and are unit-testable without a browser.
- Tests already pin `.test` host names to the local fixture through `reqwest` (`Crawler::with_resolve`). Chrome would need `--host-resolver-rules` for the same effect; fulfilling from the engine makes fixture tests work unchanged.
- The crawl's proxy, TLS and header settings apply to page resources the same way as to pages.

**Costs**: bodies cross the DevTools connection base64-encoded (about 33% larger) and every request takes one extra round trip. On a local fixture with a few hundred KB of script per page this is small, but it must be measured (spike T001). Streaming responses (Server-Sent Events, long polling) cannot be fulfilled until they end; they are answered when the engine's read timeout or the page's render cap hits, whichever comes first. WebSocket connections are not intercepted by the `Fetch` domain and are refused by a launch flag.

**Alternative kept as a fallback**: `Fetch.continueRequest` for third-party hosts only, with the permit released on `Network.loadingFinished` or `loadingFailed`. Used only if the spike shows the fulfil path costs more than 20% of render throughput.

## R7. The spec 008 limiter is owned by the coordinator

R3 says each intercepted request "asks the existing per-host limiter". In the code that limiter is not a service: the per-host state (`Host`, with its `SiteSpeed` allowance, crawl-delay and Retry-After time) and the global caps (`in_flight`, `next_global`) are private fields of the coordinator's `Run` (`crates/engine/src/crawl/runner.rs:227-256` and `560-594`), and `dispatch()` applies them (`runner.rs:870-962`). Only the coordinator task may touch them.

**Decision**: render tasks ask the coordinator for a permit over the existing message channel (`Msg::ResourcePermit` with a one-shot reply) and report the end (`Msg::ResourceDone`, with the response time and the 429/503 signal, so the AIMD allowance learns from resource responses too). The coordinator serves waiting resource permits before starting new page fetches, so a page being rendered is not starved by the raw crawl. This keeps the actor pattern (one owner of mutable state) and the rule that one host's caps cover every request to it.

**Cost**: one channel round trip per resource. At the expected render rate (tens of pages per second, a few dozen resources each) this is well under what the coordinator already handles: about 2,500 responses per second on most of one core in spec 008's benchmark.

## R8. Where the rendered input comes from

R3 says the main document is "decompressed from `bodies` or kept in memory while hot". In the code, the received body is compressed into `bodies` only when "Store page HTML" is on and is dropped otherwise (`runner.rs:326-331`). Keeping it in memory would make memory grow with the render queue (FR-036).

**Decision**: when rendering is on and "Store page HTML" is off, the compressed body of each queued page is written to a `render_inputs` table and deleted once the page reaches a final rendering state. When "Store page HTML" is on, the render stage reads `bodies`. Either way memory stays flat, and resume works from the file.

## R9. Rendered HTML cannot share the `bodies` table

R3 proposes a version column on `bodies`. `bodies` has `url_id` as its primary key (`crates/engine/src/crawl/store.rs:166-171`); adding a version to the key means rebuilding the table, which is not an additive migration step. **Decision**: a separate `rendered_bodies` table with the same columns.

## R10. Comparing raw and rendered text

FR-029 needs "the share of rendered visible text not present in the raw HTML". `extract_page` collects the visible text, hashes it and drops it (`crates/engine/src/extract/walk.rs:131-133`); `PageData` keeps only `word_count` and `text_hash`.

**Decision**: `PageInput` gains `keep_text: bool` (false in the raw crawl, so nothing changes there) and `PageData` gains `visible_text: Option<String>`. The render stage re-extracts the raw HTML with `keep_text` (cheap: 50 µs to 4 ms per page, spec 005 D1) instead of reading raw rows back from the file. The dependence measure compares word multisets: rendered-only words are, per word, the rendered count minus the raw count when positive; the percentage is rendered-only words over rendered words. This is order-insensitive and deterministic, so the expected values for fixture pages (SC-011) can be computed by hand.

*Alternative rejected*: word shingles (n-grams). More sensitive to reordering, harder to predict in tests, and not needed for "how much of this text exists only after JavaScript".

## R11. Rendered values in the existing results tabs (FR-021)

The tabs of spec 007 are static SQL over `urls u`, `pages p`, `links l`, `directives d` and `page_text` (`crates/results/src/tabs.rs`, for example lines 166-170). SQLite resolves an unqualified table name in the `temp` schema before `main`. **Decision**: when a file holds rendered data and the view asks for rendered values, the results reader creates `TEMP VIEW`s named `pages`, `page_text`, `directives`, `structured_data` and `links` (and a narrow overlay for the four `urls` columns that change: `title`, `noindex`, `nofollow`, `canonical*`) that pick the rendered rows for rendered pages and the raw rows otherwise. Every existing column and filter then shows rendered values without being written twice. The raw switch drops the views.

**Risk**: a view over `urls` may stop SQLite from using the `urls_state` index in some tab conditions. The results benchmark (`crates/results/examples/bench.rs`) is run with and without the views; the fallback is to rewrite only the affected expressions with a `{pages}` placeholder.

## R12. Closing the browser when SpiderLens dies (FR-046, SC-009)

Normal exits (completion, pause, stop, Ctrl-C, window close) close the browser from Rust. A crash or a kill of SpiderLens is harder: the workspace forbids `unsafe` code (`Cargo.toml:57`), which rules out `prctl(PR_SET_PDEATHSIG)` on Linux and Job Objects on Windows through raw system calls.

**Decision, in order of preference**:
1. Launch Chrome with `--remote-debugging-pipe` if the chosen CDP crate supports a pipe transport. Chrome exits by itself when the pipe closes, which happens when the SpiderLens process dies for any reason.
2. Otherwise, a small guard process: the `spiderlens` and desktop binaries accept a hidden `--render-guard <pid>` mode that starts Chrome, checks every 500 ms whether the parent process is alive (with `sysinfo`) and kills Chrome when it is not.

The spike (T001) checks option 1 for chromiumoxide and chromey.

## R13. Browsers on CI runners

GitHub's hosted images for `ubuntu-24.04`, `windows-latest` and `macos-14` list Google Chrome as preinstalled, and Windows also has Edge *(from the runner image documentation, unverified for the exact images this repository pins)*. Task T002 adds a CI step that prints the browser found on each runner, so this is verified before any render test depends on it. *Status 2026-10-14: the step and `scripts/find-browser.sh` exist on the branch, but the branch has not run in CI yet, so what each runner has is still to be recorded here from the first run's job summary.* Render integration tests read `SPIDERLENS_REQUIRE_BROWSER=1` in CI: without a browser they fail instead of being skipped silently. Locally they skip with a message.

## R14. Integrity of the downloaded browser

Chrome for Testing publishes download URLs but no checksums. **Decision**: each SpiderLens release pins one `chrome-headless-shell` version and records the SHA-256 of each platform's archive in `crates/render/src/fetcher/pins.rs`, generated by `scripts/pin-headless-shell.mjs` at release time. The download is refused if the hash differs. Downloads come only from `storage.googleapis.com` over HTTPS and only after consent (FR-045).

## R15. New dependencies (constitution principle VIII)

| Crate | Where | Why |
|---|---|---|
| `chromiumoxide` 0.9 (no default features, so no fetcher) | `crates/render` | Typed CDP client (R1), chosen by the spike (R16). `chromey` is the fallback. |
| `sysinfo` | `crates/render`, `crates/engine` | Free system memory and the browser process tree's memory for the guard (FR-038) and the orphan guard (R12). |
| `zip` | `crates/render` | Unpacking the `chrome-headless-shell` archive. |
| `sha2` | `crates/render` | Download integrity (R14). |
| `similar` | `crates/results` | Line diff for the raw versus rendered pane (FR-028), shared by the desktop app and `spiderlens html --diff`. |

Binary size: the generated CDP types are large. The desktop and CLI binaries are measured before and after in `benchmarks.md`; the budget is 8 MB added per binary *(target, to be confirmed by the spike)*.

## R16. Spike results (T001, 2026-10-14)

**Decision: `chromiumoxide` 0.9.1, as R1 recommended.** Both crates performed the same within run-to-run noise, so the choice rests on the rest: `chromiumoxide` adds fewer crates (43 against 72 in a small probe program), its handler leaves every paused request to us without any crawler-specific logic of its own (`chromey`'s network manager carries ad blocking, caching and request heuristics for the `spider` crawler that we would have to keep switched off), and it changes less often. `chromey` stays the fallback.

**Setup.** `cargo run --release -p spiderlens-render --features spike --example spike` (source: `crates/render/examples/spike.rs` and `spike/driver.rs`). All numbers come from a **4-core cloud container** (16 GB, no swap), not the reference laptop, with the browser, the fixture server and the spike sharing the same 4 cores. Browser: Playwright's `chromium_headless_shell` 141.0.7390.37 (the same build as Chrome for Testing's `chrome-headless-shell`, old headless mode). Fixture: `JsSiteBuilder::app_pages(2000)` from `spiderlens-test-fixtures`, 2,000 client-rendered pages, each with 3 scripts (one of them 150 KB), 1 stylesheet and 2 JSON requests. Per page: the main document is fetched once with `reqwest`, then a fresh browser context and page are created, and `Fetch` interception pauses every request. The main document is fulfilled from memory and every other request is fetched by `reqwest` from the fixture and fulfilled with `Fetch.fulfillRequest`. The page counts as rendered when the client-rendered title is in `content()`. Each configuration ran twice, alternating the crates, 10 s apart. Memory is the summed resident memory of the browser process tree sampled every 250 ms (this counts shared pages more than once, so it overstates real use); "steady" is the median of the second half of the run. In the table, memory, paused peak and maximum pause are the higher of the two runs; means are the average of the two.

| Crate | Tabs | Pages/s (run 1, run 2) | Page time p50 | Memory peak / steady | Paused requests: peak, mean | Pause time mean / max | Paused > 5 s |
|---|---|---|---|---|---|---|---|
| chromiumoxide | 4 | 19.9, 20.7 | 191 to 198 ms | 668 / 582 MB | 13, 0.8 | 5.3 / 41 ms | 0 |
| chromey | 4 | 20.0, 20.7 | 190 to 198 ms | 663 / 576 MB | 12, 0.9 | 5.5 / 93 ms | 0 |
| chromiumoxide | 8 | 22.1, 22.1 | 358 to 359 ms | 1,006 / 883 MB | 19, 1.5 | 9.9 / 41 ms | 0 |
| chromey | 8 | 23.2, 22.7 | 342 to 349 ms | 1,009 / 882 MB | 18, 1.7 | 10.1 / 59 ms | 0 |
| chromiumoxide | 16 | 22.3, 21.8 | 711 to 727 ms | 1,734 / 1,493 MB | 37, 2.9 | 19.4 / 78 ms | 0 |
| chromey | 16 | 21.5, 22.1 | 719 to 739 ms | 1,711 / 1,481 MB | 37, 3.4 | 21.5 / 81 ms | 0 |

All numbers: 4-core container. Every run rendered all 2,000 pages with no failure, and the fixture saw exactly 7 requests per page (14,000 per run), so nothing was answered twice or lost.

What this shows:

- **No back-pressure or deadlock.** Across 12 runs and 168,000 intercepted requests, no request stayed paused longer than 93 ms and none came near 5 s. The number of paused requests over time (largest count in each 10 s window) stayed flat for the whole run, for example 5 to 12 at 8 tabs, with no growth towards the end. Both crates deliver events to listeners over unbounded channels, so a slow consumer would grow memory rather than block the connection; the render code must keep its own per-page bounds.
- **Throughput levels off at 8 tabs on 4 cores** (about 22 pages/s); 16 tabs only doubles page time and adds about 70% memory. Per page at 4 tabs, creating the browser context and page took about 85 ms, navigation 62 ms, waiting for the client-rendered content 32 ms, `content()` 4 ms and closing 8 ms. The default render concurrency of 4 (plan D2) is sensible; on this container 8 is the useful maximum.
- **Fulfilling from Rust is cheap.** Base64 adds 0.20 MB per page crossing the DevTools connection, and encoding it took 0.22 to 0.32 ms per page in total. With `--mode continue` (Chrome fetches the subresources itself through `--host-resolver-rules`; the main document is continued too, because a page fulfilled from memory has no address and Chrome 141 then fails its requests to 127.0.0.1 with `net::ERR_FAILED` under its Local Network Access checks), 8 tabs gave 23.5 and 22.5 pages/s with chromiumoxide and 23.2 and 22.7 with chromey, against 22.1 (chromiumoxide) and 23.0 (chromey) on average with fulfilment. The fulfil path costs at most about 4% of throughput here, far below the 20% that would trigger the R6 fallback. R6 stands.
- **Full Chrome is much slower than the headless shell.** The same 2,000 pages with the full Chromium 141 binary in new headless mode (`--headless=new`) at 8 tabs gave 4.2 pages/s with both crates (page time p50 1.9 s, 2.1 GB peak), five times slower; creating the context and page alone took about 0.9 s per page. This matters for R2, which prefers an installed Chrome or Edge (full browsers) over downloading `chrome-headless-shell`: see the note below.
- **Pipe transport (R12 option 1): not supported by either crate.** Both connect only over a WebSocket (`chromiumoxide` 0.9.1 `src/conn.rs` and `chromey` 2.58.2 `src/conn.rs` are built on `async_tungstenite`/`tokio-tungstenite` streams), and neither has a launch option for `--remote-debugging-pipe`. R12 option 2, the guard process, is therefore the plan of record for T010.
- **Binary size.** A small release program (same profile as the workspace: thin LTO, one codegen unit, stripped) that launches a browser, intercepts, fulfils and reads `content()` grew from 4,092,280 bytes without a CDP client to 9,127,480 bytes with `chromiumoxide` (+5.0 MB) and 9,137,520 bytes with `chromey` (+5.0 MB). Both are within the 8 MB budget of R15. The real increase in the `spiderlens` binary is measured again in T080.

**Note for the plan (R2, plan D1).** Because a full Chrome or Edge rendered five times slower than the headless shell here, SC-002 (10 pages/s with defaults) may not be reachable on the reference machine with an installed browser. This is one container measurement with a Chromium build, not Google Chrome or Edge, and needs confirming on the reference laptop in T080. If it holds, the owner may want to revisit decision 1 of "Decisions for the owner" (prefer a downloaded `chrome-headless-shell` once one exists, and use an installed browser only until then).

## Sources

- Screaming Frog user guide, configuration (rendering, window size, JavaScript tab): https://www.screamingfrog.co.uk/seo-spider/user-guide/configuration/
- Screaming Frog, How to crawl JavaScript websites: https://www.screamingfrog.co.uk/seo-spider/tutorials/crawl-javascript-seo/
- Screaming Frog window size guide: https://www.screamingfrog.co.uk/guides/window-size
- Sitebulb, How to crawl JavaScript websites: https://sitebulb.com/resources/guides/how-to-crawl-javascript-websites/
- chromiumoxide docs: https://docs.rs/chromiumoxide/latest/
- CDP Fetch domain: https://chromedevtools.github.io/devtools-protocol/tot/Fetch/
- Chrome headless modes: https://developer.chrome.com/docs/chromium/headless and https://developer.chrome.com/blog/chrome-headless-shell
- Chrome for Testing versions: https://googlechromelabs.github.io/chrome-for-testing/last-known-good-versions-with-downloads.json
- Playwright browsers: https://playwright.dev/docs/browsers
- Quarto switching to chrome-headless-shell (Apr 2026): https://opensource.posit.co/blog/2026-04-14_chrome-headless-shell/
- wry WebViewBuilder: https://docs.rs/wry/latest/wry/struct.WebViewBuilder.html
- WebView2 WebResourceRequested: https://learn.microsoft.com/en-us/microsoft-edge/webview2/how-to/webresourcerequested
- Google, JavaScript SEO basics: https://developers.google.com/search/docs/crawling-indexing/javascript/javascript-seo-basics
- Google, fix lazy-loaded content: https://developers.google.com/search/docs/crawling-indexing/javascript/lazy-loading
- The 5 second render limit myth: https://tamethebots.com/blog-n-bits/the-5-second-google-render-limit-myth
- Vercel and MERJ, The rise of the AI crawler (Dec 2024): https://vercel.com/blog/the-rise-of-the-ai-crawler
- Advanced Web Ranking, What AI search crawlers read (Aug 2026): https://www.advancedwebranking.com/blog/what-ai-search-crawlers-read
- Headless Chromium resource requirements (secondary): https://webscraping.ai/faq/headless-chromium/what-are-the-resource-requirements-for-running-headless-chromium-at-scale
- GitHub Actions runner images (installed software): https://github.com/actions/runner-images
- SQLite, name resolution of temp objects: https://www.sqlite.org/lang_naming.html
