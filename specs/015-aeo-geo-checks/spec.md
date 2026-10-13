# Feature Specification: AEO and GEO Checks

**Feature Branch**: `015-aeo-geo-checks`

**Created**: 2026-10-13

**Status**: Approved 2026-10-13 (Ryware), recommended defaults for all open decisions

**Input**: User description: "Answer engine optimisation (AEO) and generative engine optimisation (GEO): what SpiderLens checks and reports so a site is found, read and cited by AI answer engines and generative search. Robots rules for AI crawlers (GPTBot, ClaudeBot, PerplexityBot, Google-Extended and the others) and their user agents; llms.txt; structured data and entity clarity; content that is answerable and citable without JavaScript; freshness and author signals; per-page and site-wide scores with explanations. Use the JavaScript dependence data from spec 014 when the crawl was rendered. Crawl speed is the top priority: checks run from stored data, never in the fetch path."

## Overview

People increasingly get answers from AI systems instead of a list of links: ChatGPT search, Perplexity, Google AI Overviews and AI Mode, Gemini, Claude and Microsoft Copilot. These systems send their own crawlers, read pages differently from a browser, and quote or cite a small number of sources per answer. A site owner now has three new questions an SEO crawler does not answer today:

1. **Can the AI systems reach my pages?** Each vendor has several crawlers with different jobs (training a model, building a search index, fetching a page because a user asked), each obeying its own robots.txt token. A site can block training and still allow AI search, or block everything by accident through a single rule or a firewall.
2. **Can they read my pages?** Most AI crawlers do not run JavaScript. Content that only appears after scripts run is invisible to them, even when Google sees it.
3. **Are my pages worth citing?** Answer engines favour pages whose answers are easy to lift out (a clear heading followed by a direct answer, lists, tables), that state facts with numbers and sources, that say who wrote them and when, and that describe the entities they are about with structured data.

This feature adds an **AI search** area to SpiderLens that answers these three questions for every crawled page and for the site as a whole, with a transparent score per page and per site, and an explanation for every point lost. It adds no new work to the crawl itself apart from fetching a few small site files once per host (llms.txt). Every check reads what the crawl already stored.

The checks are deterministic rules, not an AI model's opinion: the same crawl always produces the same results, with no account, no API key and no network access needed. Each check states what it is based on (a vendor's published documentation, published research, or common practice), so the user can judge how much weight to give it. Asking an AI model to judge a page is part of spec 016 (agent access and LLM scoring), not this spec.

This feature belongs to milestone M5. It builds on page extraction (005), crawl storage (006), the results table (007) and, when a crawl was rendered, JavaScript rendering (014). Its findings are issue rules in the issue rules engine (009) when that exists. It owns the list of known AI crawlers that agent access (016) also uses.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - See which AI crawlers can reach the site (Priority: P1)

A site owner hears that their brand never appears in ChatGPT or Perplexity answers. They crawl their site and open the AI search overview. A table shows every known AI crawler, grouped by vendor and by purpose (model training, AI search, user-requested fetch), with whether the site's robots.txt allows it for the home page and what share of the crawled pages it may fetch. They see at once that a rule written to block model training also blocks OpenAI's and Anthropic's search crawlers, so their pages cannot appear in those answers. The overview says which rule causes it, on which line of robots.txt.

**Why this priority**: Access comes first: a page an AI search crawler may not fetch cannot be cited, whatever its quality. It is also the most common and most costly mistake, it is easy to check exactly, and it needs nothing but the robots.txt files the crawl already stores.

**Independent Test**: Crawl local fixture sites whose robots.txt files allow, block and partly block known AI crawlers in different ways (by name, by wildcard, by path, with conflicting groups), and check that the access table matches the expected verdict, share and deciding rule for every crawler.

**Acceptance Scenarios**:

1. **Given** a robots.txt with `User-agent: GPTBot` / `Disallow: /` and no other groups, **When** the site is crawled, **Then** the access table shows GPTBot as blocked on 100% of crawled pages with that rule as the reason, and shows OAI-SearchBot, ChatGPT-User, ClaudeBot and every other crawler as allowed.
2. **Given** a robots.txt whose only group is `User-agent: *` / `Disallow: /`, **When** the site is crawled with a custom user agent that ignores robots.txt on the user's say-so, **Then** every AI crawler is shown as blocked by the wildcard group, and the overview says that crawlers which honour robots.txt cannot fetch any page.
3. **Given** a robots.txt that blocks `/blog/` for ClaudeBot only, **When** the site is crawled, **Then** ClaudeBot is shown as partly blocked, with the share and count of crawled pages it may not fetch, a link to the list of those pages, and the rule.
4. **Given** a robots.txt that blocks `Google-Extended`, **When** the site is crawled, **Then** the overview explains that this stops Google from using the content for Gemini model training and grounding but does not remove pages from Google Search or AI Overviews, which depend on Googlebot, and shows Googlebot's own verdict next to it.
5. **Given** a crawl of a site with several hosts (for example `www` and `blog` subdomains), **When** the user opens the access table, **Then** verdicts are shown per host, because each host has its own robots.txt.
6. **Given** a host whose robots.txt could not be fetched (server error), **When** the access table is shown, **Then** that host is marked "robots.txt unavailable: compliant crawlers fetch nothing" rather than allowed or blocked by a rule.
7. **Given** any page, **When** the user opens it in the detail pane, **Then** the AI search section lists, for each AI crawler, whether robots.txt allows that page, and whether the page's robots meta tags or `X-Robots-Tag` headers address that crawler or restrict snippets.

