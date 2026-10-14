# Feature Specification: Near Duplicate Content

**Feature Branch**: `020-near-duplicate-content`

**Created**: 2026-10-13

**Status**: Draft

**Input**: User description: "Near duplicate content: detect pages whose main content is nearly identical, with a configurable similarity threshold and main-content extraction that ignores navigation and other boilerplate (exact duplicates already exist through the text fingerprint of spec 005). Show clusters with similarity percentages and a side-by-side view, and feed a rule into the spec 009 issue catalogue. Local only, no external models. A 100,000-page crawl must be analysed within a minute or two on the reference 16 GB laptop. Crawl speed is the top priority: the analysis never slows fetching."

## Overview

Search engines pick one page out of a group of pages that say almost the same thing and filter the rest out of results. Sites create such groups by accident all the time: product variants that differ only by colour, location pages with the city name swapped, paginated or filtered listings, printer versions, old copies left after a migration. Spec 005 already stores a fingerprint of each page's visible text, so pages that are exactly identical can be found (spec 009 has the "Exact duplicate content" rule). Pages that differ by one sentence, a date or a sidebar are not caught by an exact fingerprint, and those are most of the real cases.

This feature adds **near duplicate analysis**. For every eligible page SpiderLens takes the page's **main content** (the text a reader came for, without the navigation, header, footer and sidebars every page shares), and finds the other pages whose main content is at least a set percentage similar (90% by default). Pages that are near duplicates of each other form a **cluster**. The user sees which pages are affected, how similar they are, the clusters, and a side-by-side comparison of any two pages with the differences marked. The issue feeds the spec 009 catalogue as a rule, so it shows in the Issues area, the command line and later the report (010).

Everything runs on the user's machine with a deterministic method; no model is downloaded and no service is called. The analysis runs after the crawl, not while pages are being fetched, so crawl speed is not affected.

This feature belongs to milestone M3. It builds on page extraction (005), storage (006), the results table (007) and the issue rules engine (009).

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Find pages that are nearly the same (Priority: P1)

An SEO consultant crawls an online shop. When the crawl finishes, SpiderLens analyses the content of every page and the consultant opens the Content tab with the "Near duplicates" filter. It lists every page that has at least one other page with 90% or more similar main content, with the closest match's similarity and the number of near duplicates each page has. The consultant sees that 40 product pages for the same shoe in different colours are 97% identical, and that 12 city landing pages differ only by the city name.

**Why this priority**: Finding the affected pages is the whole value. Clusters, the comparison view and the rule all build on this list.

**Independent Test**: Crawl a local fixture site with known groups of pages: identical main content with different navigation, pages with one changed sentence, pages with one changed paragraph, pages with known measured similarity from 70% to 100%, and unrelated pages. Check that the Near duplicates filter lists exactly the pages that have a partner at or above the threshold, and that each page's reported closest similarity is within the accuracy stated in SC-003.

**Acceptance Scenarios**:

1. **Given** two pages whose main content is the same 600 words except for one changed sentence, **When** the crawl finishes and analysis completes, **Then** both pages are listed under Near duplicates, each shows the other as its closest match, and the similarity shown is between 90% and 99%.
2. **Given** two pages with identical main content but completely different navigation menus, sidebars and footers, **When** analysis completes, **Then** they are listed as near duplicates with 100% similarity and marked as exact content duplicates.
3. **Given** two pages that share the same 300-word navigation, header and footer but have unrelated main content, **When** analysis completes, **Then** neither is listed as a near duplicate of the other.
4. **Given** a page whose closest match is 85% similar and the threshold is 90%, **When** analysis completes, **Then** the page is not listed under Near duplicates, and its closest similarity is still shown in the Content tab's All filter if it is at least the reporting floor (FR-011).
5. **Given** the user lowers the threshold from 90% to 80% in the crawl settings, **When** they confirm, **Then** the analysis is updated from the data already in the crawl file without recrawling, and the 85% page from scenario 4 now appears under Near duplicates.
6. **Given** a crawl is running, **When** the user opens the Content tab, **Then** the near duplicate columns show "Not analysed yet" and the filter explains that analysis runs when the crawl finishes or is paused, and no analysis work slows the running crawl.
7. **Given** the same crawl file, **When** the analysis is run from the command-line interface, **Then** it produces the same pages, clusters and similarities as the desktop app.

