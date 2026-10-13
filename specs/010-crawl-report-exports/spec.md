# Feature Specification: Crawl Report and Exports

**Feature Branch**: `010-crawl-report-exports`

**Created**: 2026-10-13

**Status**: Draft

**Input**: Product backlog entry 010, "End-of-crawl report and exports: report sections, health score, HTML/XLSX/CSV" (milestone M2), opened on the owner's approval of 2026-10-13 to write specs for every feature gap ("מאושר בכל") and their standing go-ahead ("go for everything, in parallel, and do not forget UI tests"). The product spec describes the report: when a crawl finishes the user sees a summary with a site health score, issues by priority with affected URLs, response codes, site structure and crawl completeness, and can export it as a standalone file for clients, plus bulk exports of everything the crawl found to CSV and Excel.

**Numbering note**: 010 is the backlog number reserved for this feature. It reads the issues produced by the rule engine (spec 009) and the tabs and CSV export of the results interface (spec 007); it defines no rules of its own. Change since the last crawl (012), orphan pages and sitemap sections (011), the AEO/GEO section (015) and agent-readiness (016) add their own report sections later through the same section mechanism; none of them is part of this spec.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - See how the site is doing when the crawl ends (Priority: P1)

An SEO consultant finishes crawling a client site. A Report view, reachable from the sidebar and offered with a "Crawl finished, view report" notice, shows on one screen: the site health score from 0 to 100, how complete the crawl was, the number of issues and affected URLs per priority, the top issues by impact, response codes, and the crawl depth spread. Every number is a link: clicking "12 pages with broken internal links" opens that issue in the results table.

**Why this priority**: The report is the step from "an audit" to "an answer": a consultant needs a one-screen picture before they dig in, and every other story in this spec (the shareable file, the CLI) presents the same report.

**Independent Test**: Crawl the report fixture site (known issues, known response codes, known depths), open the Report view, and compare every figure, the score, and the targets of every link against the expected values.

**Acceptance Scenarios**:

1. **Given** a finished crawl, **When** the user opens the Report view, **Then** it shows the sections listed in FR-003 with figures that equal the counts of the same things in the results table and the Issues section (spec 009).
2. **Given** the Report view, **When** the user clicks any count of URLs, **Then** the results table opens with exactly those URLs selected by the matching tab, filter or issue.
3. **Given** a crawl that was stopped early, paused, or hit a limit, **When** the user opens the report, **Then** it states clearly that the crawl is incomplete, how many URLs were not crawled and why, and the score is marked as partial.
4. **Given** a crawl that is still running, **When** the user opens the Report view, **Then** it shows the figures so far, marked as provisional, refreshing at least every 5 seconds, and becomes final within the time in the Success Criteria once the crawl ends.
5. **Given** the user changes an issue setting (spec 009, priority, threshold or on/off), **When** they return to the report, **Then** the score, counts and top issues reflect the new settings without recrawling.

---

### User Story 2 - Understand the health score (Priority: P1)

The consultant's client asks "why 72?". Next to the score the report shows how it was reached: the share of pages with High, Medium and Low issues and how much each priority took off the score, with the issues that cost the most points listed first, so fixing the top three is visibly the best next step.

**Why this priority**: A score nobody can explain is not trusted and cannot be acted on. It is also the number clients compare between crawls (spec 012), so it must be stable and documented from the first version.

**Independent Test**: On fixture crawls with known affected pages, check the score equals the documented formula, that the breakdown adds up to the score, and that the same crawl and settings always produce the same score in the app and the CLI.

**Acceptance Scenarios**:

1. **Given** a crawl with no enabled High, Medium or Low issues on any internal page, **When** the score is calculated, **Then** it is 100.
2. **Given** any crawl, **When** the user opens the score breakdown, **Then** it lists the points lost per priority and per issue, and those points add up to 100 minus the score (within rounding).
3. **Given** two crawls of the same site with the same settings where the second fixes the issue with the largest cost, **When** both are reported, **Then** the second score is higher by the cost that issue showed in the first breakdown (within rounding).
4. **Given** the user turns off a rule or lowers its priority, **When** the score is recalculated, **Then** it changes accordingly and the report states that custom issue settings are in use.

---

### User Story 3 - Hand the client a report file (Priority: P1)

