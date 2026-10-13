# Feature Specification: Crawl Comparison and Change Detection

**Feature Branch**: `012-crawl-comparison`

**Created**: 2026-10-13

**Status**: Draft, waiting for owner approval

**Input**: User description: "Crawl comparison and change detection: compare two crawl files of the same site and show which URLs were added, removed or changed; per-column differences for titles, meta descriptions, headings, status codes, canonicals and indexability; how issues from the rule catalogue (009) grew or shrank; URL mapping for site migrations and staging versus production, where the old and new crawls use different hostnames or paths; a comparison view in the desktop app and a command-line command; computed from stored data only, without fetching anything."

**Numbering note**: 012 is the backlog number reserved for crawl comparison in the product spec. The end-of-crawl report (010) renders the comparison section this spec defines; scheduled recrawls that compare automatically with the previous crawl belong to scheduling (021) and use the command line and settings file defined here.

## Overview

A single crawl answers "what is wrong with this site now". Most of the real work of an SEO team is about change: did last week's release break titles, did the migration keep every page, did the fixes actually fix the issues, did someone add a noindex to a template. Today the user has to export two CSV files and diff them in a spreadsheet.

This feature lets the user pick two crawl files, a **previous** crawl and a **current** crawl, and see what changed between them. It works entirely from the data already stored in the two files: nothing is fetched, nothing is re-parsed, and neither file is modified. It answers four questions:

1. **Which URLs came and went?** URLs that are new, URLs that disappeared, and URLs that moved in or out of each results filter (for example "now missing a title").
2. **What changed on the pages that exist in both?** Title, meta description, headings, status, indexability, canonical, directives, content and other elements, each shown as previous value next to current value.
3. **Did the site get better or worse?** For every issue in the rule catalogue (009), how many URLs were affected before and after, which URLs are newly affected, and which were fixed.
4. **Did the migration work?** With URL mapping, the old site's URLs (for example `staging.example.com` or an old path structure) are matched to the new site's URLs before comparing.

The same comparison is available in the desktop app and from the command line, with the same numbers, so it can run after a deploy and fail a build when the site regressed.

This feature belongs to milestone M2/M3. It builds on crawl storage (006), the results table and its tabs and filters (007) and the issue rules engine (009). It does not touch crawling, so it has no effect on crawl speed.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - See which URLs came and went (Priority: P1)

An SEO consultant crawls a client site every month. After this month's crawl they open the comparison with last month's crawl. An Overview lists every results tab and filter with the count in the previous crawl, the count in the current crawl and the change, and splits the change into four groups: **New** (in the current crawl and the filter, not in the previous crawl at all), **Added** (in both crawls, and moved into this filter), **Removed** (in both crawls, and moved out of this filter) and **Missing** (in the previous crawl and the filter, not in the current crawl at all). Clicking any number lists those URLs.

**Why this priority**: This is the core of comparison and the most common question. Every other story builds on matching URLs between two crawls.

**Independent Test**: Crawl a fixture site, change the fixture in known ways (add pages, delete pages, remove a title, add a long title, turn a page into a redirect), crawl again, compare, and check that every tab and filter shows the expected counts and exactly the expected URLs in each of the four groups.

**Acceptance Scenarios**:

1. **Given** a previous crawl with 500 internal HTML pages and a current crawl where 20 pages were deleted and 30 pages added, **When** the user opens the comparison, **Then** the Internal tab's "All" filter shows previous 500, current 510, change +10, New 30 and Missing 20, and each number opens exactly those URLs.
2. **Given** a page that had a title in the previous crawl and has none in the current crawl, **When** the comparison runs, **Then** the page is listed under Added for the Page titles "Missing" filter and under Removed for the Page titles "All" filter only if it left that filter's eligible set; it is not listed as New or Missing.
3. **Given** a URL that returned 200 in the previous crawl and 301 in the current crawl, **When** the comparison runs, **Then** it is listed as Added in the Response codes "Redirection (3xx)" filter and as Removed in the "Success (2xx)" filter.
4. **Given** the comparison Overview, **When** the user sorts by change, **Then** the filters with the largest absolute changes come first, and filters with no change can be hidden with one control.
5. **Given** any group of URLs opened from the Overview, **When** the user looks at the list, **Then** it supports sort, search, column choice, copy and CSV export as defined in spec 007, and shows previous and current values side by side for the columns of that tab.

