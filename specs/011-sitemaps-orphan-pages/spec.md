# Feature Specification: Sitemaps and Orphan Pages

**Feature Branch**: `011-sitemaps-orphan-pages`

**Created**: 2026-10-13

**Status**: Approved 2026-10-13 (Ryware), recommended defaults for all open decisions

**Input**: Product backlog entry 011, started on the owner's approval of 2026-10-13 to open specs for every feature gap found in the comparison with Screaming Frog (gap report rows 3 and 4). Three parts: an XML sitemap audit (URLs in sitemaps compared with what was crawled, non-indexable URLs in sitemaps, errors in the sitemap files themselves, `lastmod` sanity), orphan pages (URLs known from a sitemap or another source but not linked from any crawled page), and XML sitemap generation from a crawl with options (images, hreflang, `lastmod`, exclusions, splitting at the 50,000 URL limit).

## Overview

An XML sitemap is the list of URLs a site asks search engines to index. It is only useful when it agrees with the site: every URL in it should return 200, be indexable and be its own canonical, and every page the site wants indexed should be in it. When the two disagree, search engines waste crawl budget on redirects and errors, ignore the sitemap's hints, or never find pages that nothing links to.

SpiderLens already reads sitemaps listed in robots.txt and uses their URLs as extra seeds (spec 003, User Story 6), and already records for every URL whether it appeared in a sitemap. It does not yet tell the user anything about the sitemaps. This feature adds:

1. **Sitemap audit**: every sitemap file read is shown with its problems, every entry in it is kept with its `lastmod`, image and hreflang data, and rules compare the entries with the crawl.
2. **Orphan pages**: internal URLs that SpiderLens knows about from a source other than links (sitemaps now, Search Console and analytics later in spec 024) but that no crawled page links to.
3. **Sitemap generation**: the user builds a valid XML sitemap set (with an index file when it has to be split) from a crawl, choosing which pages go in and what extra data each entry carries.

All three work from data already stored in the crawl file. Nothing in this feature runs in the page fetch path, so crawl speed is unchanged when the feature's options are off, and changes only by the extra sitemap and orphan URLs fetched when they are on.

The audit checks are rules in the issue rules engine catalogue (spec 009) under a new "Sitemaps" category, with the same rule shape, priorities, settings and command-line behaviour as every other rule.

This feature belongs to milestone M2. It builds on frontier and scope (003), fetching and errors (004), page extraction (005: indexability, canonicals, hreflang, images), storage (006), the results table (007) and the issue rules engine (009). Its orphan list feeds Link Score (023), and spec 024 adds Search Console and analytics as orphan sources.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - See what is wrong with the URLs in my sitemaps (Priority: P1)

An SEO consultant crawls a client site with sitemaps on. When the crawl ends, the Issues area has a "Sitemaps" category listing, with counts: URLs in sitemaps that redirect, URLs in sitemaps that return 4xx or 5xx, URLs in sitemaps that are noindex, canonicalised to another URL or blocked by robots.txt, and indexable pages that are in no sitemap. Each issue explains why it matters and how to fix it and lists the affected URLs with the sitemap file each one came from. The consultant exports the list and sends it to the developer.

**Why this priority**: This is the most common sitemap question, and every answer needs both the sitemap and the crawl, which only a crawler has.

**Independent Test**: Crawl a fixture site whose sitemap lists one 200 indexable page, one 301, one 404, one noindex page, one page canonicalised elsewhere and one URL disallowed by robots.txt, and which has one indexable linked page missing from the sitemap. Verify that each of the six problem URLs appears in exactly the matching issue, with its sitemap file, and that the clean page appears in none.

**Acceptance Scenarios**:

1. **Given** a sitemap lists a URL that redirects, **When** the crawl completes, **Then** the URL appears in "Redirect in sitemap" with its final target and the sitemap that listed it.
2. **Given** a sitemap lists a URL that returns 404 or 500, **When** the crawl completes, **Then** it appears in "Client error in sitemap" or "Server error in sitemap".
3. **Given** a sitemap lists a page with a noindex directive (meta or header), **When** the crawl completes, **Then** it appears in "Noindex URL in sitemap".
4. **Given** a sitemap lists a page whose canonical points to a different URL, **When** the crawl completes, **Then** it appears in "Canonicalised URL in sitemap" with the canonical target.
5. **Given** a sitemap lists a URL disallowed by robots.txt for the configured user agent, **When** the crawl completes, **Then** it appears in "Blocked URL in sitemap" and is not requested.
6. **Given** an internal indexable page returning 200 is linked but not listed in any sitemap, **When** the crawl completes, **Then** it appears in "Indexable page not in sitemap".
7. **Given** a sitemap lists a URL that was not crawled because of an include or exclude rule or a crawl limit, **When** the crawl completes, **Then** it appears in "Sitemap URL not crawled" with the not-crawled reason from spec 003, and is not counted in any of the response or indexability issues above.
8. **Given** the same crawl, **When** the user runs the command-line `issues` command, **Then** the same sitemap issues and URL counts are reported.

---

### User Story 2 - Find orphan pages (Priority: P1)

The consultant wants pages that exist and are listed in the sitemap but that no page on the site links to, because search engines and users struggle to reach them. After the crawl, an "Orphan pages" view lists every internal URL found in a sitemap (or another non-link source) that has no followed internal link from any crawled page, with its status code, indexability and the source that revealed it. Pages linked only through nofollow links are listed separately, since they are reachable but pass no link value.

**Why this priority**: Orphan pages are one of the most valued outputs of a sitemap-aware crawl and are named in the gap report as P1. They also feed Link Score (023).

**Independent Test**: Crawl a fixture with a sitemap listing pages A (linked from home), B (linked from nowhere), C (linked only with rel="nofollow") and D (reached only through a redirect from a linked URL). Verify the orphan view lists B as an orphan, C as "linked only through nofollow", and neither A nor D.

**Acceptance Scenarios**:

1. **Given** a sitemap URL that no crawled page links to, **When** the crawl completes, **Then** it is listed as an orphan page with source "sitemap" and the sitemap file named.
2. **Given** a sitemap URL linked from at least one crawled internal page with a followed link, **When** the crawl completes, **Then** it is not an orphan.
3. **Given** a sitemap URL whose only internal links are nofollow, **When** the crawl completes, **Then** it is listed under "Linked only through nofollow", not as an orphan.
4. **Given** a sitemap URL that is the target of a redirect from a linked URL, **When** the crawl completes, **Then** it is not an orphan, because the redirect makes it reachable.
5. **Given** a page whose only internal link comes from itself, **When** the crawl completes, **Then** it is an orphan.
6. **Given** a crawl that was stopped, paused, or hit a URL or depth limit, **When** the user views orphans, **Then** the view says the list may include pages that are linked from pages that were not crawled, and shows how many internal pages were not crawled.
7. **Given** the "Crawl URLs found only in sitemaps" option is on, **When** orphans are found, **Then** each one has been requested and has its status code, indexability and page data like any crawled page; **Given** the option is off, **Then** each orphan is listed with "not crawled: found only in sitemap" and no status.
8. **Given** the crawl was run in List mode, **When** the user opens the orphan view, **Then** it explains that orphans need a Spider mode crawl, because a list crawl does not follow links.

---

### User Story 3 - Check the sitemap files themselves (Priority: P1)

The user opens a "Sitemaps" view that lists every sitemap and sitemap index SpiderLens read, as a tree (index files with their children), each with: where it was found (robots.txt, the default location, added by the user), HTTP status, compressed or not, size, number of entries, number of entries that are fine, and problems. Problems include: could not be fetched, not valid XML, not a sitemap, larger than 50 MB uncompressed, more than 50,000 URLs, index files nested too deep, entries with invalid or relative URLs, entries on another host or outside the sitemap's folder (which search engines ignore), the same URL listed twice, and an empty sitemap.

**Why this priority**: A broken sitemap file makes all its URLs invisible to search engines; today SpiderLens only shows sitemap fetch errors in the crawl summary.

**Independent Test**: Crawl a fixture whose robots.txt lists a valid index, a sitemap with 50,001 URLs, a file that is HTML, a sitemap with a `<loc>` on another host, one with a relative `<loc>`, one listing a URL twice, and one that returns 404. Verify each file shows exactly its expected problems and the valid ones show none.

