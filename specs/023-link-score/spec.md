# Feature Specification: Link Score and Internal Link Analysis

**Feature Branch**: `023-link-score`

**Created**: 2026-10-13

**Status**: Draft (awaiting owner approval)

**Input**: User description: "Feature gap row 12 (plus custom link positions, folded in from the backlog review) against Screaming Frog: Link Score (an internal PageRank-like score per URL), crawl depth and inlink analysis, and a Crawl Analysis step. Compute the score after the crawl from the stored link graph, show it with crawl depth and inlink counts as columns, make it an input for the issue rules engine (spec 009), and keep within a time budget on 100,000 pages."

## User Scenarios & Testing *(mandatory)*

### User Story 1 - See which pages the site's own links favour (Priority: P1)

An SEO consultant finishes a crawl and wants to know which pages the site's internal linking treats as important and which it starves. Every internal URL now has a Link Score from 0 to 100, next to its crawl depth and its inlink counts. Sorting the Internal tab by Link Score shows the strongest pages first; sorting ascending shows the pages that receive almost nothing. The consultant compares this with the pages the client cares about (for example a product category that sits at score 4 while the privacy policy sits at 60) and changes the internal links.

**Why this priority**: This is the whole point of the feature. Without the score and its companion columns there is nothing to analyse, and the rules, visualisations and exports below all read the same values.

**Independent Test**: Crawl a fixture site whose link graph is designed so the expected scores are known (a hub page linked from every page, a page linked only once from deep in the site, a page linked only through nofollow links), then compare the stored scores, ranks, depths and inlink counts with the expected values.

**Acceptance Scenarios**:

1. **Given** a finished crawl of a fixture site, **When** the user opens the Internal tab, **Then** the columns Link Score, Crawl depth, Unique inlinks and Unique followed inlinks are available and sortable, and Link Score is shown by default.
2. **Given** a fixture site where page A is linked from every page and page B is linked from one page at depth 5, **When** the analysis has run, **Then** A has the highest score of the two and B the lowest, and the ranking of all fixture pages matches the expected ranking exactly.
3. **Given** a page that is linked only through links marked nofollow, **When** the analysis has run, **Then** its unique followed inlinks count is 0 and it receives no score through those links.
4. **Given** the same crawl analysed twice, **When** the user compares the results, **Then** every score is identical (the analysis is deterministic).
5. **Given** an internal URL, **When** the user selects it, **Then** the detail pane shows its Link Score, its rank among internal URLs (for example "rank 12 of 4,812"), crawl depth, unique inlinks, unique followed inlinks, all inlinks and unique outlinks.

---

### User Story 2 - The analysis runs on its own when a crawl ends (Priority: P1)

The user should not have to remember an extra step. When a crawl finishes, a short Crawl Analysis step runs automatically, with its own progress in the status bar, and the new columns fill in when it ends. The crawl itself is never slowed down by it, because it only starts after the last URL has been fetched. The user can also run it again by hand, for example after resuming and finishing a stopped crawl, and can turn the automatic run off in the settings.

**Why this priority**: A score that needs a manual step is a score most users never see. Running after the crawl, never during it, keeps the top product priority intact: crawl speed.

**Independent Test**: Crawl the 100,000-page fixture site with the automatic analysis on, check that the crawl's URLs per second equal those of the same crawl with the analysis off, that the analysis starts after the crawl ends, shows progress, and finishes within the budget in the success criteria.

**Acceptance Scenarios**:

1. **Given** automatic analysis is on (the default), **When** a crawl finishes, **Then** Crawl Analysis starts by itself, the status bar shows its progress and phase, and the Link Score column fills in when it completes.
2. **Given** a crawl is still running, **When** the user looks at the Link Score column, **Then** it is shown as pending ("calculated when the crawl ends") rather than empty or zero.
3. **Given** Crawl Analysis is running, **When** the user cancels it, **Then** it stops within 1 second, no partial scores are stored, and the scores (if any) from an earlier complete run stay in place and are marked as out of date.
4. **Given** a crawl was analysed, then resumed and more URLs were crawled, **When** the user looks at the scores, **Then** they are marked as out of date until the analysis runs again, and the analysis runs again automatically when the resumed crawl finishes.
5. **Given** a crawl was stopped before it finished, **When** the user chooses Run Crawl Analysis, **Then** the analysis runs over the data stored so far and the results state that the crawl was incomplete.
6. **Given** the user turns automatic analysis off in Settings, **When** a crawl finishes, **Then** no analysis runs, and the Link Score column offers a Run Crawl Analysis action.

