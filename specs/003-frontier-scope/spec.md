# Feature Specification: Crawl Frontier and Scope

**Feature Branch**: `003-frontier-scope`

**Created**: 2026-10-11

**Status**: Approved (2026-10-11)

**Input**: User description: "Crawl frontier and scope: decides which discovered URLs get crawled and in what order. Breadth-first by depth, URL normalization and de-duplication so each page is crawled once, internal vs external classification, include/exclude rules, depth and URL limits, robots.txt rules and crawl-delay for the configured user agent, nofollow handling, XML sitemap URLs from robots.txt as an optional seed, and every URL that is not crawled records why (blocked by robots, excluded, over limit, external)."

## Overview

The frontier is the part of SpiderLens that answers two questions for every URL the crawler discovers: "should this URL be crawled?" and "if so, when?". It turns raw links found on pages into a clean, de-duplicated, breadth-first queue that stays inside the scope the user chose, obeys the website's robots.txt rules and crawl-delay, and never silently drops anything. A URL that is not crawled is still recorded, together with the reason it was not crawled, so the user can always answer "why is this page missing from my crawl?".

This feature belongs to milestone M1 (core crawl). It applies to both M1 crawl modes: Spider mode (start from one URL and follow links) and List mode (crawl a list of URLs supplied by the user).

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Crawl every page of a site exactly once, closest pages first (Priority: P1)

An SEO specialist enters a start URL and starts a Spider crawl. SpiderLens follows links found on each page, crawls the pages nearest the start URL first (the home page, then pages one click away, then two clicks away, and so on), and crawls each page only once even when it is linked hundreds of times or written in slightly different ways (different letter case in the host name, a `#section` fragment, a default port, `./` or `../` segments). Links that point to other websites are recorded as external links but those websites are not crawled.

**Why this priority**: Without a correct, de-duplicated, in-scope queue there is no usable crawl at all. Breadth-first order also means that if the user stops early, they already have the most important pages (the ones closest to the home page) and accurate click depth for every page.

**Independent Test**: Run a Spider crawl against a local fixture site with a known link graph (including duplicate links written in different forms and links to an external fixture host). Verify that each internal page is fetched exactly once, that the recorded click depth of each page equals its shortest link distance from the start URL, and that external URLs appear in results as external and were not fetched for crawling.

**Acceptance Scenarios**:

1. **Given** a site where page A is linked from the home page as `HTTP://Example.com/a`, `http://example.com:80/a#top` and `http://example.com/x/../a`, **When** the user crawls from `http://example.com/`, **Then** page A is fetched exactly once and all three links in the results point to the same URL `http://example.com/a`.
2. **Given** a site where page B is reachable both in 2 clicks and in 5 clicks from the home page, **When** the crawl completes, **Then** page B's recorded crawl depth is 2.
3. **Given** a crawl is running, **When** a page at depth 3 is waiting to be crawled and can be started, **Then** no page at depth 4 or deeper is started before it.
4. **Given** the home page links to `https://other-site.example/page`, **When** the crawl completes, **Then** that URL appears in the External tab, is marked "external", and no links from it were followed.
5. **Given** the start URL is `https://example.com/` and a page links to `https://www.example.com/about`, **When** default scope settings are used, **Then** the `www` URL is treated as internal and crawled.
6. **Given** the start URL is `https://example.com/` and a page links to `https://blog.example.com/`, **When** default scope settings are used, **Then** that URL is classified as external; **and when** the user turns on "crawl all subdomains" and crawls again, **Then** it is classified as internal and crawled.

---

### User Story 2 - Respect robots.txt and crawl-delay for my user agent (Priority: P1)

Before crawling any page on a host, SpiderLens reads that host's robots.txt, applies the rules that match the configured user agent, and does not crawl disallowed URLs. If the site asks for a crawl-delay, SpiderLens waits at least that long between requests to that host. Blocked URLs still appear in the results, labelled "Blocked by robots.txt" with the rule that blocked them, so the user can see which pages search engine crawlers may also be unable to reach.

**Why this priority**: Respectful crawling by default is a non-negotiable project principle, and "which pages does robots.txt block?" is one of the first questions SEO users ask. It ships together with User Story 1 as the minimum releasable crawl.

