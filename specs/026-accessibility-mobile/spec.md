# Feature Specification: Accessibility and Mobile Usability Checks

**Feature Branch**: `026-accessibility-mobile`

**Created**: 2026-10-13

**Status**: Approved 2026-10-13 (Ryware), recommended defaults for all open decisions

**Input**: Owner approval of the feature gap report (rows 20 and 21, "Accessibility auditing (axe rules, WCAG)" and "Mobile usability"), 2026-10-13: run an accessibility rule engine on each rendered page during the JavaScript rendering phase (spec 014), store the violations of each page with their WCAG level and user impact, add them to the issue rule catalogue (spec 009), add mobile usability checks measured on the rendered layout (viewport, tap targets, font sizes, content wider than the screen), and offer a subset that works without rendering from the HTML that spec 005 already parses (alternative text, language, heading order, form labels). The checks must not cost crawl speed when they are off, and each page gets a time budget.

## Overview

Accessibility problems are both a legal risk (the European Accessibility Act applies from June 2025, the ADA in the United States, equivalent laws elsewhere) and a quality signal that SEO teams are increasingly asked to report on. Most of these problems can only be seen on the page a browser shows: colour contrast, names computed from ARIA, elements hidden or shown by scripts, sizes of touch targets. Some can be seen in the HTML as served: a missing language, an image without alternative text, a form field without a label.

This feature adds two levels of checks:

- **Static checks** read the HTML that SpiderLens already parses for every page. They work in every crawl, cost almost nothing and need no browser.
- **Full checks** run a recognised accessibility rule engine inside the page while it is being rendered (spec 014), and measure the rendered layout for mobile usability. They need the JavaScript rendering mode.

Both levels write their findings to the crawl file, show them in an Accessibility tab and a Mobile tab, and feed the Issues area of spec 009 with one rule per check, each tagged with the WCAG success criteria and level it tests.

Automated checks find only part of the accessibility problems on a site. SpiderLens says this wherever it shows accessibility results, and never presents a page without violations as "compliant".

This feature depends on page extraction (005), crawl storage (006), the results table (007), crawl performance (008), the issue rules engine (009) and JavaScript rendering (014). Exports and the end-of-crawl report (010) and agent access (016) read its stored data.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Find the accessibility violations of a whole site (Priority: P1)

A digital agency must report on the accessibility of a client's site against WCAG 2.2 level AA. They turn on JavaScript rendering and full accessibility checks and crawl the site. Every rendered page is checked inside the browser. The Accessibility tab lists each page with its number of violations by user impact (critical, serious, moderate, minor), the WCAG level it fails, and the number of results that need a person to review. The Issues area lists every violated check, for example "Text has insufficient colour contrast (WCAG 1.4.3, AA)", with the number of pages and elements affected.

**Why this priority**: This is the reason the feature exists, and the gap report rates it P2 against Screaming Frog, which offers the same audit.

**Independent Test**: Crawl a local fixture site whose pages contain known violations (low contrast text, buttons without names, images without alternative text added by a script, a form field without a label, an ARIA attribute with an invalid value) and known clean pages, with rendering and full checks on. Compare the stored violations with the expected list per page.

**Acceptance Scenarios**:

1. **Given** a page with grey text on a white background at a contrast ratio of 2.5 to 1, **When** it is crawled with full checks, **Then** a "colour contrast" violation is stored for that page with impact "serious", WCAG 1.4.3 level AA, and the element that fails.
2. **Given** a page whose only button is an icon with no text and no accessible name, added by a script, **When** it is crawled with full checks, **Then** a "button has no accessible name" violation is stored with impact "critical" and the element, even though the button is absent from the raw HTML.
3. **Given** a crawl with full checks, **When** the user opens the Accessibility tab, **Then** each audited page shows its violations by impact, the number of distinct checks failed, the number of elements affected, the number of results needing review and the lowest WCAG level failed.
4. **Given** the WCAG target chosen for the crawl is 2.1 AA, **When** results are shown, **Then** checks that belong to WCAG 2.2 or to level AAA are not counted as violations of the target, and the user can still see them by switching the target.
5. **Given** a page where the rule engine cannot decide a check (for example text over a background image), **When** it is crawled, **Then** the result is stored as "needs review", not as a violation, and is listed separately.
6. **Given** a crawl with full checks, **When** it is run from the command-line interface, **Then** the same pages are checked with the same settings and the same results are stored.

