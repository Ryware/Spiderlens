# Feature Specification: JavaScript Rendering

**Feature Branch**: `014-javascript-rendering`

**Created**: 2026-10-13

**Status**: Draft (awaiting owner approval)

**Input**: User description: "JavaScript rendering, in the spirit of Screaming Frog's JavaScript rendering mode: an opt-in crawl mode that renders pages in a real browser engine, extracts SEO data from the rendered page, compares it with the raw HTML, discovers links that only exist after JavaScript runs, lets the user control timeouts, blocked resources and rendering concurrency, shows rendered data and raw-versus-rendered differences in the results table, and records the data the later answer engine, generative engine and AI agent features need. Crawl speed is the top priority: rendering must be opt-in and must not slow the raw crawl."

## Overview

Many modern sites build part or all of their pages with JavaScript. A crawler that only reads the HTML as served (the "raw HTML", which is what SpiderLens does today) misses titles, links, text and structured data that a browser, and Google, would see. Google renders pages in an up-to-date Chromium-based browser before indexing them. Most AI crawlers (for example the ones used by ChatGPT, Claude and Perplexity) do not run JavaScript at all and only read the raw HTML.

That gap is exactly what SEO and AEO/GEO users need to see: what does a search engine see after rendering, what does an AI crawler see without rendering, and what is missing from one or the other. This feature adds an opt-in **JavaScript rendering mode**. When it is on, every eligible page is fetched once as today, extracted from the raw HTML as today, and then also rendered in a browser engine and extracted again from the rendered page. SpiderLens stores both versions, compares them, and shows the differences.

When rendering is off (the default), nothing about the crawl changes: no browser is started, and the raw crawl keeps its full speed.

This feature belongs to milestone M4. It builds on the frontier and scope feature (003), fetching and error handling (004), page data extraction (005), crawl storage (006), the results table (007) and crawl performance (008). It prepares data for AEO and GEO checks (015) and agent access (016).

## User Scenarios & Testing *(mandatory)*

### User Story 1 - See what a search engine sees after JavaScript runs (Priority: P1)

An SEO consultant audits a site built with a JavaScript framework. In raw HTML mode most pages show an empty title, no H1 and only a handful of links. They switch the crawl to JavaScript rendering mode and crawl again. Each HTML page is now rendered the way a modern browser renders it, and the title, meta description, headings, word count, canonicals, robots directives, hreflang, structured data, images and links in the results come from the rendered page. Pages that are only reachable through links added by JavaScript are discovered and crawled.

**Why this priority**: This is the reason the feature exists. Without it SpiderLens gives wrong answers on a large share of modern sites, and users have to switch to another tool for them.

**Independent Test**: Crawl a local fixture site whose pages add their title, H1, body text, JSON-LD and navigation links with JavaScript, once with rendering off and once with it on. With it off, the stored values match the raw HTML only and the JavaScript-only pages are not found. With it on, the stored rendered values match what a reference browser shows for each page, and every JavaScript-only page is discovered and crawled.

**Acceptance Scenarios**:

1. **Given** a page whose raw HTML has `<title>Loading</title>` and whose script changes the title to "Red Shoes | Shop", **When** it is crawled with rendering on, **Then** the rendered title "Red Shoes | Shop" is stored and shown in the Page titles tab, and the raw title "Loading" is also stored.
2. **Given** a page whose navigation links are all inserted by a script, **When** it is crawled with rendering on, **Then** those links are stored as links of that page, marked as found only in the rendered page, and their targets are handed to URL discovery under the normal scope and follow rules.
3. **Given** the same site, **When** it is crawled with rendering off, **Then** no browser engine is started, the JavaScript-only links are not stored, and their targets are not crawled.
4. **Given** a page that adds a JSON-LD block with a script, **When** it is crawled with rendering on, **Then** the block is stored with the page's structured data and marked as present only in the rendered page.
5. **Given** a page whose script sets `<meta name="robots" content="noindex">` after load, **When** it is crawled with rendering on, **Then** the page's indexability reflects the rendered directive and the page is flagged as "Noindex only in rendered HTML".
6. **Given** a page whose script changes `window.location` to another URL during rendering, **When** it is crawled with rendering on, **Then** a JavaScript redirect from the page to the target is recorded and the target is handed to URL discovery.
7. **Given** a crawl with rendering on, **When** it is run from the command-line interface instead of the desktop app, **Then** the same pages are rendered with the same settings and the same data is stored, without any window being shown.

---

### User Story 2 - Rendering never slows the raw crawl (Priority: P1)

Crawl speed is the owner's top priority. A user who never turns rendering on must not pay any cost for it: same speed, same memory, no browser process, no larger install than needed. A user who does turn it on accepts that rendering is slower than fetching, but the raw part of the crawl (fetching, raw extraction, discovery from raw links) keeps running at its normal pace and is never held up waiting for the browser.