**Acceptance Scenarios**:

1. **Given** a sitemap returns 404 or times out, **When** the crawl completes, **Then** it appears in the Sitemaps view with the HTTP status or error class (spec 004) and in "Sitemap could not be fetched"; the crawl is not interrupted.
2. **Given** a sitemap that is not valid XML or whose root is neither `urlset` nor `sitemapindex`, **When** the crawl completes, **Then** it appears in "Sitemap is not valid", with the parser's message and the line where reading failed.
3. **Given** a sitemap with more than 50,000 URLs or more than 50 MB uncompressed, **When** the crawl completes, **Then** it appears in "Sitemap over size limit", and SpiderLens still reads the entries up to the limit.
4. **Given** an entry whose URL is relative, malformed or not http or https, **When** the crawl completes, **Then** it appears in "Invalid URL in sitemap" with the raw text of the entry.
5. **Given** an entry on a different host than its sitemap, or outside the sitemap's folder, and not allowed by a robots.txt `Sitemap:` line on that host, **When** the crawl completes, **Then** it appears in "Sitemap URL outside the sitemap's location".
6. **Given** the same URL listed twice in one sitemap, or in two sitemaps, **When** the crawl completes, **Then** it appears in "URL listed in more than one sitemap entry" with each place it was listed.
7. **Given** a sitemap index nested more than 3 levels deep, or an index listing itself, **When** the crawl completes, **Then** the deeper level or the loop is reported once, and the crawl does not read it again.
8. **Given** a robots.txt with no `Sitemap:` lines, **When** sitemaps are on, **Then** SpiderLens tries `/sitemap.xml` and `/sitemap_index.xml` at the start host, uses whichever exists, and reports "No sitemap listed in robots.txt" as an Opportunity.

---

### User Story 4 - Check `lastmod` values (Priority: P2)

The user wants to know whether search engines can trust the sitemap's `lastmod` dates. Issues flag: `lastmod` values that are not valid dates, dates in the future, sitemaps where every entry has the same `lastmod` (usually the generation time, not the real change time), and entries whose `lastmod` is much older than the page's own `Last-Modified` header.

**Why this priority**: Google uses `lastmod` only when it is consistently accurate; misleading dates are a common, cheap fix, but less urgent than broken URLs.

**Independent Test**: Crawl a fixture sitemap with entries dated `2026-13-40`, one day after the crawl, and a second sitemap of 200 entries all with the same timestamp; verify each lands in its issue and correctly dated entries do not.

**Acceptance Scenarios**:

1. **Given** an entry whose `lastmod` is not a W3C datetime, **When** the crawl completes, **Then** it appears in "Invalid lastmod" with the raw value.
2. **Given** an entry whose `lastmod` is more than 1 day after the crawl started, **When** the crawl completes, **Then** it appears in "Lastmod in the future".
3. **Given** a sitemap with at least 100 entries that all share one `lastmod`, **When** the crawl completes, **Then** the sitemap appears in "All lastmod values identical".
4. **Given** an entry whose page returned a `Last-Modified` header more than 30 days later than the entry's `lastmod`, **When** the crawl completes, **Then** it appears in "Lastmod older than the page" with both dates.
5. **Given** an entry with no `lastmod`, **When** the crawl completes, **Then** it is in no `lastmod` issue except the site-wide count "Entries without lastmod" (Opportunity).

---

### User Story 5 - Generate an XML sitemap from a crawl (Priority: P2)