**Independent Test**: Run a crawl against a fixture host whose robots.txt disallows `/private/` for the SpiderLens user agent, allows it for others, and sets a crawl-delay. Verify no `/private/` page was requested, each one is listed as blocked with the matching rule, and the time between successive requests to that host was never shorter than the crawl-delay.

**Acceptance Scenarios**:

1. **Given** robots.txt contains a group for `SpiderLens` that disallows `/private/` and a `*` group that allows everything, **When** the user crawls with the default user agent, **Then** no URL under `/private/` is requested and each is recorded as "Blocked by robots.txt" with the line `Disallow: /private/` shown as the reason.
2. **Given** robots.txt contains only a `*` group that disallows `/tmp/`, **When** the crawl runs, **Then** URLs under `/tmp/` are blocked and all other URLs are crawled.
3. **Given** robots.txt says `Crawl-delay: 2` for the matching group, **When** the crawl runs with default settings, **Then** requests to that host are at least 2 seconds apart and only one request to that host is in flight at a time, while other hosts are not slowed down.
4. **Given** the robots.txt request returns "404 Not Found", **When** the crawl runs, **Then** all URLs on that host are treated as allowed.
5. **Given** the robots.txt request fails with a server error or cannot be reached after retries, **When** the crawl runs, **Then** URLs on that host are not crawled and are recorded as "Blocked: robots.txt unavailable", and the user can see the robots.txt error in the results.
6. **Given** the user explicitly chooses "Ignore robots.txt" (a clearly labelled option intended for sites they control), **When** the crawl runs, **Then** disallowed URLs are crawled, the results still show which URLs robots.txt would have blocked, and the crawl's saved configuration records that robots.txt was ignored.

---

### User Story 3 - Control what is in scope with rules and limits (Priority: P2)

The user narrows or widens a crawl before starting it: include and exclude patterns (for example "only `/blog/`" or "skip anything with `?sort=`"), a maximum click depth, a maximum number of URLs, whether all subdomains count as internal, whether to stay inside the start folder, and how query parameters are handled. SpiderLens applies these rules consistently to every discovered URL, including redirect targets.

**Why this priority**: Large sites, faceted navigation and calendar-style "crawl traps" make unbounded crawls impractical. Scope control is what makes a crawl of a 1M-page site finish in reasonable time and answer the user's actual question, but a basic crawl is usable without it.

**Independent Test**: Run crawls against a fixture site with an infinite calendar trap and faceted query parameters using each rule type on its own, and verify that crawled URLs and not-crawled reasons match the rule exactly.

**Acceptance Scenarios**:

1. **Given** an exclude pattern matching `\?sort=`, **When** the crawl runs, **Then** no URL containing `?sort=` is requested and each such URL is recorded as "Excluded by rule" naming the pattern.
2. **Given** an include pattern matching `/blog/`, **When** the crawl starts from the home page, **Then** the home page is still crawled (the start URL is always crawled), links are followed only to URLs matching `/blog/`, and other internal URLs are recorded as "Not matching include rules".
3. **Given** a URL matches both an include pattern and an exclude pattern, **When** it is discovered, **Then** it is excluded.
4. **Given** maximum depth is set to 2, **When** the crawl runs, **Then** pages at depth 0, 1 and 2 are crawled, links found on depth-2 pages are recorded with reason "Over depth limit", and none of them are requested.
5. **Given** maximum URLs is set to 500, **When** 500 URLs have been crawled, **Then** no further URLs are requested, every remaining discovered in-scope URL is recorded as "Over URL limit", and the crawl ends with a summary saying the URL limit was reached.
6. **Given** the query parameter setting "remove these parameters: `utm_source`, `utm_medium`", **When** pages link to `/a?utm_source=x` and `/a?utm_medium=y`, **Then** both resolve to `/a` and it is crawled once.
7. **Given** a start URL of `https://example.com/docs/` and "stay inside start folder" turned on, **When** the crawl runs, **Then** URLs outside `/docs/` are recorded as "Outside start folder" and not crawled.
8. **Given** a page redirects to a URL excluded by a rule, **When** the redirect is followed, **Then** the target is not requested and is recorded as "Excluded by rule", while the redirect itself is recorded on the source URL.

---

### User Story 4 - See why any URL was not crawled (Priority: P2)