**Why this priority**: The owner stated speed as the top priority. A rendering feature that slows every crawl, or that makes the raw crawl wait behind a slow browser, would undo the work of the crawl performance feature (008).

**Independent Test**: Run the 100,000-page benchmark from spec 008 with rendering off on a build that includes this feature and compare with the stored baseline. Then crawl a mixed fixture site with rendering on and record, from the fixture site's request log, the raw fetch rate while rendering is in progress.

**Acceptance Scenarios**:

1. **Given** rendering is off, **When** the user crawls the 100,000-page benchmark fixture, **Then** throughput and memory are within the benchmark's normal run-to-run variation of the baseline without this feature, and no browser process is started at any point.
2. **Given** rendering is on and the browser renders more slowly than pages are fetched, **When** the crawl runs, **Then** pages keep being fetched and extracted from raw HTML at the rate the speed settings allow, pages waiting to be rendered are queued on disk, and memory use does not grow with the size of that queue.
3. **Given** rendering is on, **When** the crawl runs, **Then** each page's main document is requested from the site only once: the browser renders from the response already received rather than requesting the page again.
4. **Given** rendering is on, **When** the user looks at the progress display, **Then** it shows separately how many pages have been fetched and how many have been rendered, the rendering rate, and how many pages are waiting to be rendered.
5. **Given** rendering is on, **When** the user pauses, quits and later resumes the crawl, **Then** rendering continues with the pages that were waiting, no page is rendered twice, and no page is skipped.

---

### User Story 3 - Compare raw and rendered HTML (Priority: P1)

The user wants to know exactly what JavaScript changes on each page, because each change is a risk: content only in the rendered page depends on search engines rendering it and is invisible to AI crawlers that do not run JavaScript; a directive or canonical that changes after rendering sends search engines mixed signals. A new **JavaScript** tab lists every rendered page with what changed, and the detail pane shows raw and rendered HTML side by side with the differences highlighted.

**Why this priority**: The comparison is what turns rendering from "slower crawl" into insight, and it is the base the AEO and GEO checks in milestone M5 build on.

**Independent Test**: Crawl a fixture set of pages with known, deliberate differences between raw and rendered HTML (title changed, description added, H1 removed, canonical changed, noindex added, noindex removed, links added, text added, JSON-LD added) and check that each page appears under exactly the expected JavaScript filters with the expected values.

**Acceptance Scenarios**:

1. **Given** a page whose rendered page has 40 links of which 12 are not in the raw HTML, **When** the crawl completes, **Then** the JavaScript tab shows raw link count 28, rendered link count 40 and 12 JavaScript-only links, and the page appears under the "Contains JavaScript links" filter.
2. **Given** a page whose visible text grows from 50 words in the raw HTML to 900 words after rendering, **When** the crawl completes, **Then** the JavaScript tab shows both word counts and the change, and the page appears under the "Contains JavaScript content" filter.
3. **Given** a page with `noindex` in the raw HTML that a script removes, **When** the crawl completes, **Then** the page appears under "Noindex only in original HTML" and the detail pane explains that search engines may never render the page because they read the raw noindex first.
4. **Given** a page whose canonical in the raw HTML differs from the canonical after rendering, **When** the crawl completes, **Then** the page appears under "Canonical mismatch" with both values shown.
5. **Given** a page whose title, meta description or H1 is changed or removed by JavaScript, **When** the crawl completes, **Then** it appears under the matching "Updated by JavaScript" filter with the raw and rendered values; when the element is missing from the raw HTML and added by JavaScript, it appears under the matching "Only in rendered HTML" filter instead.
6. **Given** a rendered page selected in the results table, **When** the user opens the "Raw vs rendered" section of the detail pane, **Then** raw and rendered HTML are shown side by side, normalised so that only real differences are highlighted, and the user can jump between differences and copy either version.
7. **Given** a page where rendering changes nothing that SpiderLens extracts, **When** the crawl completes, **Then** the page appears in the JavaScript tab under "All" only and is not counted in any difference filter.

---

### User Story 4 - Control how pages are rendered (Priority: P2)

The user tunes rendering to the site and their computer: how long to wait for a page to finish rendering, the window size (mobile or desktop, matching the user agent by default), which kinds of resources the browser may load, how many pages are rendered at the same time, and whether to render all internal HTML pages or only pages matching include patterns. Defaults are fast and close to how Google renders.

**Why this priority**: Defaults cover most sites, but slow or heavy sites, low-memory laptops and sites the user does not control each need different settings. It is P2 because the defaults already deliver Stories 1 to 3.

**Independent Test**: Crawl fixture pages that finish rendering after a known delay, load images, fonts, media and third-party scripts, and load resources disallowed by robots.txt, under different settings, and check from the fixture logs and stored data that each setting had exactly the documented effect.