---

### User Story 2 - See the groups and compare two pages side by side (Priority: P1)

The consultant wants to decide what to do with each group: merge the pages, canonicalise them to one, or make each page distinct. They open the Clusters view, which lists every cluster with its size, the highest and lowest similarity inside it and a representative URL. Opening a cluster lists its pages with their similarity to the representative. Selecting two pages opens a side-by-side comparison of their main content with the shared text in plain and the differing words and sentences highlighted, so the consultant sees in seconds that the only difference is "Red" versus "Blue".

**Why this priority**: A list of affected URLs without the groups and the differences forces the user to open pages in a browser one by one. Seeing the difference is what turns the finding into a fix.

**Independent Test**: On the fixture site, open each known group and check that the cluster contains exactly the expected pages, that the comparison of two pages highlights exactly the words the fixture changed, and that the view works the same in light and dark appearance.

**Acceptance Scenarios**:

1. **Given** a group of 12 location pages that are each at least 90% similar to the others, **When** the user opens the Clusters view, **Then** one cluster of 12 pages is shown with its size and similarity range, and the clusters are sorted largest first.
2. **Given** the user selects a page in the results table, **When** they open the Duplicates section of the detail pane, **Then** it lists every near duplicate of that page (up to the limit in FR-013) with its similarity, sorted highest first, and the cluster it belongs to.
3. **Given** the user picks two pages from a cluster or from the detail pane, **When** they open the comparison, **Then** both pages' main content is shown side by side, aligned by paragraph, with added, removed and changed text highlighted, and the similarity percentage at the top.
4. **Given** the comparison is open, **When** the user switches to show the full page text, **Then** the text the main content rules removed (navigation, footer and so on) is shown too, visibly marked as not compared, so the user can check that the main content was chosen correctly.
5. **Given** a crawl file saved without stored HTML (spec 005, FR-031), **When** the user opens the comparison, **Then** it explains that the page text was not kept in this crawl and still shows the similarity and the cluster.
6. **Given** a cluster, **When** the user exports it, **Then** a CSV file lists each pair or member with both URLs, the similarity, the cluster number and whether the pair is an exact content duplicate.

---

### User Story 3 - Near duplicates appear as an issue (Priority: P1)

The consultant uses the Issues area (spec 009) as their checklist. After the crawl, "Near duplicate content" appears in the Content category with its count, priority, explanation and how to fix it, alongside "Exact duplicate content". Selecting it lists the affected pages with the evidence: the closest match and its similarity, and the number of near duplicates. In a scheduled or scripted run, the command line reports the rule like any other and can fail the run on it.

**Why this priority**: The Issues area is where users look for problems. A finding that exists only in a separate tab is easy to miss and cannot gate a build.

**Independent Test**: Run the issue evaluation on the fixture crawl in the desktop app and from the command line, and check that the rule lists exactly the pages of User Story 1 with the expected evidence, and that the command line's exit code follows the fail-on setting of spec 009.

**Acceptance Scenarios**:

1. **Given** a crawl with near duplicates, **When** the user opens the Issues area, **Then** "Near duplicate content" shows in the Content category with the count of affected pages and its default priority, and its explanation names the threshold used.
2. **Given** the user turns the rule off or changes its priority in Settings › Issues, **When** they return to the Issues area, **Then** the rule follows the setting, as for every spec 009 rule.
3. **Given** a crawl that is still running, **When** the user opens the Issues area, **Then** the rule shows as waiting for analysis, never as zero.
4. **Given** pages that are exact duplicates of each other, **When** both rules run, **Then** those pages are listed under "Exact duplicate content" and also under "Near duplicate content" with 100%, and the evidence says the content is identical.

