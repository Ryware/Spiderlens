# Feature Specification: Content Checks (Segments, SERP Preview, PDF, Spelling, AMP)

**Feature Branch**: `027-content-checks`

**Created**: 2026-10-13

**Status**: Draft

**Input**: Owner approval of the feature-gap report (2026-10-13, rows 19, 30, 31, 32, 33 and 36): "the remaining content checks: spelling and grammar on visible text with offline dictionaries per language including Hebrew, PDF crawling, AMP validation, SERP snippet preview with pixel widths, and user-defined URL segments, as one spec with separable parts so any part can be dropped. Desktop only, no servers, crawl speed unchanged on the default path."

## How this spec is split

This spec holds five independent parts. Each part has its own user stories, requirements and success criteria, its own code pull request, and can be dropped or postponed without touching the others. The only shared rules are in "Shared requirements".

| Part | What the user gets | Default | Suggested order | Gap report priority |
|---|---|---|---|---|
| **A. SERP preview** | Title and description pixel widths, truncation preview, an editable snippet, a SERP list mode | On (no fetch, no crawl cost) | 1 | P2 (pixel width), P3 (SERP mode) |
| **B. Segments** | User-defined URL groups shown in every tab, filter and count | On when segments are defined | 2 | P2 |
| **C. PDF crawling** | PDF title, properties, text size and links, PDFs as link sources | Off | 3 | P3 |
| **D. Spelling and grammar** | Spelling errors per page in many languages including Hebrew, grammar for English | Off | 4 | P3 |
| **E. AMP** | AMP page discovery and a core set of AMP validity checks | Off | 5 | P3 (earlier note: AMP is largely obsolete) |

Parts A and B are cheap and widely used. Parts C, D and E are off by default because they cost bandwidth or processor time, and their cost is paid only by users who turn them on.

## User Scenarios & Testing *(mandatory)*

### Part A: SERP preview

#### User Story A1 - See whether titles and descriptions fit in search results (Priority: P1)

An SEO consultant reviews page titles. Character counts are a poor proxy: "WWWW" is far wider than "iiii". In the Page titles and Meta description tabs the user sees, next to the character length, the width in pixels as a search result would draw it, and whether it will be cut off on desktop and on mobile. Filters list titles and descriptions that are too wide, too narrow, or missing.

**Why this priority**: Pixel width is what decides truncation in search results, every competing crawler reports it, and the data (the title and description text) is already stored, so it costs nothing at crawl time.

**Independent Test**: Open a crawl of a fixture site whose titles and descriptions have known reference widths and compare the stored or shown widths and truncation flags with the expected values.

**Acceptance Scenarios**:

1. **Given** a page whose title is drawn 640 pixels wide in the reference search result font, **When** the user opens the Page titles tab, **Then** the row shows a title width of 640 pixels (within the tolerance in SC-A1) and a "truncated on desktop" mark against the default desktop limit of 600 pixels.
2. **Given** a description that fits on desktop but not on mobile, **When** the user looks at the Meta description tab, **Then** the row shows separate desktop and mobile truncation marks.
3. **Given** a crawl file saved before this feature, **When** it is opened, **Then** pixel widths are shown for every page with a title or description, without recrawling.
4. **Given** the filter "Title over pixel width", **When** the user selects it, **Then** it lists the same URLs that the matching issue rule lists (spec 009 FR-005).

#### User Story A2 - Preview and edit a page's search snippet (Priority: P2)

Selecting a page shows a "Search preview" section in the detail pane: the page's title, URL path (as breadcrumbs) and description drawn the way a search result shows them, truncated where a search engine would truncate them, on a desktop and a mobile width. The user can type a new title or description into the preview to try alternatives and immediately see the new width and truncation. Edits are kept as notes on that URL in the crawl file and can be exported, so a consultant can hand a list of proposed titles to a client.

**Why this priority**: The preview is the most visual way to explain truncation to a client, and editing in place replaces a separate tool. It depends on A1's widths.

**Independent Test**: Select a fixture page, check the drawn preview against the expected truncation point, type a new title, and check the width and the saved proposal.