**Acceptance Scenarios**:

1. **Given** the default settings, **When** a page finishes loading and its network activity and page changes settle, **Then** rendering of that page ends as soon as it has settled, without waiting for a fixed delay.
2. **Given** a maximum render time of 10 seconds, **When** a page keeps loading resources forever, **Then** rendering of that page stops after 10 seconds, the page as rendered at that moment is stored, and the page is flagged "Render timed out".
3. **Given** the default resource settings, **When** a page is rendered, **Then** images, video, audio and fonts are not downloaded, while scripts, stylesheets and data requests the page makes are, and the fixture log confirms it.
4. **Given** the user turns on "Load all resources", **When** a page is rendered, **Then** images, media and fonts are downloaded too.
5. **Given** a script on the crawled site disallowed by the site's robots.txt for the configured user agent, **When** a page using it is rendered with default settings, **Then** the script is not downloaded, the page is flagged "Has blocked resources", and the blocked resource is listed with the rule that blocked it.
6. **Given** a rendering concurrency of 2, **When** the crawl runs, **Then** no more than 2 pages are being rendered at any moment.
7. **Given** the user agent preset "Googlebot smartphone", **When** pages are rendered with the default window size, **Then** they are rendered at a mobile window size; with a desktop user agent, at a desktop window size.
8. **Given** a crawl with rendering on and an include pattern for rendering of `/products/`, **When** the crawl runs, **Then** only internal HTML pages matching that pattern are rendered, and every other page is recorded as "Not rendered: outside rendering rules".
9. **Given** any rendering setting, **When** the crawl is saved and reopened, **Then** the setting is shown as it was used, and the command-line interface accepts the same setting with the same default.

---

### User Story 5 - Know when rendering went wrong (Priority: P2)

Rendering fails in ways fetching does not: scripts throw errors, pages never settle, resources fail to load, the browser runs out of memory or crashes. The user wants every such problem recorded against the page, never a silent gap, and wants the crawl to continue. A page whose rendering fails still keeps all its raw HTML data.

**Why this priority**: The constitution requires that failures are never silent. JavaScript errors and failed resources are also useful SEO findings in their own right (a script error can stop content from appearing for Google too).

**Independent Test**: Crawl fixture pages that throw script errors, log console errors, request missing resources, never settle, and crash the rendering process, and check the stored problems, the retry behaviour and that the crawl finishes with every page in exactly one final rendering state.

**Acceptance Scenarios**:

1. **Given** a page whose script throws an uncaught error, **When** it is rendered, **Then** the error message and its source location are stored against the page and the page appears under the "JavaScript errors" filter.
2. **Given** a page that requests a script returning 404, **When** it is rendered, **Then** the failed resource is stored with its URL and status and the page appears under "Has failed resources".
3. **Given** the rendering process crashes while rendering a page, **When** the crash happens, **Then** the crawl continues, the page is retried once in a fresh rendering process, and if it fails again it is recorded as "Render failed" with the reason while its raw HTML data stays in place.
4. **Given** the computer is running low on free memory, **When** rendering is in progress, **Then** SpiderLens reduces the number of pages rendered at once rather than letting the system swap or the browser crash, and the progress display says so.
5. **Given** no usable browser engine is available on the computer, **When** the user starts a crawl with rendering on, **Then** they are told before the crawl starts, are offered a way to get one, and can instead run the crawl in raw HTML mode; the crawl never starts in rendering mode and then silently skips rendering.
6. **Given** a completed crawl, **When** the user looks at the summary, **Then** it shows how many pages were rendered, how many timed out, how many failed and why, and how many were not rendered and why, and these add up to the number of pages eligible for rendering.

---

### User Story 6 - Keep rendered pages for later checks (Priority: P3)

The user wants the rendered HTML of each page kept inside the crawl file, just as the raw HTML is, so later checks (custom extraction in M3, AEO and GEO checks in M5, AI agents in M6) can run on the crawl without rendering again. Optionally they also want a screenshot of each rendered page to see what the browser saw.

**Why this priority**: It protects the value of a rendered crawl for later milestones and helps debugging, but the rendered data from Stories 1 to 3 is useful without it.

**Independent Test**: Crawl a fixture site with rendering on and with screenshots on and off, then retrieve the stored rendered HTML and screenshots for sample pages and compare them with what was produced at crawl time.

**Acceptance Scenarios**:

1. **Given** rendering on and "Store page HTML" on (the default from spec 005), **When** a page is rendered, **Then** its rendered HTML is stored compressed in the crawl file and can be retrieved exactly as it was serialised at the end of rendering, from the desktop app and from the command-line interface.
2. **Given** the user turns on "Save screenshots", **When** a page is rendered, **Then** a screenshot of the rendered page at the rendering window size is stored in the crawl file and shown in the detail pane.
3. **Given** "Save screenshots" is off (the default), **When** pages are rendered, **Then** no screenshots are taken or stored.
4. **Given** "Store page HTML" is off, **When** pages are rendered, **Then** all extracted rendered data is still stored but neither raw nor rendered HTML is kept.

---

### User Story 7 - See how much of each page depends on JavaScript (Priority: P3)

AI crawlers that do not run JavaScript only see the raw HTML. The user wants, for each rendered page, a simple measure of how much of its SEO-relevant content (text, headings, title, meta description, links, structured data, canonical and directives) exists only after rendering, so they can find pages that AI answer engines effectively cannot read. This feature stores and shows that measure; the AEO and GEO checks in milestone M5 turn it into issues and advice.

**Why this priority**: It is the bridge to the product's differentiator (AEO, GEO and AI agents), but the issue rules and reports that use it belong to specs 009 and 015.

**Independent Test**: Crawl fixture pages ranging from fully server-rendered to fully client-rendered and check that each page's stored JavaScript dependence values match the expected share of content found only after rendering.

**Acceptance Scenarios**:

1. **Given** a page whose raw HTML is an empty application shell and whose rendered page has all its text, **When** it is crawled with rendering on, **Then** its JavaScript dependence shows that 100% of its visible text, its headings and its main content links exist only after rendering, and it appears under the "Content hidden from non-rendering crawlers" filter.
2. **Given** a fully server-rendered page whose scripts change nothing extracted, **When** it is crawled with rendering on, **Then** its JavaScript dependence is 0% for every element.
3. **Given** a crawl with rendering on, **When** the user exports the JavaScript tab, or an agent reads the crawl through the later agent access feature, **Then** the per-page JavaScript dependence values are included.

---

### Edge Cases

- **Non-HTML resources** (images, PDFs, CSS, scripts, feeds) are never rendered; they keep their response facts only. External URLs are never rendered; they keep their status check only.
- **Redirects, error pages and pages not crawled**: only internal pages that returned a successful HTML response are rendered. A 4xx or 5xx HTML page is not rendered by default, because search engines do not index it.
- **Pages with a raw `noindex`**: rendered like any other page, so the user can see whether JavaScript removes it ("Noindex only in original HTML"); the detail pane explains that Google usually does not render pages it has already seen as noindex.
- **Pages blocked by robots.txt** are not fetched, so they are not rendered either.
- **Pages that change endlessly** (carousels, timers, live tickers, infinite network polling): rendering stops at the settle rule or the maximum render time, whichever comes first, and the page is stored as rendered at that moment.
- **Infinite scroll and content loaded on user interaction** (click to expand, load more buttons): not triggered, as search engines do not interact with pages. The rendering window is tall enough to load content that appears when it comes into view, as Google does.
- **Pages that open dialogs, pop-ups, new windows, print dialogs, file downloads or permission requests** (location, camera, notifications): these are dismissed or refused automatically and never block rendering.
- **Service workers, local storage, cookies and caches**: each page is rendered with a clean state, without data left by previously rendered pages and without the user's own browser profile, cookies or logins, as search engines do.
- **Scripts that detect automation or show bot challenges** (for example a security check page): the page is stored as rendered; if the rendered page looks like a challenge page instead of content, it is flagged "Possible bot challenge".
- **Very heavy pages** (many megabytes of scripts, huge DOMs): rendering is bounded by the maximum render time and a per-page memory limit; a page that exceeds the memory limit is recorded as "Render failed: page too large" and the crawl continues.
- **JavaScript-only redirects** (`location.href`, `location.replace`, meta refresh inserted by script): recorded as a JavaScript redirect with its target; the target is handed to discovery.
- **Links in the rendered page that point to URLs already crawled or already recorded as not crawled**: handled by the normal frontier rules; a link found only in the rendered page can make a URL eligible (for example a shorter depth), as allowed by spec 003.
- **Fragment-based routing** (`#!/products` style URLs): treated as the same URL without the fragment, as in spec 003; the difference is shown as a note in the detail pane rather than crawling fragments as separate pages.
- **Third-party scripts** (analytics, ads, chat widgets, tag managers): loaded by default because they can change the page (for example a tag manager adding structured data), but their requests do not count against the crawled site's speed limits; the user can block third-party requests with one setting.
- **Requests the browser makes to the crawled site** (scripts, stylesheets, data requests) count towards that site's simultaneous-request and URLs-per-second limits and honour its Retry-After, crawl-delay and robots.txt rules, so rendering cannot overload the site.
- **The same script or stylesheet used on thousands of pages** is downloaded once per crawl where the site allows caching, not once per page, to save time and load on the site.
- **Pause, stop and resume**: pages waiting to be rendered and pages being rendered at the moment of pausing are rendered after resuming; a stopped crawl records unrendered pages as "Not rendered: crawl stopped".
- **Turning rendering on for a crawl that was run without it**: SpiderLens can render the already crawled pages afterwards from their stored raw responses without fetching the main documents again, when the raw HTML was stored. Without stored HTML, this is not available and the user is told why.
- **Crawl files without rendering data** (made before this feature, or with rendering off) open normally; the JavaScript tab says the crawl was not rendered.
- **Low-memory computers**: the default rendering concurrency is chosen from the computer's memory and can be lowered by the user; SpiderLens never starts more rendering than the memory guard allows.

