# Feature Specification: Results Interface

**Feature Branch**: `007-results-ui`

**Created**: 2026-10-11

**Status**: Approved (2026-10-11)

**Input**: User description: "Results interface: while a crawl runs and after it finishes, the user sees live progress (crawled, queued, errors, URLs per second, elapsed time) and a results table with the M1 tabs from the brief, each with sensible filters (for example Page titles: missing, duplicate, over length); sort, search, column choice; a detail pane for the selected URL (response details, redirect hops, inlinks, outlinks, images, headings, raw HTML); copy and basic CSV export of the current view; smooth with over one million rows."

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Watch a crawl and browse its results (Priority: P1)

An SEO specialist starts a crawl and immediately sees how it is going: how many URLs have been crawled, how many are still queued, how many have failed, the current speed and the elapsed time. At the same time a results table fills up with the URLs that have finished, one row per URL. The user can switch between the tabs Internal, External, Response codes, URL, Page titles, Meta description, H1, H2, Images, Canonicals and Directives, each showing the columns that matter for that topic. When the crawl finishes, is stopped, or a saved crawl is reopened, the same view shows the final results.

**Why this priority**: Without seeing progress and results, a crawl has no value to the user. This is the minimum usable product for milestone M1.

**Independent Test**: Crawl a local fixture site of known size, observe that progress figures update during the crawl and match the fixture's known totals at the end, and that every tab lists the expected URLs with the expected column values.

**Acceptance Scenarios**:

1. **Given** a crawl has just started, **When** the user looks at the progress area, **Then** it shows the crawl state (running, paused, stopping, finished, stopped), crawled count, queued count, error count, current URLs per second and elapsed time, refreshed at least once per second.
2. **Given** a crawl is running, **When** URLs reach their final state, **Then** they appear in the results table within 2 seconds without the user doing anything and without the table jumping away from the row the user is looking at or losing the selected row.
3. **Given** a crawl of a fixture site with a known set of pages has finished, **When** the user opens each tab, **Then** each tab lists exactly the URLs that belong in it, and the counts shown in the progress area equal the number of URLs stored for each final state.
4. **Given** a saved crawl file is reopened, **When** the user browses the tabs, **Then** the results, filters and counts are identical to what was shown when the crawl finished.
5. **Given** the crawl is paused and later resumed, **When** the user looks at the elapsed time, **Then** it counts only time spent actively crawling, including time from earlier sessions of the same crawl.
6. **Given** a crawl has finished, **When** the user looks at the progress area, **Then** it states how complete the crawl was: URLs crawled, URLs that failed (by error class), URLs blocked by robots.txt or configuration, and URLs left uncrawled because a limit was reached.

---

### User Story 2 - Find problems with filters, sort and search (Priority: P1)

The user wants to answer questions like "which pages have no title?", "which titles are duplicated?", "which URLs return 404?". Each tab offers filters suited to its topic (for example Page titles: All, Missing, Duplicate, Over 60 characters, Below 30 characters, Multiple, Same as H1). Each filter shows how many URLs match. The user can sort by any column, type a search term to narrow the list, and choose which columns are shown and in what order.

**Why this priority**: Finding issues among thousands or millions of URLs is the core reason to use an SEO crawler; a table without filters is unusable at scale.

**Independent Test**: On a fixture site seeded with known issues (missing titles, duplicate titles, long meta descriptions, 404 pages, noindex pages, images without alt text), apply each filter and verify the exact set of URLs returned and the count shown.

**Acceptance Scenarios**:

1. **Given** a finished crawl where 3 pages have no title, **When** the user selects Page titles > Missing, **Then** exactly those 3 URLs are listed and the filter shows the count 3.
2. **Given** two pages share the title "Home", **When** the user selects Page titles > Duplicate, **Then** both pages are listed, and selecting either shows the other as sharing the same value.
3. **Given** any tab and filter, **When** the user clicks a column header, **Then** rows are sorted by that column ascending, a second click sorts descending, and the current sort is visibly indicated.
4. **Given** any tab and filter, **When** the user types a search term, **Then** only rows whose URL or main text column (for example the title on the Page titles tab) contains the term, ignoring letter case, remain, and the visible count updates.
5. **Given** any tab, **When** the user hides, shows, reorders or resizes columns, **Then** the change applies immediately and is remembered for that tab the next time the app is opened, for any crawl.
6. **Given** a crawl is running, **When** the user has a filter, search and sort applied, **Then** newly finished URLs that match appear in their correct sorted position and filter counts keep updating.