---

### User Story 3 - Find linking problems with filters and rules (Priority: P2)

The user wants direct answers: which pages are buried too deep, which important pages have a single internal link, which pages have no followed internal links at all, which pages link nowhere. A Links tab collects these views with counts, and the same values are available to the issue rules engine (spec 009) so its rules can raise issues such as "High crawl depth" or "Only one followed inlink" with priorities and fixes.

**Why this priority**: The columns already answer these questions by sorting; filters and rules make them one click and visible in the issues overview. It depends on Stories 1 and 2.

**Independent Test**: On a fixture site seeded with known linking problems, open each Links tab filter and check the exact set of URLs and the count; run the 009 rules that use these inputs and check the raised issues.

**Acceptance Scenarios**:

1. **Given** a finished, analysed crawl, **When** the user opens the Links tab, **Then** it lists internal HTML pages with the columns Address, Link Score, Crawl depth, Unique inlinks, Unique followed inlinks, Unique outlinks and Indexability, and offers the filters All, Crawl depth 4 or more, One followed inlink, No followed inlinks, No internal outlinks, Nofollow internal outlinks, each with its count.
2. **Given** a page that is reachable only through a sitemap and has no internal inlinks, **When** the user opens No followed inlinks, **Then** the page is listed, and its Link Score is the lowest possible value.
3. **Given** the issue rules engine is available, **When** a rule's condition refers to Link Score, crawl depth, unique inlinks, unique followed inlinks or unique outlinks, **Then** it evaluates against the same stored values the table shows.
4. **Given** the user exports the Links tab with a filter applied, **When** the CSV opens, **Then** its rows and values match the view (export rules as in spec 007).

---

### User Story 4 - Know where on the page each link sits (Priority: P2)

A link in the main text says more about a page than the same link repeated in a mega menu or footer on every page. The user wants each internal link classified by its position on the page: Navigation, Header, Content, Sidebar, Footer. SpiderLens classifies links with built-in rules that recognise common page structure, and the user can add their own rules for their site (for example "links inside `.mega-menu` are Navigation", "links inside `#related` are Content") and test them against a page before applying them. The position shows as a column on Inlinks and Outlinks and in exports, it can drive filters and rules ("only linked from the footer"), and the user can choose to weight Link Score by position so boilerplate links count for less.

**Why this priority**: It makes the inlink analysis and the score much more honest on sites with big menus, but the score is already useful without it.

**Independent Test**: Crawl fixture pages with known navigation, header, content, sidebar and footer links, run the analysis with the built-in rules and with custom rules, and compare every link's position and the resulting weighted scores with the expected values.

**Acceptance Scenarios**:

1. **Given** a fixture page whose links sit inside nav, header, main, aside and footer elements, **When** Crawl Analysis has run, **Then** each link is classified as Navigation, Header, Content, Sidebar or Footer respectively, and links matching no rule are Content.
2. **Given** a custom rule "links inside elements matching `.mega-menu` are Navigation" placed above the built-in rules, **When** the analysis runs, **Then** links in that element are Navigation, and the rule order decides when several rules match.
3. **Given** the custom rules editor, **When** the user tests their rules against a crawled URL, **Then** they see every link on that page with the position it would get and the rule that gave it, without re-running the analysis.
4. **Given** a selected URL, **When** the user opens Inlinks or Outlinks in the detail panel, **Then** each link shows its position, and the list can be filtered by position.
5. **Given** the user turns on "Weight Link Score by link position" with the default weights, **When** the analysis runs, **Then** links count with their position's weight, so a page linked only from footers scores lower than with weighting off, and the result records that weighting was on and the weights used.
6. **Given** raw HTML was not stored for a page (spec 005 FR-031), **When** the analysis runs, **Then** that page's links have position Unknown, count with the Content weight, and the result says how many links could not be classified.

---

### User Story 5 - Same analysis from the command line (Priority: P3)

A developer runs crawls from a script and wants the scores too. The command-line interface runs Crawl Analysis on a crawl file (automatically after `spiderlens crawl`, unless turned off) and exports the Links tab like any other tab.

**Why this priority**: Required by the constitution (everything in the desktop app is possible from the command line), but most users meet the feature in the app first.

**Independent Test**: Run the CLI analysis on a fixture crawl file and compare the stored scores with those produced by the desktop app for the same file.

**Acceptance Scenarios**:

