# Feature Specification: Custom Extraction and Custom Search

**Feature Branch**: `013-custom-extraction-search`

**Created**: 2026-10-13

**Status**: Approved 2026-10-13 (Ryware), defaults assumed for all owner decisions

**Input**: User description: "Custom extraction and custom search, in the spirit of the custom extraction and custom search features of other SEO crawlers: user-defined extractors (CSS selector, XPath, regex, JSON-LD path) that produce extra columns per page, and custom search (text or regex, contains or does not contain, in the HTML or in the visible text) with match counts. Both are configured per crawl profile, run during a crawl when configured up front, and can also be run on an existing crawl from its stored HTML without crawling again. Crawl speed is the top priority: rules must not slow the crawl noticeably, and a crawl with no rules must not change at all."

## Overview

Every site has data that no built-in column covers: the price and stock state of a product, the author and publish date of an article, the version of an analytics tag, a breadcrumb trail, the number of reviews, a "this product is discontinued" banner, or a tracking snippet that must be on every page. SEO teams find these with two tools:

- **Custom extraction**: the user describes where a value lives on a page (a CSS selector, an XPath expression, a regular expression, or a path into the page's JSON-LD) and SpiderLens stores the matching values for every page as extra columns.
- **Custom search**: the user gives a word, phrase or regular expression and says whether pages should contain it or not contain it; SpiderLens counts the matches on every page and lists the pages that meet the condition.

SpiderLens adds both, with two things the usual tools lack: rules can be **run again on a finished crawl** from its stored HTML (spec 005 keeps every page's HTML by default), so a forgotten or wrong rule does not mean crawling the site again; and every rule can be **tested on a real page** before a crawl starts.

When no rules are configured, nothing about the crawl changes and it keeps its full speed.

This feature belongs to milestone M3. It builds on crawl configuration and profiles (002), page data extraction (005), crawl storage (006), the results table (007) and crawl performance (008). Extraction from JavaScript-rendered pages (014) is a later extension described in the Assumptions.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Pull site-specific values into columns (Priority: P1)

An SEO consultant audits an online shop. Before crawling they open the crawl settings, go to Custom extraction and add three extractors: "Price" (CSS selector `.product-price`, extract text), "SKU" (XPath `//meta[@itemprop="sku"]/@content`), and "Review count" (JSON-LD path to `aggregateRating.reviewCount` of the Product item). They start the crawl. A Custom extraction tab in the results shows one column per extractor, filled in for each page as it is crawled. They sort by Review count, filter to pages where Price is empty, and export the view to CSV.

**Why this priority**: Extraction is the most used of the two features and the reason most users ask for it: it turns the crawler into a site-wide data collection tool. Without it users copy values by hand or write their own scrapers.

**Independent Test**: Crawl a local fixture site whose pages contain known values in known places (element text, attributes, inner HTML, JSON-LD properties, text matched by a pattern) with one extractor of each kind configured, then compare the stored values for every page with the expected values.

**Acceptance Scenarios**:

1. **Given** an extractor "Price" with CSS selector `.product-price` and mode "Text", **When** a page containing `<span class="product-price"> $49.00 </span>` is crawled, **Then** the stored value for Price on that page is "$49.00" (whitespace trimmed and collapsed as in spec 005).
2. **Given** an extractor with XPath `//meta[@itemprop="sku"]/@content`, **When** a page with `<meta itemprop="sku" content="AB-123">` is crawled, **Then** the stored value is "AB-123".
3. **Given** an extractor with CSS selector `a.breadcrumb` and mode "Attribute" with attribute `href`, **When** a page with three breadcrumb links is crawled, **Then** all three href values are stored in document order, the first is shown in the table column, and the match count is 3.
4. **Given** an extractor with mode "Inner HTML" and CSS selector `.specs`, **When** a page is crawled, **Then** the stored value is the markup inside the first matching element, as serialised from the parsed document.
5. **Given** an extractor with mode "Outer HTML", **When** a page is crawled, **Then** the stored value includes the matched element itself and its content.
6. **Given** a regex extractor `"gtm\.js\?id=(GTM-[A-Z0-9]+)"`, **When** a page whose source contains `gtm.js?id=GTM-K9X2` is crawled, **Then** the stored value is "GTM-K9X2" (the first capture group when the pattern has groups, otherwise the whole match).
7. **Given** a JSON-LD path extractor that selects `offers.price` of items whose `@type` is `Product`, **When** a page with a Product JSON-LD block whose offer price is 49 is crawled, **Then** the stored value is "49"; when the Product is nested inside an `@graph` array the same path still finds it.
8. **Given** an XPath extractor that uses a function, such as `count(//div[@class="review"])`, **When** a page with five reviews is crawled, **Then** the stored value is "5".
9. **Given** an extractor whose selector matches nothing on a page, **When** the page is crawled, **Then** the page is recorded as having no match for that extractor (distinct from a match whose value is empty), and the table cell is empty.
10. **Given** a crawl with extractors configured, **When** the user opens the Custom extraction tab, **Then** each extractor appears as a column named after it, rows fill in live as pages finish, and the columns can be sorted, searched, hidden, reordered and exported like any other column (spec 007).

---

### User Story 2 - Find pages that contain, or are missing, something (Priority: P1)

A marketing team needs to know which pages are missing the new consent banner script and which pages still mention a discontinued product. The user adds two searches: "Consent script" (text `consent-v2.js`, does not contain, search in HTML) and "Old product" (text `Model X100`, contains, search in visible text, ignore case). After the crawl, the Custom search tab has a filter for each search showing how many pages meet it; clicking "Consent script: does not contain" lists exactly the pages without the script.

**Why this priority**: Checking that something is present everywhere, or gone everywhere, is a daily task in migrations, tag rollouts and content clean-ups. It is as common as extraction and much simpler to use.

**Independent Test**: Crawl a fixture site where a known set of pages contains a term in the HTML only, in the visible text only, in both, or nowhere, with searches configured for each combination of target and condition; check every page's match count and which filter lists it.

**Acceptance Scenarios**:

1. **Given** a search "Consent script" for the text `consent-v2.js` with condition "does not contain" in HTML, **When** a fixture site where 7 of 20 pages lack the script is crawled, **Then** the filter "Consent script" lists exactly those 7 pages and shows the count 7.
2. **Given** a search for `Model X100` in visible text with "ignore case" on, **When** a page whose body says "model x100 is no longer sold" is crawled, **Then** the page's match count is 1 and the page is listed under the search's filter.
3. **Given** a search in visible text, **When** a page contains the term only inside a script, a style block, an HTML comment or an attribute value, **Then** the page's match count is 0, while the same search set to HTML counts those occurrences.
4. **Given** a regex search `\b(?:lorem|ipsum)\b`, **When** a page with "lorem" twice and "ipsum" once in its visible text is crawled, **Then** the match count is 3.
5. **Given** any search, **When** the user selects a page in the Custom search tab, **Then** the detail pane shows the count for every search and, for each search with matches, up to the first 10 matches with some surrounding text so the user can see where they are.
6. **Given** a search with "contains" condition, **When** the user opens its filter, **Then** pages with 1 or more matches are listed; with "does not contain", pages with 0 matches are listed. The match count is stored for every page either way, so the user can also sort by it.

---

### User Story 3 - Run rules on a crawl that has already finished (Priority: P1)

The consultant finishes a 200,000-page crawl and realises they forgot to extract the author of each article. They open Custom extraction, add an "Author" extractor and choose **Run on this crawl**. SpiderLens applies the new rule to the stored HTML of every page, without fetching anything from the site, shows progress, and fills the Author column when done. Later they fix a typo in the Price selector and run the changed rule again; the old Price values are replaced.

**Why this priority**: This is where SpiderLens is clearly better than the usual tools, which require a full recrawl to add or fix a rule. It saves hours on big sites and avoids putting load on the client's server again. It depends on stored HTML, which is on by default (spec 005).

**Independent Test**: Crawl the fixture site with no rules, then add one extractor and one search and run them on the crawl; compare the stored values with a crawl of the same site that had the rules configured from the start. Repeat on a crawl made with "Store page HTML" off.

**Acceptance Scenarios**:

1. **Given** a finished crawl with stored HTML and no rules, **When** the user adds an extractor and a search and runs them on the crawl, **Then** every HTML page gets the same values and counts as a crawl that had the rules from the start, and no request is sent to the site.
2. **Given** a rule that already has values in the crawl, **When** the user changes it and runs it on the crawl, **Then** the previous values for that rule are replaced by the new ones, and the values of other rules are not touched.
3. **Given** a crawl with 100,000 pages, **When** rules are run on it, **Then** progress (pages done, pages left, pages per second) is shown, the results table stays usable, and the user can cancel; after cancelling, the rule shows which pages have values and which were not processed.
4. **Given** a crawl made with "Store page HTML" off, **When** the user tries to run rules on it, **Then** the action is unavailable and the app explains that the crawl has no stored HTML and that a new crawl is needed.
5. **Given** a crawl where only some pages have stored HTML (for example the setting was changed between pause and resume), **When** rules are run on it, **Then** pages with stored HTML get values and the others are marked "Not available: no stored HTML".
6. **Given** a crawl file the app can only open read-only (made by a newer version, spec 006), **When** the user views it, **Then** run on this crawl is unavailable and the reason is shown.
7. **Given** the command-line interface, **When** the user runs rules from a profile or rule file on an existing crawl file, **Then** the same values are stored as from the desktop app.

---

### User Story 4 - Test a rule before crawling (Priority: P2)

Writing a selector or regex that works on every template is trial and error. While editing a rule, the user can test it on a page: either a page from the crawl that is open, or any URL they type. The test shows, for every rule, the values or match count it produces on that page, and highlights rules that match nothing. The user fixes the rule and tests again in a second, without starting a crawl.

**Why this priority**: Without testing, users discover a broken rule only after a long crawl. It ranks P2 because Story 3 already allows fixing a rule after the fact on crawls with stored HTML.

**Independent Test**: For a set of fixture pages and rules with known results, run the test from the settings and from the command line and check the reported values and counts.

**Acceptance Scenarios**:

1. **Given** the rule editor is open and a crawl is open, **When** the user picks a crawled page to test on, **Then** each rule's result on that page is shown within one second, from the stored HTML, without contacting the site.
2. **Given** the user types a URL to test on, **When** they run the test, **Then** SpiderLens fetches that one URL once with the crawl's user agent, headers and robots.txt setting, and shows each rule's result; a fetch error is shown as such.
3. **Given** a rule with invalid syntax (for example an unclosed bracket in a regex or an invalid XPath), **When** the user enters it, **Then** the error is shown next to the rule before any test or crawl, with the position of the problem where possible, and the crawl cannot start while it is invalid.
4. **Given** a rule that matches more values than the per-page limit, **When** it is tested, **Then** the test shows the limit was reached.

---

### User Story 5 - Keep rule sets with profiles and use them from the command line (Priority: P2)

An agency audits the same platform for many clients. They save their extractors and searches in a crawl profile "Shopify audit" and reuse it for every client. Every crawl file stores the rules it was run with, so a colleague who opens the file sees which rules produced the columns. A scheduled job on a server runs the same profile from the command line and exports the custom columns to CSV.

**Why this priority**: Reuse and reproducibility make the feature practical for repeat work and follow constitution principles III and V, but a single ad hoc crawl is useful without them.

**Independent Test**: Save rules in a profile, load the profile in a new crawl from the app and from the command line, and check that the stored rules and results are identical; reopen the crawl file and check the rules are shown.

**Acceptance Scenarios**:

1. **Given** rules saved in a profile, **When** the profile is loaded for a new crawl, **Then** all rules with all their options are restored exactly.
2. **Given** a finished crawl, **When** it is reopened later or on another computer, **Then** the rules that produced each column are shown with the crawl, including when each rule was last run and on how many pages.
3. **Given** the command-line interface, **When** the user starts a crawl with a profile that has rules, or adds rules from a rule file, **Then** the crawl stores the same values as from the desktop app, and the custom columns can be exported to CSV.
4. **Given** the user wants to share rules without sharing a whole profile, **When** they export the rules, **Then** a rule file is written that can be imported into another profile or passed to the command line.

---

### User Story 6 - Narrow a search to part of the page (Priority: P3)

The user wants to know which pages mention "free shipping" in the main content, not in the site-wide footer that contains it on every page. They limit the search to the element matched by a selector (for example `main` or `//article`). Only text inside that element is searched.

**Why this priority**: Useful for precise audits, but most searches work on the whole page.

**Independent Test**: Crawl fixture pages where the term appears inside and outside a known element and check counts with and without the limit.

**Acceptance Scenarios**:

1. **Given** a search for "free shipping" in visible text limited to the element `main`, **When** a page with the phrase once in `main` and once in the footer is crawled, **Then** the match count is 1.
2. **Given** a search limited to an element that does not exist on a page, **When** the page is crawled, **Then** the match count is 0 and the page is marked "Element not found" for that search, so it is not confused with a page that lacks the term.

---

### Edge Cases

- **No rules configured**: no extra work is done per page, no extra data is stored, and the Custom extraction and Custom search tabs explain how to add rules.
- **Pages the rules apply to**: rules run on the same pages that get page extraction in spec 005 (HTML responses of internal URLs, whatever their status code, including 4xx and 5xx pages with a body). Non-HTML resources, external URLs and URLs that were not fetched get no values and are not listed in the custom tabs. Redirect responses have no page and are not listed.
- **Pages with a content problem** (truncated, encoding, extraction failure, spec 005): rules run on the part that could be read, and the page keeps its content problem flag so the user knows the values may be incomplete.
- **Many matches**: each extractor stores up to 100 values per page in document order; when more exist, the count still reflects the true number of matches and the page is flagged "More matches than stored". Custom search counts every match.
- **Huge values** (inner HTML of the whole body, a regex that matches megabytes): each stored value is capped at 32,000 characters; longer values are cut and flagged as truncated.
- **Overlapping regex matches**: matches are counted the standard way for regular expressions, scanning left to right without overlap.
- **Empty matches**: a regex or selector that matches an empty string or an empty element counts as a match with an empty value; a regex that can only match the empty string (such as `a*` on text without "a") is rejected or warned about when it is entered, because it matches everywhere.
- **Case**: searches are case-sensitive unless "ignore case" is on; regex rules accept the same option. CSS selectors follow HTML matching rules (element and attribute names case-insensitive, class and id values case-sensitive).
- **Character references and whitespace**: searches in visible text and text extracts see decoded text with whitespace collapsed (spec 005, FR-009), so `&amp;` in the source matches "&" in text mode and "&amp;" in HTML mode.
- **Multiple JSON-LD blocks and `@graph`**: a JSON-LD path is applied to every parseable block on the page and to every item inside `@graph` arrays; values are collected in document order. Blocks that are not valid JSON are skipped and the page is flagged for that extractor.
- **Rules that are slow on some pages**: a rule that takes too long on one page stops for that page only, the page is marked "Rule timed out" for that rule, and the crawl continues; one slow rule never stalls the crawl.
- **Rules edited during a crawl**: rules cannot be changed while the crawl is running. While it is paused they can be changed; pages crawled after resuming use the new rules, and the app offers to run the changed rules on the pages crawled before.
- **Renaming or deleting a rule**: renaming keeps its values; deleting removes its column and stored values from the open crawl after confirmation.
- **Two rules with the same name**: not allowed; names must be unique within a profile.
- **Crawl files made before this feature**: open normally; the custom tabs say no rules were run, and rules can be run on them if they have stored HTML.
- **Pages crawled with JavaScript rendering on (spec 014)**: until the rendered extension exists, rules run on the HTML as served, and the rule settings say so.
- **Pause, stop and resume**: pages crawled before a pause keep their values; a stopped crawl keeps values for every page that finished.

## Requirements *(mandatory)*

### Functional Requirements

**Rules**

- **FR-001**: Users MUST be able to define custom extractors, each with a unique name, a kind (CSS selector, XPath, regular expression, JSON-LD path) and an expression.
- **FR-002**: CSS selector and XPath extractors MUST offer the modes Text, Inner HTML, Outer HTML and Attribute (with an attribute name); XPath extractors MUST also return the value of expressions that produce a number, text or true/false (for example `count(...)`, `string(...)`).
- **FR-003**: CSS selectors MUST support the selector syntax that browsers support for static documents (CSS Selectors Level 4, excluding pseudo-classes that depend on user interaction, layout or scripts); XPath MUST support XPath 1.0 as browsers implement it for HTML documents.
- **FR-004**: Regular expression extractors MUST run on the page's HTML source and return the first capture group of each match when the expression has groups, otherwise the whole match.
- **FR-005**: JSON-LD path extractors MUST select values from the page's parseable JSON-LD blocks (spec 005, FR-028), including items inside `@graph` arrays, and MUST be able to filter items by `@type`.
- **FR-006**: Users MUST be able to define custom searches, each with a unique name, a search term (plain text or regular expression), a condition (contains or does not contain), a target (HTML source or visible text), an ignore-case option, and optionally an element (CSS selector or XPath) that limits the search to that element's content.
- **FR-007**: Visible text for searches and Text extracts MUST be the same text spec 005 uses for word count (excluding script, style, template and comment content), with character references decoded and whitespace collapsed.
- **FR-008**: The system MUST validate every rule when it is entered or loaded, MUST show the reason and, where possible, the position of any error, and MUST NOT start a crawl or a run while a rule is invalid.
- **FR-009**: The system MUST support at least 100 extractors and 100 searches per crawl.
- **FR-010**: Rules MUST be part of the crawl configuration: saved and loaded with profiles (spec 002), stored with every crawl file together with when each rule was last run and on how many pages, and exportable to and importable from a rule file.

**Running rules**

- **FR-011**: When rules are configured before a crawl starts, the system MUST apply them to every eligible page (see Edge Cases) as part of crawling that page, from the same received content that spec 005 extracts from.
- **FR-012**: Users MUST be able to run some or all rules on an existing crawl from its stored HTML, from the desktop app and from the command line, without sending any request to the crawled site.
- **FR-013**: Running a rule on an existing crawl MUST replace that rule's previous values for every processed page and MUST NOT change any other data.
- **FR-014**: A run on an existing crawl MUST show progress, MUST be cancellable, and after cancelling MUST record which pages were processed; pages without stored HTML MUST be recorded as "Not available: no stored HTML".
- **FR-015**: Applying rules to one page MUST produce the same stored values whether done during the crawl or afterwards from stored HTML, and whether from the desktop app or the command line.
- **FR-016**: Rules MUST NOT be changeable while a crawl is running; while it is paused, changed rules MUST apply to pages crawled after resuming, and the app MUST offer to run changed rules on pages crawled before.
- **FR-017**: A rule that fails or exceeds its time budget on one page MUST be recorded for that page and rule ("Rule timed out" or the error) and MUST NOT stop the crawl, the run, or other rules on the same page.
- **FR-018**: Users MUST be able to test all rules on one page, either a page of the open crawl (from stored HTML, no request) or a typed URL (one request using the crawl's user agent, headers and robots.txt setting), and see each rule's values or match count.

**Stored results**

- **FR-019**: For every eligible page and extractor, the system MUST store the matched values in document order (up to 100 per page, each up to 32,000 characters), the true number of matches, and flags for "no match", "more matches than stored" and "value truncated".
- **FR-020**: For every eligible page and search, the system MUST store the number of matches, whether the page meets the search's condition, and "Element not found" when an element limit is set and absent.
- **FR-021**: The system MUST keep up to the first 10 matches of each search per page with a short piece of surrounding text, for display in the detail pane.

**Results interface and export**

- **FR-022**: The results MUST include a Custom extraction tab with one column per extractor (first value and, in a second column that can be shown, the match count) and a Custom search tab with one column per search (match count), following the table behaviour of spec 007 (live updates, sort, search, column choice, copy, CSV export).
- **FR-023**: The Custom search tab MUST offer one filter per search, labelled with the search name and condition and showing how many pages meet it; the Custom extraction tab MUST offer, per extractor, the filters "Has value", "No match" and "More matches than stored".
- **FR-024**: The detail pane MUST show, for the selected page, every extractor's values (all stored values, not only the first) and every search's count with its stored matches in context.
- **FR-025**: CSV export MUST be able to write either the first value per extractor or all values of an extractor joined by a chosen separator.
- **FR-026**: The rule settings MUST show, for each rule, how many pages it matched in the open crawl and when it was last run.

**Speed and safety**

- **FR-027**: When no rules are configured, the crawl MUST do no additional work per page and store no additional data.
- **FR-028**: Rules MUST reuse the page's single parse from spec 005; the system MUST NOT parse a page's HTML a second time to evaluate rules during a crawl.
- **FR-029**: Rule evaluation MUST NOT run page scripts or fetch any resource.
- **FR-030**: Rule evaluation MUST be deterministic: the same content and rules always produce the same stored values.

### Key Entities

- **Extractor**: A user-defined rule that returns values: name, kind (CSS selector, XPath, regex, JSON-LD path), expression, mode (Text, Inner HTML, Outer HTML, Attribute, Function value), attribute name, position in the rule list.
- **Search**: A user-defined rule that counts matches: name, term, term kind (text or regex), condition (contains or does not contain), target (HTML or visible text), ignore case, optional element limit.
- **Rule Set**: The ordered extractors and searches of a profile or crawl; exportable as a rule file.
- **Rule Run**: One application of rules to a crawl: which rules and their exact definitions, when, during the crawl or afterwards, pages processed, pages not available, cancelled or complete.
- **Extracted Value**: One value of one extractor on one page: order, text, truncated flag; plus per page and extractor the match count and "no match" and "more than stored" flags.
- **Search Result**: Per page and search: match count, meets condition, element not found, and up to 10 matches with context.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: With no rules configured, the 100,000-page benchmark from spec 008 on a build with this feature stays within 2% of the throughput and within 5% of the peak memory of the build without it.
- **SC-002**: With a typical rule set of 5 CSS selector, 5 XPath, 5 regex and 5 JSON-LD path extractors plus 10 searches (half in HTML, half in visible text), the same benchmark keeps at least 85% of the throughput with no rules, and memory stays flat as the crawl grows.
- **SC-003**: Running the typical rule set of SC-002 on an existing 100,000-page crawl with stored HTML finishes in under 2 minutes on the reference machine (spec 008), and the results table stays responsive during the run.
- **SC-004**: On the custom rules fixture suite, 100% of expected values, match counts and flags are stored correctly, both during a crawl and when run afterwards on stored HTML, from the desktop app and from the command line.
- **SC-005**: On a fixture set of at least 50 pages, CSS selector and XPath extractors return the same values as a reference browser's own selector and XPath evaluation on the same HTML for at least 98% of page and rule pairs.
- **SC-006**: Testing all rules of the typical set on a page of the open crawl shows results within 1 second.
- **SC-007**: A rule constructed to be pathologically slow on one fixture page is stopped on that page within its time budget, the page is marked "Rule timed out", and the crawl's overall throughput drops by no more than the time budget for that one page.
- **SC-008**: A user who has never used the feature can add an extractor, test it on a page and see its column filled after a crawl in under 3 minutes in usability checks, without reading documentation.

## Assumptions

- **Scope**: Milestone M3. Spider and List modes, desktop app and command line.
- **Pages covered** are the pages spec 005 extracts from; extending rules to external pages or non-HTML files (for example PDFs) is out of scope.
- **Rendered pages**: extraction and search on JavaScript-rendered HTML (spec 014) is a later extension: each rule will get a "raw or rendered" choice once rendering is merged. Until then rules always use the HTML as served.
- **Running afterwards needs stored HTML**: "Store page HTML" is on by default (spec 005). Crawls made with it off can only be rerun by crawling again; this is explained in the app rather than worked around.
- **Limits** (100 extractors, 100 searches, 100 values per page, 32,000 characters per value, 10 search matches kept per page) are starting values that the owner may change during `/speckit-clarify`; they keep crawl files and the table predictable on very large crawls.
- **Per-rule time budget** is set in planning from benchmark results; its purpose is to protect the crawl from pathological expressions, not to limit normal rules.
- **Value types**: values are stored as text; numeric sorting of extracted columns happens when every value in the column is a number. Typed columns (dates, currency) are out of scope.
- **Out of scope**: a visual "point and click" selector picker; extraction from screenshots; extraction by AI prompts (spec 016 covers AI prompts per page); custom JavaScript snippets run on each page (gap report, after spec 014); turning custom search results into issues with priorities (spec 009 may build on the stored results later); comparing custom values between crawls (spec 012 may build on them later).
- **Storage**: new crawl file data follows the storage rules of spec 006 (additive change, older files keep opening, files without custom data open normally).
- **Security**: rules are user-entered expressions evaluated locally on the user's own crawl data; they never execute code and never reach the network, except the single user-requested fetch when testing on a typed URL.

## Owner decisions (defaults approved 2026-10-13)

1. **Rerunning replaces values**: running a changed rule replaces its old values; no history of earlier runs is kept (a new crawl or the comparison feature covers history).
2. **Rules locked while running**: rules can be edited only while a crawl is paused or finished.
3. **Limits** as listed in the Assumptions.
4. **JSON-LD path** is included in this spec as a fourth extractor kind, in addition to CSS, XPath and regex.