---

### User Story 4 - Control what counts as main content and which pages are compared (Priority: P2)

Some sites mark up their pages in ways the default main content rules get wrong: a site without `<main>` or `<article>` elements, a big "related products" block that should be ignored, a cookie notice inside the body. The consultant opens the crawl settings, section Content, and adds elements to ignore (for example the element with class `related-products`) or names the element that holds the main content (for example `#content`). They can also choose whether non-indexable pages are compared, the threshold, and the minimum amount of main content a page needs to be compared.

**Why this priority**: The defaults work for most sites, but when they do not, the results are wrong in a way the user cannot fix without these settings. Lower than P1 because the defaults cover the common case.

**Independent Test**: On a fixture page with a large "related products" block that makes unrelated pages look similar, run analysis with the defaults, then with that block excluded, and check that the pair is reported before and not after. Check that each setting is stored with the crawl and survives reopening the file.

**Acceptance Scenarios**:

1. **Given** two pages whose main content differs but which share a 500-word "related products" block inside the main content area, **When** the user excludes that block and re-runs the analysis, **Then** the pair is no longer reported, without recrawling, if the crawl kept its HTML.
2. **Given** the user names a content element that does not exist on some pages, **When** analysis runs, **Then** those pages fall back to the default main content rules, and the Content tab marks them so the user can see the setting did not match.
3. **Given** the default setting to compare indexable pages only, **When** a page is noindex or canonicalised to another URL, **Then** it is left out of the comparison and the Content tab says why; **When** the user turns the setting off, **Then** such pages are compared too.
4. **Given** a page whose main content has fewer words than the minimum (default 50 words), **When** analysis runs, **Then** it is not compared, and the Content tab shows it as "too little content to compare" (thin content is the separate spec 009 rule).
5. **Given** the user changed any of these settings, **When** they save the crawl, reopen it later or open it from the command line, **Then** the same settings and the same results are used.

---

### User Story 5 - Older and very large crawls (Priority: P2)

The consultant opens a crawl saved with an earlier version of SpiderLens, and also works with a client site of a million URLs. For the older crawl, near duplicate analysis runs on open if the crawl kept its HTML, and otherwise the feature says it is unavailable for that file and why. For the large crawl, analysis finishes in a few minutes, memory stays bounded, and a cluster of thousands of nearly identical template pages does not stall the analysis or the interface.

**Why this priority**: Crawl files must keep working across versions (constitution, Product Constraints) and the product must scale by design (principle VI), but most users will first meet the feature on a new crawl of a normal size.

**Independent Test**: Open a crawl file from before this feature with and without stored HTML and check both outcomes. Run analysis on the 100,000-page benchmark crawl and on a generated one-million-page crawl that contains a 20,000-page group of near-identical pages, and measure time and peak memory against SC-001, SC-002 and SC-005.

**Acceptance Scenarios**:

1. **Given** a crawl file from before this feature that kept its HTML, **When** it is opened in a version with this feature, **Then** the user is offered to analyse it, and after analysis the results are the same as for a new crawl of the same pages, subject to the file's read-only rules from spec 006 (a newer-format file opened by an older app stays view-only).
2. **Given** a crawl file without stored HTML and without the data this feature records during a crawl, **When** it is opened, **Then** the Content tab and the rule show near duplicate analysis as unavailable for this file with the reason, never as zero.
3. **Given** a group of 20,000 pages that are all at least 95% similar, **When** analysis runs, **Then** it completes within the time in SC-005, the group forms one cluster, each page lists its closest matches up to the limit, and the cluster view opens without freezing the interface.
4. **Given** analysis is running, **When** the user cancels it or closes the crawl, **Then** it stops promptly, nothing partial is shown as complete, and the next open offers to run it again.

---

### Edge Cases

