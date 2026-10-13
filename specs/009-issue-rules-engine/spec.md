# Feature Specification: Issue Rules Engine

**Feature Branch**: `009-issue-rules-engine`

**Created**: 2026-10-13

**Status**: Draft (awaiting owner approval)

**Input**: Product backlog entry 009, "Issue rules engine: rule format, priorities, explanations, the first rule set" (milestone M2), started on the owner's go-ahead of 2026-10-13 ("go for everything, in parallel, and do not forget UI tests"). The engine turns a crawl into an audit: every problem SpiderLens can detect from stored crawl data becomes a named issue with a priority, a plain explanation, how to fix it and the list of affected URLs, shown per page and site-wide, in the desktop app and the command line.

**Numbering note**: 009 is the backlog number reserved for this feature. The end-of-crawl report with its health score (010), sitemap checks such as orphan pages (011), crawl compare (012), JavaScript rendering checks (014) and AEO/GEO checks (015) all build on this engine and add their own rules; none of them is part of this spec.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - See every issue on the site, by priority (Priority: P1)

An SEO consultant crawls a client site. As soon as pages arrive, an Issues section at the top of the sidebar lists every problem found so far, grouped by priority (High, Medium, Low, Opportunity), each with the number of affected URLs. Clicking an issue fills the results table with exactly the affected URLs and shows, above the table, what the issue means and how to fix it. When the crawl ends, the counts are final.

**Why this priority**: This is the step from "a list of URLs" to "an audit". Every M2 deliverable (the report, exports, the health score) reads the issues this story produces.

**Independent Test**: Crawl the issues fixture site, which contains one or more pages that trigger each rule of the first rule set and pages that trigger none, then check that the Issues section lists exactly the expected issues with the expected counts, and that clicking each one lists exactly the expected URLs.

**Acceptance Scenarios**:

1. **Given** a finished crawl in which 3 internal pages have no title, **When** the user looks at the Issues section, **Then** "Page title: missing" appears under High with the count 3, and clicking it lists exactly those 3 URLs.
2. **Given** the Issues section, **When** the user looks at it, **Then** issues are grouped by priority in the order High, Medium, Low, Opportunity, sorted by count within a group, and issues with no affected URLs are hidden unless the user chooses to show all rules.
3. **Given** the user has clicked an issue, **When** they look above the table, **Then** they see the issue's name, priority, a one or two sentence explanation of why it matters, how to fix it, and the threshold it uses where it has one (for example "over 60 characters").
4. **Given** a crawl is running, **When** pages are stored, **Then** issue counts update at least every 5 seconds without slowing the crawl, and the section marks counts that can still change until the crawl ends (for example duplicates and broken links).
5. **Given** an issue is selected, **When** the user sorts, searches, exports or copies, **Then** these work exactly as on any results tab (spec 007), and the CSV export holds every affected URL.

---

### User Story 2 - See every issue on one page (Priority: P1)

A developer is fixing one page. Selecting its row shows an Issues section in the detail pane listing every issue that page has, with its priority, explanation and the specific evidence (for example "title is 74 characters", "links to 2 broken URLs", "canonical points to a 404"). A small marker in the table row shows the page's highest priority.

**Why this priority**: Issues are fixed page by page. Without the per-page view, the user has to click through every issue to learn what is wrong with one URL.

**Independent Test**: Select fixture pages with a known set of issues and check that the detail pane lists exactly those issues with the expected evidence, and that the row marker shows the highest priority among them.

**Acceptance Scenarios**:

1. **Given** a page whose title is 74 characters and which has two H1 headings, **When** the user selects it, **Then** the detail pane lists "Page title: over 60 characters" with the evidence "74 characters" and "H1: multiple" with "2 H1 headings".
2. **Given** a page that links to two URLs that returned 404, **When** the user selects it, **Then** "Links: broken internal links" lists the two target URLs, and each can be opened in the main table.
3. **Given** a page with no issues, **When** the user selects it, **Then** the Issues section says no issues were found and names the rule set version used.
4. **Given** an issue in the detail pane, **When** the user clicks its name, **Then** the sidebar selects that issue and the table lists every URL that has it.

---

### User Story 3 - Understand every rule (Priority: P1)