**Acceptance Scenarios**:

1. **Given** a selected page with a 75-character title, **When** the user opens Search preview, **Then** the title is shown cut with an ellipsis at the point where the desktop limit is reached, and the mobile preview cuts at the mobile limit.
2. **Given** the user types a new title in the preview, **When** they stop typing, **Then** the width, the character count and the truncation update within 100 ms and the proposal is saved against the URL.
3. **Given** proposals exist for 20 URLs, **When** the user exports "Proposed titles and descriptions", **Then** a CSV lists each URL with the current and proposed title and description and their widths.
4. **Given** a page with no meta description, **When** the preview is shown, **Then** it says that search engines will choose the snippet text themselves, instead of drawing an empty description.

#### User Story A3 - Check titles and descriptions from a list without crawling (Priority: P3)

A copywriter has a spreadsheet of draft titles and descriptions for pages that do not exist yet. In SERP list mode the user pastes or loads a list of rows (URL, title, description) and gets the same widths, truncation marks and previews, with no network access at all. The same is available from the command line.

**Why this priority**: Useful but niche, and it reuses A1 and A2 entirely.

**Independent Test**: Load a three-column CSV with known widths and compare the output in the app and from the command line.

**Acceptance Scenarios**:

1. **Given** a CSV with 500 rows of URL, title and description, **When** it is loaded in SERP list mode, **Then** all 500 rows show widths and truncation within 2 seconds and no network request is made.
2. **Given** the same CSV, **When** the user runs the command line SERP check, **Then** it prints or writes the same widths and marks as the app.

### Part B: Segments

#### User Story B1 - Group URLs into segments and see every tab by segment (Priority: P1)

A site has a blog, a shop and a help centre. The user defines segments such as "Blog: URL contains /blog/", "Products: URL matches /p/[0-9]+" and "Help: host is help.example.com". Every results tab gets a Segment column and a segment filter in the toolbar, the sidebar counts can be shown for one segment, and issue counts (spec 009) can be broken down by segment. URLs that match no segment are in "No segment".

**Why this priority**: Segments turn one large crawl into readable sections and are used by most audits of sites over a few thousand pages. They cost nothing during the crawl because they are evaluated on stored URLs.

**Independent Test**: Define three segments on a fixture crawl, then check the segment of every URL and the counts per segment in three tabs and the issues list.

**Acceptance Scenarios**:

1. **Given** segments "Blog" (URL contains `/blog/`) and "Shop" (URL contains `/shop/`), **When** the user opens the Internal tab, **Then** each row shows its segment and the segment filter offers Blog, Shop and No segment with counts.
2. **Given** a URL that matches two segments, **When** segments are evaluated, **Then** it belongs to the first matching segment in the user's order, and moving that segment down changes the result.
3. **Given** a finished crawl, **When** the user adds or edits a segment, **Then** every tab and count reflects the change within 5 seconds for a 100,000-URL crawl, without recrawling.
4. **Given** the segment filter is set to "Shop", **When** the user switches tabs, **Then** the filter stays on "Shop" until the user clears it.
5. **Given** the issues list (spec 009) is open, **When** the user chooses "by segment", **Then** each issue shows its affected URL count per segment.

#### User Story B2 - Define segments by more than the URL (Priority: P2)

Some groups are not visible in the URL: "noindex pages", "pages with over 1,000 words", "pages in a given language". A segment condition can test URL parts (host, path, query) with contains, starts with, ends with, equals or a pattern, and can also test a short list of stored page facts: status code class, content type, indexability, language, crawl depth and word count. Conditions combine with "all of" or "any of".

**Why this priority**: URL patterns cover most needs; page facts cover the rest without a general query language.

**Independent Test**: Define segments using each condition type on a fixture crawl and check membership.

**Acceptance Scenarios**:

1. **Given** a segment "Thin: word count under 300 AND status is 200", **When** it is evaluated, **Then** only 200 pages with fewer than 300 words belong to it.
2. **Given** an invalid pattern, **When** the user saves the segment, **Then** it is rejected with a message pointing at the error, and the previous definition stays in force.
3. **Given** the scope tester (spec 007), **When** the user tests a URL, **Then** it also shows which segment the URL would belong to.

