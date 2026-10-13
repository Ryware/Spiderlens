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

Each item has a recommended default, which the spec already assumes.

1. **Browser source**: (a) installed Chrome or Edge first, else download with consent *(recommended)*; (b) always download a pinned version; (c) bundle in the installer.
2. **What to render**: (a) every eligible internal HTML page, with optional include and exclude patterns *(recommended, matches SF)*; (b) a sample first (for example 100 pages per URL pattern), then only the patterns where rendering changes something. (b) is much faster on big sites but more complex; it could come later as a "Smart" mode.
3. **Default resource blocking**: (a) skip images, media and fonts *(recommended, faster, rarely changes the DOM)*; (b) load everything like Google.
4. **Links found only after rendering**: (a) crawl them *(recommended, as Google does)*; (b) record them only.
5. **Screenshots in M4**: (a) included, off by default *(recommended)*; (b) postponed.

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