A user new to SEO wants to know what each issue means before acting on it. Every rule has a stable name, a priority, a category, a short explanation of why it matters, how to fix it, what URLs it applies to (for example "internal HTML pages that returned 200"), and its threshold if any. The full list of rules can be browsed in the app, including rules that currently have no affected URLs, and printed from the command line.

**Why this priority**: An issue nobody understands is noise. The explanations are also what the end-of-crawl report (010) and later the AI agent features (016) present to users and models, so they must exist from the first rule.

**Independent Test**: Open the rule list in the app and run the command line rule listing, and check that every rule in the first rule set appears in both with all its fields filled and identical text.

**Acceptance Scenarios**:

1. **Given** the Issues section, **When** the user chooses "Show all rules", **Then** every rule appears with its count, including zero, and rules that cannot run on this crawl file appear as unavailable with the reason (see User Story 6).
2. **Given** any rule, **When** the user opens its explanation, **Then** it shows the priority, category, why it matters, how to fix, which URLs it checks and the threshold, in plain language without jargon left unexplained.
3. **Given** the command line, **When** the user lists the rules, **Then** every rule is printed with the same identifier, name, priority, category, threshold and text as in the app.

---

### User Story 4 - Tune rules to the site (Priority: P2)

Different sites have different standards: a news site accepts 70-character titles, a shop does not care about the missing x-default. The user can turn any rule off, change its priority, and change its threshold (for example the title length limit) in Settings › Issues. Changes apply to the open crawl within seconds without recrawling, are saved in the crawl profile, and travel with exported profiles.

**Why this priority**: Fixed thresholds were accepted for M1 (spec 007, Assumptions) on the understanding that M2 audits make them configurable. It ranks after Stories 1 to 3 because the defaults already serve most sites.

**Independent Test**: Change the title length threshold from 60 to 70 on a finished fixture crawl and check that the issue count, the Page titles tab filter label and count, and the detail pane all change to the new threshold within the time in the Success Criteria, then reset to defaults and check they return.

**Acceptance Scenarios**:

1. **Given** a finished crawl, **When** the user changes "Page title: over N characters" from 60 to 70, **Then** the issue's count and URLs, its label, and the matching filter on the Page titles tab (spec 007) all use 70, and no recrawl happens.
2. **Given** the user turns a rule off, **When** they look at the Issues section, the detail pane and the command line output for that crawl, **Then** the rule appears in none of them, except under "Show all rules", where it is marked as off.
3. **Given** the user changes a rule's priority from Medium to High, **When** they look at the Issues section, **Then** the issue moves to the High group, and the row markers of affected pages update.
4. **Given** the user resets the issue settings to defaults, switches profile, or imports a profile, **When** they look at the Issues section, **Then** the rules follow the settings of the active profile.
5. **Given** an invalid threshold (for example a negative length), **When** the user enters it, **Then** it is rejected with a message and the previous value stays in force.

---

### User Story 5 - Audit from the command line and in CI (Priority: P2)

A developer runs SpiderLens in a build pipeline after deploying a site. The command line can list the issues of a crawl file with counts, export the affected URLs of one issue or of all issues, and end with a failing exit code when issues of a chosen priority or higher are present, so a deploy that adds a broken link or a noindex on the home page fails the build.

**Why this priority**: The constitution requires that everything the app can do is possible from the command line (principle III). It is also the cheapest path to automated audits and to the agent features of M6.

**Independent Test**: Run the issues command on the fixture crawl file and compare its counts and URL lists with the app's; run it with a priority threshold on a crawl with and without High issues and check the exit codes.

**Acceptance Scenarios**:

1. **Given** a crawl file, **When** the user runs the issues command, **Then** it prints each issue with priority, identifier, name and count, grouped and ordered as in the app, as text or as JSON.
2. **Given** a crawl file and an issue identifier, **When** the user exports that issue, **Then** the CSV holds the same rows and columns as the app's export of that issue.
3. **Given** a crawl file with one High issue, **When** the user runs the issues command with "fail on High", **Then** it exits with a dedicated non-zero code (distinct from the exit codes already defined in specs 001 to 006); with no High issues it exits with 0.
4. **Given** a crawl command run with an issue settings file, **When** it finishes, **Then** the issues use those settings, as a profile does in the app.

---

### User Story 6 - Older crawl files and newer rules (Priority: P3)