---

### User Story 2 - See what an AI crawler can read without JavaScript (Priority: P1)

An SEO consultant audits a site built with a JavaScript framework. Google shows its pages, but AI answers never cite it. In the AI search tab they filter "Main content missing without JavaScript" and see that the product pages' descriptions, prices and FAQ answers exist only after rendering. Because AI crawlers that do not run JavaScript see nothing of that, those pages are marked as unreadable for them, and the page score explains why.

**Why this priority**: After access, readability is the most common reason a page is not used by AI systems, and SpiderLens is placed to show it exactly because it can compare raw and rendered pages (spec 014). It is the main difference from tools that only check a page as a browser sees it.

**Independent Test**: Crawl fixture pages ranging from fully server-rendered to fully client-rendered, once without rendering and once with rendering, and check the readability verdict, the evidence shown and the score for every page against expected values.

**Acceptance Scenarios**:

1. **Given** a crawl with rendering on, and a page whose JavaScript dependence (spec 014) is 90%, **When** the AI checks run, **Then** the page is listed under "Main content missing without JavaScript", the AI crawlers that do not run JavaScript are marked as seeing only 10% of the page's text, and the page score explains the loss.
2. **Given** a crawl with rendering on, and a fully server-rendered page, **When** the AI checks run, **Then** the page passes the readability check for every crawler.
3. **Given** a crawl without rendering, and a page whose raw HTML has almost no visible text but loads large scripts and has an empty application container, **When** the AI checks run, **Then** the page is listed under "Possibly built with JavaScript" with the evidence found, and the explanation recommends a rendered crawl to confirm; no JavaScript dependence percentage is invented.
4. **Given** a crawl with rendering on, **When** the user looks at any AI check for a page, **Then** the check states whether it used the raw HTML (what crawlers without JavaScript see) or the rendered page (what Google sees), and checks for crawlers that do not run JavaScript always use the raw HTML.
5. **Given** a page whose title, main heading or structured data exist only after rendering, **When** the AI checks run, **Then** each of those is reported separately as missing for crawlers without JavaScript.

---

### User Story 3 - Find pages that are hard to quote or cite (Priority: P2)

A content lead wants to know which articles are likely to be quoted in AI answers and which need rework. The AI search tab gives every page an answer readiness rating and lists what helps or hurts it: whether headings ask or name the questions the page answers, whether a short direct answer follows those headings, whether the page uses lists and tables for steps and comparisons, whether it states facts with numbers and cites sources, how long its paragraphs are, and whether it contains a question-and-answer section. They sort by rating, open a weak article and see exactly which headings lack a direct answer.

**Why this priority**: This is where most of the editorial work is, and it is what the AEO and GEO market talks about most. It depends on content patterns rather than exact rules, so it ranks below the access and readability checks, which are exact.

**Independent Test**: Crawl a fixture set of pages written to pass or fail each answer readiness check in isolation, and check that each page passes and fails exactly the expected checks, with the expected evidence (heading text, paragraph length, counts).

**Acceptance Scenarios**:

1. **Given** a page with the heading "How long does shipping take?" followed by a 35-word paragraph that starts with the answer, **When** the AI checks run, **Then** that heading is counted as a question heading with a direct answer.
2. **Given** a page with a question heading followed directly by an image, a 400-word paragraph or another heading, **When** the AI checks run, **Then** that heading is reported as a question without a direct answer, with the heading text and what followed it.
3. **Given** a page of 1,500 words with no subheadings, lists or tables, **When** the AI checks run, **Then** the page is reported as "Long text without structure" with its word count and longest paragraph length.
4. **Given** a page that states figures (percentages, amounts, dates) and links to external sources in its main text, **When** the AI checks run, **Then** the counts of figures and of cited external sources are shown and the page passes the "Facts with sources" check.
5. **Given** a page with a question-and-answer section marked up as FAQ structured data whose questions do not appear in the visible text, **When** the AI checks run, **Then** the page is flagged "Structured data content not visible on the page".
6. **Given** a crawl whose page HTML was not stored (spec 005 setting off), **When** the AI checks run, **Then** checks that need the page's full structure are marked "Not available: page HTML not stored" instead of failed, and the score is computed from the checks that are available, saying so.

---

### User Story 4 - Check that pages say who wrote them, when, and about what (Priority: P2)

A publisher wants their articles to be trusted sources. The AI search tab shows, for every article, whether it names an author (visibly and in structured data), whether that author has a profile page, when it was published and last updated (visible date, structured data, HTTP header), whether these dates agree, and how old the content is. At site level it shows whether the site describes its organisation with a name, logo, contact details and links to its official profiles elsewhere, and whether those facts are consistent across pages.