---

### User Story 2 - Check accessibility basics without rendering (Priority: P1)

A user crawls a large site in raw HTML mode for speed and still wants to see the accessibility basics that can be read from the HTML: images without alternative text, a missing or invalid page language, skipped heading levels, form fields without labels, links and buttons without text, frames without titles, a viewport that blocks zooming.

**Why this priority**: Most crawls run without rendering because it is much faster. The static checks give useful accessibility data on every crawl at almost no cost.

**Independent Test**: Crawl the fixture site in raw HTML mode with static checks on. Compare the stored static findings with the expected list, and verify no browser was started.

**Acceptance Scenarios**:

1. **Given** a page with `<input type="email" id="e">` and no label, `aria-label`, `aria-labelledby` or `title`, **When** it is crawled with static checks, **Then** a "form field has no label" finding is stored with the field.
2. **Given** a page with `<html lang="english">`, **When** it is crawled with static checks, **Then** an "invalid language code" finding is stored.
3. **Given** a page with `<meta name="viewport" content="width=device-width, user-scalable=no">`, **When** it is crawled with static checks, **Then** a "zooming disabled" finding is stored.
4. **Given** a crawl in raw HTML mode, **When** the user opens the Accessibility tab, **Then** it shows the static results and states that full checks need JavaScript rendering, with a way to turn it on for the next crawl or to audit this crawl later (User Story 5).
5. **Given** static checks are off, **When** the crawl runs, **Then** no accessibility data is collected or written and crawl speed is unchanged.

---

### User Story 3 - See how the site works on a phone (Priority: P2)

A user wants to know whether pages are usable on a phone: whether they declare a viewport for mobile, whether content is wider than the screen so the user must scroll sideways, whether text is too small to read and whether links and buttons are too small or too close to tap. With rendering on at a mobile window size, the Mobile tab lists each page with these findings and the elements involved.

**Why this priority**: Mobile usability is part of the gap report (P3) and the measurements come from the same rendered page as the accessibility checks at a small extra cost. Google retired its own Mobile Usability report in 2023 and the related Lighthouse audits since, so these checks are presented as user experience and accessibility findings, not as ranking factors.

**Independent Test**: Crawl fixture pages built to fail one mobile check each (no viewport, a 1,200-pixel-wide table, body text at 10 pixels, 16-pixel icon links packed together) and one clean page, rendered at the mobile window size. Compare the stored findings with the expected ones.

**Acceptance Scenarios**:

1. **Given** a page with no viewport meta tag, **When** it is crawled with mobile checks (with or without rendering), **Then** a "no mobile viewport" finding is stored.
2. **Given** a page rendered at the mobile width of 411 pixels whose content is 1,200 pixels wide, **When** it is crawled with mobile checks, **Then** a "content wider than the screen" finding is stored with the measured content width and the widest element.
3. **Given** a page where most visible text is set at 10 pixels, **When** it is crawled with mobile checks, **Then** a "text too small to read" finding is stored with the share of text below the threshold.
4. **Given** a page with links that are 16 by 16 pixels and touch each other, **When** it is crawled with mobile checks, **Then** a "tap targets too small" finding is stored with the number of targets affected and examples, following WCAG 2.5.8 (24 by 24 pixels, or enough spacing).
5. **Given** a crawl rendered at a desktop window size, **When** the user opens the Mobile tab, **Then** layout checks are marked "not measured: rendered at desktop size" and only the static viewport checks are shown.

---

### User Story 4 - Fix one page (Priority: P2)

A developer receives a list of pages to fix. They select a page and the detail pane shows each violation with what it means, who it affects, the WCAG success criteria, the elements that fail (a selector and a short piece of their HTML), and a link to the rule's documentation. With screenshots on (spec 014), the failing elements are outlined on the page screenshot.

**Why this priority**: Without element-level detail, a count of violations does not help anyone fix them.

**Independent Test**: Select fixture pages with known violations and check every field shown in the detail pane against the expected values.

**Acceptance Scenarios**:

1. **Given** a selected page with a contrast violation on three elements, **When** the user opens the Accessibility section of the detail pane, **Then** the violation shows its description, impact, WCAG criteria and level, the three elements with selector and HTML excerpt, and the measured and required contrast ratio.
2. **Given** a page with more failing elements than the stored limit for one check, **When** it is shown, **Then** the stored elements are listed and the total count is shown with a note that the rest were not stored.
3. **Given** screenshots were saved for the crawl, **When** the user selects a failing element, **Then** its outline is drawn on the screenshot when its position is known.

---

### User Story 5 - Audit an existing crawl (Priority: P3)

A user has a crawl file made without accessibility checks. They run the checks afterwards on that crawl, without crawling again: static checks from the stored HTML, and full checks by rendering from the stored HTML as spec 014 allows (FR-006 there).

**Why this priority**: Useful but not essential; the same result is reachable by crawling again.

**Independent Test**: Crawl the fixture site with HTML stored and checks off, run the checks on the file, and compare with a crawl that had the checks on.

**Acceptance Scenarios**:

1. **Given** a crawl file with stored HTML and no accessibility data, **When** the user runs static checks on it, **Then** the stored findings equal those of a crawl made with static checks on, and no request is made to the site.
2. **Given** a crawl file without stored HTML, **When** the user asks to run checks on it, **Then** the action is refused with an explanation.
3. **Given** a crawl file opened view-only (spec 006), **When** the user asks to run checks, **Then** the action is refused with an explanation.

---

### User Story 6 - Accessibility in continuous integration (Priority: P3)

A team runs SpiderLens in a build pipeline against a staging site and wants the build to fail when a new critical or serious violation appears.

**Why this priority**: The command line and the issue exit code from spec 009 make this possible with little extra work.

**Independent Test**: Run the command-line crawl with full checks against the fixture site and assert the exit code and the printed summary.

**Acceptance Scenarios**:

1. **Given** the command line with accessibility checks and a fail-on priority (spec 009), **When** the crawl finds a violation mapped to that priority or higher, **Then** the process exits with the issues exit code from spec 009 and prints the violations by check and page count.
2. **Given** the command line, **When** the user asks for the accessibility results of a crawl file, **Then** it prints or exports (CSV or JSON) every violation with page, check, impact, WCAG criteria and element.

---

### Edge Cases

- **Rendering off with full checks chosen**: the crawl runs static checks only and says so before it starts; it never starts a browser for accessibility alone.
- **Pages not rendered** (outside rendering rules, render failed, render timed out, spec 014 FR-039): get static checks only, and the Accessibility tab shows why full checks did not run.
- **Check time budget reached**: the page is marked "Check timed out" with the time spent; no partial violation list is stored for it, so counts are never silently incomplete; static results are kept.
- **Very large pages** (tens of thousands of elements): the time budget applies; the number of elements stored per check and per page is capped; totals are always stored.
- **Images and fonts during rendering**: loaded when checks are on (FR-012); an image that fails to load leaves contrast over it undecided, stored as "needs review".
- **Frames**: same-origin frames are checked as part of the page; cross-origin frames are recorded as "not checked" and the frame itself is still checked for a title.
- **Shadow DOM**: open shadow roots are checked; closed ones cannot be reached and are not reported as clean.
- **Bot challenge pages** (spec 014 FR-044): checks are not run on a page flagged as a possible bot challenge, so a challenge page's violations are not reported as the site's.
- **Non-HTML resources, external URLs, redirects and error pages**: not checked, unless spec 014's "render error pages" option is on, in which case rendered error pages are checked like other pages.
- **Right-to-left pages**: checks and layout measures work the same; "content wider than the screen" also catches overflow to the left.
- **Duplicate checks between static and full levels**: when both levels test the same thing on a page (for example image alternative text), the full result is used for that page and the static one is kept only as evidence, so the page is not counted twice.
- **Older crawl files**: open with the tabs explaining that the crawl has no accessibility data; newer files with data from a newer rule engine version open with the stored results, which are never re-labelled.

## Requirements *(mandatory)*

### Functional Requirements

#### Settings