The user opens a completed crawl and chooses "Generate XML sitemap". A sheet shows the pages that will be included, with a live count, and options: which pages to include (by default internal HTML pages that return 200, are indexable and are their own canonical, or have no canonical), whether to also include noindex or canonicalised pages, include and exclude patterns, `lastmod` (none, the page's `Last-Modified` header, the crawl date, or a fixed date), `changefreq` and `priority` (off by default), images (as image sitemap entries in the same file, or as a separate image sitemap), hreflang alternates, the maximum URLs per file (50,000, lower if the user wants) and compression. The user picks a folder and the base URL where the files will live; SpiderLens writes the sitemap files and, when there is more than one, a sitemap index.

**Why this priority**: Many small and medium sites have no reliable sitemap; generating one from the crawl is a quick win, but auditing comes first.

**Independent Test**: Generate a sitemap set from a fixture crawl with 120,000 indexable pages, 10 noindex pages, 10 redirects and images with and without alt text, with images and hreflang on. Verify three files of at most 50,000 URLs plus an index, that every file validates against the sitemap protocol schemas, that no noindex, redirect or error URL is in them, and that reading the files back as sitemaps produces exactly the chosen pages.

**Acceptance Scenarios**:

1. **Given** default options, **When** the user generates a sitemap, **Then** the files contain exactly the internal HTML pages that returned 200, are indexable and are self-canonical or have no canonical, and no other URL.
2. **Given** more pages than the URL limit per file, or a file that would exceed 50 MB, **When** the user generates, **Then** the pages are split across numbered files and a sitemap index lists them by the base URL the user entered.
3. **Given** images are on, **When** the user generates, **Then** each page's internal images that returned 200 are listed with it, using the image sitemap extension, up to 1,000 per page; images on other hosts are included only when the user turns on "Include images on other hosts".
4. **Given** hreflang is on, **When** the user generates, **Then** each page lists its hreflang alternates (including itself and x-default) using the `xhtml:link` extension, and only alternates that are themselves included pages are written.
5. **Given** `lastmod` is set to "Last-Modified header", **When** a page has no such header, **Then** that entry has no `lastmod` rather than a made-up date.
6. **Given** an exclude pattern, **When** the user generates, **Then** matching pages are left out, and the sheet's count updates before the files are written.
7. **Given** the chosen folder already contains files with the generated names, **When** the user generates, **Then** SpiderLens asks before replacing them.
8. **Given** the same crawl and options, **When** the user runs the command-line `sitemap generate` command, **Then** byte-identical files are written.
9. **Given** the crawl is still running, **When** the user opens "Generate XML sitemap", **Then** it is unavailable with the reason "Wait for the crawl to finish or stop it".

---

### User Story 6 - Choose where sitemaps come from (Priority: P2)

Not every site lists its sitemap in robots.txt. In the crawl settings the user can add sitemap URLs by hand, and choose a local sitemap file (for example one a developer has not published yet) to compare with the live site. The "Use sitemaps" option from spec 003 is split into "Read sitemaps" (the audit) and "Crawl URLs found only in sitemaps" (the seeds), so the user can audit a sitemap without crawling URLs nothing links to.

**Why this priority**: Widens the audit to the sitemaps users actually care about; the robots.txt and default-location sources from User Story 3 cover most sites.

**Independent Test**: Crawl a fixture with no sitemap in robots.txt, adding one sitemap URL by hand and one local file; verify both are read, labelled with their source, and audited, and that with "Crawl URLs found only in sitemaps" off, orphan URLs are listed but not requested.

**Acceptance Scenarios**:

1. **Given** the user added a sitemap URL, **When** the crawl runs, **Then** it is read with the same user agent, size limits and index depth as robots.txt sitemaps, and labelled "added by you".
2. **Given** the user chose a local sitemap file, **When** the crawl runs, **Then** it is read from disk without any request, labelled with its file name, and its entries are checked against the crawl exactly like a fetched sitemap; the location rule (Story 3, scenario 5) is checked against the base URL the user gives for the file.
3. **Given** "Read sitemaps" is on and "Crawl URLs found only in sitemaps" is off, **When** the crawl runs, **Then** sitemap URLs that are also linked are crawled normally, sitemap-only URLs are recorded as "not crawled: found only in sitemap", and the sitemap issues that need a response say "not checked" for them.
4. **Given** a crawl file saved before this feature, **When** it is opened, **Then** the sitemaps it read are shown with the data the older file has, and issues that need data the file does not have say "not available in this crawl file" instead of showing zero.
5. **Given** the crawl is in List mode and the user pastes a sitemap URL as the list source, **When** the crawl runs, **Then** the URLs in that sitemap (and its children, if it is an index) become the list.

---

### Edge Cases

- A sitemap URL redirects: SpiderLens follows the redirect (as for pages, spec 004), reads the target, and reports "Sitemap redirects" (Low), since search engines may not follow it.
- A sitemap served with an HTML content type, or gzip-compressed without a `.gz` name: content is detected from the bytes, not the name or header (as in spec 003 today); a mismatched content type is not an issue by itself.
- Sitemaps with namespaces other than the standard ones (news, video): their page URLs and `lastmod` are read and audited; their news and video fields are kept unread and are not checked.
- An entry URL that differs from the crawled URL only by normalization (trailing slash on the host, default port, case of the host, `#fragment`): it is matched after the same normalization as spec 003, and the raw text is kept to show it. An entry with `http` while the site serves `https` (or `www` vs bare host) is not normalized away: it is matched to the URL actually listed and usually ends up in "Redirect in sitemap".
- A sitemap with millions of entries across many files: entries are read and stored as they stream in, memory use stays flat (constitution VI), and the crawl's URL limit (spec 003, FR-019) still applies to crawling them; entries above the limit are recorded and appear in "Sitemap URL not crawled" with reason "Over URL limit".
- A `<loc>` with surrounding whitespace or XML entities: whitespace is trimmed and entities decoded before matching, and "Invalid URL in sitemap" is raised only if the result is still not a valid absolute URL.
- The same page listed with and without a query string: they are different URLs and are audited separately.
- hreflang in a sitemap that disagrees with the page's own hreflang: both are kept; the sitemap's alternates are handed to the hreflang rules of spec 009 as an extra source, and a mismatch is reported by those rules, not by a separate sitemap rule.
- An image listed in an image sitemap entry: image URLs are recorded and status-checked like page images (spec 003, FR-033); broken ones appear in "Broken image in sitemap".
- A crawl stopped before sitemaps finished loading: the Sitemaps view shows which files were not read, and orphan and "not in sitemap" results say they are incomplete.
- Generating from a crawl where a page appears through two URLs that canonicalise to each other: only the canonical one is included.
- Generating with a base URL on a different host than the pages: SpiderLens warns that search engines ignore sitemaps for URLs outside the sitemap's host unless robots.txt on that host lists it, and lets the user continue.
- Generating when no page matches the options: no file is written and the sheet says why.

## Requirements *(mandatory)*

### Functional Requirements

**Sitemap sources**

- **FR-001**: The "Use sitemaps from robots.txt" setting of spec 003 (FR-034) MUST be replaced by two settings: "Read sitemaps" and "Crawl URLs found only in sitemaps". A crawl profile or crawl file that had the old setting on MUST load with both new settings on; one with it off MUST load with both off.
- **FR-002**: When "Read sitemaps" is on, System MUST read sitemaps from: the `Sitemap:` lines in robots.txt of the start host; when that lists none, `/sitemap.xml` and then `/sitemap_index.xml` on the start host; sitemap URLs the user added; and local sitemap files the user chose, with the base URL the user gives for each file.
- **FR-003**: Every sitemap read MUST record: its URL or file name, its source (robots.txt, default location, added by the user, local file, list source), the index that listed it, HTTP status or error class (spec 004), final URL after redirects, whether it was compressed, size in bytes, entry count, and the time it was read.
- **FR-004**: Reading sitemaps MUST keep the existing limits of spec 003 (50 MB uncompressed per file, index files followed 3 levels deep, same user agent as page requests, failures never stop the crawl) and MUST NOT read the same sitemap URL twice in one crawl.
- **FR-005**: In List mode, a sitemap URL or local sitemap file MUST be usable as the list source, with every page URL it (and its children) lists becoming a list entry.

**Sitemap entries**

- **FR-006**: System MUST store every entry of every sitemap read, with: the sitemap it came from, the raw `<loc>` text, the normalized URL (spec 003 normalization) or the reason it could not be normalized, `lastmod` as written and as a parsed date when valid, `changefreq` and `priority` as written, image URLs listed with it, and hreflang alternates (`xhtml:link`) listed with it.
- **FR-007**: Sitemap entries MUST be stored as they are read, in batches, without holding a whole sitemap in memory, and MUST be linked to the crawled URL they match so every crawled URL can show the sitemaps that list it.
- **FR-008**: Image URLs in sitemap entries MUST be recorded and status-checked like page images (spec 003, FR-033) when external or image checks are on; hreflang alternates in sitemaps MUST be added as discovery sources under the normal scope rules and handed to the hreflang rules of spec 009 as a source named "sitemap".
- **FR-009**: When "Crawl URLs found only in sitemaps" is on, internal in-scope URLs that appear only in sitemaps MUST be queued as in spec 003 (FR-034); when it is off, they MUST be recorded with the new not-crawled reason "Found only in sitemap" and not requested.

**Orphan pages**

- **FR-010**: An internal URL MUST be classed as an **orphan page** when it is known from at least one non-link source (sitemaps in this feature; spec 024 adds Search Console and analytics) and no crawled internal HTML page other than itself links to it with a followed anchor or area link, and it is not the target of a redirect, meta refresh or JavaScript redirect (spec 014) from a URL that is linked.
- **FR-011**: An internal URL from a non-link source whose only internal links are nofollow MUST be classed as **linked only through nofollow**, separately from orphans. Links from canonical and hreflang annotations MUST NOT count as links for this purpose.
- **FR-012**: Orphan classification MUST be computed from stored data after the crawl ends, is stopped, or a file is opened; during a crawl the orphan count MAY be shown and MUST be labelled provisional.
- **FR-013**: When the crawl did not reach every internal URL (stopped, paused, URL or depth limit), the orphan view and the orphan issue MUST state that orphans may be linked from pages that were not crawled, with the number of such pages.
- **FR-014**: Each orphan MUST show its sources (the sitemaps that list it), status code and indexability when crawled, and depth "none". The orphan list MUST be available to other features (Link Score 023, report 010, agents 016) through the same results data as the views.
- **FR-015**: Orphan pages MUST NOT be computed for List mode crawls; the view MUST say why.

**Sitemap audit rules**

- **FR-016**: The rules below MUST be added to the spec 009 rule catalogue in a new "Sitemaps" category, with the rule shape, settings (on or off, priority, threshold), per-page and site-wide kinds and command-line behaviour defined there.
- **FR-017**: Rules that compare entries with the crawl MUST treat URLs that were not fetched (not crawled, out of scope, sitemap-only with crawling off) as "not checked", never as broken, following spec 009 FR-011.
- **FR-018**: Each affected URL in a sitemap rule MUST carry, as evidence, the sitemap file or files that list it, and, where relevant, the redirect target, canonical target, status code, or `lastmod` value that triggered the rule.

**Sitemap generation**

- **FR-019**: Users MUST be able to generate XML sitemaps from a completed or stopped crawl, in the desktop app and from the command line with the same options and output.
- **FR-020**: By default, generation MUST include exactly the internal HTML pages that returned 200, are indexable (no noindex from meta or header, not blocked by robots.txt) and whose canonical is themselves or absent. Options MUST allow also including noindex pages and canonicalised pages, and MUST allow include and exclude patterns using the pattern syntax of spec 002.
- **FR-021**: Options MUST include: `lastmod` source (none, `Last-Modified` header, crawl date, fixed date; default `Last-Modified` header); `changefreq` (off by default, or one value for all); `priority` (off by default, one value for all, or by crawl depth); images (off by default; inline in the page sitemap or a separate image sitemap; internal hosts only unless the user adds other hosts; at most 1,000 images per page as the image sitemap extension allows); hreflang alternates (off by default; only alternates that are themselves included pages); maximum URLs per file (50,000 default and maximum); gzip compression (off by default); output folder; base URL for the files.
- **FR-022**: Generation MUST split output so no file has more than the URL limit or more than 50 MB uncompressed, MUST write a sitemap index when more than one file is produced, and MUST order entries deterministically (by URL) so the same crawl and options always produce byte-identical files.
- **FR-023**: Every generated file MUST be valid against the sitemap protocol schemas, and the image and hreflang extensions where used; URLs MUST be written escaped as the protocol requires.
- **FR-024**: Before writing, the sheet MUST show how many pages, images and files will be produced, update that count as options change, and ask before replacing existing files.
- **FR-025**: Generation options last used MUST be remembered per crawl profile, and the options used MUST be shown when the files are written so the user can repeat them from the command line.

**Desktop app**

- **FR-026**: The results table (spec 007) MUST gain a "Sitemaps" tab with filters: All sitemap URLs, In sitemap and indexable, In sitemap and not indexable, Not in a sitemap (indexable pages), Orphan pages, Linked only through nofollow, Not crawled; a "Sitemaps" column on URL rows showing how many sitemaps list the URL; and an inspector section listing the sitemaps that list the selected URL with each entry's `lastmod`.
- **FR-027**: A "Sitemap files" view MUST show the sitemaps read as a tree (indexes with their children) with the data of FR-003 and each file's problems, and selecting a file MUST filter the Sitemaps tab to its entries.
- **FR-028**: "Generate XML sitemap" MUST be reachable from the command palette and the crawl's menu, and MUST open as a sheet in the design of spec 017. It MUST be unavailable, with a reason, while a crawl is running.
- **FR-029**: All new screens MUST follow spec 017 and 018 (tokens only, every palette and brightness step, keyboard reachable) and MUST have UI tests in the project's UI test suite.

**Command line**

- **FR-030**: The command line MUST offer `sitemaps <crawl file>` (the sitemap files and their problems, plain or JSON), `orphans <crawl file>` (the orphan list, plain, CSV or JSON), and `sitemap generate <crawl file>` with every option of FR-021; sitemap rules MUST be part of the `issues` command of spec 009.
- **FR-031**: Every setting in this spec MUST be stored with the crawl (constitution V) and available from the command-line crawler with the same defaults as the desktop app.

**Performance and data**

- **FR-032**: Audit rules, orphan classification and generation MUST run from the stored crawl data only, MUST NOT fetch anything, and MUST NOT run in the page fetch path.
- **FR-033**: New stored data MUST be added as additive, nullable schema steps through the project's migration mechanism (spec 006), so older files open and older apps can view newer files read-only.

### First rule set (Sitemaps category)

Priorities as in spec 009: H = High, M = Medium, L = Low, O = Opportunity. "Sitemap URL" means an entry URL that is internal and was fetched unless stated otherwise.

| Rule | Applies to | Default | Threshold |
|---|---|---|---|
| Client error in sitemap (4xx) | sitemap URLs | H | |
| Server error or no response in sitemap | sitemap URLs | H | |
| Redirect in sitemap | sitemap URLs | M | |
| Noindex URL in sitemap | sitemap URLs | H | |
| Canonicalised URL in sitemap | sitemap URLs | M | |
| Blocked URL in sitemap (robots.txt) | sitemap URLs | M | |
| Sitemap URL not crawled (with reason) | sitemap URLs not fetched | L | |
| Indexable page not in sitemap | pages | M | |
| Orphan page | internal URLs from non-link sources | H | |
| Linked only through nofollow | internal URLs from non-link sources | M | |
| URL listed in more than one sitemap entry | sitemap entries | L | |
| Broken image in sitemap | image URLs in sitemap entries | M | |
| Invalid URL in sitemap | sitemap entries | M | |
| Sitemap URL outside the sitemap's location | sitemap entries | M | |
| Invalid lastmod | sitemap entries | L | |
| Lastmod in the future | sitemap entries | L | more than 1 day after crawl start |
| Lastmod older than the page | sitemap entries with a `Last-Modified` header | O | 30 days |
| All lastmod values identical | sitemaps | L | 100 entries or more |
| Entries without lastmod | sitemap entries (site-wide) | O | |
| Sitemap could not be fetched | sitemaps | H | |
| Sitemap is not valid | sitemaps | H | |
| Sitemap over size limit | sitemaps | H | 50,000 URLs or 50 MB |
| Sitemap redirects | sitemaps | L | |
| Empty sitemap | sitemaps | M | |
| Sitemap index nested too deep or looping | sitemaps | M | 3 levels |
| No sitemap listed in robots.txt | site-wide | O | |
| No sitemap found | site-wide | M | |

### Key Entities *(include if feature involves data)*

- **Sitemap Source** (extends spec 003): one sitemap or sitemap index read during a crawl: location, source, parent index, HTTP status or error class, final URL, compressed, size, entry count, read time, problems.
- **Sitemap Entry**: one `<url>` or `<sitemap>` element: its sitemap, raw location text, normalized URL or invalid reason, `lastmod` (raw and parsed), `changefreq`, `priority`, image URLs, hreflang alternates, and the crawled URL it matches.
- **Orphan Classification**: per internal URL from a non-link source: orphan, linked only through nofollow, or linked; its sources; whether the result is final or provisional, and the crawl's completeness note.
- **Sitemap Generation Options**: the options of FR-021 plus include and exclude patterns, saved per crawl profile and printed with the output.
- **Generated Sitemap Set**: the files written: page sitemaps, optional image sitemaps, optional index, each with its URL count and size.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: On the fixture sites for User Stories 1 to 4, every seeded problem appears in exactly its expected rule and no clean URL or file appears in any rule (zero false positives, zero misses).
- **SC-002**: With "Read sitemaps" off, crawl throughput on the spec 008 benchmark is unchanged (within run-to-run noise); with it on and sitemaps listing only URLs that are also linked, throughput drops by no more than 2%.
- **SC-003**: Reading and storing 1,000,000 sitemap entries across 20 files on the reference machine adds no more than 30 seconds to a crawl and no more than 50 MB to peak memory.
- **SC-004**: Sitemap rules and orphan classification for a crawl of 1,000,000 URLs with 1,000,000 sitemap entries finish within 20 seconds of the crawl ending on the reference machine, and partial results show within 2 seconds.
- **SC-005**: Generating a sitemap set for 1,000,000 pages with images and hreflang takes no more than 60 seconds, and 100% of generated files validate against the protocol schemas.
- **SC-006**: A sitemap set generated from a crawl, read back by SpiderLens as sitemaps, lists exactly the pages selected by the options, and running the same generation twice produces byte-identical files.
- **SC-007**: Every acceptance scenario maps to at least one automated test against local fixture sites, and every new screen has UI tests in all palettes and brightness steps, passing in CI on Windows, macOS and Linux.

## Assumptions

- Defaults, with the reasoning:
  - **"Read sitemaps" is on by default** for new crawl profiles (spec 003 had sitemaps off). Reading a few sitemap files is a small, polite cost and every audit in this spec needs it. "Crawl URLs found only in sitemaps" is also on by default, so orphans get a status code. Existing profiles and files keep their old value (FR-001). This is the one default the owner may want to reverse.
  - Default sitemap fallback locations are `/sitemap.xml` then `/sitemap_index.xml`, tried only when robots.txt lists none. Other common names (for example a CMS's own path) can be added by hand.
  - Generated sitemaps leave out `changefreq` and `priority` by default, because Google ignores both; `lastmod` defaults to the `Last-Modified` header and is omitted where the header is missing, never invented.
  - Orphan detection counts anchor and area links only; canonicals and hreflang are annotations, not links a user or crawler follows for navigation. Redirect and meta refresh targets of linked URLs count as reachable.
  - Thresholds: future `lastmod` more than 1 day after crawl start (time zones), "older than the page" at 30 days, "all identical" at 100 entries or more; all adjustable as spec 009 rule thresholds.
- News and video sitemap fields, HTML sitemaps, and sitemaps found in places other than robots.txt, the two default locations, user-added URLs and local files are out of scope.
- Search Console and analytics as orphan sources (and Search Console's own sitemap reports) belong to spec 024; Link Score from the orphan list to spec 023; comparing sitemaps between two crawls to spec 012; scheduled regeneration of sitemaps to spec 021; the end-of-crawl report sections to spec 010.
- Uploading generated sitemaps to a server, submitting them to search engines, or pinging search engines is out of scope (Google retired the ping endpoint; the app has no servers).
- Generation from a crawl file saved before this feature works for everything except image and hreflang options when the file lacks that data (spec 005 data is needed); those options are then disabled with the reason.
- Sitemap rules depend on the spec 009 rules engine; if this feature's code lands first, its checks ship as Sitemaps tab filters sharing one definition with the later rules (spec 009 FR-005).
- Schema numbers for the new tables are taken at merge time following the storage migration rule (spec 006), not reserved here.