---

### User Story 3 - Inspect one URL in detail (Priority: P2)

Selecting a row opens a detail pane for that URL. It shows the response details (final state, status code, content type, size, response time, response headers, error class, attempts and message if it failed), each redirect hop, the pages linking to it (inlinks), the links on it (outlinks), its images, its headings in page order, and its raw HTML.

**Why this priority**: The table tells the user what is wrong; the detail pane tells them why and where to fix it (for example which pages link to a broken URL). It builds on the table, so it comes second.

**Independent Test**: Select fixture URLs with known properties (a redirect chain, a page with known inlinks and outlinks, a page with known headings and images, a failed URL) and verify every section of the detail pane shows the expected values.

**Acceptance Scenarios**:

1. **Given** a URL that redirected twice, **When** the user selects it, **Then** the redirect section lists each hop in order with its URL and status code and the final destination.
2. **Given** a URL that returned 404 and is linked from 5 pages, **When** the user opens the Inlinks section, **Then** it lists the 5 source pages with anchor text, link attributes (nofollow, sponsored, ugc) and whether the link was followed.
3. **Given** an HTML page, **When** the user opens Outlinks, Images or Headings, **Then** they see every outbound link (with target final state), every image (with source and alt text, and whether alt is missing or empty), and every heading with its level, in document order.
4. **Given** an HTML page whose content was stored, **When** the user opens Raw HTML, **Then** the stored HTML is shown as text exactly as received (after decompression), and the user can search within it.
5. **Given** a URL that failed with a timeout after 3 attempts, **When** the user selects it, **Then** the response section shows the error class, the number of attempts and the recorded message.
6. **Given** the user moves the selection with the keyboard up and down arrows, **When** the selection changes, **Then** the detail pane follows the selected URL.

---

### User Story 4 - Copy and export what I see (Priority: P2)

The user wants to send a list of broken links to a developer or paste duplicate titles into a spreadsheet. They can copy selected cells or rows to the clipboard, and export the current view (current tab, filter, search, sort and visible columns) to a CSV file that opens correctly in common spreadsheet applications.

**Why this priority**: Sharing findings is the step after finding them. A basic export is enough for M1; the full report suite comes in M2.

**Independent Test**: Apply a filter, search and sort, export to CSV, and compare the file row for row and column for column with what the view shows; copy rows and paste into a plain text editor and a spreadsheet.

**Acceptance Scenarios**:

1. **Given** a view with a filter, search, sort and chosen columns, **When** the user exports to CSV, **Then** the file contains a header row with the visible column names in display order followed by every matching row (not only the rows visible on screen) in the displayed sort order.
2. **Given** one or more selected rows, **When** the user copies, **Then** the clipboard holds those rows as tab-separated text with a header row, which pastes into a spreadsheet as cells.
3. **Given** a single selected cell, **When** the user copies it, **Then** the clipboard holds exactly that cell's full value (not the truncated display).
4. **Given** an export of a very large view is in progress, **When** the user keeps working, **Then** the app stays responsive, shows export progress, and lets the user cancel; a cancelled export leaves no partial file.
5. **Given** a crawl is still running, **When** the user exports, **Then** the file contains the results as they were when the export started, and the export completes even while new rows keep arriving.

---

### User Story 5 - Stay fast on very large crawls (Priority: P2)

A user crawling a large site with over one million URLs expects the results interface to feel the same as on a small site: scrolling is smooth, switching tabs and filters, sorting and searching return quickly, and the app does not slow the crawl or run out of memory.

**Why this priority**: Large sites are a key reason users choose a desktop crawler; an interface that freezes at scale makes the product unusable for them. It is a quality bar on Stories 1-4 rather than a separate capability.