#### User Story B3 - Keep segments with the profile and use them everywhere (Priority: P2)

Segments are saved in the crawl profile (spec 002), so the same segments apply to every crawl of that site, travel with exported profiles, and are stored in the crawl file with the crawl. CSV exports include the Segment column, the command line accepts a segment filter, and later features (report 010, comparison 012, agents 016) can read a URL's segment.

**Acceptance Scenarios**:

1. **Given** a profile with segments, **When** a new crawl starts with that profile, **Then** URLs get segments as they are stored and the Segment column fills during the crawl.
2. **Given** a crawl opened on another machine, **When** the user opens it, **Then** it shows the segments it was saved with.
3. **Given** the command line, **When** the user exports a tab with a segment filter, **Then** only URLs of that segment are written.

### Part C: PDF crawling

#### User Story C1 - Audit PDFs like pages (Priority: P1)

A university site has thousands of PDFs. Search engines index PDFs and show their document title in results, so the user wants each PDF's title, author, subject, keywords, creation and change dates, page count, amount of text, language when declared, and whether it is a scan with no text. A PDFs tab lists them with these columns and filters such as "Missing title", "No text (scanned)", "Over size limit" and "Encrypted".

**Why this priority**: PDFs are the most common non-HTML content with SEO value, and today the crawler records only their response facts.

**Independent Test**: Crawl a fixture site with PDFs whose properties are known, including a scanned PDF, an encrypted one, a damaged one and one over the size limit, and compare the stored facts.

**Acceptance Scenarios**:

1. **Given** "Crawl PDFs" is on and a PDF with document title "Annual Report 2025" and 12 pages, **When** it is crawled, **Then** the PDFs tab shows that title, 12 pages and its word count.
2. **Given** a PDF made of scanned images only, **When** it is crawled, **Then** it is marked "No text" and its word count is 0.
3. **Given** a password-protected PDF, **When** it is crawled, **Then** it is marked "Encrypted", its unencrypted properties are kept, and the crawl continues.
4. **Given** a damaged PDF, **When** it is crawled, **Then** the facts that could be read are kept, it is marked "Partly read" with the reason, and the crawl continues.
5. **Given** a PDF larger than the PDF size limit (default 20 MB), **When** it is crawled, **Then** the download stops at the limit, response facts are kept, and it is marked "Over size limit".
6. **Given** "Crawl PDFs" is off (the default), **When** a PDF is found, **Then** it is handled exactly as today: response facts only, no body download beyond what is needed for the status.

#### User Story C2 - Follow links inside PDFs (Priority: P2)

PDFs link to pages, and those links count for search engines. With "Follow PDF links" on, links found in a PDF are stored like page links (target, link text when the PDF has it, internal or external) and handed to URL discovery, so pages linked only from PDFs are crawled and PDFs appear as link sources in the inlinks of a page.

**Acceptance Scenarios**:

1. **Given** a PDF that links to `/only-from-pdf`, **When** "Follow PDF links" is on, **Then** `/only-from-pdf` is crawled and its inlinks show the PDF as the source.
2. **Given** "Follow PDF links" is off, **When** the same PDF is crawled, **Then** its links are stored but not handed to discovery.

#### User Story C3 - PDF work never slows the HTML crawl (Priority: P1)

A crawl with many large PDFs must not stall page crawling. PDF reading runs separately from page extraction, with its own size limit and per-document time limit, and when PDFs back up the crawl keeps fetching pages and catches up on PDFs.

**Acceptance Scenarios**:

1. **Given** a fixture site of 10,000 pages and 1,000 PDFs of 5 MB each, **When** it is crawled with "Crawl PDFs" on, **Then** the rate of HTML pages crawled is within 10% of the same crawl with PDFs off (SC-C2).
2. **Given** a PDF whose text takes longer to read than the per-document time limit (default 5 seconds), **When** the limit is reached, **Then** the facts read so far are kept, it is marked "Partly read: time limit", and the next PDF starts.

### Part D: Spelling and grammar