A user opens a crawl saved by an older version of SpiderLens. Rules whose data the file has run and show results; rules that need data the file does not have (for example page content in a file saved before extraction existed) show as unavailable with the reason, never as zero. When the app has a newer rule set than the one the crawl was audited with, the issues are re-evaluated on open and the user is told which rule set version they see.

**Why this priority**: Old files must keep opening after upgrades (constitution, Product Constraints) and must never report "no issues" when the truth is "not checked". It is P3 because fresh crawls do not need it.

**Independent Test**: Open fixture crawl files from each earlier format version and check that every rule is either evaluated correctly or marked unavailable with the right reason.

**Acceptance Scenarios**:

1. **Given** a crawl file saved before page extraction existed, **When** it is opened, **Then** response code, redirect and URL rules show results and content rules (titles, descriptions, headings, canonicals, directives, images) show as unavailable with the reason.
2. **Given** a crawl file audited with rule set version 1 and an app with version 2, **When** the file is opened, **Then** the issues shown are those of version 2, the Issues section states the version, and nothing is written to a file that is opened view-only (spec 006).
3. **Given** a rule that needs the stored HTML and a crawl made with HTML storage off, **When** the file is opened, **Then** that rule shows as unavailable because the HTML was not stored.

---

### Edge Cases

- **Live crawl and site-wide rules**: per-page rules (title length, missing H1) are final as soon as the page is stored; site-wide rules (duplicates, broken links, hreflang return links, canonical targets) can change until the crawl ends, because a later page can create or resolve them. The UI marks such counts as provisional while the crawl runs.
- **A URL that is retried later** (spec 004 "Retry failed URLs") changes its issues in place; it never appears twice in an issue.
- **Paused and resumed crawls** show the same issues as an uninterrupted crawl of the same site.
- **Rules whose target was not crawled**: a canonical, hreflang or link target outside the crawl's scope, blocked by robots.txt or left uncrawled by a limit is reported as "not checked", not as broken. The rule's explanation says so.
- **External URLs** are only checked by rules about responses and links; content rules apply to internal HTML pages that returned 200, the same set the 007 content tabs use (spec 007, FR-010).
- **Non-HTML resources** (PDF, images) are checked by response, redirect, URL, security and header-directive rules only.
- **Very large issues**: an issue affecting 1,000,000 URLs lists, sorts, searches and exports as fast as any 007 tab.
- **Evidence on a large page**: a page linking to 5,000 broken URLs shows the count and a scrollable list, not 5,000 lines of text.
- **Same rule in two places**: where a rule matches an existing 007 filter (for example "Page title: missing"), the issue and the filter MUST list the same URLs and count, with the same threshold.
- **Rule set changes between app versions**: rule identifiers are stable; a renamed rule keeps its identifier; a removed rule's identifier is never reused, so saved settings and CI scripts keep working.
- **Settings for a rule that no longer exists** in an imported profile are ignored with a notice, not an error.
- **Unicode**: lengths are in characters, not bytes, as in spec 007 (FR-012); right-to-left titles are measured the same way.

## Requirements *(mandatory)*

### Functional Requirements

**Rules and rule set**

- **FR-001**: The system MUST define every issue check as a rule with: a stable identifier, a name, a category, a default priority (High, Medium, Low or Opportunity), the set of URLs it applies to, the condition that makes a URL affected, an optional threshold with its default, the data it needs from the crawl file, a plain explanation of why it matters, and how to fix it.
- **FR-002**: Rules MUST be defined in one rule catalogue, as data, that the desktop app, the command line and later features (report, compare, agents) all read; no feature may define its own copy of a rule's condition or text.
- **FR-003**: The rule catalogue MUST carry a version number that increases whenever a rule is added, removed or changes its condition; rule identifiers MUST never be reused.
- **FR-004**: The first rule set MUST contain the rules listed in the table under "First rule set" below, with the default priorities and thresholds given there.
- **FR-005**: Where a rule and a spec 007 tab filter test the same thing, they MUST share one definition, so they always list the same URLs with the same threshold.

**Evaluation**