**Independent Test**: Open a generated crawl file with 1,000,000 URLs and run a scripted set of interactions (scroll the whole table, switch every tab and filter, sort each column, search, select rows, export) measuring response times and memory; repeat during a live crawl of the 100,000-page local fixture site.

**Acceptance Scenarios**:

1. **Given** a crawl with 1,000,000 URLs, **When** the user drags the scrollbar from top to bottom, **Then** rows render without visible freezes and every row reached shows its data within 200 ms of stopping.
2. **Given** a crawl with 1,000,000 URLs, **When** the user switches tab or filter, sorts or searches, **Then** the first page of results appears within the times stated in the Success Criteria.
3. **Given** a crawl running at 500 URLs per second, **When** the user clicks, scrolls, types or switches tabs, **Then** the interface responds within about 100 ms and the crawl speed does not drop because of the interface.

---

### Edge Cases

- **No results yet**: before any URL finishes, each tab shows an empty state explaining that results will appear as the crawl progresses; a filter with no matches says "No URLs match this filter" rather than showing a blank table.
- **Selected row leaves the view**: if, during a live crawl, the selected URL stops matching the current filter (for example its title becomes a duplicate of a newly crawled page, or it moves to a different filter), the detail pane keeps showing it until the user selects another row.
- **Duplicate counts change during the crawl**: duplicate filters are re-evaluated as new pages arrive; a page can move into Duplicate after it was first listed. Counts are final only when the crawl finishes.
- **Very long values**: URLs, titles and other text longer than the column width are truncated with an ellipsis in the table; the full value is available in a tooltip, in the detail pane, and when copying or exporting.
- **Non-Latin and right-to-left text**: titles, headings and URLs in any script display correctly; internationalised URLs show their readable form, with the encoded form available in the detail pane.
- **Raw HTML not available**: for non-HTML resources, failed fetches or content not stored, the Raw HTML section explains why instead of showing an empty box. For very large documents (over 5 MB), the section shows the content progressively without freezing the app.
- **Huge link lists**: a page with 100,000 inlinks or outlinks shows them in a scrollable list that stays as responsive as the main table, with a count shown.
- **URL updated after it was listed**: when the end-of-crawl "retry failed URLs" action re-crawls a URL, its row updates in place to its new final state rather than appearing twice.
- **Older crawl files**: if a reopened crawl file lacks some data (for example it was made by an older version that did not store a field), the affected columns show as empty, the affected filters are shown as unavailable with an explanation, and nothing fails.
- **Export failures**: if the destination is not writable or the disk fills during export, the user sees a clear message, no partial file is left behind, and the crawl is not affected.
- **Spreadsheet formula injection**: an exported value that begins with `=`, `+`, `-` or `@` must not be executed as a formula when the file is opened in a spreadsheet.
- **Copying a huge selection**: selecting all 1,000,000 rows and copying is redirected to CSV export with an explanation, rather than freezing the app or overloading the clipboard.
- **Crawl stops due to a local problem** (for example disk full): the progress area shows the paused state and the reason, and the results already stored remain browsable.
- **External URLs not checked**: when external link checking is off, external URLs are still listed on the External tab with the state "not checked".

## Requirements *(mandatory)*

### Functional Requirements

**Progress**

- **FR-001**: The system MUST show, for the open crawl, its state (running, paused, stopping, finished, stopped) and, when paused because of a problem, the reason.
- **FR-002**: The system MUST show the number of URLs crawled (reached a final state), queued (discovered and waiting, including those waiting to retry), and errors (final state is an error class), refreshed at least once per second while a crawl runs.
- **FR-003**: The system MUST show the current crawl speed in URLs per second as a rolling average over the last 5 seconds (the same window as spec 008; amended 2026-10-13 with plan D13), and the average speed for the whole crawl once it ends.
- **FR-004**: The system MUST show elapsed crawl time that counts only active crawling, excludes paused time, and accumulates across pause, quit, reopen and resume of the same crawl.
- **FR-005**: When a crawl ends or is stopped, the system MUST show a completeness summary: URLs crawled, URLs failed broken down by error class, URLs blocked by robots.txt or by configuration, and URLs not crawled because a limit (such as max URLs or max depth) was reached.
- **FR-006**: Progress counts at the end of a crawl MUST equal the number of URLs stored in each corresponding final state.