## Requirements *(mandatory)*

### Functional Requirements

**Rendering mode**

- **FR-001**: Users MUST be able to choose the rendering mode for a crawl: "Raw HTML" (the default) or "JavaScript rendering". The choice MUST be stored with the crawl's configuration and be available from the command-line interface with the same default.
- **FR-002**: When the rendering mode is "Raw HTML", the system MUST NOT start any browser engine or rendering process, MUST NOT reserve memory for rendering, and MUST behave exactly as it does without this feature.
- **FR-003**: When the rendering mode is "JavaScript rendering", the system MUST render every page that is eligible under FR-004 in a browser engine of the same family and a comparable version to the one Google uses to render pages, and MUST store the engine name and version with the crawl.
- **FR-004**: A page MUST be eligible for rendering when it is an internal URL whose final response is a successfully fetched HTML page (status 2xx) and it matches the rendering include and exclude patterns, if any. Users MUST be able to also render internal 4xx and 5xx HTML pages. Every other URL MUST NOT be rendered.
- **FR-005**: Users MUST be able to set rendering include and exclude patterns (same pattern rules as spec 003) to limit which eligible pages are rendered. Eligible pages that are not rendered MUST be recorded with the reason "Not rendered: outside rendering rules".
- **FR-006**: Users MUST be able to render the pages of an existing crawl afterwards, using the stored raw HTML of each page, without requesting the main documents from the site again. This MUST be refused, with an explanation, for pages whose raw HTML was not stored.

**Rendering behaviour**

- **FR-007**: The system MUST render each page from the main document response already received during the crawl, so that the page's own URL is requested from the site only once per crawl (retries of failures excepted).
- **FR-008**: The system MUST render with the crawl's configured user agent and, by default, a window size that matches it: a mobile window size for mobile user agents and a desktop window size for desktop user agents. Users MUST be able to choose another window size from presets or set width and height.
- **FR-009**: The system MUST make the rendering window tall enough that content which loads when it scrolls into view on a long page is loaded, as Google's renderer does, without scrolling or interacting with the page.
- **FR-010**: The system MUST consider a page rendered when it has finished loading and its network activity and document changes have settled for a short quiet period, or when the maximum render time is reached, whichever comes first. The default maximum render time MUST be 10 seconds, adjustable by the user from 1 to 60 seconds. Users MUST be able to add a fixed minimum wait for sites that update the page after a delay.
- **FR-011**: The system MUST render every page with a clean state: no cookies, storage, caches of page state or service workers carried over from other pages, and never the user's own browser profile, cookies or signed-in accounts.
- **FR-012**: The system MUST automatically dismiss or refuse dialogs, pop-ups, new windows, downloads, print requests and permission requests during rendering, and MUST NOT perform clicks, typing or scrolling on the page.
- **FR-013**: The system MUST record, as a JavaScript redirect, any navigation of the page to a different URL caused by scripts during rendering, with its target, and MUST hand the target to URL discovery.

**Resources**