After (or during) a crawl, the user filters results to "not crawled" and sees every discovered URL that was not fetched, each with exactly one reason: blocked by robots.txt, excluded by rule, not matching include rules, outside start folder, over depth limit, over URL limit, external, nofollow, unsupported scheme, malformed URL, or crawl stopped before it was reached. For any URL the user can also see where it was first found (the linking page, or "start URL", "list", "sitemap"). Before starting a crawl, the user can paste a URL into the scope settings and see whether it would be crawled and which rule decides it.

**Why this priority**: SpiderLens promises that nothing is silently dropped. Being able to explain a missing page is what makes users trust the crawl, and testing a rule before a long crawl saves wasted hours.

**Independent Test**: Run one crawl against a fixture site designed to produce every not-crawled reason at least once. Verify each such URL appears exactly once with the expected reason and source, and that the count of crawled plus not-crawled URLs equals the count of distinct discovered URLs.

**Acceptance Scenarios**:

1. **Given** a completed crawl, **When** the user filters by "Not crawled", **Then** every listed URL shows one reason from the defined list and the page where it was first discovered.
2. **Given** a completed crawl, **When** the user views the crawl summary, **Then** it shows the number of URLs per not-crawled reason, and crawled plus not-crawled equals all distinct discovered URLs.
3. **Given** scope settings with an exclude rule and robots.txt already fetched for the start host, **When** the user tests the URL `https://example.com/private/x` in the scope tester, **Then** SpiderLens shows "would not be crawled: blocked by robots.txt (`Disallow: /private/`)" without starting a crawl.
4. **Given** the user stops a crawl while URLs are still queued, **When** the crawl is saved without resuming, **Then** those URLs are recorded as "Not crawled: crawl stopped" and become queued again if the user resumes the crawl.
5. **Given** a crawl run from the command line, **When** it finishes, **Then** the same reasons and counts are available in its output and crawl file as in the desktop app.

---

### User Story 5 - Choose how nofollow is handled (Priority: P3)

By default SpiderLens behaves like a search engine: it records links marked `rel="nofollow"` and links on pages whose meta robots or X-Robots-Tag says `nofollow`, but does not follow them. The user can choose to follow internal nofollow links, external nofollow links, or both (for example to audit a site that overuses nofollow).

**Why this priority**: Nofollow handling changes which pages are found and is a common audit question, but the default behaviour is sufficient for most users.

**Independent Test**: Crawl a fixture where some pages are reachable only through nofollow links (link-level and page-level), once with defaults and once with "follow internal nofollow" on, and compare the crawled sets.

**Acceptance Scenarios**:

1. **Given** page X is linked only through `<a rel="nofollow">`, **When** the crawl uses default settings, **Then** X is not requested and is recorded as "Nofollow" with the linking page, and the link itself is shown with its nofollow attribute.
2. **Given** a page has meta robots `nofollow`, **When** it is crawled with default settings, **Then** none of its links are followed, and URLs reachable only from it are recorded as "Nofollow".
3. **Given** "follow internal nofollow links" is on, **When** the crawl runs, **Then** page X from scenario 1 is crawled and its record notes that it was reached through a nofollow link.
4. **Given** page X is linked once with nofollow and once without, **When** the crawl runs with default settings, **Then** X is crawled, because at least one followable link reaches it.

---

### User Story 6 - Use XML sitemaps listed in robots.txt as extra seeds (Priority: P3)

The user turns on "Use sitemaps from robots.txt". SpiderLens reads the `Sitemap:` lines in the start host's robots.txt, reads those XML sitemaps (including sitemap index files and compressed sitemaps), and adds the in-scope URLs they list to the queue, so pages that are not linked from anywhere (orphan pages) are still crawled. Each URL records whether it was found in a sitemap, through links, or both.

**Why this priority**: Finding orphan pages is valuable but optional; many crawls work well with link discovery alone. Off by default to keep the default crawl a pure link crawl.

**Independent Test**: Crawl a fixture site whose robots.txt lists a sitemap index pointing to two sitemaps, one compressed, containing one orphan page. Verify that the orphan is crawled only when the option is on and is marked as found via sitemap only.

**Acceptance Scenarios**:

1. **Given** the option is on and the sitemap lists an orphan page, **When** the crawl completes, **Then** the orphan page is crawled and marked "found in sitemap only".
2. **Given** the option is on and a sitemap lists an external URL or a URL excluded by a rule, **When** the crawl runs, **Then** that URL is recorded with the matching not-crawled reason and not requested.
3. **Given** a sitemap cannot be fetched or is not valid XML, **When** the crawl runs, **Then** the crawl continues with link discovery, and the sitemap's problem is recorded and shown in the crawl summary.
4. **Given** the option is off, **When** the crawl runs, **Then** no sitemap is requested.