**Results table and tabs**

- **FR-007**: The system MUST present results in a table with one row per URL that has reached a final state, organised in these tabs: Internal, External, Response codes, URL, Page titles, Meta description, H1, H2, Images, Canonicals, Directives.
- **FR-008**: During a crawl, the table MUST add and update rows automatically within 2 seconds of URLs reaching their final state, while keeping the user's scroll position anchored to the rows in view and keeping the current selection.
- **FR-009**: Each tab MUST offer the default columns and filters listed in the table below. Every filter MUST show the number of matching URLs, refreshed at least every 5 seconds during a crawl and exact once the crawl has ended.

| Tab | Default columns (others available) | Filters |
|-----|-----------------------------------|---------|
| Internal | Address, Content type, Status code, Status, Final state, Indexability, Title, Meta description, H1, Word count, Size, Response time, Depth, Inlinks, Outlinks | All, HTML, JavaScript, CSS, Images, PDF, Other |
| External | Address, Content type, Status code, Status, Final state, Inlinks | All, HTML, JavaScript, CSS, Images, PDF, Other, Not checked |
| Response codes | Address, Status code, Status, Final state, Error class, Attempts, Redirect target, Redirect hops, Inlinks | All, Blocked by robots.txt, Blocked by configuration, No response (any error class), Success (2xx), Redirection (3xx), Redirect loop or chain over limit, Client error (4xx), Server error (5xx); each also available for Internal only or External only |
| URL | Address, Length, Query string, Depth | All, Non-ASCII characters, Underscores, Uppercase, Contains query parameters, Multiple slashes, Contains spaces, Over 115 characters |
| Page titles | Address, Title, Title length, Occurrences | All, Missing, Duplicate, Over 60 characters, Below 30 characters, Same as H1, Multiple |
| Meta description | Address, Meta description, Length, Occurrences | All, Missing, Duplicate, Over 155 characters, Below 70 characters, Multiple |
| H1 | Address, H1 (first), H1 length, H1 (second), Occurrences | All, Missing, Duplicate, Over 70 characters, Multiple |
| H2 | Address, H2 (first), H2 length, H2 (second), Occurrences | All, Missing, Duplicate, Over 70 characters, Multiple |
| Images | Address (image URL), Alt text, Alt length, Status code, Size, Found on (count) | All, Missing alt attribute, Empty alt text, Alt text over 100 characters, Over 100 KB (checked images only) |
| Canonicals | Address, Canonical (HTML), Canonical (HTTP header), Indexability | All, Contains canonical, Self-referencing, Canonicalised (points elsewhere), Missing, Multiple or conflicting |
| Directives | Address, Meta robots, X-Robots-Tag, Indexability | All, Index, Noindex, Follow, Nofollow, None, Noarchive, Nosnippet, Max-snippet / max-image-preview / max-video-preview, Unavailable_after |

- **FR-010**: Content filters on the Page titles, Meta description, H1, H2, Canonicals and Directives tabs MUST apply to internal URLs whose final response was a successfully fetched HTML page; the All filter on those tabs lists the same set of URLs.
- **FR-011**: "Duplicate" filters MUST list every eligible URL whose value (after trimming leading and trailing whitespace and collapsing internal runs of whitespace) is identical, case-sensitively, to the value of at least one other eligible URL; empty values are covered by "Missing", not "Duplicate".
- **FR-012**: Length thresholds MUST be measured in characters (not bytes) and the threshold MUST appear in the filter's label, so the user knows what "over length" means.
- **FR-013**: Directive filters MUST combine the meta robots tag and the X-Robots-Tag header, and the table MUST show which source each directive came from.
- **FR-014**: The system MUST remember, per tab and for the duration of the session, the selected filter, search term and sort, so switching tabs and back restores them.

**Sort, search and columns**