- **FR-014**: By default the browser MUST NOT download images, video, audio and fonts while rendering, and MUST download scripts, stylesheets, documents in frames and data requests made by the page. Users MUST be able to choose "Load all resources" and to block, individually, images, media, fonts, stylesheets and third-party requests (requests to hosts other than the crawled site's internal hosts).
- **FR-015**: Every request the browser makes to an internal host of the crawl MUST count towards that host's simultaneous-request allowance and the global caps from spec 008, and MUST honour that host's Retry-After waits and crawl-delay. Requests to third-party hosts MUST be limited by a separate cap on simultaneous third-party requests and MUST NOT count against the crawled site's allowance.
- **FR-016**: By default the browser MUST NOT download resources on any host that the host's robots.txt disallows for the configured user agent, and MUST record each such resource as blocked with the rule that blocked it. The user's existing "Ignore robots.txt" choice (spec 003) MUST also apply to page resources.
- **FR-017**: The system MUST reuse a resource already downloaded during the crawl (for example a shared script or stylesheet) when rendering later pages, within the limits of the resource's caching headers and a bounded cache size, instead of downloading it again for every page.
- **FR-018**: For every rendered page, the system MUST record the resources it requested, with URL, type, host, internal or third-party, status or failure reason, and whether it was blocked by settings or by robots.txt. Users MUST be able to see these in the detail pane.

**Extraction from the rendered page**

- **FR-019**: The system MUST extract from the rendered page every item that spec 005 extracts from raw HTML (titles, meta descriptions, headings, word count, language, text fingerprint, robots directives from meta tags, canonicals and hreflang from link elements, links, images, structured data), using the same rules, so that raw and rendered values are directly comparable. Values that come from HTTP headers are the same for both versions.
- **FR-020**: The system MUST keep the raw values from spec 005 unchanged and store the rendered values alongside them for each rendered page.
- **FR-021**: When rendering is on, the values shown in the existing results tabs and used for indexability MUST be the rendered values for rendered pages and the raw values for every other page, and each row MUST make clear which version is shown. Users MUST be able to switch the results view to raw values for the whole crawl.
- **FR-022**: For a rendered page, the system MUST hand the targets of links, canonicals, hreflang entries and image sources found only in the rendered page to URL discovery, under the same scope, follow and nofollow rules as raw links (spec 003). A rendered page's nofollow directives MUST apply to links found in the rendered page.
- **FR-023**: Each link, image, heading, structured data block, canonical, hreflang entry and robots directive of a rendered page MUST be marked as found in raw HTML only, rendered page only, or both.

**Raw versus rendered comparison**

- **FR-024**: For each rendered page, the system MUST compute and store: raw and rendered word count and the difference; raw and rendered link counts and the number of links found only in the rendered page and only in the raw HTML; whether the title, meta description, first H1, canonical, robots directives (meta) or hreflang set differ between raw and rendered, and both values when they do; the number of structured data blocks found only after rendering.
- **FR-025**: The system MUST show a JavaScript tab with one row per URL eligible for rendering, with default columns Address, Rendering status, Render time, Word count (raw), Word count (rendered), Word count change, Links (raw), Links (rendered), JavaScript-only links, JavaScript dependence, JavaScript errors and Blocked resources, and with the filters listed in FR-026. Every filter MUST show its count, refreshed as defined by spec 007.
- **FR-026**: The JavaScript tab MUST offer these filters: All; Rendered; Not rendered; Render timed out; Render failed; Contains JavaScript links; Contains JavaScript content; Title only in rendered HTML; Title updated by JavaScript; Meta description only in rendered HTML; Meta description updated by JavaScript; H1 only in rendered HTML; H1 updated by JavaScript; Canonical only in rendered HTML; Canonical mismatch; Noindex only in original HTML; Noindex only in rendered HTML; Nofollow only in original HTML; Nofollow only in rendered HTML; Structured data only in rendered HTML; JavaScript redirect; JavaScript errors; Has failed resources; Has blocked resources; Possible bot challenge; Content hidden from non-rendering crawlers.
- **FR-027**: "Contains JavaScript content" MUST list pages whose rendered word count is higher than the raw word count by more than a threshold, by default 20% and at least 50 words, adjustable by the user; the threshold MUST appear in the filter's label.
- **FR-028**: The detail pane MUST add, for rendered pages, the sections: Rendered HTML, Raw vs rendered (side-by-side, normalised, differences highlighted, next and previous difference, copy each side), Resources (FR-018), JavaScript console (errors and warnings with source location), and Screenshot (when stored). Existing sections (Headings, Outlinks, Images and the others from spec 007) MUST show for each entry whether it was found in raw HTML only, rendered page only or both.

**JavaScript dependence (for AEO, GEO and agents)**

- **FR-029**: For each rendered page, the system MUST compute and store a JavaScript dependence measure per content type: the share of visible text, headings, internal links, structured data blocks and images that exist only in the rendered page, plus whether the title, meta description, canonical and robots directives are present in the raw HTML. It MUST also store one overall JavaScript dependence percentage for the page, defined as the share of the page's rendered visible text that is not present in the raw HTML.
- **FR-030**: "Content hidden from non-rendering crawlers" MUST list pages whose overall JavaScript dependence is above a threshold, by default 50%, adjustable by the user, or whose title or main heading exists only in the rendered page.
- **FR-031**: JavaScript dependence values and both raw and rendered extracted values MUST be stored so that later features (issue rules in 009, exports in 010, AEO and GEO checks in 015, agent access in 016) can read them from the crawl file without rendering again.

**Storage**

- **FR-032**: When "Store page HTML" (spec 005) is on, the system MUST store each rendered page's serialised rendered HTML compressed in the crawl file, alongside its raw HTML, retrievable exactly as serialised at the end of rendering, from the desktop app and the command-line interface.
- **FR-033**: Users MUST be able to turn on "Save screenshots" (default off). When on, the system MUST store one compressed screenshot per rendered page at the rendering window size, capped at a maximum height, and show it in the detail pane.
- **FR-034**: Every rendering setting MUST be stored with the crawl's configuration, and a reopened crawl MUST show the settings that produced it. Crawl files without rendering data MUST keep opening, with the JavaScript tab explaining that the crawl was not rendered.

**Speed, concurrency and resource use**

- **FR-035**: Rendering MUST run separately from fetching: fetching, raw extraction and discovery from raw links MUST continue at the rate the speed settings allow while pages wait to be rendered, and MUST NOT wait for rendering, except as needed to keep the waiting queue bounded on disk.
- **FR-036**: Pages waiting to be rendered MUST be kept on disk, so memory use does not grow with the number of waiting pages, and MUST survive pause, quit and resume.
- **FR-037**: Users MUST be able to set the number of pages rendered at the same time. The default MUST be derived from the computer's memory (on the 16 GB reference machine from spec 008, the default is 4), with an upper limit of 16.
- **FR-038**: The system MUST watch the memory used by rendering and the computer's free memory. It MUST lower the number of pages rendered at once when free memory runs low or the rendering memory budget is reached (default 2 GB on the reference machine, adjustable), and MUST raise it again gradually when memory recovers.
- **FR-039**: A crawl with rendering on MUST finish only when every eligible page has reached exactly one final rendering state: Rendered, Render timed out, Render failed (with reason), or Not rendered (with reason: outside rendering rules, crawl stopped, browser unavailable).
- **FR-040**: The progress display (desktop and command line) MUST show, in addition to the figures from spec 008: pages rendered, pages waiting to be rendered, pages being rendered, the current rendering rate, and, when rendering is slower than fetching, that rendering is the limiting factor and why (concurrency setting, memory guard, slow pages, site limits).
- **FR-041**: The end-of-crawl summary MUST include the rendering totals by final rendering state and reason, the average and slowest render times, and the totals for JavaScript errors, failed resources and blocked resources.

**Errors and recovery**

- **FR-042**: A page whose rendering fails because of a renderer crash or an unexpected renderer error MUST be retried once in a fresh rendering context. If it fails again it MUST be recorded as "Render failed" with the reason. A rendering failure MUST NOT change or remove the page's raw data, fetch result or final crawl state.
- **FR-043**: The system MUST record, for each rendered page, uncaught script errors and console errors with their message and source location, up to a bounded number per page, plus the total count.
- **FR-044**: The system MUST flag a rendered page as "Possible bot challenge" when the rendered page matches known patterns of bot challenge and interstitial pages instead of the content seen in the raw HTML.
- **FR-045**: When no usable browser engine is available, the system MUST tell the user before the crawl starts, MUST offer a way to obtain one with its download size shown and the user's consent, and MUST let the user run the crawl in raw HTML mode instead. A crawl MUST NOT start in rendering mode without a usable engine. From the command-line interface, the same check MUST fail with a clear message and a non-zero exit code unless the user explicitly allows the download.
- **FR-046**: The system MUST not let rendering processes outlive the crawl: when a crawl ends, is stopped or the app quits (including after a crash of the app), all rendering processes it started MUST be closed.

**Parity**

- **FR-047**: Every capability and setting in this spec MUST be available from the command-line interface with the same defaults, without opening any window, and MUST produce the same stored data as the desktop app for the same crawl.

### Key Entities *(include if feature involves data)*

- **Rendering settings**: Mode (Raw HTML or JavaScript rendering), maximum render time, minimum wait, window size, resource loading choices, third-party blocking, robots.txt for resources, rendering include and exclude patterns, render non-2xx pages, concurrency, memory budget, save screenshots, thresholds for "JavaScript content" and "hidden from non-rendering crawlers". Saved with the crawl.
- **Rendering engine record**: Engine name and version used for the crawl, and where it came from. Saved with the crawl.
- **Rendering result**: Per eligible page: final rendering state and reason, attempts, render time, window size used, JavaScript redirect target if any, error and console counts, possible bot challenge flag.
- **Rendered page content**: The same structure as spec 005's page content (titles, meta descriptions, headings, word count, language, text fingerprint, directives, canonicals, hreflang, links, images, structured data), for the rendered version, with each item marked raw only, rendered only or both.
- **Raw vs rendered comparison**: Per rendered page: word counts and change, link counts and JavaScript-only links, changed title, description, H1, canonical, directives and hreflang with both values, structured data only in rendered HTML, and the JavaScript dependence values.
- **Page resource**: Per rendered page: URL, resource type, host, internal or third-party, outcome (loaded with status, failed with reason, blocked by setting, blocked by robots.txt with rule), served from crawl cache or not.
- **Console entry**: Per rendered page: level (error or warning), message, source location, order.
- **Stored rendered body**: The compressed rendered HTML of one page.
- **Screenshot**: The compressed image of one rendered page, with its size.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: With rendering off, the 100,000-page benchmark from spec 008 on a build with this feature stays within 2% of the throughput and within 5% of the peak memory of the build without it, and no rendering process is started.
- **SC-002**: With rendering on and default settings, on the reference machine and a local fixture site of 10,000 typical client-rendered pages, SpiderLens renders at least 10 pages per second on average, and the crawl's raw fetching runs at least as fast as with rendering off until the waiting queue limit is reached.
- **SC-003**: With rendering on and default settings, the total memory used by SpiderLens and its rendering processes stays below 3 GB on the reference machine throughout a 100,000-page rendered crawl, and memory at 100,000 pages is no more than 20% higher than at 10,000 pages.
- **SC-004**: On a fixture set of at least 50 JavaScript-dependent pages, the rendered title, meta description, headings, canonical, robots directives, link list and structured data stored by SpiderLens match those produced by a reference browser of the same engine family for at least 98% of pages.
- **SC-005**: On a fixture set covering every JavaScript tab filter, 100% of pages appear under exactly the expected filters with the expected raw and rendered values.
- **SC-006**: Fixture site logs show, in 100% of rendered crawls, that each rendered page's own URL was requested once (retries excepted), that no rendering request broke the site's caps, Retry-After or crawl-delay, and that no resource disallowed by robots.txt was requested with default settings.
- **SC-007**: A page that never settles finishes rendering within the maximum render time plus 2 seconds in 100% of cases, and is stored as "Render timed out".
- **SC-008**: In a fault-injection run where the rendering process is killed repeatedly, the crawl completes, every eligible page ends in exactly one final rendering state, and no raw page data is lost.
- **SC-009**: After the desktop app or the command-line crawler is closed or killed during a rendered crawl, no rendering process from that crawl is still running 10 seconds later.
- **SC-010**: A desktop user who has never used rendering can turn it on, run a rendered crawl and find the pages with JavaScript-only links in under 3 minutes in usability checks, without reading documentation.
- **SC-011**: For 100% of rendered fixture pages, the stored JavaScript dependence percentage is within 2 percentage points of the expected value.

## Assumptions

- **Scope**: Milestone M4. Spider and List modes. The AEO and GEO issue rules, reports and advice that use the JavaScript dependence data belong to specs 009 and 015; agent access to it belongs to spec 016; custom extraction from rendered HTML belongs to spec 013.
- **Out of scope**: logins and form-based authentication for rendering; scripted interactions (clicking, scrolling, typing, custom scripts run on each page); rendering with several user agents in one crawl; Core Web Vitals and other performance metrics of rendered pages (a later spec, possibly through PageSpeed Insights); visual comparison of screenshots between crawls (spec 012); rendering external pages; using a browser engine other than the Chromium family; rendering with the user's own browser profile or cookies.
- **Engine family**: Google renders with an up-to-date Chromium-based engine, so "comparable to Google" means a current Chromium-based engine. How the engine is obtained (the user's installed Chrome or Edge, a one-time download, or shipping one with the app) is a planning decision recorded in research.md; the spec only requires FR-003, FR-045 and the install-size goal below.
- **Install size**: Rendering should not make the SpiderLens installer larger for users who never render. Obtaining the engine on first use, with consent, is assumed to be acceptable to users who turn rendering on.
- **Window sizes**: Default mobile size 411 × 731 and desktop size 1024 × 768 (CSS pixels), the Googlebot smartphone and desktop sizes other SEO crawlers use; the rendering window is then extended in height as described in FR-009 (Google is reported to extend to about 12,000 pixels). To be confirmed in planning.
- **Settle rule**: "Settled" means no network requests in flight and no document changes for 500 ms after the load event; the exact rule is tuned in planning against the fixture set and real sites.
- **Default concurrency** of 4 renders on the 16 GB reference machine is based on typical memory use of about 100 to 300 MB per page being rendered plus the shared browser process; the plan's benchmarks confirm or adjust it.
- **Rendering throughput target** (SC-002) assumes a local fixture with typical client-rendered pages (a few hundred KB of script, data from the same host, no third-party requests). Real sites will be slower; the speed indicator explains why.
- The rendering window height extension, the clean state per page and the refusal of permissions follow publicly documented behaviour of Google's renderer; differences from Google that remain are listed in the documentation.
- Thresholds (20% and 50 words for "JavaScript content", 50% for "hidden from non-rendering crawlers") are starting values the owner may tune during `/speckit-clarify`.
- Rendered HTML is stored under the same "Store page HTML" switch as raw HTML; a separate switch is not added (constitution principle VIII, simplicity).
- Crawl file format changes follow the storage feature (006): files without rendering data keep opening.