The consultant clicks Export report and chooses HTML or PDF. They can set the report title and add a short note for the client. The file opens on any computer with no SpiderLens, no network and no other files: the summary, score breakdown, completeness, every issue with its explanation and how to fix it, and the affected URLs (up to a limit per issue, with the full lists in a companion Excel export).

**Why this priority**: Sharing findings with a client or developer is the main reason a paid user runs a crawl, and users of desktop crawlers expect a client-ready deliverable.

**Independent Test**: Export the fixture crawl's report as HTML and PDF, open both with networking disabled, and check every section, figure and text against the Report view; check the HTML loads no external resource.

**Acceptance Scenarios**:

1. **Given** a finished crawl, **When** the user exports the report as HTML, **Then** one self-contained file is written that opens in current browsers offline, shows every section of the Report view with the same figures, and makes no network request.
2. **Given** the same crawl, **When** the user exports the report as PDF, **Then** the PDF holds the same content, paginated with section headings that do not split from their first lines, and is readable printed in black and white.
3. **Given** an issue with more affected URLs than the per-issue limit, **When** it is exported in the report file, **Then** the report lists the first URLs up to the limit, states the total, and names the bulk export that has them all.
4. **Given** the user set a title and note, **When** the report is exported, **Then** both appear on the first page, and the report states the start URL, crawl date, duration, app version and rule set version.
5. **Given** a crawl that is still running, **When** the user exports the report, **Then** the file is clearly marked provisional with the time it was taken.

---

### User Story 4 - Export everything in one go (Priority: P1)

A technical SEO wants to work on the crawl in a spreadsheet or hand it to a developer. Export › Bulk export offers: every tab with all its columns, every issue with its affected URLs and evidence, all links (every inlink and outlink with source, target, anchor text, rel, follow and target status), all images, all redirect chains, and all response headers. They pick what to include and a format: one Excel workbook (an overview sheet plus one sheet per tab or issue) or a folder of CSV files.

**Why this priority**: Spec 007 exports only the current view; real audits need everything, and links, the largest table, are the export users ask for most.

**Independent Test**: Run every bulk export on the fixture crawl in both formats and compare each sheet or file row for row with the corresponding view or issue in the app; open the workbook in common spreadsheet applications.

**Acceptance Scenarios**:

1. **Given** a finished crawl, **When** the user exports all tabs as CSV, **Then** a folder is written with one file per tab and filter "All", named predictably, each with the same rows and columns (all columns, not only visible ones) as the app's export of that tab.
2. **Given** the same crawl, **When** the user exports all issues to Excel, **Then** the workbook has an overview sheet (issue, priority, category, count, explanation, how to fix) linking to one sheet per issue holding its affected URLs and evidence.
3. **Given** a sheet with more rows than a spreadsheet allows, **When** it is written, **Then** it continues on numbered sheets with the same header and the overview notes the split; no row is lost.
4. **Given** the all-links export, **When** it completes, **Then** it holds one row per stored link, and filtering it by source or target gives the same rows as that URL's Outlinks or Inlinks in the detail pane (spec 007).
5. **Given** a bulk export is running, **When** the user keeps working or a crawl is running, **Then** the app stays responsive, progress is shown, the export can be cancelled, and a cancelled or failed export leaves no partial files.

---

### User Story 5 - Reports and exports from the command line (Priority: P2)

A developer runs SpiderLens in a scheduled job or build pipeline. One command writes the report (HTML) and the bulk exports (CSV folder or Excel workbook) for a crawl file, and prints the score and per-priority counts as text or JSON, so the pipeline can archive the report and track the score.

**Why this priority**: Everything the app does must be possible from the command line (constitution principle III), and automated reports are the base for scheduling (spec 021).

**Independent Test**: Run the report and export commands on the fixture crawl file and compare their output files byte-for-byte in content (ignoring the generation timestamp) with the app's exports of the same crawl and settings.

**Acceptance Scenarios**:

1. **Given** a crawl file, **When** the user runs the report command with JSON output, **Then** it prints the score, its breakdown, completeness and per-priority counts, matching the Report view.
2. **Given** a crawl file and a list of bulk exports, **When** the user runs the export command, **Then** it writes the same files the app writes for the same choices.
3. **Given** a crawl file that cannot be read, **When** either command runs, **Then** it exits with the existing crawl-file error exit code and writes no partial files.

---

### User Story 6 - Older crawl files (Priority: P3)