- **FR-015**: Users MUST be able to sort any visible column ascending or descending by clicking its header; numbers sort numerically, text sorts in natural, case-insensitive order, and empty values sort last in both directions.
- **FR-016**: Users MUST be able to search the current tab and filter by a text term that matches, ignoring letter case, any part of the URL or of the tab's main text column; the matching count MUST update with the results.
- **FR-017**: Users MUST be able to show, hide, reorder and resize columns per tab; these choices MUST persist across app restarts and apply to every crawl, with a way to reset a tab to its default columns.
- **FR-018**: The table MUST be fully operable from the keyboard: moving the selection, selecting ranges, opening the detail pane, copying, and switching tabs.

**Detail pane**

- **FR-019**: Selecting a row MUST show a detail pane for that URL with the sections: Response details, Redirects, Inlinks, Outlinks, Images, Headings, Raw HTML. Sections that do not apply to the URL (for example Headings for an image) MUST say so.
- **FR-020**: Response details MUST include the URL (readable and encoded form), final state, status code and text, content type, size, response time, crawl depth, indexability, the response headers received, and, for failures, the error class, number of attempts and recorded message.
- **FR-021**: Redirects MUST list every hop in order with its URL and status code, and the final destination and its final state; redirect loops and chains over the hop limit MUST be identified as such.
- **FR-022**: Inlinks MUST list every source page linking to the URL with anchor text (or image alt text for image links), rel attributes (nofollow, sponsored, ugc) and whether the link was followed; Outlinks MUST list every link on the page with target URL, anchor text, rel attributes, internal or external, and the target's final state.
- **FR-023**: Images MUST list every image on the page with its source URL, alt text, and whether the alt attribute is missing or empty; Headings MUST list every heading (H1 to H6) with its level, in document order.
- **FR-024**: Raw HTML MUST show the stored HTML of the page as received (decompressed), as read-only text the user can search and copy; when no HTML is stored the section MUST explain why.
- **FR-025**: Lists in the detail pane MUST remain responsive with 100,000 or more entries, MUST show their total count, and MUST support copying and opening a listed URL in the main table.

**Copy and export**

- **FR-026**: Users MUST be able to copy a single cell's full value, and copy one or more selected rows as tab-separated text with a header row of the visible columns.
- **FR-027**: Copying more than 10,000 rows MUST NOT be attempted through the clipboard; the system MUST explain this and offer CSV export of the selection or view instead.
- **FR-028**: Users MUST be able to export the current view to a CSV file containing a header row and every row matching the current tab, filter and search, in the current sort order, with the visible columns in display order and full (untruncated) values.
- **FR-029**: Exported CSV files MUST be UTF-8 encoded in a form common spreadsheet applications open with non-Latin text intact, MUST quote values correctly so commas, quotes and line breaks inside values survive, and MUST neutralise values that a spreadsheet would treat as formulas.
- **FR-030**: Export MUST run without blocking the interface or the crawl, show progress, be cancellable, and leave no partial file when cancelled or failed. An export started during a crawl MUST contain the results as they were when it started.
- **FR-031**: The command-line interface MUST be able to export any tab and filter of a crawl file to CSV with the same rows and default columns as the desktop app, so the same tab and filter definitions serve both.

**Scale and responsiveness**