- **FR-006**: Rules MUST be evaluated from the data stored in the crawl file only; evaluating rules MUST NOT fetch anything, re-parse pages that are already extracted, or change the stored crawl data.
- **FR-007**: During a crawl, issue counts MUST update at least every 5 seconds, and per-page issues MUST be available for a page within 5 seconds of it being stored.
- **FR-008**: When a crawl ends, is stopped, or a crawl file is opened, the system MUST produce the final issues for the whole crawl, and MUST show partial results while doing so rather than an empty section.
- **FR-009**: Evaluation MUST be deterministic: the same crawl data, rule set version and settings always produce the same issues, in the desktop app and the command line.
- **FR-010**: A rule that cannot run because the crawl file lacks the data it needs MUST be reported as unavailable with the reason, never as having zero affected URLs.
- **FR-011**: Targets that were not fetched (out of scope, blocked, not crawled because of a limit, or not checked because external checking is off) MUST be treated as "not checked" by rules about link, canonical, hreflang and pagination targets, never as broken.
- **FR-012**: For each affected URL the system MUST record the evidence that triggered the rule (for example the measured length, the duplicate count, the broken target URLs) so it can be shown without re-evaluating.
- **FR-013**: The crawl file MUST record the rule set version and the issue settings used for its last evaluation; opening a file with an older rule set version MUST re-evaluate with the current rule set and state the version shown. Files opened view-only MUST NOT be written (spec 006).

**Issues in the desktop app**

- **FR-014**: The sidebar MUST show an Issues section listing each issue with affected URLs, grouped by priority in the order High, Medium, Low, Opportunity and ordered by count within a group, with a choice to show all rules including those with zero, unavailable or turned off.
- **FR-015**: Selecting an issue MUST show its affected URLs in the results table with columns relevant to the issue (for example the title and its length for title rules, the broken target and status for link rules) and the evidence, and MUST support sort, search, column choice, copy and CSV export as defined in spec 007.
- **FR-016**: Selecting an issue MUST show its name, priority, explanation, how to fix and threshold above the table.
- **FR-017**: The detail pane MUST have an Issues section listing every issue of the selected URL with priority and evidence; clicking an issue there MUST select it in the sidebar.
- **FR-018**: Each row in the results table MUST be able to show the highest priority among the URL's issues and the number of issues, as an optional column available on every tab, sortable and filterable.
- **FR-019**: Issue counts that can still change before the crawl ends MUST be visibly marked as provisional while the crawl runs.
- **FR-020**: Every issue view, explanation and setting MUST be reachable and operable by keyboard and announced correctly by screen readers, as the rest of the UI (spec 017).

**Settings**

- **FR-021**: Users MUST be able to turn each rule on or off, change its priority and change its threshold where it has one, in a Settings › Issues page with a search box and the rules grouped by category.
- **FR-022**: Issue settings MUST be part of the crawl profile (spec 002): saved, exported, imported and reset with it, and stored with each crawl as part of its configuration.
- **FR-023**: Changing an issue setting MUST re-evaluate the open crawl without recrawling, MAY be done while a crawl runs, and MUST update the Issues section, the matching 007 filters and the detail pane.
- **FR-024**: Thresholds MUST be validated (type and range per rule); invalid values MUST be rejected with a message and leave the previous value in force.

**Command line**

- **FR-025**: The command line MUST list the rules of the catalogue with all their fields, as text or JSON.
- **FR-026**: The command line MUST list the issues of a crawl file with counts, as text or JSON, and MUST export the affected URLs of one issue, or of every issue into one file per issue, as CSV with the same rows and columns as the desktop app.
- **FR-027**: The command line MUST accept a minimum priority to fail on and end with a dedicated non-zero exit code when any enabled issue of that priority or higher has affected URLs.
- **FR-028**: The command line MUST accept issue settings from a profile or settings file, with the same validation as the app.

**Performance**

- **FR-029**: Evaluating rules during a crawl MUST NOT reduce crawl speed by more than 5% on the 100,000-page local fixture site (the spec 008 benchmark).
- **FR-030**: Evaluating rules MUST NOT load the whole crawl into memory; memory use MUST stay flat as the crawl grows (constitution principle VI).

### First rule set

Priorities: H = High, M = Medium, L = Low, O = Opportunity. "Pages" means internal HTML pages that returned 200 (spec 007, FR-010) unless stated otherwise. Thresholds are defaults the user can change (FR-021). Where a 007 filter tests the same thing, the threshold is the same (FR-005).