#### User Story D1 - Find spelling mistakes on every page, in the page's language (Priority: P1)

A content team wants a list of spelling mistakes across a site in English and Hebrew. With "Check spelling" on, the visible text of each page (title, description, headings, body; not code, navigation repeated on every page is counted once per site) is checked against a dictionary for the page's language, taken from the page's `lang` attribute or, when missing, the language the user sets as default. A Spelling tab lists pages with error counts, and the detail pane shows each misspelt word in context with suggestions. Everything runs on the user's machine; no text leaves it.

**Why this priority**: Spelling mistakes look careless to users, and the stored HTML lets the check run on any crawl, so it is the core of this part.

**Independent Test**: Crawl fixture pages in English and Hebrew with planted misspellings and correct words, and check that exactly the planted words are reported with the right positions.

**Acceptance Scenarios**:

1. **Given** an English page with "recieve" and "acommodate" in its body, **When** spelling is checked, **Then** both words are reported with their sentence and with "receive" and "accommodate" among the suggestions.
2. **Given** a Hebrew page (`lang="he"`) with one misspelt word and words carrying prefixes such as ו, ה, ב, ל, מ, ש, כ, **When** spelling is checked, **Then** the misspelt word is reported and correctly prefixed words are not.
3. **Given** a page in a language whose dictionary is not installed, **When** spelling is checked, **Then** the page is marked "Not checked: no dictionary for <language>" and the app offers to add that dictionary.
4. **Given** product names and brand words on the site, **When** the user adds them to the site's ignore list, **Then** they are no longer reported on any page, and the list is saved in the crawl profile.
5. **Given** a finished crawl with stored HTML (spec 005) and spelling off during the crawl, **When** the user runs "Check spelling" on it, **Then** the check runs on the stored text without recrawling.
6. **Given** a page that mixes English product names in Hebrew text, **When** spelling is checked, **Then** words written in a different script than the page language are checked against that script's dictionary when one is installed, and ignored otherwise.

#### User Story D2 - Add and manage dictionaries offline (Priority: P1)

Dictionaries are files on the user's machine. The app ships with English, and offers a list of other languages (at least 25, including Hebrew, Arabic, Spanish, French, German, Portuguese, Italian, Dutch and Russian) that the user can add in Settings › Spelling. Adding one downloads it once from the same static release location the app itself is shipped from, shows its size and licence, and from then on works offline. The user can also point to dictionary files they already have, such as those installed by their operating system or office suite.

**Acceptance Scenarios**:

1. **Given** Settings › Spelling, **When** the user adds Hebrew, **Then** the dictionary's size and licence are shown before the download, and after it the language is listed as installed.
2. **Given** a computer with no network, **When** a crawl is checked with installed dictionaries, **Then** the check works fully.
3. **Given** the user points to a dictionary file pair on disk, **When** it is valid, **Then** it is added under the language it declares; when invalid, it is rejected with a message.

#### User Story D3 - Grammar checks for English (Priority: P3)

For English pages the user can also turn on grammar and style checks (for example repeated words, "a" before a vowel sound, common confusions such as "their/there"). Grammar is reported separately from spelling. Other languages show spelling only; this is stated in the app, not hidden.

**Acceptance Scenarios**:

1. **Given** an English page with "the the" and "an book", **When** grammar is checked, **Then** both are reported as grammar issues with a suggestion.
2. **Given** a Hebrew page, **When** grammar is on, **Then** the page shows "Grammar: not available for Hebrew" and spelling results only.

#### User Story D4 - Spelling never slows the crawl (Priority: P1)

Spelling runs on stored text, separately from fetching and extraction, at a lower priority than the crawl. A crawl with spelling on finishes fetching at the same speed as with spelling off; spelling results may complete after the last page is fetched, with a progress indicator.

**Acceptance Scenarios**:

1. **Given** a 10,000-page crawl with spelling on, **When** it runs, **Then** pages fetched per second stay within 5% of the same crawl with spelling off (SC-D3), and the Spelling tab shows "Checking: N of M pages" until done.

### Part E: AMP