- **FR-032**: The interface MUST request only the data needed for what is currently displayed (visible rows, counts, the selected URL's details) and MUST NOT load a whole crawl into memory; its memory use MUST NOT grow in proportion to the number of URLs in the crawl.
- **FR-033**: The results interface MUST work identically for a live crawl, a paused crawl, a finished crawl and a reopened crawl file.

### Key Entities

- **Crawl progress**: a live summary of one crawl: state, pause reason, crawled, queued and error counts, current and average URLs per second, active elapsed time, and the completeness summary at the end.
- **Tab**: a named view over the crawl's URLs (for example Page titles), defining which URLs are eligible, its default and optional columns, and its filters.
- **Filter**: a named rule within a tab (for example "Over 60 characters") selecting a subset of the tab's URLs, with a live count.
- **View**: what the user currently sees: tab, filter, search term, sort column and direction, visible columns in order and their widths. A view is what copy and export act on.
- **Column preferences**: per-tab choice, order and width of columns, kept across sessions and crawls.
- **URL detail**: everything known about one URL, drawn from the crawl data: response details, redirect hops, inlinks, outlinks, images, headings and raw HTML.
- **Export job**: a request to write a view to a CSV file, with destination, progress, and outcome (completed, cancelled, failed with reason).

## Success Criteria *(mandatory)*

### Measurable Outcomes

Timings below are measured on the reference machine described in Assumptions.

- **SC-001**: During a crawl, progress figures refresh at least once per second, and at the end of the crawl every count equals the number of stored URLs in that final state (100% agreement on every fixture site in the test suite).
- **SC-002**: On every fixture site seeded with known issues, every filter in FR-009 returns exactly the expected set of URLs (no missing or extra URLs) and shows the matching count.
- **SC-003**: With a crawl of 1,000,000 URLs, switching tab or filter shows the first screen of rows within 1 second, and its count within 2 seconds.
- **SC-004**: With a crawl of 1,000,000 URLs, sorting by any column or applying a search term shows the first screen of results within 2 seconds.
- **SC-005**: With a crawl of 1,000,000 URLs, scrolling (wheel, keyboard or scrollbar drag) never freezes the interface for more than 100 ms, and rows reached show their data within 200 ms of scrolling stopping.
- **SC-006**: Selecting a row shows the detail pane's response details within 300 ms, even for a URL with 100,000 inlinks.
- **SC-007**: While crawling the 100,000-page local fixture site at 500 URLs per second or more, user interactions (click, type, scroll, switch tab) respond within about 100 ms, and the crawl speed with the results interface open is at least 95% of the speed of the same crawl run from the command-line interface.
- **SC-008**: The interface's memory use with a 1,000,000-URL crawl open is no more than 20% higher than with a 100,000-URL crawl open, during scrolling, filtering, sorting and searching.
- **SC-009**: Exporting a view of 1,000,000 rows to CSV completes within 60 seconds, and the file opens in common spreadsheet applications with every row and column matching the view, including non-Latin text and values containing commas, quotes and line breaks.
- **SC-010**: A first-time user can find all pages with missing titles and export them to CSV in under 1 minute without help.

## Assumptions

- **Users**: SEO specialists, developers and site owners using SpiderLens on their own desktop; a single user per app window. No accounts or sharing beyond saving files.
- **Reference machine**: timings are measured on a mid-range laptop no more than four years old, with 4 to 8 CPU cores, 16 GB of memory and a solid-state disk, on each supported operating system (the same reference machine is used by every SpiderLens spec).
- **Dependencies**: this feature displays data produced and stored by other M1 features: crawl configuration and modes (spec 002), fetch errors and retries with their final states and error classes (spec 004), page extraction (spec 005), and the crawl file with save, reopen and resume (crawl storage feature). It does not change what is collected.
- **Which URLs appear**: the table lists URLs once they reach a final state; URLs still queued or waiting to retry are counted in the progress area but not listed. Blocked URLs (robots.txt or configuration) are listed on the Response codes tab but are not counted as errors.
- **Crawl modes**: the same interface serves Spider and List mode crawls; in List mode, the supplied URLs appear on the Internal or External tab according to the scope rules.
- **Thresholds**: filter thresholds (60/30 characters for titles, 155/70 for meta descriptions, 70 for headings, 115 for URLs, 100 for alt text, 100 KB for images) are fixed, well-known industry defaults in M1. Making them configurable belongs to M2 audits.
- **Indexability**: a URL is shown as non-indexable when it is not a successful HTML response, has a noindex directive, or is canonicalised to another URL; the reason is shown alongside.
- **Search**: plain-text, case-insensitive "contains" matching. Regular-expression search, multi-column sort and saved views are out of scope for M1.
- **Copy limit**: 10,000 rows is the largest selection copied through the clipboard; larger selections go through CSV export.
- **CSV**: "basic" export means the current view only, as one CSV file. Formula-like values are neutralised by prefixing them so spreadsheets show them as text.
- **Out of scope for this feature (later milestones)**: issues and audit overviews, custom and additional tabs (hreflang, structured data, pagination, etc.), charts and site visualisations, bulk exports and reports, other export formats (M2); crawl comparison, saved views, configurable thresholds and pixel-width title checks (M2/M3); rendered-page views and screenshots (M4); AEO/GEO views (M5); AI agent features (M6).