A user opens a crawl file made by an older version, for example before page extraction (spec 005) or issues (spec 009). The report still opens: sections whose data the file lacks say "not available for this crawl file" with the reason, and the score states which rules could not run, rather than reporting a perfect site.

**Why this priority**: Old files must keep opening after upgrades (constitution, Product Constraints) and must never imply "no problems" when the truth is "not checked". P3 because fresh crawls do not need it.

**Independent Test**: Open the stored fixture files of each older format, view and export the report, and check that missing data is labelled and the score is marked as partial.

**Acceptance Scenarios**:

1. **Given** a crawl file without extraction data, **When** the report opens, **Then** content sections say they are unavailable and why, and the score breakdown lists the rules that could not run.
2. **Given** a crawl file opened view-only (spec 006), **When** the user views or exports the report, **Then** the file is not modified.

---

### Edge Cases

- **Empty or tiny crawls**: a crawl with one URL, or whose start URL failed, still produces a report; the score is shown as "not available" when there are no internal HTML pages to score, never as 100.
- **Huge crawls**: a 1,000,000-URL crawl with 50,000,000 links reports and exports within the Success Criteria with flat memory; the report file stays a size a mail client accepts by applying the per-issue URL limit.
- **Spreadsheet limits**: cells longer than a spreadsheet accepts are cut at the limit with a visible marker, and the full value stays in the CSV export; sheet names are made valid and unique.
- **Formula injection**: values starting with `=`, `+`, `-` or `@` are neutralised in Excel and CSV exactly as in spec 007.
- **Non-Latin text and right-to-left text**: titles in Hebrew, Arabic, Chinese and other scripts are shown correctly in the report file, PDF and workbook.
- **Destination problems**: an unwritable folder, an existing file or a full disk gives a clear message; existing files are never overwritten without confirmation; nothing partial is left.
- **Settings changed after export**: an exported report records the issue settings and rule set version used, so two reports of the same crawl with different settings can be told apart.
- **External URLs**: external URLs are counted in response codes and link sections but never in the health score.
- **Site-wide issues** (spec 009 rules whose result is the whole site, not pages) appear in the report with their explanation and count as described in FR-010.

## Requirements *(mandatory)*

### Functional Requirements

**Report content**

- **FR-001**: The system MUST produce a report for any crawl (running, paused, stopped, finished or reopened) from the data stored in the crawl file and the issues of spec 009 only; producing a report or export MUST NOT fetch anything and MUST NOT change the stored crawl data.
- **FR-002**: The report MUST be built from an ordered list of sections, each with a title, its figures and links into the results table; later specs (011, 012, 014, 015, 016) MUST be able to add sections without changing existing ones, and a section that cannot be built MUST say why instead of disappearing.
- **FR-003**: The first version MUST contain these sections: **Summary** (start URL, crawl dates and active duration, configuration profile name and main settings, app version, rule set version, URLs found, crawled, blocked, failed and not crawled, average response time, health score); **Crawl completeness** (failures by error class, retries, paused hosts, URLs not crawled per limit, using the counts of spec 007 FR-005 and spec 004); **Issues by priority** (each issue with priority, category, affected URL count, explanation and how to fix); **Top issues** (the issues that cost the most score points, at most 10); **Response codes** (counts of 2xx, 3xx, 4xx, 5xx and no response, internal and external, with redirect chains and loops); **Site structure** (pages per crawl depth, pages with only one inlink, deepest pages); **Content** (counts of pages with missing, duplicate and over-length titles, descriptions and H1, and exact duplicate pages, as reported by spec 009).
- **FR-004**: Every count of URLs in the Report view MUST open the results table filtered to exactly those URLs, using the existing tab, filter or issue that defines them (spec 007, spec 009).
- **FR-005**: Every figure in the report MUST equal the count of the same thing in the results table and the Issues section for the same crawl and settings.
- **FR-006**: While the crawl can still change, the report MUST be marked provisional and refresh at least every 5 seconds; when the crawl ends it MUST become final within the time in SC-002.
- **FR-007**: A report of an incomplete crawl (stopped, paused, limit reached, URLs left failing) MUST say so at the top with the number of URLs not crawled and why, and MUST mark the score as partial.

**Health score**