---

### Edge Cases

- **Crawl traps** (infinite calendars, ever-growing query strings, repeating path segments like `/a/a/a/a/`): bounded by depth and URL limits and by the maximum URL length; URLs over the maximum length are recorded as "Malformed URL (too long)" and not crawled.
- **Non-web links** (`mailto:`, `tel:`, `javascript:`, `data:`, `ftp:`): recorded as links with reason "Unsupported scheme", never requested.
- **Unparseable hrefs** (bad characters, invalid host, empty link): recorded as "Malformed URL" with the raw text kept; the crawl continues.
- **International domain names and non-ASCII paths**: normalized to one canonical form so that the Unicode and encoded spellings of the same URL are de-duplicated.
- **Same URL found at different depths at the same time**: the shortest depth wins.
- **Start URL redirects** (for example `http://` to `https://`, or `example.com` to `www.example.com`): the redirect target's host becomes part of the internal scope, so the crawl does not end after one page.
- **Start URL is itself blocked by robots.txt or excluded by an exclude rule**: the crawl does not silently do nothing; the user is told before or at crawl start, with the reason, and the start URL is recorded with that reason.
- **robots.txt is very large, redirects, or is served as HTML**: up to a size limit (default 500 KiB) is read and parsed; robots.txt redirects are followed (up to 5 hops); content that cannot be parsed as rules is treated as "allow all" and a warning is recorded.
- **robots.txt returns 429 or 5xx temporarily**: retried using the standard retry policy; while unresolved, URLs on that host wait rather than being crawled.
- **Very long crawl-delay** (for example 60 seconds or more): honoured by default; the user is warned that the host will crawl slowly, with the resulting maximum rate per hour.
- **Conflicting rules in robots.txt** (both Allow and Disallow match): the most specific (longest) matching rule wins; on a tie, Allow wins.
- **List mode**: every listed URL is crawled at depth 0 even if it is on a different host (each listed host counts as internal for that URL), robots.txt still applies, and links found are recorded but not followed unless the user sets a crawl depth above 0.
- **Duplicate URLs in a List mode input**: crawled once; the duplicates are counted in the import summary.
- **Pause, quit, crash and resume**: the queue, the set of already-seen URLs and every recorded not-crawled reason survive; after resume no URL is crawled twice and none is lost.
- **URL limit reached exactly while requests are in flight**: in-flight requests complete and count; nothing new is started.
- **A URL first recorded as "Over depth limit" or "Nofollow" is later discovered through a valid, followable, in-scope path**: it is crawled and its not-crawled reason is removed, so each URL keeps exactly one final state.

## Requirements *(mandatory)*

### Functional Requirements

**URL normalization and de-duplication**

- **FR-001**: System MUST resolve every discovered link against its page's base URL and convert it to one normalized form: lower-case scheme and host, default ports removed, fragment removed, dot segments (`.` and `..`) resolved, an empty path written as `/`, percent-encoding written in one consistent form (unreserved characters decoded, hex digits upper-case), and international host names converted to one consistent encoding.
- **FR-002**: System MUST keep the path and query string case-sensitive and MUST NOT treat a URL with and without a trailing slash as the same URL.
- **FR-003**: System MUST crawl each normalized URL at most once per crawl (retries of transient failures excepted), no matter how many times or in how many spellings it is discovered.
- **FR-004**: System MUST keep the original link text exactly as written in the page alongside the normalized URL, for every link.
- **FR-005**: Users MUST be able to choose how query parameters are handled: keep all as written (default), remove all query strings, or remove a user-supplied list of parameter names; and separately, optionally, sort parameters alphabetically so that order differences de-duplicate.
- **FR-006**: System MUST treat URLs longer than a configurable maximum length (default 2,000 characters) as "Malformed URL (too long)" and not crawl them.

**Internal vs external classification**