---

### User Story 2 - Detect content changes on pages that exist in both crawls (Priority: P1)

A developer released a template change and wants to know what it did to the pages that were already there. A **Change detection** view lists, for each tracked element, how many pages changed and which ones, with the previous and current value side by side: page title, meta description, H1, H2, word count, status code, final state, indexability, canonical, robots directives, redirect target, structured data, visible content, crawl depth, unique inlinks, internal outlinks and hreflang.

**Why this priority**: Unintended changes to titles, canonicals or indexability on existing pages are the most expensive SEO regressions and the hardest to spot in a single crawl, because each page on its own looks fine.

**Independent Test**: Crawl a fixture site, change exactly one tracked element on a known set of pages per element, crawl again, compare, and check that each element's change list contains exactly those pages with the right previous and current values, and that pages with only whitespace or letter-case-insensitive-equal changes are handled as the rules below define.

**Acceptance Scenarios**:

1. **Given** 12 pages whose title changed between the crawls, **When** the user opens Change detection, **Then** "Page title" shows 12 changed pages, and its list shows each URL with previous title, current title, and both lengths.
2. **Given** a page whose title only differs in leading or trailing spaces or in runs of internal whitespace, **When** the comparison runs, **Then** it is not counted as changed (the same normalisation as spec 007 FR-011); a change in letter case is counted as changed.
3. **Given** a page whose word count went from 1,000 to 1,050, **When** the comparison runs with the default word count threshold, **Then** it is not counted as changed; a page that went from 1,000 to 700 is counted as changed with the difference shown in words and percent.
4. **Given** a page whose canonical changed from itself to another URL, **When** the comparison runs, **Then** it is listed under "Canonical" and also under "Indexability" (indexable to canonicalised), each with previous and current values.
5. **Given** a page whose visible text changed but whose title, description and headings did not, **When** the comparison runs, **Then** it is listed under "Content" based on the stored text fingerprint (spec 005), without storing or comparing the full text.
6. **Given** the Change detection settings, **When** the user turns an element off or changes its threshold, **Then** the comparison updates without recrawling and the change is remembered for later comparisons.

---

### User Story 3 - See whether issues got better or worse (Priority: P1)

A site owner fixed broken links and duplicate titles after the last audit and wants proof. An **Issues** view of the comparison lists every issue from the rule catalogue (009) with the affected URL count in the previous and the current crawl, the change, how many URLs are **newly affected** and how many were **fixed** (affected before, present and not affected now). Issues are grouped by priority, as in 009. Both crawls are evaluated with the same rule set and the same issue settings, so a change in the numbers means a change in the site, not in the rules.

**Why this priority**: "Did it get better" is what managers and clients ask for, and it is the input for regression gates in build pipelines (story 5) and the comparison section of the report (010).

**Independent Test**: Use a fixture pair where known issues are introduced and fixed between the crawls, compare, and check per issue the previous count, current count, newly affected URLs and fixed URLs.

**Acceptance Scenarios**:

1. **Given** a previous crawl with 40 pages affected by "Duplicate title" and a current crawl where 30 of them were fixed and 5 new pages are affected, **When** the user opens the Issues view, **Then** the issue shows previous 40, current 15, change -25, newly affected 5, fixed 30, and each number opens exactly those URLs.
2. **Given** a page affected by an issue in the previous crawl that is missing from the current crawl, **When** the comparison runs, **Then** it is not counted as fixed; it is counted under a separate "gone" figure for that issue.
3. **Given** crawl files that were evaluated with different rule set versions or different issue settings, **When** the comparison runs, **Then** both crawls are evaluated with the current rule set and the issue settings chosen for the comparison (by default the current crawl's), the view states which settings were used, and neither file is written.
4. **Given** a rule that cannot run on one of the two crawls because that file lacks the data (009 FR-010), **When** the comparison runs, **Then** the issue is shown as "not comparable" with the reason, never as a change from or to zero.
5. **Given** the Issues view, **When** the user chooses "only regressions", **Then** only issues with newly affected URLs are shown, ordered by priority and then by the number of newly affected URLs.

---

### User Story 4 - Compare a migration or staging with production using URL mapping (Priority: P2)

An agency is moving a client from `old-shop.example` to `www.example.com/shop/`, with new URL paths. Before launch they crawl staging (`staging.example.com/shop/`) and compare it with the live old site; after launch they compare the new site with the last crawl of the old one. URL mapping rules rewrite the previous crawl's URLs into the current crawl's form before matching, so the same page is compared with itself even though its address changed.

**Why this priority**: Migrations are where the largest SEO losses happen and where a comparison is most valuable, but they are less frequent than routine comparisons of the same site, and the core comparison works without mapping.

**Independent Test**: Use a fixture pair where the current crawl uses a different hostname and a changed path pattern; compare once without mapping (almost nothing matches) and once with mapping rules, and check that every page is matched to its counterpart and that unmatched pages are listed as unmatched on each side.

**Acceptance Scenarios**:

1. **Given** a previous crawl of `staging.example.com` and a current crawl of `www.example.com` with the same paths, **When** the user sets up a comparison, **Then** the app detects that the two crawls have different start hosts and offers a host mapping from one to the other, pre-filled, which the user can accept or edit.
2. **Given** mapping rules that replace `old-shop.example/products/` with `www.example.com/shop/p/`, **When** the comparison runs, **Then** each mapped previous URL is matched with the current URL it maps to, and lists show both the original previous URL and the mapped URL.
3. **Given** a mapping rule written as a regular expression with capture groups, **When** the user tests it in the mapping tester with sample URLs or with URLs from the previous crawl, **Then** the tester shows each URL's mapped result and whether that result exists in the current crawl, before the comparison runs (as the scope tester in spec 007 does for include and exclude patterns).
4. **Given** mapping rules under which two previous URLs map to the same current URL, **When** the comparison runs, **Then** the collision is reported with both previous URLs, both are compared with the current URL, and the Overview shows the number of collisions.
5. **Given** a migration comparison, **When** the user opens the "Migration" view, **Then** it lists previous pages with no counterpart in the current crawl, previous pages whose counterpart is not a 200 indexable page, and previous pages that are now reached only through a redirect, with the redirect chain and its final target.

---

### User Story 5 - Compare from the command line and fail a build on regressions (Priority: P2)

A developer runs SpiderLens in a deploy pipeline: crawl staging after each deploy, compare with the crawl of the last successful deploy, and fail the pipeline when the deploy introduced new High issues, removed pages or made indexable pages non-indexable.

**Why this priority**: The command line is a constitutional requirement (principle III), and comparison is the natural building block for automated checks and for scheduled crawls (021), but the desktop flow comes first.

**Independent Test**: Run the compare command on the same fixture pairs as stories 1 to 4 and check that its summary, its JSON output and its CSV exports match the desktop app's numbers and URL lists exactly; run it with each failure condition on pairs with and without that regression and check the exit codes.

**Acceptance Scenarios**:

1. **Given** two crawl files, **When** the user runs the compare command, **Then** it prints a summary with the totals of both crawls, the New, Missing, Added and Removed counts for the main filters, the number of changed pages per tracked element and the issue deltas, and with a JSON option prints the full comparison summary as JSON.
2. **Given** the compare command with an export folder, **When** it runs, **Then** it writes one CSV file per non-empty group (each filter group, each changed element, each issue's newly affected and fixed URLs, and migration lists), with the same rows and columns as the desktop app's exports.
3. **Given** the compare command with "fail on new issues at High", **When** the current crawl has newly affected URLs for at least one High issue, **Then** it exits with a dedicated non-zero exit code distinct from the codes defined in specs 001 to 009; otherwise it exits with 0.
4. **Given** the compare command with "fail on missing pages" or "fail on lost indexable pages" and an optional minimum count, **When** the number of missing internal HTML pages or of pages that went from indexable to non-indexable reaches that count, **Then** it exits with the same dedicated code and names the condition that failed.
5. **Given** a comparison settings file saved from the desktop app (mapping rules, tracked elements, thresholds, issue settings choice), **When** it is passed to the compare command, **Then** the command uses exactly those settings and produces the same numbers as the app.

---

### User Story 6 - Stay fast and safe with large crawls (Priority: P2)

A user compares two crawls of a large e-commerce site with a million URLs each. The comparison must not load both crawls into memory, must show its first results quickly and fill in the rest progressively, and must never risk damaging either crawl file, including the user's only copy of last year's crawl.

**Why this priority**: The constitution requires that no component loads a whole crawl into memory (principle VI) and that crawl data is never lost or damaged (principle V); large sites are where comparison matters most.

**Independent Test**: Compare two generated crawl files of 100,000 and of 1,000,000 URLs, measuring time to first results, total time and peak memory; check both files are byte-identical before and after.

**Acceptance Scenarios**:

1. **Given** two crawl files of 1,000,000 URLs each, **When** the user starts a comparison, **Then** the Overview shows a progress indicator and partial results, the app stays responsive, and the user can cancel at any time.
2. **Given** any comparison, **When** it runs and after it finishes, **Then** neither crawl file has been written, locked for writing, migrated or upgraded, and each file can be opened for crawling by another window at the same time (spec 006 read-only access).
3. **Given** a crawl file from an older format version, **When** it is used in a comparison, **Then** it is read without upgrading it; a crawl file from a newer version is used if the app can read it safely and refused with the version needed otherwise.
4. **Given** a crawl running in one window, **When** the user compares two other crawl files in another window, **Then** the running crawl's speed is not measurably reduced (within the run-to-run variation of the 008 benchmark).

---

### Edge Cases

- **Same file twice**: comparing a crawl file with itself is refused with a message; comparing two copies of the same crawl shows zero changes everywhere.
- **Crawl still running or paused**: a crawl that is running can be used as either side; the comparison uses a snapshot of its data at the moment the comparison starts, says so, and offers to refresh. A paused or stopped crawl is compared as it is.
- **Incomplete crawls**: when either crawl ended because of a limit (max URLs, max depth), was stopped by the user, or failed partway, the comparison shows that first. URLs in the previous crawl that the current crawl never reached because of a limit are reported as "Not reached" rather than Missing, using the frontier record (003) of the current crawl where available.
- **Different configurations**: when the two crawls used different settings that change what is collected (mode Spider vs List, scope rules, subdomain handling, robots handling, user agent, JavaScript rendering on or off, limits, external link checking), the comparison lists those differences at the top of the Overview, because they explain many changes.
- **Rendering on one side only**: if JavaScript rendering (014) was on in one crawl and off in the other, elements are compared using raw HTML values on both sides, and the comparison says so; if it was on in both, rendered values are compared, and the user can switch to raw values.
- **Data not stored in one file**: crawl files created before page extraction (005) landed have empty extraction columns; elements that a file does not have are shown as "not available" for that comparison, never as changed or removed.
- **Different sites without mapping**: when the two crawls share fewer than 10% of their URLs, the comparison warns that the crawls may be of different sites or need URL mapping, and offers the mapping setup.
- **URL normalisation**: URLs are matched exactly as stored after the crawl's own normalisation (002, 003). The comparison does not add hidden normalisation such as ignoring trailing slashes, letter case or query parameters; the user expresses those with mapping rules, and the mapping tester shows the result.
- **Redirects**: a URL that redirected in one crawl and returned a page in the other is matched by its own URL, not by its redirect target; the Migration view follows redirect chains to their final target.
- **Duplicate URLs within one crawl**: a crawl file holds each URL once; with mapping, collisions are handled as in story 4, scenario 4.
- **External URLs**: external URLs are compared on the External tab only, by their status and final state; tracked page elements apply to internal HTML pages.
- **Very different sizes**: comparing a 50-URL crawl with a 1,000,000-URL crawl works and stays within the same time and memory limits as two large crawls.
- **File moved or deleted during comparison**: the comparison stops with a clear message; nothing is written.
- **Cancel**: cancelling leaves no partial export files and no temporary data behind.

## Requirements *(mandatory)*

### Functional Requirements

**Choosing what to compare**

- **FR-001**: Users MUST be able to start a comparison from the start screen, the command palette and the File menu, by choosing a previous and a current crawl from the recent crawls list (spec 006) or from any crawl file on disk; with a crawl open, "Compare with…" MUST pre-select the open crawl as the current crawl.
- **FR-002**: The system MUST suggest as the previous crawl the most recent other crawl of the same start host, and MUST let the user swap previous and current.
- **FR-003**: Before running, the setup MUST show both crawls' start URL, date, number of URLs, completeness and app version, and the configuration differences that change what is collected.
- **FR-004**: The comparison MUST open both crawl files for reading only (spec 006) and MUST NOT write, lock for writing, migrate or upgrade either file, at any point.

**Matching URLs**

- **FR-005**: The system MUST match URLs between the two crawls by their stored URL exactly, after applying the mapping rules (if any) to the previous crawl's URLs.
- **FR-006**: Mapping rules MUST be an ordered list of find-and-replace rules, each either plain text or a regular expression with capture groups; the first rule that matches a URL applies.
- **FR-007**: When the two crawls have different start hosts, the system MUST offer a pre-filled host mapping rule.
- **FR-008**: The setup MUST include a mapping tester that shows, for sample URLs typed by the user or taken from the previous crawl, the mapped URL and whether it exists in the current crawl.
- **FR-009**: The system MUST report mapping collisions (several previous URLs mapped to one current URL) with their URLs, and MUST compare each of them with the current URL.
- **FR-010**: The system MUST warn when fewer than 10% of the URLs of the smaller crawl are matched, and offer the mapping setup.

**Overview and filter deltas**

- **FR-011**: The Overview MUST show, for every tab and filter of spec 007 (and the tabs and filters later specs add, such as the JavaScript tab of 014), the previous count, current count and change, and the counts of New, Added, Removed and Missing URLs as defined in story 1.
- **FR-012**: Filter membership on each side MUST use exactly the same filter definitions as the results table (spec 007) and the rules (009), so a comparison never disagrees with the numbers the user sees when opening either crawl on its own.
- **FR-013**: Every count in the comparison MUST open the list of exactly those URLs, with previous and current values side by side for the columns of the tab, and these lists MUST support sort, search, column choice, copy and CSV export as defined in spec 007.
- **FR-014**: The Overview MUST start with a summary: both crawls' totals by final state (spec 004), New, Missing, Not reached and matched URL counts, mapping collisions, configuration differences and completeness of each crawl.
- **FR-015**: URLs the current crawl did not reach because of a limit, a stop or a failure MUST be reported as "Not reached" and MUST NOT be counted as Missing.

**Change detection**

- **FR-016**: The system MUST detect changes, on internal HTML pages present in both crawls, in these elements: page title, meta description, first H1, set of H1s, first H2, word count, status code, final state and error class, indexability and its reason, canonical, robots directives (meta and header combined, spec 007 FR-013), redirect target, hreflang set, structured data (types present and block fingerprint), visible content (text fingerprint from spec 005), crawl depth, unique inlinks count and internal outlinks count.
- **FR-017**: Text values MUST be compared after the whitespace normalisation of spec 007 FR-011 and case-sensitively; sets (H1s, hreflang, structured data types) MUST be compared as sets, ignoring order.
- **FR-018**: Numeric elements MUST have a change threshold: word count changes by 10% or more, unique inlinks changes by 20% or more, internal outlinks changes by 20% or more, crawl depth changes by 1 or more; these defaults MUST be adjustable per element.
- **FR-019**: Users MUST be able to turn each tracked element on or off and change its threshold; changes MUST update the open comparison without reading the crawl files again where possible, and MUST be remembered for later comparisons.
- **FR-020**: Response time MUST be available as an optional tracked element, off by default, with a threshold expressed as a factor (default: twice as slow or more).
- **FR-021**: Each changed page MUST be listed with the previous value, the current value and, for numbers, the difference in absolute and percent terms; the detail pane MUST show the selected URL's previous and current details side by side for every section of spec 007 FR-019 that both crawls have.

**Issue deltas**

- **FR-022**: The Issues view MUST list every rule of the catalogue (009) with previous and current affected counts, change, newly affected, fixed and gone counts, grouped by priority as in 009, and each count MUST open exactly those URLs with their evidence on each side.
- **FR-023**: Both crawls MUST be evaluated with the same rule set version (the app's current one) and the same issue settings; by default the current crawl's issue settings, with the option to choose the previous crawl's settings or the app's current settings. The view MUST state the rule set version and settings used.
- **FR-024**: Evaluating issues for a comparison MUST NOT write issue results into either crawl file; results that are already stored in a file with the same rule set version and settings MAY be reused.
- **FR-025**: A rule that cannot run on either crawl (009 FR-010) MUST be shown as "not comparable" with the reason.
- **FR-026**: The view MUST offer "only regressions" (issues with newly affected URLs) and "only improvements" (issues with fixed URLs) filters.

**Migration view**

- **FR-027**: With mapping rules, the Migration view MUST list previous internal HTML pages that are: unmatched, matched to a URL that is not a successful indexable HTML page, or matched to a URL reached only through a redirect (with the chain and final target), and current pages with no counterpart in the previous crawl.

**Comparison settings**

- **FR-028**: Mapping rules, tracked elements, thresholds and the issue settings choice MUST together form the comparison settings, remembered across app restarts, and MUST be savable to and loadable from a comparison settings file that the command line also accepts.

**Command line**

- **FR-029**: The command line MUST compare two crawl files and print a summary as text, or the full summary as JSON, with the same numbers as the desktop app.
- **FR-030**: The command line MUST export, to a chosen folder, one CSV file per non-empty group of the comparison (filter groups, changed elements, issue deltas, migration lists), with the same rows and columns as the desktop app's exports and the CSV rules of spec 007 FR-029.
- **FR-031**: The command line MUST accept mapping rules directly and a comparison settings file.
- **FR-032**: The command line MUST accept failure conditions: newly affected URLs for issues at or above a chosen priority, missing internal HTML pages at or above a count, and pages that went from indexable to non-indexable at or above a count; when any condition is met it MUST end with one dedicated non-zero exit code, distinct from the codes of specs 001 to 009, and name the conditions met.

**Report**

- **FR-033**: The comparison MUST provide a comparison section for the end-of-crawl report (010), which 010 renders: both crawls' summary and configuration differences, totals by final state with change, the New, Missing and Not reached counts, the ten filters with the largest changes, issue deltas grouped by priority with newly affected and fixed counts, the number of changed pages per tracked element with up to 20 example URLs each, and the mapping rules used.

**Performance and safety**

- **FR-034**: The comparison MUST NOT load either crawl into memory as a whole; its memory use MUST NOT grow in proportion to the number of URLs (constitution principle VI).
- **FR-035**: The comparison MUST show progress and partial results while it runs, keep the interface responsive, and be cancellable at any time without leaving partial files or temporary data behind.
- **FR-036**: The comparison MUST NOT fetch anything or re-parse stored HTML; it MUST use only data stored in the two crawl files.
- **FR-037**: The comparison MUST NOT run in or slow down the crawl path; a crawl running in another window MUST keep its speed.
- **FR-038**: Every comparison view, setting and list MUST be reachable and operable by keyboard and announced correctly by screen readers, and MUST follow the design system and themes of specs 017 and 018.

### Key Entities

- **Comparison**: a pairing of a previous and a current crawl file with comparison settings, producing matched URLs, filter deltas, element changes, issue deltas and, with mapping, migration lists. It is computed from the two files and is not stored inside either of them.
- **Previous crawl / Current crawl**: the two crawl files being compared, each read only, with its start URL, date, configuration, app version, format version and completeness.
- **URL match**: a pair of a previous URL (original and mapped form) and a current URL, or a URL present on one side only, with its group (matched, New, Missing, Not reached).
- **Mapping rule**: an ordered find-and-replace rule (plain text or regular expression) applied to previous URLs before matching.
- **Filter delta**: for one tab filter, the previous and current counts and the New, Added, Removed and Missing URL sets.
- **Element change**: for one tracked element and one matched page, the previous and current values and, for numbers, the difference.
- **Issue delta**: for one rule, the previous and current affected counts and the newly affected, fixed and gone URL sets, with the rule set version and settings used.
- **Comparison settings**: mapping rules, tracked elements and thresholds, and the issue settings choice; savable as a file shared by the app and the command line.

## Success Criteria *(mandatory)*

### Measurable Outcomes

Timings are measured on the reference machine described in Assumptions.

- **SC-001**: On every fixture pair in the test suite, every filter delta, element change, issue delta and migration list contains exactly the expected URLs (no missing or extra URLs).
- **SC-002**: Comparing two crawls of 100,000 URLs each shows the Overview summary within 5 seconds and completes within 30 seconds; two crawls of 1,000,000 URLs each complete within 5 minutes.
- **SC-003**: Peak memory of a comparison of two 1,000,000-URL crawls is no more than 20% higher than of two 100,000-URL crawls.
- **SC-004**: After any comparison, completed or cancelled, both crawl files are byte-identical to before and their modification times are unchanged.
- **SC-005**: For every fixture pair, the command line's summary, JSON output and CSV exports agree 100% with the desktop app's counts and URL lists.
- **SC-006**: On the migration fixture pair (different host and path pattern), the suggested host mapping plus one path rule matches 100% of the pages that exist on both sides, and every unmatched page is listed.
- **SC-007**: The compare command's exit code matches each failure condition on every fixture pair with and without that regression (no false passes, no false failures).
- **SC-008**: A crawl of the 100,000-page benchmark fixture (008) running while a 1,000,000-URL comparison runs in another window keeps at least 95% of its speed.
- **SC-009**: A first-time user can find the pages whose title changed since the previous crawl and export them to CSV in under 2 minutes without help.

## Decisions for the owner

Each has a recommended default that this spec already assumes. Approving the spec approves these defaults.

1. **Comparisons are not saved as files in this version.** Re-running a comparison from the two crawl files is fast (SC-002), and the settings are remembered and savable; saving results would add a new file type to maintain. Exports and the report section (010) cover sharing. Alternative: a saved comparison file.
2. **Issue settings: the current crawl's settings for both sides**, so numbers reflect site changes, not settings changes. Alternatives: each crawl's own settings (numbers mix site and settings changes), or the app's current settings.
3. **A running crawl can be compared as a snapshot**, useful during long crawls. Alternative: only finished, paused or stopped crawls.
4. **No hidden URL normalisation**: trailing slash, letter case and query parameter differences are handled only by explicit mapping rules the user can see and test. Alternative: built-in options for these three.
5. **Response time is off by default in change detection**, because it varies from run to run and would bury real changes.
6. **Thresholds**: word count 10%, inlinks and outlinks 20%, depth 1, response time twice as slow, the "different sites" warning at 10% matched.

## Assumptions

- **Users**: SEO specialists, developers and site owners comparing their own crawls on their own desktop; no accounts or sharing beyond files.
- **Reference machine**: a mid-range laptop no more than four years old, with 4 to 8 CPU cores, 16 GB of memory and a solid-state disk, on each supported operating system (the same reference machine as every SpiderLens spec).
- **Dependencies**: crawl files and read-only access (006), final states and error classes (004), extracted page data and text fingerprints (005), the frontier record of URLs not reached (003), results tabs, filters, lists, detail pane and CSV export (007), the rule catalogue and evaluation (009). Filter and issue deltas for tabs and rules added by later specs (014, 015, 016) come for free because they reuse those definitions.
- **Scope of this feature**: comparing two crawls. Comparing three or more crawls, trends over many crawls, charts of changes over time, comparing the stored HTML text line by line, comparing rendered screenshots, and automatic recurring comparison (scheduling, 021) are out of scope. Comparing segments (gap report row 30) waits for segmentation.
- **Report**: this spec defines the content of the comparison section; layout and export formats of the report belong to 010.
- **Exit codes**: the exact number of the dedicated exit code is chosen in the plan, after 009 takes its own.