**Why this priority**: Answer engines and Google's guidance give weight to clear authorship, freshness and well-defined entities, and these checks are mostly exact. They rank after access, readability and answer readiness because they matter most for editorial and commercial sites, less for every page.

**Independent Test**: Crawl fixture pages with known authors, dates and organisation markup, including conflicting and missing values, and check each page's reported author, dates, age, conflicts and entity findings against expected values.

**Acceptance Scenarios**:

1. **Given** an article whose structured data says `dateModified` 2026-09-01, whose visible text says "Updated 1 September 2026" and whose server sends a `Last-Modified` header of the same day, **When** the AI checks run, **Then** the page shows one last updated date with three agreeing sources.
2. **Given** an article whose structured data `dateModified` is two years after the visible date, **When** the AI checks run, **Then** the page is flagged "Dates disagree" with both values and their sources.
3. **Given** an article with no visible or structured author, **When** the AI checks run, **Then** it is flagged "No author" ; an article with an author in structured data that links to a crawled author page is shown with that page.
4. **Given** a site whose home page has Organization structured data with name, logo and links to official profiles, **When** the AI checks run, **Then** the site entity summary shows these values, and pages that describe the same organisation with a different name are listed.
5. **Given** an article whose last updated date is older than the staleness threshold (by default 12 months), **When** the AI checks run, **Then** it is listed under "Not updated in 12 months" with its age.

---

### User Story 5 - Validate llms.txt and other AI policy files (Priority: P2)

A developer has added an `llms.txt` file following the community proposal, and wants to know whether it is well formed and whether its links work. SpiderLens fetches `/llms.txt` and `/llms-full.txt` for each crawled host, checks the format, lists the links it contains with their status from the crawl, and flags missing, redirected, broken, blocked or non-canonical links. It also reports other machine-readable AI policies it finds: content usage signals in robots.txt, AI-related robots meta values and headers, and snippet restrictions that limit what an answer can quote.

**Why this priority**: llms.txt is widely discussed and cheap to check, but the major AI vendors have not committed to using it, so its value is lower than robots.txt access. The overview says so plainly rather than treating a missing llms.txt as a serious problem.

**Independent Test**: Serve fixture hosts with a valid llms.txt, a malformed one, one with broken and redirected links, one served as HTML, and none, and check the stored validation result, the link list with statuses and the overview message for each.

**Acceptance Scenarios**:

1. **Given** a host with a valid `llms.txt` (an H1 title, a summary, sections of links), **When** the site is crawled, **Then** the file is stored, shown as valid, and each of its links is listed with its crawl status.
2. **Given** an `llms.txt` without an H1 title, or served as an HTML page, **When** the site is crawled, **Then** it is shown as invalid with each problem and the line it is on.
3. **Given** an `llms.txt` that links to a URL which returns 404, to one that redirects, and to one blocked by robots.txt, **When** the site is crawled, **Then** each link is listed with that problem.
4. **Given** a host without `llms.txt`, **When** the site is crawled, **Then** the overview says the file is absent, that it is optional, and that no major AI search vendor has said it reads it; this costs at most a small part of the site score.
5. **Given** a robots.txt with a `Content-Signal` line, or pages with AI-related robots meta values (`noai`, `noimageai`), `nosnippet`, `max-snippet` or `data-nosnippet` sections, **When** the AI checks run, **Then** these are reported with what each one asks AI systems to do and which vendors are documented to honour it.
6. **Given** a crawl, **When** the user chooses "Generate llms.txt", **Then** SpiderLens produces a draft file from the crawled, indexable pages (site name and description from the home page, sections from the site's main folders, each link with its title and description) for the user to review and save; nothing is published to the site.

---

### User Story 6 - One score per page and per site, with reasons (Priority: P1)

A consultant needs to report AI readiness to a client in one number, and then show what to fix first. Every page gets an AI readiness score from 0 to 100 and the site gets one too. Each score breaks down into five parts (Access, Readable without JavaScript, Answer readiness, Trust and freshness, Structured data), each part lists the checks that passed and failed with the points they carry, and the site overview lists the fixes that would raise the site score most, ordered by points gained and pages affected.

**Why this priority**: The score is what users will quote and compare, so it must exist from the first release of this feature, even if only the access and readability parts are complete. Without explanations a score would be untrustworthy; with them it becomes a to-do list.

**Independent Test**: Run the checks on fixture crawls with known check outcomes and verify that every page and site score equals the documented formula applied to those outcomes, that every lost point is explained, and that the same crawl always gives the same scores.

**Acceptance Scenarios**:

1. **Given** a page that passes every check, **When** the AI checks run, **Then** its score is 100 and every part is full.
2. **Given** a page that AI search crawlers may not fetch, **When** the AI checks run, **Then** its score is capped at the documented ceiling for blocked pages, whatever its content, and the explanation says access is the reason.
3. **Given** a page where a check is not available (for example no stored HTML), **When** the AI checks run, **Then** that check's points are left out of both the earned and the possible total, and the score says how many checks it is based on.
4. **Given** the same crawl file, **When** the AI checks run twice, on any computer, **Then** every score and finding is identical.
5. **Given** a completed crawl, **When** the user opens the site overview, **Then** it shows the site score, the score of each part, the share of pages per score band, and the top fixes with the points each would add.
6. **Given** a score in the app, **When** the user opens its explanation, **Then** each check shows its weight and whether it is based on vendor documentation, published research or common practice, with a link to the source.

---

### User Story 7 - Use the results everywhere else in SpiderLens (Priority: P3)

The user filters, sorts and exports the AI findings like any other data, and runs the same checks from the command line in a scheduled job.

**Why this priority**: Parity with the rest of the app and the command line is required by the constitution, but it reuses mechanisms from specs 007, 009 and 010.

**Independent Test**: Run the checks from the command-line interface on a fixture crawl file and compare its output with what the desktop app shows for the same file; export the AI search tab and compare.

**Acceptance Scenarios**:

1. **Given** a crawl file, **When** the user runs the AI checks from the command-line interface, **Then** it prints the site score, the parts and the top fixes, can write every page result as CSV or JSON, and exits with a non-zero code when the site score is below a threshold the user sets.
2. **Given** the AI search tab, **When** the user exports it, **Then** the export contains every visible column and the filters applied.
3. **Given** a crawl file made before this feature, **When** it is opened, **Then** the AI checks can be run on it from its stored data, and any check that needs data the old file lacks is marked not available.

---

### Edge Cases

- **Crawlers that ignore robots.txt or are not documented**: the access table only states what robots.txt asks; it says which crawlers their vendors document as honouring robots.txt and does not claim a block is enforced. Crawlers that vendors document as not honouring robots.txt for user-requested fetches are labelled so.
- **Robots tokens that differ from the user agent string** (for example Google-Extended and Applebot-Extended are robots tokens only, with no crawler of their own): evaluated as tokens, and the explanation says which real crawler they affect.
- **Firewall and bot-protection blocks**: robots.txt can allow a crawler that a firewall then blocks by user agent or network. The optional user agent probe (FR-012) can reveal user-agent-based blocks; network-based blocks cannot be seen from the user's computer, and the overview says so.
- **robots.txt that changes during the crawl**: the copy stored at the start of the crawl for each host is used, as for crawling (spec 003).
- **robots.txt larger than the parsing limit, or with syntax errors**: evaluated as the crawler's own rules evaluate it (spec 003), with the problems listed.
- **Pages not crawled** (blocked for SpiderLens, outside scope, errors): included in the access table share only when they were discovered; pages with no stored content are left out of content checks and counted as such.
- **Non-HTML resources** (PDFs, images): get access verdicts only; content checks do not apply. PDFs are a known AI citation source but their content is not extracted in this release.
- **Very large sites**: checks run on stored data in a background step after the crawl and on demand; the results table shows partial results while they run.
- **Translated pages** (hreflang sets): each language version is checked on its own; questions and answers are recognised in the languages listed in Assumptions, and in other languages only language-independent checks apply.
- **Pages with several articles, or listing pages** (home, categories, tag pages): answer readiness and authorship checks apply to pages classified as articles or answers; listing and utility pages are scored on access, readability, structured data and entity checks only, and the classification is shown.
- **Structured data in microdata or RDFa instead of JSON-LD**: read the same way, where spec 005 stores it.
- **Structured data that cannot be parsed**: reported as invalid with the parse error, and treated as absent for the checks that use it.
- **Dates in many formats and languages**: visible dates are recognised in common formats; a page whose date cannot be recognised is reported as "No visible date found" rather than given a wrong date.
- **Dates in the future**: reported as invalid, not as fresh.
- **llms.txt served with a redirect, an error, a very large body or a wrong content type**: recorded with the outcome; bodies are capped at a documented size.
- **Hosts the crawl only touched externally**: llms.txt and robots.txt AI checks are made for internal hosts only.
- **The AI crawler list gets outdated**: the list ships with the app, shows its own version and date, and users can add their own crawler tokens, which are evaluated the same way.

## Requirements *(mandatory)*

### Functional Requirements

**When and how the checks run**

- **FR-001**: The system MUST compute every check in this spec from data stored in the crawl file (spec 005, 006, 014) and the small per-host files of FR-010. It MUST NOT add work to the fetch, extraction or storage path of page crawling, except the per-host fetches of FR-010.
- **FR-002**: The checks MUST run automatically when a crawl completes, and on demand for any opened crawl file, including files made before this feature. While they run, the crawl's other results MUST stay usable, and the AI search tab MUST show progress and partial results.
- **FR-003**: Users MUST be able to turn off the automatic run (default on). The setting MUST be stored with the crawl.
- **FR-004**: Every check result MUST be deterministic: the same crawl file and the same app version MUST produce the same results on any computer. No check may need a network connection, an account or an API key.
- **FR-005**: The results MUST be stored in the crawl file with the app version and AI crawler list version that produced them, so that reopening the file shows them without running the checks again, and running them again on a newer version replaces them.

**Known AI crawlers**

- **FR-006**: The system MUST ship a list of known AI crawlers. Each entry MUST have: vendor, crawler name, robots.txt token, user agent string pattern, purpose (model training, AI search or answer index, user-requested fetch, or other), whether the vendor documents that it honours robots.txt, whether it is documented or observed to run JavaScript, and a link to the vendor's documentation. The first list MUST cover at least the crawlers listed in research.md R1.
- **FR-007**: The list MUST carry a version and date, shown in the app. Users MUST be able to add their own entries (name, robots token, user agent pattern, purpose), stored in their settings and used in every crawl's checks.
- **FR-008**: The list MUST be readable by other features (agent access, spec 016; issue rules, spec 009) from one place, so that SpiderLens has one definition of each AI crawler.

**AI crawler access**

- **FR-009**: For each internal host and each AI crawler, the system MUST evaluate the stored robots.txt with that crawler's token using the same matching rules the crawler uses for SpiderLens itself (spec 003) plus each vendor's documented fallbacks (for example Applebot follows the Googlebot group when no Applebot group exists), and MUST record: a verdict for the host's home page, the number and share of the host's crawled URLs that the crawler may not fetch, and the rule (group, line number and text) that decides each blocked URL. A host whose robots.txt was unavailable MUST be recorded as such, not as allowed. Crawlers whose vendors say robots.txt may not apply to them (user-requested fetchers such as ChatGPT-User and Perplexity-User) MUST be labelled "robots.txt may not apply" next to their verdict.
- **FR-010**: For each internal host, the system MUST fetch `/llms.txt`, `/llms-full.txt` and `/.well-known/tdmrep.json` once per crawl, under the same politeness rules as robots.txt, and store each response (status, final URL, content type, size, body up to a cap of 1 MB, outcome). These fetches MUST NOT delay the start of page crawling. Users MUST be able to turn them off (default on).
- **FR-011**: The system MUST show an AI crawler access table with one row per crawler, grouped by vendor and purpose, and one column per internal host, with the verdict, the blocked share and the deciding rule. It MUST summarise, in plain words, which purposes are allowed and blocked for each vendor (for example "OpenAI: training blocked, search allowed, user fetches allowed").
- **FR-012**: Users MUST be able to run an optional user agent probe: request the home page and a sample of crawled pages (by default 10 per host) once with each selected AI crawler's published user agent string, and compare the response status, size and type with the crawl's own response, to reveal blocks by user agent, challenge pages and payment-required responses (HTTP 402 with a crawl price, as used by pay-per-crawl services). The probe MUST be off by default, MUST be started by an explicit user action that states what it does, MUST honour the host's speed limits, robots.txt for SpiderLens's own token and Retry-After, and MUST label its results as indicative, because real AI crawlers are also recognised by their network addresses, which the probe cannot reproduce.
- **FR-013**: For each page, the system MUST record which robots meta tags and `X-Robots-Tag` headers address AI crawlers by name or apply to all crawlers, and the snippet restrictions that limit what an answer engine may quote: `nosnippet`, `max-snippet`, `noarchive`, `data-nosnippet` sections, Bing's `data-snippet` marking of quotable text, and the AI-related values `noai` and `noimageai`. Each MUST be explained with which vendors document honouring it and what it does there (for example: `nosnippet` keeps a page out of Google AI Overviews and AI Mode as direct input; `noarchive` keeps it out of Copilot answers; no major vendor documents honouring `noai`).
- **FR-014**: The system MUST record the machine-readable AI usage and licensing signals it finds and report them per host or page with their meaning and status (proposal, draft standard or provider convention): `Content-Signal` lines in robots.txt (search, ai-input, ai-train), IETF AI preferences `Content-Usage` lines in robots.txt and `Content-Usage` HTTP headers, TDMRep (`/.well-known/tdmrep.json` when fetched, `tdm-reservation` and `tdm-policy` headers and meta tags), and RSL licence references (`License:` in robots.txt, `rel="license"` links of the RSL type). Unknown robots.txt lines MUST be listed, not dropped. These are preferences: the report MUST say that they do not block a crawler.

**Readable without JavaScript**

- **FR-015**: When the crawl was rendered (spec 014), the system MUST use, for each rendered page, the stored JavaScript dependence values to decide what crawlers that do not run JavaScript can read: the share of visible text, whether the title, main heading, meta description, canonical, robots directives and structured data exist in the raw HTML, and the share of internal links found only after rendering.
- **FR-016**: Checks MUST be evaluated per crawler capability: for crawlers that do not run JavaScript, every content check MUST use the raw values; for crawlers that do (such as Googlebot, which feeds Google's AI features), the rendered values when available. Each result MUST state which version it used.
- **FR-017**: When the crawl was not rendered, the system MUST flag pages as "Possibly built with JavaScript" from raw HTML evidence only (very little visible text relative to page size, an empty application container, a large share of the page made of scripts, a framework's client-side markers), MUST show the evidence, MUST recommend a rendered crawl to confirm, and MUST NOT report a JavaScript dependence percentage.
- **FR-018**: The system MUST depend on spec 014 only through its stored values (FR-029 and FR-031 of spec 014). When those values are absent, every check in this group MUST still produce a result from the raw data, marked as based on raw HTML only.

**Answer readiness**

- **FR-019**: The system MUST classify each indexable HTML page as an article or answer page, a listing page, or a utility page (for example contact, login, search, legal), from its structure and structured data, MUST show the classification and why, and MUST let the user override it by URL pattern. Answer readiness checks (FR-020 to FR-024) MUST apply to article and answer pages only.
- **FR-020**: The system MUST identify question headings (headings phrased as a question, or starting with a question word, in the supported languages) and, for each, whether it is directly followed by an answer: a paragraph or list item that starts within the first block after the heading and whose first sentence is at most 60 words long (the threshold is adjustable). It MUST report each question heading without a direct answer, with its text and what followed it.
- **FR-021**: The system MUST report the page's structure for extraction: the number of subheadings per 1,000 words, the longest paragraph in words, the share of paragraphs longer than 150 words, and the number of lists and tables in the main content, and MUST flag "Long text without structure" when a page of at least 800 words has no subheadings, lists or tables.
- **FR-022**: The system MUST count, in the page's main content, statements with figures (numbers with units, percentages, amounts, dates), quotations attributed to a person or organisation, and links to external sources, and MUST report pages of article type with none of these as "No facts with sources".
- **FR-023**: The system MUST detect question-and-answer sections (visible questions with answers, and FAQ or QA structured data) and MUST flag structured data whose question or answer text does not appear in the page's visible text.
- **FR-024**: The system MUST check that the page's title and main heading state its topic (both present, not empty, not identical across many pages of the site, not a generic placeholder), reusing the findings of spec 005 where they exist, and MUST flag a main heading that is missing or appears only in the rendered page.
- **FR-025**: Checks that need the page's full HTML structure (FR-020 to FR-023) MUST use the stored page HTML. When it is not stored, they MUST be marked "Not available: page HTML not stored", not failed.

**Trust and freshness**

- **FR-026**: The system MUST determine for each page its published and last updated dates from structured data (`datePublished`, `dateModified`), visible text near the main heading or byline, `<time>` elements, the `Last-Modified` HTTP header, and the sitemap `lastmod` when the crawl stored it, and MUST show each source's value, the date chosen, and whether the sources agree within 2 days.
- **FR-027**: The system MUST flag article pages with no date, dates that disagree, dates in the future, and pages not updated for longer than a staleness threshold (default 12 months, adjustable).
- **FR-028**: The system MUST determine for each article page whether it names an author visibly (a byline) and in structured data (`author` as a Person or Organization with a name), whether the author entry links to a page about the author or to external profiles (`url`, `sameAs`), and whether that author page was crawled and is indexable. It MUST flag articles without an author.
- **FR-029**: The system MUST build a site entity summary from structured data on the home page and across the site: the organisation or person the site belongs to, with name, logo, URL, contact details and official profiles (`sameAs`); and the site's name (`WebSite`). It MUST flag a missing site entity, entity names that differ between pages, and `sameAs` values that are not absolute URLs.
- **FR-030**: The system MUST check, site-wide, whether pages that answer engines commonly use to judge a source exist and are crawlable: an about page, a contact page and, for sites with authors, author pages. It MUST report each as found (with URL) or not found, from links and URL patterns, and let the user point to the right page when detection is wrong.

**Structured data**

- **FR-031**: The system MUST validate every stored structured data item (JSON-LD, microdata, and RDFa where stored): parse errors with their position, types and properties that are not part of the bundled Schema.org vocabulary, values of the wrong kind (for example text where a URL or date is expected), and missing properties that the bundled rules mark required or recommended for the types listed in research.md R4. The vocabulary and rules MUST ship with the app with their version, so that validation works offline.
- **FR-032**: The system MUST report structured data coverage per page type and site-wide: which types are present on what share of pages of each classification (FR-019), and which pages of a type lack the structured data most pages of that type have.
- **FR-033**: The system MUST flag structured data whose key facts disagree with the visible page: name or headline different from the title and main heading, dates different from the visible dates (FR-026), and FAQ content not on the page (FR-023).

**Scores and explanations**

- **FR-034**: The system MUST compute, for each indexable HTML page, an AI readiness score from 0 to 100 as the sum of five parts with these default maximum points: Access 30, Readable without JavaScript 25, Answer readiness 20, Trust and freshness 15, Structured data 10. Structured data carries the fewest points because a controlled study found no citation uplift from adding it; it is scored for validity and agreement with the visible page. Each check MUST carry a documented number of points within its part. The formula, parts, checks and points MUST be published in the app's documentation and in the explanation of every score.
- **FR-035**: A page that AI search crawlers (purpose "AI search or answer index") of every vendor may not fetch MUST have its score capped at 20, and the explanation MUST say that access is the reason.
- **FR-036**: Checks that do not apply to a page (by classification) or are not available (missing data) MUST be left out of both the earned and the possible points, and the score MUST be scaled to 100 over the possible points, stating how many checks it is based on. A page with fewer than half of its checks available MUST show its score as provisional.
- **FR-037**: The system MUST compute a site score from 0 to 100: 70% the average page score of indexable HTML pages, weighted equally, and 30% site checks (AI crawler access for each purpose, robots.txt availability, site entity, about and contact pages, llms.txt validity when present). The explanation MUST show both parts.
- **FR-038**: Every check MUST state its basis: "vendor documentation" (with the vendor and link), "published research" (with the reference), or "common practice". The explanation MUST show this for every check, and the help MUST say plainly that no score can guarantee that an AI system will cite a page.
- **FR-039**: The site overview MUST list the fixes that would raise the site score most, each with the check, the number of pages affected, the points the site score would gain if fixed everywhere, and a short instruction, ordered by points gained.
- **FR-040**: Users MUST be able to change the weight of each part and the thresholds named in this spec, as a scoring profile stored in their settings and with the crawl's results; the default profile MUST always be available and be the one used in reports unless the user picks another.

**Where results appear**

- **FR-041**: The system MUST add an AI search tab to the results table (spec 007) with one row per indexable HTML page and default columns Address, Page type, AI readiness score, Access, Readable without JavaScript, Answer readiness, Trust and freshness, Structured data, Question headings, Direct answers, Last updated, Author, and filters with counts: All; Blocked for AI search crawlers; Blocked for AI training only; Blocked for user-requested fetches; Snippet restricted; Main content missing without JavaScript; Possibly built with JavaScript; Question without a direct answer; Long text without structure; No facts with sources; Structured data content not visible on the page; No date; Dates disagree; Not updated in 12 months (threshold in the label); No author; Structured data errors; Structured data warnings; Score below 50.
- **FR-042**: The system MUST add an AI search overview showing the site score and parts, the AI crawler access table (FR-011), llms.txt status (FR-045), content usage signals (FR-014), the site entity summary (FR-029), about and contact page status (FR-030), structured data coverage (FR-032), the score distribution and the top fixes (FR-039).
- **FR-043**: The detail pane MUST add an AI search section for the selected page with its score explanation (parts, checks passed and failed with points and basis), access per crawler, the version (raw or rendered) each check used, question headings with their answers, dates with sources, author, and structured data validation messages.
- **FR-044**: Every check in this spec MUST be defined in the rule shape of the issue rules engine (spec 009, FR-001): a stable unique identifier, name, category ("AI search"), priority (High, Medium, Low or Opportunity), scope, condition with optional threshold, the crawl data it needs (missing data marks it unavailable, which this spec calls "not available"), kind (per page or site-wide), explanation, how to fix and evidence per URL. When the issue rules engine is available, every failed check MUST appear in its issue list, reports and exports with every other issue, and the user's per-profile changes there (disable, priority, threshold) MUST apply to the AI checks too. This spec adds to each check its part, its points and its basis (FR-034, FR-038).

**llms.txt**

- **FR-045**: For each fetched `llms.txt`, the system MUST check it against the llms.txt proposal (research.md R2): served as plain text or Markdown with a success status, an H1 title as the first and only required element, an optional summary block, sections introduced by H2 headings containing Markdown link lists, optional notes after links, and an optional section named "Optional" for links that can be skipped. `llms-full.txt` is a community convention outside the proposal and MUST only be checked for status, type, size and whether its links work. It MUST report each problem with its line, and list every link with the target's crawl status, flagging links that are broken, redirected, blocked by robots.txt for SpiderLens or for AI crawlers, not indexable or not canonical. Link targets not otherwise crawled MUST be checked under the crawl's scope rules.
- **FR-046**: The system MUST explain llms.txt as optional and aimed mainly at AI agents and coding tools, state that no major AI search vendor has said it reads the file and that Google says no special AI files are needed (research.md R2), and give a missing llms.txt at most 2 of the site score's points.
- **FR-047**: Users MUST be able to generate a draft `llms.txt` from a crawl: the site's name and description from the home page and site entity, H2 sections from the main folders or the most linked internal pages, and one link per indexable, canonical page chosen (by default up to 100), with its title and meta description. The draft MUST be shown for review and saved only to a file the user chooses.

**Parity, storage and performance**

- **FR-048**: Every capability in this spec MUST be available from the command-line interface with the same defaults and results: run the checks on a crawl file, print the site summary and top fixes, export page results as CSV or JSON, generate a draft llms.txt, run the user agent probe on explicit request, and exit with a non-zero code below a site score threshold set by the user.
- **FR-049**: Results MUST be stored in the crawl file following the storage feature's format rules (spec 006): files made before this feature keep opening, and older app versions keep opening files that contain AI results.
- **FR-050**: The checks MUST run on a crawl's stored data without loading the whole crawl into memory, and MUST use the computer's cores without making the app unresponsive.

### Key Entities *(include if feature involves data)*

- **AI crawler**: Vendor, name, robots token, user agent pattern, purpose, honours robots.txt (documented), runs JavaScript (documented or observed), documentation link; built-in (with list version and date) or user-defined.
- **Host AI policy**: Per internal host: robots.txt availability, verdict per AI crawler for the home page, blocked count and share of crawled URLs, deciding rules, content usage signals, llms.txt and llms-full.txt fetch results and validation, user agent probe results when run.
- **Page AI result**: Per page: page type and reason, access per crawler (robots.txt, meta and header directives, snippet restrictions), readability for crawlers with and without JavaScript, answer readiness findings, dates with sources and chosen dates, author, structured data validation messages, score with parts and the version of the rules and crawler list used.
- **Check**: A rule in the spec 009 shape (identifier, name, category, priority, scope, condition and threshold, needed data, kind, explanation, how to fix, evidence), plus its score part, points, the page types it applies to and its basis (vendor documentation, published research, common practice) with source.
- **Scoring profile**: Weights per part and thresholds (direct answer length, staleness, long text, JavaScript dependence, score bands); default or user-defined.
- **Site AI summary**: Site score with its two parts, part scores, score distribution, site entity summary, about and contact pages, structured data coverage, top fixes.
- **llms.txt draft**: The generated text and the choices used to build it.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: Crawl throughput and peak memory on the 100,000-page benchmark from spec 008 stay within the benchmark's normal run-to-run variation of the baseline without this feature, with the automatic AI checks on; the only added requests are the three small per-host files of FR-010.
- **SC-002**: On the reference machine, the AI checks for a 100,000-page crawl with stored HTML finish within 60 seconds after the crawl ends, while the app stays responsive, and memory during the checks stays within the crawl's own peak.
- **SC-003**: On a fixture set of at least 40 robots.txt files covering AI crawler tokens, wildcards, path rules, conflicting groups, unavailable files and Google-Extended, the access verdicts and deciding rules match the expected values for 100% of crawler and URL pairs.
- **SC-004**: On a fixture set of pages written to pass or fail each content check (question headings, direct answers, structure, facts and sources, FAQ visibility, dates, authors, entities), each page passes and fails exactly the expected checks in at least 98% of cases, and 100% for the exact checks (access, dates in structured data, structured data validity).
- **SC-005**: For a rendered fixture crawl, 100% of pages whose JavaScript dependence is above the threshold are reported as missing content for crawlers that do not run JavaScript, and none below it.
- **SC-006**: Running the checks twice on the same crawl file, on Windows, macOS and Linux, produces byte-identical stored results.
- **SC-007**: For 100% of fixture pages, the score equals the documented formula applied to the check outcomes, and every lost point is shown in the explanation with its check.
- **SC-008**: The llms.txt validator reports exactly the expected problems and link statuses on a fixture set of at least 15 valid and invalid llms.txt files.
- **SC-009**: A user who has never used the AI search area can find which AI crawlers are blocked on their site, and the three fixes that would raise the site score most, in under 2 minutes in usability checks, without documentation.
- **SC-010**: The desktop app and the command-line interface produce identical results for the same crawl file.

## Assumptions

- **Scope**: Milestone M5. Spider and List modes. Deterministic checks only; asking an AI model to judge or rewrite pages, measuring whether a brand actually appears in AI answers (AI visibility tracking), and connecting to vendor accounts are out of scope (spec 016 covers LLM scoring and agent access; visibility tracking would need third-party services and is a later decision under constitution principle II).
- **Out of scope**: PDF and other document content checks; image and video content for AI; server log analysis to see which AI crawlers really visited (a later spec, it needs the user's logs); verifying AI crawlers by network address; checking Markdown versions of pages (`.md` alternates) beyond links listed in llms.txt; automatic changes to the user's site.
- **No AI vendor publishes how it chooses sources.** The answer readiness and trust checks follow published vendor guidance where it exists (for example Google Search Central's guidance on AI features), published research (for example the GEO study by Aggarwal et al.), and widely shared practice; research.md lists the basis of each check, and the scoring weights are starting values the owner may tune during `/speckit-clarify`.
- **Languages**: question headings and visible dates are recognised in English, Hebrew, Spanish, French, German, Italian, Portuguese and Dutch in this release; other languages get the language-independent checks. The list is a planning choice and can grow.
- **Main content**: checks that speak of "main content" use the page's main element or article element when present, else the body without navigation, header, footer and aside; the exact rule is set in planning.
- **Rendered crawls**: the readability checks reach their full value only on rendered crawls (spec 014). On raw-only crawls they use the evidence-based heuristic of FR-017 and say so.
- **Per-host fetches**: fetching two small files per host is assumed acceptable under constitution principle IV (respectful crawling), as robots.txt fetching is; they use SpiderLens's own user agent, except in the explicit user agent probe.
- **The user agent probe** uses other vendors' user agent strings, which constitution principle IV allows only as an explicit, labelled user action intended for sites the user controls; it is therefore off by default and asks before it runs.
- **Thresholds** (60-word direct answer, 150-word paragraph, 800-word long text, 12-month staleness, 2-day date agreement, score cap 20 for blocked pages, 2 points for llms.txt) are starting values.
- **Issue rules**: the checks are rules of the spec 009 rule table (FR-044). If this feature's code lands before the 009 engine, the AI search tab and overview present the findings on their own from the same rule definitions, and they join the issue list when 009 lands.
- **Storage**: results take one new step of the crawl file format (spec 006): the latest step on main plus one at merge time (expected 7), with new columns nullable.