| Category | Rule | Applies to | Default | Threshold |
|---|---|---|---|---|
| Response codes | Internal client error (4xx) | internal URLs | H | |
| Response codes | Internal server error (5xx) | internal URLs | H | |
| Response codes | Internal no response (any spec 004 error class) | internal URLs | H | |
| Response codes | External client or server error | external URLs that were checked | M | |
| Response codes | Internal redirect (3xx) | internal URLs | L | |
| Response codes | Redirect loop | all URLs | H | |
| Response codes | Redirect chain | all URLs | M | 2 or more hops |
| Response codes | Internal URL blocked by robots.txt | internal URLs | L | |
| Response codes | Slow response | pages | L | over 1,000 ms |
| Links | Pages with broken internal links | pages | H | |
| Links | Pages with broken external links | pages | M | |
| Links | Internal links to redirects | pages | L | |
| Links | Nofollowed internal links | pages | L | |
| Links | Pages without internal outlinks | pages | L | |
| Links | Non-descriptive anchor text ("click here", "read more", and similar) | pages | O | |
| Links | Deep pages | pages | O | crawl depth over 4 |
| Page titles | Missing | pages | H | |
| Page titles | Duplicate | pages | M | |
| Page titles | Over N characters | pages | M | 60 |
| Page titles | Below N characters | pages | M | 30 |
| Page titles | Multiple title elements | pages | M | |
| Page titles | Same as H1 | pages | L | |
| Page titles | Title outside head | pages | M | |
| Meta description | Missing | pages | L | |
| Meta description | Duplicate | pages | L | |
| Meta description | Over N characters | pages | L | 155 |
| Meta description | Below N characters | pages | L | 70 |
| Meta description | Multiple | pages | M | |
| H1 | Missing | pages | M | |
| H1 | Duplicate | pages | L | |
| H1 | Over N characters | pages | L | 70 |
| H1 | Multiple | pages | L | |
| H2 | Missing | pages | L | |
| H2 | Duplicate | pages | L | |
| H2 | Over N characters | pages | L | 70 |
| Headings | Non-sequential (a level skipped, for example H1 then H3) | pages | L | |
| Content | Exact duplicate content (same visible text fingerprint, spec 005 FR-015) | pages | M | |
| Content | Thin content | pages | L | below 200 words |
| Content | Partially extracted or encoding problem (spec 005 FR-008) | pages | M | |
| Content | Large HTML page | pages | L | over 1 MB decoded |
| Content | Missing language attribute | pages | L | |
| Images | Missing alt attribute | images on pages | L | |
| Images | Alt text over N characters | images on pages | L | 100 |
| Images | Broken image (4xx, 5xx or no response) | images that were checked | H | |
| Images | Large image | images that were checked | M | over 100 KB |
| Images | Missing width and height attributes | images on pages | L | |
| Canonicals | Canonicalised (points to another URL) | pages | L | |
| Canonicals | Missing canonical | pages | O | |
| Canonicals | Multiple or conflicting canonicals | pages | H | |
| Canonicals | Canonical outside head | pages | M | |
| Canonicals | Canonical points to a non-200 or non-indexable URL | pages | H | |
| Directives | Noindex | internal HTML pages | M | |
| Directives | Nofollow (page level) | internal HTML pages | L | |
| Directives | Directive outside head | pages | M | |
| Directives | Conflicting directives (for example index and noindex, or meta and header disagree) | internal HTML pages | M | |
| Directives | Noindexed page linked from many internal pages | internal HTML pages | M | 10 or more inlinks |
| Hreflang | Missing return link | pages with hreflang | H | |
| Hreflang | Target not 200 or not indexable | pages with hreflang | H | |
| Hreflang | Missing self reference | pages with hreflang | L | |
| Hreflang | Missing x-default | pages with hreflang | O | |
| Hreflang | Invalid language or region code | pages with hreflang | M | |
| Pagination | rel next or prev target not 200 | pages with rel next or prev | M | |
| Structured data | JSON-LD block that is not valid JSON | pages | M | |
| Security | Internal page served over HTTP | internal HTML pages | H | |
| Security | Mixed content (HTTPS page loads HTTP images, scripts or styles) | internal HTTPS pages | M | |
| Security | Missing HSTS header | internal HTTPS pages | L | |
| Security | Missing X-Content-Type-Options header | internal HTML pages | L | |
| URL | Uppercase characters | internal URLs | L | |
| URL | Underscores | internal URLs | L | |
| URL | Query parameters | internal URLs | L | |
| URL | Over N characters | internal URLs | L | 115 |
| URL | Non-ASCII characters | internal URLs | L | |
| URL | Multiple slashes | internal URLs | L | |
| URL | Spaces | internal URLs | L | |