- **FR-001**: Users MUST be able to choose, per crawl, one of three accessibility levels: "Off", "Static checks" or "Full checks". The choice MUST be stored with the crawl's configuration and be available from the command-line interface with the same default. The default level is listed under Assumptions.
- **FR-002**: "Full checks" MUST require the JavaScript rendering mode (spec 014). When rendering is off, the system MUST run static checks only, MUST say so before the crawl starts, and MUST NOT start a browser engine for accessibility.
- **FR-003**: Users MUST be able to choose the WCAG target used to count violations: WCAG 2.0, 2.1 or 2.2, at level A, AA or AAA (default 2.2 AA), and whether "best practice" checks that are not tied to a WCAG success criterion are included (default included, shown separately).
- **FR-004**: Users MUST be able to turn mobile usability checks on or off independently (default on whenever accessibility checks are on).
- **FR-005**: Users MUST be able to set the time budget for all in-page checks of one page (accessibility and mobile together), from 1 to 30 seconds (default 5 seconds), separate from the maximum render time of spec 014. This budget is the page's total check allowance that the rendering stage enforces for every in-page check, including those of other features.

#### Static checks

- **FR-006**: When the level is "Static checks" or "Full checks", the system MUST evaluate, for every internal HTML page with a 2xx status, from the HTML already parsed by spec 005 and without parsing it a second time, at least: images and image buttons without alternative text; image-map areas without alternative text; a missing, empty or invalid page language; an invalid language on an element; skipped heading levels; form fields (input, select, textarea) without a label or accessible name; buttons and links without discernible text; frames and iframes without a title; a viewport that disables zooming or limits maximum zoom below 2; a positive `tabindex`; `accesskey` values used twice; a page title that is missing or empty; a `meta refresh` that redirects or reloads with a delay; deprecated elements that harm accessibility (`blink`, `marquee`); and embedded plugins (`object`, `embed`, `applet`) that phones do not show.
- **FR-007**: Checks that spec 009 already defines (missing alternative text, missing language attribute, non-sequential headings, missing title) MUST NOT be duplicated as new rules: those rules MUST be tagged with their WCAG success criteria so they appear in the accessibility views, and the static checks of this feature MUST add only the checks 009 does not have.

#### Full checks

- **FR-008**: When the level is "Full checks", the system MUST run a recognised, maintained, open-source accessibility rule engine on each rendered page after the page has settled (spec 014 FR-010) and before its rendering context is closed, in the same browser context, without requesting the page again and without performing clicks, typing or scrolling. The engine's name and version MUST be stored with the crawl.
- **FR-009**: Full checks MUST NOT be able to change what spec 014 stores for the page (rendered HTML, extracted values, links, screenshot): the rendered HTML and screenshot MUST be captured before the checks run or be unaffected by them.
- **FR-010**: For each page and each check that fails, the system MUST store: the check's identifier, the user impact (critical, serious, moderate or minor), the WCAG success criteria and level it maps to, or "best practice", the number of failing elements, and for up to 20 elements per check (and up to 500 per page) a selector, an HTML excerpt of at most 300 characters, the check-specific data (for example measured and required contrast ratio), and the element's position and size on the page when known.
- **FR-011**: Results the engine cannot decide MUST be stored as "needs review", separately from violations, with the same details, and MUST NOT count as violations or issues by default.
- **FR-012**: When full checks or mobile checks are on, the rendering MUST load web fonts and images, because sizes, layout and contrast depend on them, even when spec 014's default blocks them; media (video, audio) follow the spec 014 settings. Images and fonts still go through spec 014's host limits, robots.txt rules and resource cache.
- **FR-013**: When the time budget (FR-005) is reached, the system MUST stop the checks for that page, record "Check timed out" with the time spent, store no partial list of violations, and continue with the next page.
- **FR-014**: Pages flagged by spec 014 as a possible bot challenge MUST NOT be checked and MUST be recorded "Not checked: possible bot challenge".

#### Mobile usability