#### User Story E1 - Find AMP versions and check that they are valid (Priority: P3)

A news publisher still serves AMP pages. With "Check AMP" on, each page's `amphtml` link is followed, the AMP page is crawled and a core set of AMP checks runs on it: the AMP marker on the root element, the required runtime script, the required boilerplate style, a viewport, a canonical link back to a page, no author-written scripts outside allowed components, and inline style within the size limit. An AMP tab lists AMP pages with the canonical they point to and the checks they fail, and pairs whose canonical and `amphtml` links do not point at each other.

**Why this priority**: AMP no longer brings ranking features, so only publishers who still serve it need this. It is the first part to drop if time is short.

**Independent Test**: Crawl a fixture site with one valid AMP page, one AMP page per failing check, and one mismatched pair, and compare the reported checks.

**Acceptance Scenarios**:

1. **Given** a page with `<link rel="amphtml" href="/a/amp">`, **When** "Check AMP" is on, **Then** `/a/amp` is crawled and listed in the AMP tab with its canonical page.
2. **Given** an AMP page with an author-written `<script>` that is not an AMP component, **When** it is checked, **Then** it fails "Custom scripts are not allowed" with the script's position.
3. **Given** an AMP page whose canonical does not point back to the page that links to it, **When** it is checked, **Then** the pair is reported as "AMP and canonical do not match".
4. **Given** "Check AMP" is off (the default), **When** a page has an `amphtml` link, **Then** the link is stored as today and no AMP checks run.

### Edge Cases

- **Part A**: Titles with emoji, Hebrew or Arabic (right-to-left), Chinese, Japanese or Korean characters: widths use the font fallback a search result would use for those scripts; right-to-left text truncates from its logical end. A title with leading or trailing spaces or repeated spaces is measured as search engines display it (collapsed).
- **Part A**: A page with several titles: the first (primary, spec 005) is measured and previewed; the preview notes that more than one exists.
- **Part A**: A title over 1,000 characters: width is measured up to the point it is truncated, and stored as "over N pixels" without measuring the whole string.
- **Part B**: No segments defined: the Segment column and filter are hidden; nothing changes for the user.
- **Part B**: More than 100 segments, or a pattern that is slow to match: segments stay usable; evaluation of 100 segments over 1,000,000 URLs finishes within SC-B1.
- **Part B**: Redirected URLs, external URLs and blocked URLs: segments apply to every stored URL, including these, using the URL as stored.
- **Part C**: A URL ending in `.pdf` that returns HTML, or a PDF served as `application/octet-stream`: the content type and the file's first bytes decide, not the extension.
- **Part C**: A PDF returned with `X-Robots-Tag: noindex` or a canonical header: header facts are stored as today (spec 005 FR-003) and shown in the PDFs tab.
- **Part C**: A PDF that links to itself or to thousands of URLs: links are stored and deduplicated like page links; discovery limits (spec 003) apply.
- **Part D**: Words with digits, URLs, email addresses, code in `<code>`/`<pre>`, all-capital abbreviations and words joined with hyphens: digits, URLs, emails and code are skipped; abbreviations are skipped by default (setting); hyphenated words are checked part by part.
- **Part D**: A page that declares the wrong language (for example `lang="en"` on a Hebrew page): the page is checked in the script its text is written in when that differs clearly from the declared language, and the mismatch is reported as a finding.
- **Part D**: Text repeated on every page (menus, footers): counted once per site in the site-wide list so one footer typo does not appear as 10,000 errors, while still shown on each page.
- **Part E**: An `amphtml` link pointing to another host or to a URL that redirects: followed under the crawl's scope rules; the redirect is reported.

## Requirements *(mandatory)*

### Shared requirements