### Key Entities

- **Rule**: one issue check: identifier, name, category, default priority, scope (which URLs it applies to), condition, optional threshold with type, range and default, data it needs, explanation, how to fix, whether its result is per page or site-wide (provisional during a crawl).
- **Rule catalogue**: the versioned set of all rules; one per app version.
- **Issue settings**: per rule, on or off, priority and threshold; part of the crawl profile and stored with each crawl.
- **Issue**: a rule with at least one affected URL in a crawl, with its count.
- **Affected URL**: one URL that triggers one rule, with its evidence (for example a length, a count, target URLs).
- **Evaluation**: one run of the rule catalogue over a crawl, with the rule set version and settings used, and whether it is final or provisional.

## Success Criteria *(mandatory)*

### Measurable Outcomes

Timings are measured on the reference machine defined in spec 007, Assumptions.

- **SC-001**: On the issues fixture site, which has at least one positive and one negative example per rule, 100% of rules report exactly the expected URLs and evidence, in the desktop app and the command line.
- **SC-002**: Crawling the 100,000-page local fixture site with every rule on runs at no less than 95% of the speed of the same crawl with rules off (spec 008 benchmark).
- **SC-003**: Final issues for a 100,000-URL crawl are complete within 3 seconds of the crawl ending; for a 1,000,000-URL crawl file within 30 seconds of opening it, with partial counts shown within 2 seconds and the interface responsive throughout.
- **SC-004**: After a threshold or on/off change, the affected issue's count and list update within 2 seconds on a 100,000-URL crawl.
- **SC-005**: Selecting an issue with 1,000,000 affected URLs shows the first screen of rows within 1 second and supports sort, search and export within the spec 007 limits (SC-003, SC-004, SC-009).
- **SC-006**: Memory used by rule evaluation on a 1,000,000-URL crawl is no more than 20% higher than on a 100,000-URL crawl.
- **SC-007**: For every rule that also exists as a 007 filter, the issue and the filter return identical URL sets on every fixture crawl.
- **SC-008**: Every rule in the catalogue has a non-empty explanation and how-to-fix text, and a first-time user can find all pages with broken internal links and export them in under 1 minute without help.
- **SC-009**: The command line exits with the dedicated failing code if and only if an enabled issue at or above the requested priority has affected URLs, on every fixture crawl.

## Assumptions

- **Priorities**: four levels, High, Medium, Low and Opportunity, as in the product spec's end-of-crawl report. Opportunity means "not a problem, but worth doing". Default priorities follow common industry practice and can be changed per rule (FR-021).
- **Data**: the rules use only data that specs 003 to 006 already store (response facts, error classes, redirects, links, titles, descriptions, headings, directives, canonicals, hreflang, images, structured data blocks, headers, text fingerprints). This feature adds no extraction and no fetching.
- **Health score, report and exports beyond CSV** belong to spec 010; this spec provides the issues, priorities and texts they present.
- **Sitemap rules** (orphan pages, non-indexable URLs in sitemaps, URLs missing from sitemaps) belong to spec 011, which adds them to this catalogue.
- **Near-duplicate content, readability, spelling and grammar, pixel-width title and description checks, custom user-written rules, and structured data validation against vocabularies** are out of scope (M3, M5 or later). Structured data here only checks that JSON-LD parses.
- **JavaScript rendering rules** (content only present after rendering, rendering errors) belong to spec 014; **AEO, GEO and AI bot rules** belong to spec 015; both add rules to the same catalogue.
- **Ignoring an issue for a specific URL** ("this one is fine") is out of scope for this spec; turning a rule off or changing its priority covers the common cases.
- **Language**: rule names, explanations and fixes are written in English, like the rest of the UI. Translation is a later concern.
- **Naming**: product text never refers to other crawler products by name.