- **Pages without main content markup**: when a page has no `<main>`, `<article>` or a matching user setting, the main content is the body without navigation, header, footer, aside and elements marked with the equivalent accessibility roles. This default is documented in the settings.
- **Very short pages**: pages below the minimum word count are not compared (US4.4), because a few shared words make short pages look similar or different at random.
- **Exact duplicates**: identical main content is 100% and also flagged as exact. The spec 005 fingerprint covers the whole visible text, so two pages can be 100% near duplicates on main content while their full-page fingerprints differ (different navigation); both facts are shown.
- **Huge clusters**: a site with thousands of near-identical pages (faceted listings, product variants) forms large clusters. The number of matches stored per page is limited (FR-013) so time and file size stay bounded; the page's total number of near duplicates is still counted, and the cluster lists every member.
- **Chains**: A is 92% similar to B, B is 92% similar to C, but A is only 85% similar to C. A, B and C are one cluster (FR-012), and the cluster shows its lowest and highest pairwise similarity so the user can see it is a chain.
- **Languages without spaces** (for example Chinese, Japanese, Thai): word-based comparison is weak there. The comparison uses runs of characters for such text so similarity remains meaningful; accuracy targets in SC-003 apply to space-separated languages.
- **Translations**: a page and its translation are not near duplicates by this method (different words). Cross-language duplicates are out of scope; hreflang checks belong to spec 009.
- **Pages with different language versions of the same template**: shared boilerplate in each language is removed by the main content rules, so only the content is compared.
- **Rendered pages**: when JavaScript rendering (spec 014) is on, the comparison uses the rendered main content, and the setting is stored with the analysis.
- **Partially extracted pages** (spec 005 FR-008): compared on the text that was extracted, and marked as partial in the evidence.
- **Paused and stopped crawls**: analysis runs when a crawl completes, is stopped, or is paused, and is marked provisional for a paused crawl; resuming the crawl clears it and it runs again when the crawl next ends.
- **Changed settings on a crawl that did not keep HTML**: settings that only change the threshold, the reporting floor or which pages are compared apply without HTML. Settings that change what main content is (US4.1) need the HTML; without it the app says the change applies to the next crawl.
- **Resource limits**: if the machine is low on memory or disk during analysis, it stops with a clear message, the crawl data is untouched, and nothing partial is shown as complete.

## Requirements *(mandatory)*

### Functional Requirements

**Which pages and which content**

- **FR-001**: The system MUST compare internal HTML pages that returned 200 (the set spec 007 FR-010 and spec 009 call "pages"), and by default only those that are indexable (not noindex, not canonicalised to another URL). A setting MUST allow including non-indexable pages.
- **FR-002**: The system MUST compare pages by their main content: by default the text inside `<main>` or, failing that, `<article>`; when neither exists, the body text without navigation, header, footer, aside, forms used for site search, and elements with the matching accessibility roles. Script, style, template and hidden content is excluded as for the spec 005 word count.
- **FR-003**: The user MUST be able to name an element that holds the main content and elements to exclude, using the same kind of element selectors the user would write in a browser's developer tools; pages where the named content element is missing MUST fall back to FR-002 and be marked.
- **FR-004**: Pages whose main content has fewer words than a minimum (default 50, user setting) MUST be left out of the comparison and marked as such.
- **FR-005**: For text in languages written without spaces, the comparison MUST use character sequences instead of words.
- **FR-006**: When JavaScript rendering (spec 014) is on, the comparison MUST use the rendered page's main content.

**Similarity and analysis**