1. **Given** a crawl file, **When** the user runs the CLI analysis command on it, **Then** it stores the same scores the desktop app would, reports progress, and exits with success.
2. **Given** a CLI crawl, **When** it finishes, **Then** the analysis runs unless the user passed the option to skip it, and the exit code reflects the crawl result, not the analysis.
3. **Given** an analysed crawl file, **When** the user exports the Links tab from the CLI, **Then** the CSV matches the desktop export of the same tab and filter.

### Edge Cases

- **Redirects**: a link to a URL that redirects passes everything it receives on to the redirect target, so the final page gets the credit; the redirecting URL still shows the score it received, so users can see how much value flows through redirects. Redirect chains are followed to the end within the crawl's hop limit; loops pass nothing on.
- **Error and blocked targets**: a URL that returned an error, is blocked by robots.txt or by configuration, or was not crawled receives score from links to it but passes none on (it has no known outlinks).
- **Non-HTML targets**: images, PDFs and other files receive score from links to them and pass none on.
- **Pages with no outlinks**: their score is spread evenly over all scored URLs, as the standard method does, so the total is preserved and the result does not depend on such pages.
- **Several links from one page to the same target**: count once for the score and for unique inlinks; every occurrence still counts in "All inlinks".
- **Self links**: a page linking to itself does not raise its own score and does not count as an inlink.
- **Nofollow**: links with rel nofollow (and links on a page with a page-level nofollow) pass no score; they still count in "All inlinks" and are shown as nofollow. Sponsored and ugc are treated like nofollow.
- **Canonicals and hreflang**: canonical and hreflang declarations are not links and pass no score. The score is reported on the URL as linked, not folded into its canonical.
- **External links**: links to external URLs are left out of the graph; external URLs have no Link Score.
- **Head links, images and resources**: only anchor and image-map area links from the page body count, including image links; `link rel=next/prev`, image sources, scripts and stylesheets do not.
- **Start URL and List mode**: the start URL gets no special treatment beyond its links. In List mode, the score is computed over the links between the listed URLs; with no links between them, every URL has the same score and the result says so.
- **Tiny crawls**: a crawl with one internal URL gives it the top score; a crawl with no internal HTML pages shows no scores and explains why.
- **Very large crawls**: a crawl of 1,000,000 URLs with 50,000,000 links is analysed within the memory and time limits in the success criteria, without loading the whole crawl into memory.
- **Older crawl files**: files made before this feature open with the Link Score column shown as "not calculated" and offer Run Crawl Analysis. On a file that opens read-only (newer format, spec 006), the analysis can still run for viewing, but its results are not saved to the file and the user is told so.
- **Saved crawl opened elsewhere**: scores are stored in the crawl file, so a shared file shows the same scores without recalculation.
- **Crawl depth meaning**: crawl depth is the fewest link steps from the start URL (or 0 for each URL in List mode and for seeds from sitemaps, as defined in spec 003), as already stored by the crawl. This feature shows it; it does not recompute it.

## Requirements *(mandatory)*

### Functional Requirements

**Score**

- **FR-001**: The system MUST compute, for every internal URL in a crawl, a Link Score using an iterative link-weight method of the PageRank family over the crawl's internal link graph, with a damping factor of 0.85, until the largest change between rounds is below a fixed tolerance or a fixed round limit is reached.
- **FR-002**: The graph MUST contain one node per internal URL and one edge per distinct (source page, target URL) pair formed by followed anchor or area links in the page body, excluding self links; nofollow, sponsored and ugc links and links from page-level nofollow pages MUST NOT form edges.
- **FR-003**: A redirecting URL MUST pass its whole value on to its redirect target (following chains to the end within the hop limit; loops pass nothing). Error, blocked, uncrawled and non-HTML URLs MUST receive value but pass none on; value from pages without outgoing edges MUST be spread evenly over all nodes.
- **FR-004**: The raw scores MUST be mapped to a 0-100 scale on a logarithmic curve, so the highest-scored URL in the crawl is 100 and the lowest possible value is 0, with whole numbers shown in the table and one decimal place in the detail pane and exports. The rank (1 = highest, ties sharing a rank) MUST be stored with it.
- **FR-005**: The analysis MUST be deterministic: the same crawl file always yields the same scores and ranks on every platform, to the shown precision.

**Counts and columns**