- **FR-007**: System MUST classify every discovered URL as internal or external. By default a URL is internal when its host is the same as the start URL's host, with the bare domain and its `www.` variant treated as the same host; the `http` and `https` schemes are both internal.
- **FR-008**: Users MUST be able to turn on "crawl all subdomains", making every subdomain of the start URL's registered domain internal (for example `blog.example.com` and `shop.example.com` for a start URL on `example.com`).
- **FR-009**: When the start URL redirects to a different host, System MUST add the redirect target's host to the internal scope.
- **FR-010**: System MUST record external URLs with reason "External" and MUST NOT follow links found on them. Users MUST be able to choose whether external URLs get a single status check (one request each, no link following); the default is on, and these checks follow the same robots.txt, rate-limit and retry rules as internal requests.
- **FR-011**: Users MUST be able to restrict the crawl to the start URL's folder ("stay inside start folder", default off); internal URLs outside it are recorded as "Outside start folder".

**Include and exclude rules**

- **FR-012**: Users MUST be able to add any number of include patterns and exclude patterns, written as regular expressions matched against the full normalized URL.
- **FR-013**: When include patterns exist, System MUST only crawl internal URLs that match at least one; others are recorded as "Not matching include rules". The start URL and List mode URLs are always crawled regardless of include patterns.
- **FR-014**: System MUST NOT crawl any URL that matches an exclude pattern, and MUST record it as "Excluded by rule" with the pattern that matched. Exclude MUST win over include.
- **FR-015**: System MUST validate patterns before the crawl starts and refuse to start with an invalid pattern, showing which pattern is invalid and why.
- **FR-016**: Users MUST be able to enter a URL into a scope tester and see whether it would be crawled and the single rule that decides it (including robots.txt rules once robots.txt for that host has been fetched), without starting a crawl.

**Depth and URL limits**

- **FR-017**: System MUST record for every URL its crawl depth: 0 for the start URL and List mode URLs, otherwise one more than the shortest depth of any crawled page that links to it.
- **FR-018**: Users MUST be able to set a maximum crawl depth (default: no limit). URLs deeper than the limit are recorded as "Over depth limit" and not crawled.
- **FR-019**: Users MUST be able to set a maximum number of URLs to crawl (default: no limit). Once that many URLs have been requested, no new requests start, every remaining queued in-scope URL is recorded as "Over URL limit", and the crawl summary states that the limit was reached. External status checks and robots.txt and sitemap requests do not count toward this limit.

**Crawl order**

- **FR-020**: System MUST crawl in breadth-first order: no URL at depth d+1 is started while a URL at depth d or less is waiting and can be started. Within a depth, URLs are started in the order they were discovered.
- **FR-021**: The ordering rule in FR-020 MAY be relaxed only where a host's rate limit, crawl-delay or paused state prevents its next URL from starting; in that case URLs from other hosts may proceed, and retried URLs are placed at the back of the queue as defined by the project's retry policy.
- **FR-022**: Users MUST be able to pause, quit and resume a crawl with the queue, the set of seen URLs, each URL's depth and each recorded not-crawled reason preserved, so that after resuming no URL is crawled twice and no discovered URL is lost.

**robots.txt and crawl-delay**

- **FR-023**: Before requesting any URL on a host (scheme, host and port), System MUST fetch and apply that host's robots.txt once per crawl, and reuse it for every URL on that host.
- **FR-024**: System MUST apply the robots.txt group that matches the configured user agent's product token (case-insensitive), falling back to the `*` group when no specific group matches, following the published robots.txt standard (longest matching rule wins; Allow wins ties; `*` and `$` wildcards supported).
- **FR-025**: System MUST treat a robots.txt response of 4xx (other than 429) as "allow all", and a 5xx, 429 or unreachable robots.txt (after the standard retries) as "disallow all" for that host, recording the robots.txt outcome where the user can see it.
- **FR-026**: System MUST record each blocked URL with reason "Blocked by robots.txt" and the exact rule line that blocked it, or "Blocked: robots.txt unavailable" when FR-025's disallow-all applies.
- **FR-027**: System MUST honour a `Crawl-delay` value from the matching group by default: at most one request in flight to that host and at least the delay between the start of consecutive requests to it, overriding faster speed settings for that host only.
- **FR-028**: Users MUST be able to ignore robots.txt rules and, separately, ignore crawl-delay, each only through an explicit, clearly labelled option stating it is intended for sites the user controls. When either is ignored, System MUST still record which URLs robots.txt would have blocked, and the crawl's saved configuration MUST show the override.
- **FR-029**: robots.txt and sitemap requests MUST be identified with the same configured user agent as page requests.