- **FR-015**: When mobile checks are on, the system MUST report from the HTML: no viewport meta tag; a viewport without `width=device-width` (or a fixed width); zooming disabled (shared with FR-006); embedded plugins (shared with FR-006).
- **FR-016**: When mobile checks are on and the page was rendered at a mobile window size (spec 014 FR-008), the system MUST measure on the rendered layout and report: content wider than the screen (document width larger than the window width by more than a small tolerance, default 5 CSS pixels), with the measured width and the widest element; text too small to read (share of visible text with a computed size below a threshold, default 12 CSS pixels, above a share, default 40%), with the share; and tap targets too small (links, buttons and other interactive elements smaller than 24 by 24 CSS pixels without enough spacing, per WCAG 2.5.8), with the count and up to 20 examples.
- **FR-017**: When the page was rendered at a desktop window size, layout checks MUST be recorded "Not measured: rendered at desktop size" and MUST NOT be reported as passed.

#### Results and issues

- **FR-018**: Every static, full and mobile check MUST be a rule in the spec 009 rule catalogue, in the categories "Accessibility" and "Mobile", with stable identifiers, a default priority, its WCAG criteria and level, explanation, how to fix, and evidence. Default priorities MUST follow impact: critical to High, serious to Medium, moderate and minor to Low, best practice to Opportunity; mobile checks Low, except "content wider than the screen" Medium. Users MUST be able to change them as for every 009 rule.
- **FR-019**: The rule catalogue MUST record the accessibility engine version its rules come from; a newer engine version that adds or removes checks MUST change the catalogue version (spec 009 FR-003), and identifiers of removed checks MUST NOT be reused.
- **FR-020**: Accessibility issues MUST count only checks inside the WCAG target (FR-003); checks outside it MUST remain visible in the Accessibility tab and detail pane and be selectable by switching the target, without checking again.
- **FR-021**: The system MUST show an Accessibility tab with one row per internal HTML page, with default columns Address, Check level (static, full, not checked and why), Violations, Critical, Serious, Moderate, Minor, Needs review, Checks failed, Lowest WCAG level failed and Check time; and filters: All; Checked; Not checked; Check timed out; Has violations; one filter per WCAG version and level (2.0 A, 2.0 AA, 2.1 A, 2.1 AA, 2.2 AA, AAA); Best practice; one filter per impact; Needs review. Every filter MUST show its count, refreshed as defined by spec 007.
- **FR-022**: The system MUST show a Mobile tab with one row per internal HTML page, with default columns Address, Viewport, Measured (yes or why not), Content width, Small text share, Small tap targets and Mobile issues; and filters: All; No viewport; Viewport not device width; Zoom disabled; Content wider than screen; Text too small; Tap targets too small; Plugins; Not measured.
- **FR-023**: The detail pane MUST add an Accessibility section and a Mobile section for the selected page, with every result as in User Story 4, grouped by check and ordered by impact, with a link to the check's public documentation, opened in the user's browser only when clicked.
- **FR-024**: When screenshots were stored for the page (spec 014 FR-033), the detail pane MUST be able to outline a selected failing element on the screenshot when its position was stored.
- **FR-025**: Every place that shows accessibility results (tab, detail pane, export, command line summary) MUST state that automated checks find only part of accessibility problems and that a page without violations is not necessarily accessible.
- **FR-026**: Users MUST be able to export, from the desktop app and the command-line interface, all violations (one row per page, check and element), the violations of one WCAG level, all "needs review" results, and a summary per check with page count, element count, impact, priority and one example page, as CSV and JSON.

#### Speed, storage and other surfaces

- **FR-027**: When the level is "Off", the system MUST NOT collect, compute or write any accessibility or mobile data, and crawl throughput and memory MUST be within the normal run-to-run variation of a build without this feature.
- **FR-028**: Static checks MUST run as part of the existing extraction of each page, MUST NOT parse the page again, and MUST NOT slow the raw crawl by more than the limit in SC-003.
- **FR-029**: Full and mobile checks MUST run inside the rendering stage of spec 014 and MUST NOT make the fetch stage wait (spec 014 FR-035); their time counts toward rendering, and the progress display MUST show the number of pages checked and the average check time.
- **FR-030**: All results MUST be stored in the crawl file so later features (report 010, comparison 012, agents 016) can read them without checking again; the stored data MUST include the engine version and settings used. Crawl files without accessibility data MUST keep opening, with the tabs explaining that the crawl has no accessibility data.
- **FR-031**: Users MUST be able to run static checks and full checks on an existing crawl file with stored HTML (User Story 5), replacing that file's previous accessibility results after confirmation; this MUST be refused for files without stored HTML and for files opened view-only.
- **FR-032**: Every capability in this spec MUST be available from the command-line interface with the same defaults, without opening a window, and MUST produce the same stored data as the desktop app for the same crawl.
- **FR-033**: The end-of-crawl summary MUST include pages checked by level, pages not checked by reason, total violations by impact, the top checks by number of pages, and the share of pages with at least one violation of the WCAG target.