- **FR-006**: The system MUST store per internal URL: unique inlinks (distinct internal source pages linking to it), unique followed inlinks, all inlinks (every link occurrence), unique outlinks (distinct internal targets from it), unique external outlinks, and the share of internal HTML pages that link to it, as a percentage.
- **FR-007**: The Internal tab MUST offer the columns Link Score, Link Score rank, Unique inlinks, Unique followed inlinks, All inlinks, Unique outlinks, Unique external outlinks and % of pages linking, with Link Score shown by default; Crawl depth remains as defined in spec 007.
- **FR-008**: The detail pane's Response details MUST show Link Score, rank out of the number of scored URLs, crawl depth and the counts in FR-006 for the selected internal URL.
- **FR-009**: Until the analysis has run, Link Score and the counts that depend on it MUST be shown as pending during a crawl and as "not calculated" afterwards, never as 0. Scores from an earlier run MUST be marked as out of date when the crawl's data has changed since.

**Links tab**

- **FR-010**: The system MUST add a Links tab listing internal HTML pages with the default columns Address, Link Score, Crawl depth, Unique inlinks, Unique followed inlinks, Unique outlinks and Indexability, and the filters All, Crawl depth 4 or more, One followed inlink, No followed inlinks, No internal outlinks, Nofollow internal outlinks; each filter shows its count, and the tab supports sort, search, columns, copy and export as defined in spec 007.
- **FR-011**: The Links tab MUST appear in the sidebar with the other result tabs and its count, and the start URL MUST be excluded from the No followed inlinks filter.

**Crawl Analysis step** (shared with spec 009, which owns it)

- **FR-012**: Crawl Analysis MUST run only when no crawl is fetching in the same file, MUST start automatically when a crawl finishes unless turned off in Settings (on by default), and MUST be available as a manual action (toolbar or command palette, and the "not calculated" state of the column).
- **FR-013**: Crawl Analysis MUST show its progress and current phase in the status bar, MUST be cancellable within 1 second, and MUST store its results only when it completes, so a cancelled or failed run leaves the previous results (if any) untouched.
- **FR-014**: Crawl Analysis MUST record with its results the time it ran, the app version, whether the crawl was complete at that time, whether position weighting was on and its weights, the position rules used, and the number of URLs and links analysed; the detail of an out-of-date or incomplete-crawl result MUST be visible to the user.
- **FR-015**: Crawl Analysis results MUST be stored in the crawl file, so a reopened or shared file shows them without recalculation; adding them to the file MUST follow the storage rules of spec 006 (additive change, older files keep opening). On a read-only file, results are computed for viewing only and not saved.
- **FR-016**: Link Score, the counts in FR-006 and link positions MUST be delivered as passes of the shared Crawl Analysis step defined by the issue rules engine (spec 009 plan, decision D12), not as a step of their own; the Link Score pass runs after the orphan pages pass of spec 011 when that pass exists. FR-012 to FR-015 describe what the user sees of that shared step for this feature.

**Rules and other features**

- **FR-017**: Link Score, rank, crawl depth and the counts in FR-006 MUST be available as inputs to the issue rules engine (spec 009) under stable names, with the "pending / not calculated" state exposed so rules that need them are reported as not evaluated rather than passing or failing.
- **FR-018**: The values MUST be available to the site visualisations (spec 022) for sizing and colouring nodes.

**Link positions**

- **FR-019**: Crawl Analysis MUST classify every stored body link from an internal HTML page into one position: Navigation, Header, Content, Sidebar, Footer, or Unknown when the page's HTML is not stored.
- **FR-020**: The built-in rules MUST classify by the link's enclosing page structure (nav, header, main or article, aside and footer elements, and their equivalent landmark roles), with Content as the default when no rule matches.
- **FR-021**: Users MUST be able to define ordered custom rules, each mapping a CSS selector for an enclosing element to a position, placed before the built-in rules; the first matching rule wins. Custom rules MUST be saved with the crawl configuration (spec 002), so they travel with profiles and crawl files.
- **FR-022**: Users MUST be able to test custom rules against any crawled URL and see each link's resulting position and the rule that matched, before applying them.
- **FR-023**: Changing the rules MUST mark the positions and any position-weighted scores out of date and offer to re-run Crawl Analysis; no recrawl is needed.
- **FR-024**: Link position MUST be shown as a column on the detail panel's Inlinks and Outlinks lists with a filter by position, included in exports, and available to the issue rules engine (spec 009) and to the anchor text clouds of spec 022.
- **FR-025**: Users MUST be able to turn on "Weight Link Score by link position" (off by default), with editable weights per position from 0 to 1 (defaults: Content 1.0, Sidebar 0.5, Header 0.3, Navigation 0.3, Footer 0.2, Unknown 1.0); with weighting on, each edge's share MUST be proportional to the highest weight among the links forming it.
- **FR-026**: The Links tab MUST add the filter "Linked only from navigation, header or footer" (internal pages with followed inlinks, none of them in Content or Sidebar).
- **FR-027**: Classifying positions MUST read only stored data (the stored HTML) and MUST NOT fetch anything.