- **FR-007**: Similarity between two pages MUST be the share of their main content's overlapping word sequences (short runs of consecutive words), reported as a whole percentage from 0% to 100%, so moved, added and removed passages all reduce it and the same pair always gets the same value.
- **FR-008**: The system MUST use a deterministic local method: no model download, no network call, the same input and settings MUST give the same result on every platform.
- **FR-009**: The analysis MUST NOT run in the fetching path. Anything recorded per page during the crawl to make the analysis possible MUST keep crawl throughput within the limit in SC-004. The analysis itself MUST run when the crawl completes, is stopped or paused, on demand, and when the user changes a setting that affects it.
- **FR-010**: The near duplicate threshold MUST default to 90% and MUST be a user setting from 50% to 100%.
- **FR-011**: The system MUST record, for every compared page, its closest match and that similarity when it is at least a reporting floor (default 50%, or the threshold if lower), and the number of other pages at or above the threshold.
- **FR-012**: Pages connected by near duplicate pairs at or above the threshold MUST form one cluster. Each cluster MUST have a stable number within the analysis, a representative page (the page with the most inlinks, then the shortest URL), its size and its lowest and highest pairwise similarity among the pairs found.
- **FR-013**: For each page, the system MUST keep at least its 50 most similar near duplicates with their similarity; when a page has more, the total count MUST still be correct and the user MUST be told the list is limited.
- **FR-014**: Pairs whose main content is identical MUST be marked as exact content duplicates.
- **FR-015**: Results MUST be stored in the crawl file with the settings and the analysis version that produced them, so reopening the file shows them without re-running, and a version of SpiderLens with a changed method re-runs the analysis on open and says so.
- **FR-016**: The analysis MUST be cancellable, MUST show progress, and MUST NOT block the interface.

**Results, comparison and export**