**Nofollow**

- **FR-030**: By default System MUST NOT follow links marked `rel="nofollow"`, nor any link on a page whose meta robots or X-Robots-Tag directives contain `nofollow` or `none` for all crawlers or for the configured user agent. Such links are recorded, and a target reached only through them is recorded as "Nofollow".
- **FR-031**: Users MUST be able to choose to follow internal nofollow links, external nofollow links, or both. Links marked only `sponsored` or `ugc` are followed unless they also carry `nofollow`.
- **FR-032**: A URL reached through at least one followable link MUST be crawled even if other links to it are nofollow.

**Discovery sources and sitemaps**

- **FR-033**: System MUST add to the frontier, subject to all scope rules, URLs discovered from: anchor links, redirect targets, canonical URLs (HTML and HTTP header), hreflang alternate URLs, and (when enabled) sitemaps. Image URLs are recorded and status-checked like external URLs by default but their content is not parsed for links.
- **FR-034**: Users MUST be able to turn on "Use sitemaps from robots.txt" (default off). When on, System MUST read every `Sitemap:` URL listed in the start host's robots.txt, follow sitemap index files (up to 3 levels), read compressed sitemaps, and add listed URLs to the queue at depth 1 unless a shorter depth is found through links. *Superseded in part by [spec 011](../011-sitemaps-orphan-pages/spec.md): the setting is now two switches (read sitemaps, crawl URLs only a sitemap lists), on by default, with user-added sitemap URLs and local files; `use_sitemaps` in older profiles maps to both.*
- **FR-035**: System MUST record for every URL how it was first discovered (start URL, list, link from a named page, redirect, canonical, hreflang, or sitemap) and whether it also appeared in a sitemap.
- **FR-036**: Sitemap fetch or parse failures MUST NOT stop the crawl; each failure is recorded with its error class and shown in the crawl summary.

**Recording not-crawled URLs**

- **FR-037**: System MUST record every distinct discovered URL that is not crawled with exactly one reason from this list: Blocked by robots.txt; Blocked: robots.txt unavailable; Excluded by rule; Not matching include rules; Outside start folder; Over depth limit; Over URL limit; External; Nofollow; Unsupported scheme; Malformed URL; Crawl stopped.
- **FR-038**: When a URL qualifies for more than one reason, System MUST record the first that applies in this order: Malformed URL, Unsupported scheme, External, Outside start folder, Excluded by rule, Not matching include rules, Over depth limit, Nofollow, Blocked by robots.txt / robots.txt unavailable, Over URL limit, Crawl stopped.
- **FR-039**: If a URL already recorded as not crawled is later reached through a path that makes it eligible (for example a shorter depth or a followable link), System MUST crawl it and replace its not-crawled reason, so each URL ends with exactly one final state.
- **FR-040**: Users MUST be able to filter results by "Not crawled" and by each reason, and the crawl summary MUST show counts per reason, with crawled plus not-crawled equal to the number of distinct discovered URLs.

**Mode, configuration and parity**

- **FR-041**: In List mode, System MUST crawl every supplied URL at depth 0, apply robots.txt and exclude rules to them, and follow links from them only up to a user-set depth (default 0, meaning links are recorded but not followed).
- **FR-042**: Every scope, limit, robots.txt, nofollow and sitemap setting in this spec MUST be available from the command-line crawler with the same defaults as the desktop app, and MUST be stored with the crawl so a reopened crawl shows the exact settings that produced it.
- **FR-043**: The frontier MUST keep its queue and seen-URL set on disk rather than fully in memory, so memory use does not grow in proportion to the number of discovered URLs.

### Key Entities