- **FR-008**: The health score MUST be a whole number from 0 to 100 calculated from the enabled issues on internal HTML pages that returned 200 (the set spec 007 FR-010 uses): each such page loses a penalty equal to the weight of its highest-priority issue (High 1.0, Medium 0.5, Low 0.2, Opportunity 0), and the score is 100 times one minus the average penalty, rounded to the nearest whole number.
- **FR-009**: The report MUST show the score breakdown: points lost per priority and per issue, where an issue's cost is the points the score would regain if no page had it; the breakdown MUST be ordered by cost.
- **FR-010**: Site-wide issues (spec 009 FR-001, result kind site-wide) MUST be listed in the report with their priority and explanation but MUST NOT change the page-based score.
- **FR-011**: The score MUST be deterministic for a given crawl, rule set version and issue settings, MUST be identical in the app and the command line, and MUST record the score formula version so later changes can be told apart (spec 012 compares scores).
- **FR-012**: When there are no pages to score, the score MUST be "not available" with the reason; when rules could not run (spec 009 FR-010), the report MUST list them and mark the score as partial.

**Report file**

- **FR-013**: Users MUST be able to export the report as a single self-contained HTML file and as a PDF file, from the Report view and the main menu.
- **FR-014**: The HTML report MUST open offline in current browsers with no other files, MUST NOT load or reference any external resource, MUST NOT contain scripts that send data anywhere, and MUST print to paper legibly.
- **FR-015**: The report file MUST contain every section of the Report view with the same figures, the explanation and how to fix of every listed issue, and per issue the affected URLs up to a limit (default 100, user-adjustable from 0 to 10,000) with the total and the name of the bulk export holding the full list.
- **FR-016**: Users MUST be able to set a report title and a note shown on the first page; the report MUST show the start URL, crawl date, app version, rule set version, score formula version, generation time and, when a crawl was running, that it is provisional.
- **FR-017**: The report file MUST follow the app's light design tokens (spec 017) with sufficient contrast for printing, and MUST render right-to-left and non-Latin text correctly.

**Bulk exports**

- **FR-018**: Users MUST be able to choose, in one Bulk export dialog, any combination of: all tabs (every tab of spec 007 with filter All and all columns), all issues (every enabled issue with affected URLs and evidence), all links, all images, redirect chains (one row per chain with every hop and the final destination and state), response headers (one row per URL and header), and the crawl summary.
- **FR-019**: Bulk exports MUST be available as a folder of CSV files with predictable names, or as one Excel workbook with an overview sheet listing every sheet with its row count and, for issues, priority, explanation and how to fix.
- **FR-020**: A tab or issue exported in bulk MUST have the same rows, column names and values as the app's single export of that tab or issue (spec 007 FR-028, spec 009 FR-015), with all its columns.
- **FR-021**: The all-links export MUST hold one row per stored link with source URL, target URL, anchor text or image alt text, rel attributes, followed or not, internal or external, link type (hyperlink, image, canonical, hreflang, redirect and the other types spec 005 stores), and the target's status code and final state.
- **FR-022**: Excel output MUST keep numbers as numbers and dates as dates, freeze the header row, and split a table larger than a sheet allows over numbered continuation sheets with the same header; no row may be lost.
- **FR-023**: CSV output MUST follow spec 007 FR-029 (UTF-8 that spreadsheets open with non-Latin text intact, correct quoting, formula neutralisation); Excel output MUST neutralise formulas in the same way.
- **FR-024**: Exports and report generation MUST run without blocking the interface or the crawl, show progress, be cancellable, and leave no partial file or folder when cancelled or failed; an export started during a crawl MUST contain the data as it was when it started (spec 007 FR-030).
- **FR-025**: Exports MUST NOT overwrite an existing file or a non-empty folder without the user's confirmation.

**Command line**

- **FR-026**: The command line MUST print a crawl file's report summary (score, breakdown, completeness, counts per priority, top issues) as text or JSON.
- **FR-027**: The command line MUST write the report file (HTML) and any bulk export (CSV folder or Excel workbook) with the same options and content as the app, taking issue settings and report options from a profile or settings file.
- **FR-028**: Command-line failures MUST use the existing exit codes for usage errors, crawl-file errors and local problems such as a full disk (specs 001, 006), and MUST leave no partial output.

**Performance and scale**

- **FR-029**: Report figures and exports MUST be computed from stored data after pages are stored, never in the fetch path; viewing the report or running an export during a crawl MUST NOT reduce crawl speed by more than 5% on the spec 008 benchmark.
- **FR-030**: Report generation and exports MUST stream data and MUST NOT load a whole crawl, issue or link table into memory; memory use MUST stay flat as the crawl grows (constitution principle VI).