- **FR-017**: The results table MUST have a Content tab with default columns Address, Main content words, Closest similarity, Closest match, Near duplicates, Cluster, Exact duplicate; and filters All, Near duplicates, Exact duplicates (spec 009's exact rule definition), Not compared (with the reason), Low content (below the minimum). Filter counts follow spec 007's rules.
- **FR-018**: The detail pane MUST have a Duplicates section listing the selected page's near duplicates with similarity, sorted highest first, and its cluster.
- **FR-019**: A Clusters view MUST list clusters with size, similarity range and representative page, sorted by size, and open each to its member pages.
- **FR-020**: A comparison view MUST show the main content of two pages side by side, aligned by paragraph, with the differing text highlighted and the similarity shown; it MUST offer showing the full page text with the excluded parts marked; it MUST work from stored HTML or stored main content and explain when neither was kept.
- **FR-021**: The user MUST be able to export the current Content tab view (spec 007 export rules), the list of near duplicate pairs and a single cluster to CSV, with both URLs, similarity, cluster number and exact flag.
- **FR-022**: All new screens MUST follow the design system (spec 017) and appearance settings (spec 018), be fully usable from the keyboard and meet the contrast rules of spec 017, and have automated UI tests.

**Issues and command line**

- **FR-023**: The feature MUST add a rule "Near duplicate content" to the spec 009 catalogue in the Content category, default priority Medium, applying to the pages of FR-001, threshold taken from FR-010, site-wide kind, with evidence: closest match URL, similarity, number of near duplicates, cluster number. Until analysis has run the rule MUST show as waiting or unavailable, never as zero.
- **FR-024**: The command-line interface MUST be able to run the analysis on a crawl file with optional overrides for the threshold, the minimum words and the indexable-only setting, and output the pairs, the clusters or the per-page summary as CSV or JSON. Its results MUST equal the desktop app's for the same file and settings (constitution principle III).
- **FR-025**: The analysis settings (threshold, reporting floor, minimum words, indexable only, content and excluded elements) MUST be part of the crawl configuration (spec 002), stored with the crawl and in saved profiles.

**Compatibility**

- **FR-026**: Crawl files from before this feature MUST open as before; if they kept HTML the user MUST be offered the analysis, otherwise the feature MUST show as unavailable with the reason. Data added by this feature MUST follow the spec 006 rules for additive format changes.

### Key Entities

- **Main content**: the text of a page that is compared, chosen by the default rules or the user's content and exclusion settings, with its word count and how it was chosen (main element, article, fallback, user element).
- **Content signature**: a compact, fixed-size summary of a page's main content from which similarity can be estimated without the full text; recorded once per page.
- **Near duplicate pair**: two pages, their similarity, and whether their content is identical.
- **Cluster**: pages connected by pairs at or above the threshold, with number, size, representative and similarity range.
- **Analysis run**: when it ran, the settings and method version used, how many pages were compared, left out (with reasons) and found in clusters, and whether it is final or provisional.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: On the reference laptop (16 GB, as in spec 008), analysis of the 100,000-page benchmark crawl completes within 60 seconds after the crawl ends, from the data in the crawl file, without stored HTML being re-read; peak extra memory for the analysis stays below 1 GB.
- **SC-002**: Analysis of a one-million-page crawl completes within 10 minutes on the reference laptop, with peak extra memory below 2 GB, and memory does not grow with the number of near duplicate pairs beyond the per-page limit of FR-013.
- **SC-003**: On a fixture set of page pairs with known similarity (space-separated languages), the reported similarity is within 3 percentage points of the true value for at least 95% of pairs, and at least 99% of pairs whose true similarity is 5 points or more above the threshold are found.
- **SC-004**: Crawl throughput on the spec 008 100,000-page benchmark with this feature is at least 98% of the throughput without it, and no analysis work runs while pages are being fetched.
- **SC-005**: A crawl with a 20,000-page group of near-identical pages within 100,000 pages is analysed within the time of SC-001, and the Clusters view and the comparison view open in under 1 second for any cluster.
- **SC-006**: Changing the threshold, the reporting floor or the indexable-only setting updates the results within 10 seconds for 100,000 pages, without recrawling.
- **SC-007**: Re-running the analysis on content settings from stored HTML (US4.1) completes within 3 minutes for 100,000 pages on the reference laptop.
- **SC-008**: The desktop app and the command line produce identical pairs, clusters and similarities for the same crawl file and settings on Windows, macOS and Linux.
- **SC-009**: The size added to a crawl file by this feature stays below 1 KB per compared page plus the stored pairs.

## Assumptions

- The default threshold of 90% and the default exclusion of navigation, header and footer match what SEO users expect from established desktop crawlers, so results are familiar.
- The similarity percentage estimates overlap of the page text, not meaning. Two pages that say the same thing in different words are not near duplicates by this method. Semantic similarity (with an embedding model) is out of scope for this spec; if added later it must run locally and be opt-in (constitution principle II).
- The method is chosen in planning. The working assumption, to be justified in research.md, is a fixed-size signature per page (MinHash over word sequences) recorded during extraction, plus a banded index to find candidate pairs without comparing every page with every other, because comparing all pairs of 100,000 pages (5 billion pairs) cannot meet SC-001.
- Thin content (spec 009 rule, below 200 words) and the minimum words for comparison (50) are separate settings with separate purposes.
- Clustering by connected pairs (FR-012) is chosen over stricter grouping because it is predictable and explainable; the similarity range shows chains.
- The rule's default priority is Medium, the same as "Exact duplicate content" in spec 009.

## Dependencies

- Spec 005 (page extraction): visible text, word counts, the text fingerprint, stored HTML.
- Spec 006 (storage): additive format changes as a migration step numbered at merge time, read-only rules for newer files.
- Spec 007 (results table): tab, filter, detail pane and export rules.
- Spec 009 (issue rules engine): rule catalogue shape, Settings › Issues, CLI exit codes.
- Spec 014 (JavaScript rendering): rendered main content when rendering is on.
- Spec 017 and 018 (design system and appearance).

## Out of Scope

- Semantic similarity with embeddings or any language model.
- Near duplicate detection across different crawls or against external sites (crawl comparison is spec 012).
- Similarity of titles, meta descriptions or headings beyond the exact duplicate filters of spec 007.
- Cross-language duplicate detection.
- Automatic recommendations of which page to keep (the representative page is a display choice, not advice).
- Per-URL ignore lists (out of scope in spec 009 too).