- **FR-000a**: Each part MUST be deliverable, testable and removable on its own; no part may require another part of this spec.
- **FR-000b**: With every part at its default setting, a crawl MUST fetch pages at the same rate as before this feature (SC-000).
- **FR-000c**: All checks MUST run on the user's machine. No part may send page text, URLs or files to any server; the only network use is the one-time dictionary download in Part D at the user's request.
- **FR-000d**: Every check result that is an issue (too wide, misspelt, PDF missing title, AMP check failed) MUST be defined as a rule in the spec 009 rule catalogue, with the rule shape of spec 009 FR-001, so issues, filters, the command line, the report (010) and agents (016) see one definition.
- **FR-000e**: Every setting this spec adds MUST be stored in the crawl profile (spec 002) and with the crawl in the crawl file, and MUST be available from the command line.
- **FR-000f**: Crawl files saved before this feature MUST keep opening (constitution); data a part needs but the file lacks MUST show as unavailable with the reason, never as zero. Changes to the crawl file MUST follow the additive storage rules of spec 006.
- **FR-000g**: Every view this spec adds MUST follow the design system of spec 017 and the appearance rules of spec 018, and MUST have automated UI tests.

### Part A: SERP preview

- **FR-A01**: The system MUST compute, for the primary title and meta description of every HTML page, the width in pixels as drawn in a search result on desktop and on mobile, using reference font metrics that ship with the app (no system fonts, so results are the same on every operating system).
- **FR-A02**: The system MUST show the pixel widths and desktop and mobile truncation marks as columns in the Page titles and Meta description tabs, sortable and filterable, and in the detail pane.
- **FR-A03**: Default limits MUST be: title 600 pixels on desktop and 580 on mobile; description 990 pixels on desktop and 680 on mobile; title under 200 pixels as "short". The user MUST be able to change each limit in the rule thresholds (spec 009 FR-021).
- **FR-A04**: Pixel widths MUST be available for crawl files saved before this feature, from their stored titles and descriptions.
- **FR-A05**: The detail pane MUST show a desktop and a mobile search preview with the title, a breadcrumb form of the URL and the description, truncated with an ellipsis where the limits are reached.
- **FR-A06**: The user MUST be able to type a proposed title and description in the preview, see width and truncation update while typing, keep proposals per URL in the crawl file, and export them as CSV with current and proposed values.
- **FR-A07**: SERP list mode MUST accept a pasted list or a CSV of URL, title and description, show the same columns and previews, make no network request, and be available from the command line.
- **FR-A08**: The first rule set additions MUST include: title over pixel width (desktop), title over pixel width (mobile), title under pixel width, description over pixel width (desktop), description over pixel width (mobile).

### Part B: Segments

- **FR-B01**: The user MUST be able to create, rename, reorder, colour (from the design system palette), enable and delete segments in Settings › Segments and from the segment filter.
- **FR-B02**: A segment condition MUST support URL host, path, query and whole URL with contains, does not contain, starts with, ends with, equals and pattern; and page facts: status class, content type, indexability, language, crawl depth and word count with comparison operators. Conditions MUST combine with "all of" or "any of".
- **FR-B03**: Each URL MUST belong to at most one segment: the first enabled segment in the user's order whose condition matches; otherwise "No segment".
- **FR-B04**: Every results tab MUST offer a Segment column and a segment filter that persists across tabs; sidebar counts MUST follow the active segment filter.
- **FR-B05**: The issues list (spec 009) MUST be able to show affected URL counts per segment.
- **FR-B06**: Changing segments MUST apply to the open crawl without recrawling, within SC-B1.
- **FR-B07**: Segments MUST be validated when saved, with the error shown against the condition that caused it.
- **FR-B08**: The scope tester (spec 007) MUST show the segment a test URL would belong to.
- **FR-B09**: CSV exports MUST include the Segment column when segments exist, and the command line MUST accept a segment filter for any tab export.

### Part C: PDF crawling