**Interface**

- **FR-031**: The sidebar MUST have a Report entry above the results tabs; when a crawl ends the app MUST show a notice offering to open the report, and MUST NOT switch away from what the user is viewing unless they turned on "Open report when a crawl ends" in settings.
- **FR-032**: The Report view, the export dialogs and their options MUST be fully operable by keyboard and announced correctly by screen readers, follow the design system (spec 017) in every theme and palette (spec 018), and use only design tokens for colour.

### Key Entities

- **Report**: the set of sections for one crawl at one moment, with the issue settings, rule set version and score formula version used, and whether it is final or provisional.
- **Report section**: one titled part of the report (summary, completeness, issues, response codes, structure, content, and later sections from other specs) with its figures, links into the results table, and an availability state with a reason.
- **Health score**: a whole number from 0 to 100 with its breakdown per priority and per issue, the formula version, and whether it is partial.
- **Report file**: an exported, self-contained HTML or PDF rendering of a report with its title, note and per-issue URL limit.
- **Bulk export**: a chosen set of tables (tabs, issues, links, images, redirect chains, headers, summary), a format (CSV folder or Excel workbook), a destination, and its progress.

## Success Criteria *(mandatory)*

### Measurable Outcomes

Timings are measured on the reference machine defined in spec 007, Assumptions.

- **SC-001**: On the report fixture site, 100% of report figures, score values and link targets equal the expected values, in the Report view, the HTML and PDF files and the command line.
- **SC-002**: The final report of a 100,000-URL crawl is shown within 3 seconds of its issues being final (spec 009 SC-003); for a 1,000,000-URL crawl file within 10 seconds, with the summary shown within 2 seconds and the interface responsive throughout.
- **SC-003**: Exporting the report as HTML or PDF for a 100,000-URL crawl takes under 10 seconds, and the HTML file with the default URL limit is under 10 MB.
- **SC-004**: Bulk exporting all tabs and all issues of a 100,000-URL crawl to Excel completes within 60 seconds; the all-links export of a crawl with 10,000,000 links to CSV completes within 120 seconds.
- **SC-005**: Memory used by report generation and any export of a 1,000,000-URL crawl is no more than 20% higher than for a 100,000-URL crawl.
- **SC-006**: With the Report view open and a bulk export running, crawling the 100,000-page local fixture site runs at no less than 95% of the speed of the same crawl without them (spec 008 benchmark).
- **SC-007**: Every bulk-exported tab and issue matches the app's single export of the same tab or issue row for row and column for column on every fixture crawl, and the exported workbook and CSV files open in common spreadsheet applications with non-Latin text, commas, quotes and line breaks intact.
- **SC-008**: The score equals the FR-008 formula, and its breakdown adds up to 100 minus the score within 1 point, on every fixture crawl.
- **SC-009**: A first-time user can open the report of a finished crawl, find the issue costing the most points, and export a client report file in under 2 minutes without help.

## Assumptions

- **Issues come from spec 009**: this spec adds no rule, no extraction and no fetching. Content and structure figures that 009 does not provide (depth spread, inlink counts) come from data specs 003 to 007 already store.
- **Health score formula**: the per-page worst-issue penalty with weights 1.0 / 0.5 / 0.2 / 0 is the default chosen here because it is simple to explain ("72% of the way to no problems"), cannot go below 0, does not punish a site twice for many issues on one page, and lets a fix's effect be predicted. Opportunity issues never lower the score. The weights are not user settings in this version; the formula carries a version so a later change stays comparable.
- **PDF**: the PDF is produced on the user's machine from the same content as the HTML report; the command line writes HTML, and PDF from the command line is a later concern.
- **Report title, note and URL limit** are report options remembered by the app; logos and custom branding (white-label reports) are out of scope for this version.
- **No storage change**: the report is computed from the crawl file on demand and is not stored in it, so this spec needs no schema step; if the plan finds a stored summary is needed for speed, it takes a migration step at merge time under the project's schema rule.
- **Out of scope**: change since the last crawl and score history (spec 012), orphan pages and sitemap sections (spec 011), AEO/GEO and AI agent sections (specs 015, 016), Google Sheets and Looker Studio exports (spec 024, integrations), scheduled automatic exports (spec 021), charts beyond simple bars in the report, report templates, and translation of report text (English, like the rest of the UI).
- **Naming**: product text never refers to other crawler products by name.