### Key Entities *(include if feature involves data)*

- **Accessibility audit (per page)**: page, level run (static, full), state (checked, timed out, not checked with reason), engine name and version, check time, counts by impact and needs-review count, lowest WCAG level failed.
- **Accessibility result**: page, check identifier, kind (violation or needs review), source (static or full), impact, WCAG criteria and level or best practice, failing element count.
- **Failing element**: result, selector, HTML excerpt, check-specific data, position and size when known. Capped per check and per page.
- **Mobile measurement (per page)**: viewport content and state, window used (mobile or desktop), measured or not with reason, content width, widest element, small-text share, small tap target count and examples.
- **Accessibility settings**: level, WCAG target, best practice on or off, mobile checks on or off, time budget, thresholds; stored with the crawl configuration.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: On the accessibility fixture suite (at least 60 pages with known violations covering every static check and at least 30 engine checks, plus at least 10 clean pages), 100% of expected violations are found with the expected check, impact and WCAG criteria, and clean pages report no violations.
- **SC-002**: With the level "Off", the 100,000-page benchmark from spec 008 stays within 2% of the stored baseline throughput and 5% of its peak memory (median of 7 runs), and no accessibility data is written.
- **SC-003**: With "Static checks" in raw HTML mode, the same benchmark is at most 3% slower than with "Off".
- **SC-004**: With "Full checks" and mobile checks on, the median extra time per rendered page is at most 25% of the median render time on the rendered fixture site, and no page's checks run longer than its time budget plus 1 second.
- **SC-005**: On the mobile fixture pages, 100% of expected mobile findings are reported with measured values within 2 CSS pixels (widths) or 2 percentage points (shares) of the expected values.
- **SC-006**: Results of the same crawl data, engine version and settings are identical between the desktop app and the command-line interface and between two runs.
- **SC-007**: For every check, a user can reach from an issue to a failing element on a page in at most three clicks.

## Assumptions

- **Default level**: "Static checks" by default, because the cost target in SC-003 is small and the basics are useful on every crawl; "Full checks" are selected by the user and need rendering. If the benchmark shows static checks cost more than SC-003 allows, the default becomes "Off". Listed as an owner decision in research.md.
- **Rule engine**: the full checks use axe-core, the open-source engine Screaming Frog, Lighthouse and most browser extensions use, pinned to one version per release; the comparison with alternatives and the licence analysis are in research.md. The spec requires only "a recognised, maintained, open-source engine" (FR-008).
- **Language of check texts**: English in the first version, as the rest of the app; the engine's translated texts can be offered later.
- **Mobile thresholds**: 24 by 24 CSS pixels for tap targets (WCAG 2.2, 2.5.8 AA), 12 CSS pixels for small text with 40% of text as the limit (the former Lighthouse rule), 5 CSS pixels of overflow tolerance. To be confirmed in planning against the fixture suite.
- **Mobile window**: layout checks use the mobile window size of spec 014 (411 by 731 CSS pixels for the default mobile size). No second rendering at another size is made; crawls rendered at a desktop size do not get layout checks (FR-017).
- **Not a compliance certificate**: the product reports automated findings only; it does not claim WCAG or legal conformance.

## Out of Scope

- A single accessibility score per page or site (the Lighthouse score comes through the PageSpeed integration of spec 024; a SpiderLens score may come with the report in spec 010).
- Checks that need interaction: keyboard traps, focus order while tabbing, content shown on hover or focus, timing of moving content. Spec 014 forbids input events.
- Screen reader simulation, colour-blindness simulation and automatic fixes.
- Accessibility of PDF and other documents (the gap report lists PDF separately).
- Rendering a second time at another window size to measure mobile layout on a desktop-rendered crawl.
- Per-element ignore lists; per-URL ignore stays out of scope as in spec 009.