- **FR-C01**: A "Crawl PDFs" setting, off by default, MUST make the crawler download PDF bodies up to a PDF size limit (default 20 MB, user-adjustable) and read them.
- **FR-C02**: For each PDF read the system MUST store: document title, author, subject, keywords, creator application, producer, creation and modification dates, page count, declared language, word count of the text, whether it is encrypted, whether it has no text, whether it is tagged for accessibility, and a reading status (read, partly read with reason, over size limit, failed with reason).
- **FR-C03**: The system MUST store the links in each PDF with target, link text when present, page number and internal or external flag, and MUST hand them to URL discovery only when "Follow PDF links" is on (default on when "Crawl PDFs" is on).
- **FR-C04**: PDF reading MUST run separately from page extraction, with a per-document time limit (default 5 seconds) and bounded memory per document, so slow or large PDFs never delay HTML pages (SC-C2).
- **FR-C05**: The system MUST store the PDF's text under the same retention setting as raw HTML (spec 005), compressed, so later checks (Part D, 015, 016) can use it without refetching.
- **FR-C06**: A PDFs tab MUST list PDFs with the facts in FR-C02, header directives and canonicals (spec 005 FR-003), inlink counts and filters for missing title, no text, encrypted, over size limit, partly read and noindex.
- **FR-C07**: Rule additions MUST include: PDF missing title, PDF has no text, PDF over size limit, PDF could not be read, PDF linked from internal pages but noindex.
- **FR-C08**: The command line MUST be able to crawl PDFs and export the PDFs tab.

### Part D: Spelling and grammar

- **FR-D01**: A "Check spelling" setting, off by default, MUST check the visible text of HTML pages (and PDF text when Part C is present) against a dictionary for each page's language.
- **FR-D02**: The page language MUST be taken from the root `lang` attribute (spec 005), falling back to the user's default language; text in a different script than the page language MUST be checked with an installed dictionary for that script or skipped.
- **FR-D03**: The app MUST ship with an English dictionary and MUST offer at least 25 more languages including Hebrew, each added on request by a one-time download from the app's static release location, with size and licence shown first. Installed dictionaries MUST work with no network.
- **FR-D04**: The user MUST be able to add a dictionary from files on disk in the common open dictionary format used by office suites and operating systems.
- **FR-D05**: Hebrew checking MUST accept words with the standard one-letter prefixes and their combinations, and MUST handle text with and without vowel marks (niqqud).
- **FR-D06**: The user MUST be able to keep a site ignore list in the crawl profile, ignore a word once or everywhere from the detail pane, and choose whether all-capital words are checked.
- **FR-D07**: Spelling MUST run on stored text, after extraction, at a lower priority than crawling, and MUST be runnable on any finished crawl that has stored HTML, without recrawling.
- **FR-D08**: For each page the system MUST store each finding with the word, its position, its text context (a short excerpt), its source element (title, description, heading, body, PDF) and up to 5 suggestions.
- **FR-D09**: Text that repeats on most pages of a site (for example navigation and footer text) MUST be grouped in a site-wide findings list in addition to per-page counts.
- **FR-D10**: Grammar checks MUST be available for English, off by default, reported separately from spelling; for other languages the app MUST say grammar is not available.
- **FR-D11**: A Spelling tab MUST list pages with spelling and grammar counts and the language used, with progress while checking; the detail pane MUST list findings in context.
- **FR-D12**: Rule additions MUST include: spelling errors on page, grammar issues on page (English), page not checked (no dictionary), declared language does not match text.
- **FR-D13**: The command line MUST run the spelling check on a crawl file and export findings.

### Part E: AMP

- **FR-E01**: A "Check AMP" setting, off by default, MUST follow `amphtml` links (subject to scope rules) and run AMP checks on pages marked as AMP.
- **FR-E02**: The AMP checks MUST include: AMP marker on the root element, required runtime script, required boilerplate style, viewport meta, canonical link present, canonical and `amphtml` links point to each other, no author-written scripts other than AMP components, inline style within the AMP size limit, and no disallowed elements from a short published list.
- **FR-E03**: Each failed check MUST be stored with the element position, and the AMP tab MUST list AMP pages, their canonical, and failed checks.
- **FR-E04**: The AMP tab MUST state that the checks are a core subset, not the full official AMP validator.
- **FR-E05**: Rule additions MUST include one rule per AMP check in FR-E02.

### Key Entities