- **Discovered URL**: A distinct normalized URL found during a crawl. Attributes: normalized URL, internal/external classification, crawl depth, first discovery source and the page it was found on, whether it appeared in a sitemap, and final state (crawled, or not crawled with one reason and its detail such as the matching rule).
- **Link**: One occurrence of a reference from a page to a URL. Attributes: source page, target discovered URL, original text as written, anchor text, rel attributes (nofollow, sponsored, ugc), whether it was followed, and the link type (anchor, redirect, canonical, hreflang, image).
- **Scope Configuration**: The user's settings for a crawl: crawl mode, start URL or list, subdomain handling, start-folder restriction, include and exclude patterns, maximum depth, maximum URLs, maximum URL length, query parameter handling, nofollow handling, sitemap option, external status checks, robots.txt and crawl-delay overrides, user agent. Saved with the crawl.
- **Host Robots Policy**: The outcome of fetching one host's robots.txt for the configured user agent: fetch result (ok, not found, unavailable), the matched group, its allow/disallow rules, crawl-delay, and listed sitemap URLs.
- **Sitemap Source**: One sitemap or sitemap index read during the crawl: its URL, parent index (if any), number of URLs listed, and any error.
- **Frontier Queue**: The ordered set of URLs waiting to be crawled, by depth then discovery order, which survives pause, quit and crash.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: On the reference fixture site with duplicate link spellings, 100% of internal pages are fetched exactly once and 0 pages are fetched twice (retries excluded).
- **SC-002**: On a fixture site with a known link graph, the recorded depth of 100% of pages equals their shortest click distance from the start URL.
- **SC-003**: On the robots.txt fixture, 0 requests are made to disallowed URLs under default settings, and 100% of the gaps between consecutive requests to a host with a crawl-delay are at least that delay.
- **SC-004**: For every completed or stopped crawl, crawled URLs plus not-crawled URLs equals the number of distinct discovered URLs, and 100% of not-crawled URLs carry exactly one reason from the defined list.
- **SC-005**: On a fixture designed to produce each not-crawled reason, a user can find the reason for any missing page in under 30 seconds using the Not crawled filter or the scope tester.
- **SC-006**: On the local 100,000-page fixture site, scope decisions and de-duplication do not lower throughput below the project target of 500 URLs per second, and memory use stays flat (within 10%) between 10,000 and 100,000 discovered URLs.
- **SC-007**: A crawl that discovers 1,000,000 distinct URLs completes de-duplication and scope classification without memory growing in proportion to the number of URLs.
- **SC-008**: After pausing or force-quitting and resuming a crawl of the 100,000-page fixture, 0 URLs are crawled twice and 0 discovered URLs are missing from the final results.
- **SC-009**: Running the same crawl with the same settings from the desktop app and the command line produces the same set of crawled URLs and the same not-crawled reasons on the fixture sites.
- **SC-010**: With the URL limit set to N, the number of URLs requested for crawling never exceeds N plus the number already in flight when the limit was reached.

## Assumptions

- Defaults, chosen to match common SEO crawler behaviour and the project's "respectful crawling" principle: robots.txt honoured; crawl-delay honoured; nofollow not followed; sitemaps off; external status checks on; no depth or URL limit; query parameters kept as written; same host (with `www.` and bare domain treated as one) counts as internal; start-folder restriction off; maximum URL length 2,000 characters; robots.txt read up to 500 KiB.
- The configured user agent and its robots.txt product token are set in the crawl configuration (default product token `SpiderLens`); this spec only defines how robots.txt is applied for it.
- Network fetching, retries, error classes, adaptive concurrency and per-host rate limits come from the crawl engine feature and the project-wide retry policy; this spec uses them but does not redefine them. Crawl-delay overrides faster per-host settings.
- Page parsing (finding links, rel attributes, meta robots, X-Robots-Tag, canonical and hreflang) comes from the extraction feature; this spec decides what to do with those discoveries.
- Saving, reopening, pausing and resuming crawl files come from the storage feature; this spec requires that frontier state is part of what is saved.
- The results table, tabs and filters come from the results UI feature; this spec adds the "Not crawled" filter, per-reason counts and the scope tester.
- Regular expressions are the pattern syntax for include/exclude rules, as is common in desktop SEO crawlers; simpler wildcard syntax may be added later.
- Supplying a URL in List mode does not override robots.txt: a listed URL that robots.txt disallows is recorded as blocked unless the user has explicitly chosen to ignore robots.txt.
- **Out of scope for this feature**: JavaScript-rendered link discovery (M4); sitemap discovery from locations other than robots.txt, uploading sitemap files, and sitemap audits such as "in sitemap but not crawled" reports (M2); URL rewriting rules beyond query parameter handling (M3); Compare and SERP crawl modes (M3 or later); crawling of resources such as scripts and stylesheets (M4); authentication and form-based login for crawling (later milestone); AEO/GEO and AI-agent-driven scope (M5/M6); custom per-host crawl schedules.