**Command line**

- **FR-028**: The command-line interface MUST be able to run Crawl Analysis on a crawl file with progress output, MUST run it automatically after a crawl unless told not to, and MUST export the Links tab with the same rows and default columns as the desktop app.

**Performance and limits**

- **FR-029**: Crawl Analysis MUST NOT run while URLs are being fetched and MUST NOT change anything in the fetch path, so it has no effect on crawl speed.
- **FR-030**: Crawl Analysis MUST NOT load the whole crawl into memory; its memory use MUST stay within the limit in SC-004 regardless of crawl size, and the interface MUST stay responsive while it runs.

### Key Entities

- **Link Graph**: The internal URLs of a crawl and the distinct followed links between them, after redirects are resolved. Derived from stored links; not stored as such.
- **Link Metrics**: Per internal URL: Link Score (0-100), rank, unique inlinks, unique followed inlinks, all inlinks, unique outlinks, unique external outlinks, % of pages linking.
- **Link Position**: Per stored body link: Navigation, Header, Content, Sidebar, Footer or Unknown, and the rule that decided it.
- **Link Position Rule**: An ordered custom rule (selector for an enclosing element, position), saved with the crawl configuration.
- **Analysis Run**: One completed Crawl Analysis: when it ran, app version, crawl completeness at the time, URLs and links analysed, phases included, and whether it is out of date.

## Success Criteria *(mandatory)*

### Measurable Outcomes

Timings are measured on the reference machine shared by every SpiderLens spec (spec 007 Assumptions).

- **SC-001**: On the Link Score fixture sites, every stored score matches the expected value (computed independently by a reference implementation of the same method) to the shown precision, and the rank order matches exactly.
- **SC-002**: On the 100,000-page fixture site with at least 5,000,000 internal links, Crawl Analysis completes in under 10 seconds from start to scores visible in the table.
- **SC-003**: On a 1,000,000-URL crawl file with 50,000,000 internal links, Crawl Analysis completes in under 3 minutes.
- **SC-004**: Crawl Analysis memory use stays under 500 MB on the 1,000,000-URL file and under 100 MB on the 100,000-page file.
- **SC-005**: The 100,000-page fixture crawl runs at the same URLs per second (within 2%) with automatic analysis on and off.
- **SC-006**: While Crawl Analysis runs, interactions in the results table respond within about 100 ms, and cancelling stops the analysis within 1 second.
- **SC-007**: Every Links tab filter returns exactly the expected set of URLs on the seeded fixture site, and desktop and CLI exports of the same tab and filter are identical.
- **SC-008**: On the link position fixture pages, 100% of links get the expected position with the built-in rules and with the fixture's custom rules, and classifying positions adds no more than 50% to the SC-002 analysis time on the 100,000-page fixture site.
- **SC-009**: Scores for the same crawl file are identical on Windows, macOS and Linux.

## Assumptions

- **Method**: the classic damped link-weight method (as in the original PageRank paper) is used because it is well understood, explainable and matches what users know from other crawlers. Exact parity with any other tool's numbers is not a goal; the ranking is. The scale is relative to the crawl, so scores from different crawls are not comparable in absolute terms (crawl comparison, spec 012, compares ranks and moves).
- **What counts as a link**: body anchors and area links only, as recorded by spec 005.
- **Link positions after the crawl**: positions are worked out in Crawl Analysis from the stored HTML, not during fetching, so the fetch path is unchanged and rules can be edited and re-applied without recrawling. Pages crawled with raw HTML retention off get Unknown. The default weights are a reasonable starting point, not an industry standard, and weighting stays off by default so the classic score is what users see first.
- **Rendered links**: when JavaScript rendering (spec 014) adds links, the analysis uses the links the crawl stored for the page; which set is stored is defined by spec 014.
- **Thresholds**: "Crawl depth 4 or more" follows a common industry default and is fixed in this tab; configurable thresholds and priorities belong to the issue rules engine (spec 009).
- **Orphan pages**: finding URLs known from sitemaps, analytics or Search Console but not linked is spec 011 and spec 024; this feature only reports that such URLs have no inlinks and the lowest score, and offers its analysis step for their phase (FR-016).
- **Out of scope**: link score per external domain, custom damping factor or per-link weights, simulating link changes ("what if I add this link"), anchor text analysis (anchor text word clouds are spec 022; non-descriptive anchor text rules are spec 009), and charts of score distribution beyond the visualisations in spec 022.