- **Snippet measurement** (A): title and description widths on desktop and mobile, truncation flags, per page; may be computed rather than stored.
- **Snippet proposal** (A): proposed title and description for a URL, with the time it was written.
- **Segment** (B): name, colour, order, enabled flag, condition (one or more clauses joined by all of / any of). Belongs to a profile and is copied into the crawl.
- **URL segment** (B): the segment a URL belongs to, derived from the segments and the stored URL facts.
- **PDF document** (C): the facts in FR-C02, its links and optionally its text, belongs to one crawled resource.
- **Dictionary** (D): language, script, version, licence, source (shipped, downloaded, user file), stored on the user's machine outside crawl files.
- **Spelling finding** (D): page, word, position, context, source element, kind (spelling or grammar), suggestions.
- **AMP check result** (E): page, check, pass or fail, position.

## Success Criteria *(mandatory)*

### Shared

- **SC-000**: On the speed benchmark of spec 008, with every part at its default setting, pages per second are within 2% of the same build without this feature.

### Part A

- **SC-A1**: On a fixture set of 200 titles and 200 descriptions with reference widths, 95% of measured widths are within 2% of the reference and every truncation flag matches.
- **SC-A2**: Widths for all pages of a 100,000-page crawl are available in the tabs within 3 seconds of opening the crawl.
- **SC-A3**: Typing in the preview updates width and truncation within 100 ms.

### Part B

- **SC-B1**: Changing a segment updates every tab and count within 5 seconds for 100,000 URLs and 30 seconds for 1,000,000 URLs, with 100 segments defined.
- **SC-B2**: On a segments fixture, 100% of URLs land in the expected segment.

### Part C

- **SC-C1**: On the PDF fixture set, 100% of expected PDF facts and links match the stored values, and no damaged, encrypted or oversized PDF stops the crawl.
- **SC-C2**: With "Crawl PDFs" on, HTML pages per second on the PDF benchmark site are within 10% of the same crawl with PDFs off.
- **SC-C3**: Memory used to read one PDF stays under 4 times the PDF size limit.

### Part D

- **SC-D1**: On fixture pages in English and Hebrew with planted errors, at least 95% of planted misspellings are found and fewer than 2% of correct words are reported.
- **SC-D2**: Spelling of a 10,000-page crawl of 500 words per page completes within 2 minutes after the crawl ends on the reference laptop (spec 008).
- **SC-D3**: With spelling on, pages fetched per second stay within 5% of the same crawl with spelling off.

### Part E

- **SC-E1**: On the AMP fixture set, every check passes and fails exactly where expected.

## Assumptions

- Search result pixel limits change over time and differ by search engine; the defaults in FR-A03 follow the widths commonly used by SEO tools in 2026 and are user-adjustable. Reference font metrics are for the fonts search results use today; updating them is a data change, not a code change.
- Pixel widths are computed from the title and description in the HTML as served. After JavaScript rendering (spec 014) lands, the rendered title can be measured too; that is a follow-up, not part of this spec.
- Segments are evaluated on stored data and therefore add no network or fetch cost. Segment conditions on custom extraction fields (spec 013) or on issues (spec 009) are possible follow-ups, not part of this spec.
- Dictionaries are not part of the installer except English, to keep the download small. Dictionaries have their own licences (some, including the common Hebrew one, are copyleft); they are downloaded as separate data files at the user's request with their licence shown, and are not linked into the app. The plan confirms each licence.
- Grammar checking is offered for English only because no offline, local, permissively licensed grammar checker covers other languages, including Hebrew, at acceptable quality. More languages can be added later without changing this spec's shape.
- PDF reading covers text-based PDFs. Reading text from scanned PDFs (optical character recognition) is out of scope; such PDFs are marked "No text".
- AMP checks are a core subset. Full validation needs the official validator, which is not part of the app.
- Out of scope: readability scores and content area settings (gap row 29), checks on rendered text (after 014), Office documents and other file types, semantic or AI-based writing checks (016), segments by issue or custom extraction, SERP data from real search results (rankings, rich results).
- Dependencies: Part A and FR-000d use the spec 009 rule catalogue (if 009 is not merged first, Part A ships columns and filters and adds the rules when 009 lands). Part D uses stored HTML from spec 005. Parts C and E change crawl behaviour and follow the scope rules of spec 003 and the storage rules of spec 006.
