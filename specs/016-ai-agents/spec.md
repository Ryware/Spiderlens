# Feature Specification: AI Agents

**Feature Branch**: `016-ai-agents`

**Created**: 2026-10-13

**Status**: Approved 2026-10-13 (Ryware), recommended defaults for all open decisions (D1 to D6 in research.md)

**Input**: User description: "AI agent features, both sides. First, agent readiness of the crawled site: can AI agents find, read and act on it (agent-facing user agents, llms.txt and machine-readable entry points, forms and actions reachable without JavaScript, API or MCP endpoints the site exposes). Second, SpiderLens itself as a tool for agents: a local MCP server over the crawl file and the command line, no cloud, no servers, secrets never stored. Desktop only. Crawl speed is the top priority: checks run from stored data, never in the fetch path."

## Overview

In 2026 a growing share of visits to a website come from AI agents acting for a person: ChatGPT agent, Claude in Chrome, Google-Agent, Perplexity Comet and many smaller ones. They do not just read a page to answer a question (that is the AEO and GEO side, spec 015); they navigate, fill in forms, search, compare and buy. A site can be easy or impossible for them, and today no desktop crawler tells its owner which.

This feature has two halves that share one data model.

1. **Agent readiness of the crawled site.** SpiderLens checks, from what it already stores, whether an agent can find the site's machine-readable entry points, read its content without running JavaScript, and complete its key actions (search, contact, sign-up, checkout) without a mouse. It reports each finding per page and per host, with a transparent Agent readiness score whose every point comes from a named rule.
2. **SpiderLens as a tool for agents.** The user's own AI assistant (Claude Desktop, Claude Code, Cursor, ChatGPT desktop or any client that speaks the Model Context Protocol, MCP) can open SpiderLens crawls through a local MCP server that SpiderLens starts on demand. The agent can ask "which pages lost their H1 after rendering?" or "summarise the 404s by template" and get answers from the crawl file. Nothing leaves the computer except what the user's own assistant sends to its own provider; SpiderLens runs no server and stores no keys.

This feature belongs to milestone M6. It builds on page extraction (005), crawl storage (006), the results table (007), the issue rules engine (009), JavaScript rendering (014) and AEO and GEO checks (015). It does not duplicate them:

- **Spec 015 owns** the AI crawler and agent registry (vendor, robots token, user agent, purpose, whether it renders JavaScript), fetching and parsing `/llms.txt` and `/llms-full.txt`, evaluating robots.txt for AI bots, and content-usage signals. This spec reads those results.
- **Spec 014 owns** rendering and the per-page JavaScript dependence values. This spec reads them.
- **Spec 009 owns** the generic rule table, severities and the Issues view. Every agent readiness check in this spec is a named rule in that table.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Ask my AI assistant about my crawl (Priority: P1)

A consultant has crawled a client's 40,000-page site. Instead of building filters by hand, they open Claude Desktop and ask: "In the crawl I just ran, which product pages have no meta description, grouped by folder?" The assistant calls SpiderLens's local MCP server, which reads the crawl file and returns the answer in pages of results. The consultant follows up with "show me the raw and rendered H1 of the first five" and "export those URLs to CSV in my Downloads folder" (the last one SpiderLens politely declines, because the server is read-only; the assistant gives them the list to paste instead).

**Why this priority**: It is the product's differentiator for AI-agent users, it reuses every piece of data SpiderLens already stores, and it is useful on every crawl from day one, before any agent readiness check exists. It needs no new crawling.

**Independent Test**: Crawl a fixture site, start the MCP server from the command line against the crawl file, and drive it with a scripted MCP client: list crawls, get the summary, run each documented query, page through a large result, fetch one page's details. Compare every answer with the same query run through the existing command-line interface.

**Acceptance Scenarios**:

1. **Given** the user turned on "Allow AI assistants to read my crawls" and added SpiderLens to their assistant with the snippet shown in Settings, **When** the assistant starts, **Then** it sees the SpiderLens tools without the user typing any path, key or port.
2. **Given** a saved crawl, **When** the assistant asks for the crawl summary, **Then** it receives the start URL, dates, configuration summary, page counts by status class and by content type, the completion statement from spec 006, and the issue counts from spec 009.
3. **Given** a crawl with 100,000 pages, **When** the assistant asks for all pages with a 404 status, **Then** it receives the first page of results with a total count and a continuation token, and each further page on request, never the whole set at once.
4. **Given** a page, **When** the assistant asks for its details, **Then** it receives the same fields the detail pane shows (status, redirects, titles, descriptions, headings, directives, canonicals, hreflang, links in and out counts, structured data types, JavaScript dependence when rendered, agent readiness findings), and the stored raw or rendered HTML only when asked, cut to a stated maximum size with the cut marked.
5. **Given** a crawl that is still running in the desktop app, **When** the assistant queries it, **Then** it gets consistent answers from the data committed so far, labelled as an in-progress crawl, and the running crawl's speed is unaffected (SC-004).
6. **Given** the assistant asks for anything that would change data (delete, edit, export to a path, start a crawl) while only read access is allowed, **Then** the server refuses with a message that says which setting allows it, and changes nothing.

---

### User Story 2 - See whether agents can act on my site without JavaScript (Priority: P1)

An e-commerce SEO lead wants to know whether shopping agents can search the catalogue, add to basket and reach checkout. After a crawl, a new **Agents** tab lists every HTML page with its agent readiness findings. Filters show, with counts: forms that only exist after JavaScript, form fields with no label, buttons with no accessible name, "links" that are not links (no address, `#`, or `javascript:`), actions behind a CAPTCHA, search forms an agent cannot submit with a plain request, and pages hidden from non-rendering agents. The detail pane lists each form with its fields, labels, method and target, marked as found in raw HTML, rendered HTML or both.

**Why this priority**: Acting on a site is what separates agents from AI crawlers, and it is the part of agent readiness that only a crawler with stored raw and rendered HTML can measure at scale. It runs entirely from stored data, so it costs nothing in crawl speed.

**Independent Test**: Crawl a fixture site with deliberate cases (a search form in raw HTML, a contact form injected by JavaScript, unlabeled inputs, icon-only buttons with and without accessible names, `href="#"` click handlers, a form with a known CAPTCHA widget, a form carrying WebMCP declarative attributes, Schema.org `SearchAction`) with rendering off and on, and check every page lands under exactly the expected filters.

**Acceptance Scenarios**:

1. **Given** a page whose contact form exists only after JavaScript runs, **When** the crawl used rendering, **Then** the page is listed under "Form only after JavaScript"; **When** the crawl did not use rendering, **Then** the form is not reported as missing, and the tab says that rendering is needed to check JavaScript-only forms.
2. **Given** a form field without a label, accessible name or description, **When** the page is checked, **Then** it is listed under "Form fields without labels" with the field's name, type and position.
3. **Given** a clickable element with no usable address (no `href`, `#`, or `javascript:`) that looks like navigation, **When** the page is checked, **Then** it is listed under "Actions without a link" with its text.
4. **Given** a form carrying a known CAPTCHA or bot challenge widget, **When** the page is checked, **Then** it is listed under "Action behind a challenge", naming the widget.
5. **Given** a form with WebMCP declarative attributes, **When** the page is checked, **Then** the Agents tab shows its tool name and description, flags a missing description or undescribed fields, and counts it under "Declares agent tools".
6. **Given** a site-wide `SearchAction` in structured data, **When** the crawl is checked, **Then** the host view shows it and checks that its URL template yields a page that returns 200 for a sample query, using a stored result if the crawl already fetched one, otherwise reporting "not verified".

---

### User Story 3 - Find the site's machine-readable entry points (Priority: P2)

A developer advocate wants to know what their site tells agents about itself. The host view for each internal host lists the agent entry points SpiderLens found: the `llms.txt` and `llms-full.txt` results (from spec 015), an AI Catalog, MCP server cards and A2A agent cards it links to, an API catalog, OpenAPI or other service descriptions linked from pages or headers, Markdown versions of pages, and sitemaps. Each entry point shows its address, status, whether it parses, and its problems (wrong content type, robots.txt rules blocking it, broken links inside it, an MCP endpoint that does not answer a discovery request).

**Why this priority**: Discovery files are cheap to check and the conventions are new and often wrong. It is P2 because adoption is still low in 2026 (see research.md), so for most sites the answer is "none found", which is useful but not urgent.

**Independent Test**: Serve fixture hosts that publish valid, invalid and missing versions of each entry point, crawl them, and compare the host view with the expected list, statuses and problems. Check from the fixture logs the exact number of extra requests per host.

**Acceptance Scenarios**:

1. **Given** a host with a valid AI Catalog that links one MCP server card and one A2A agent card, **When** the crawl ends, **Then** the host view lists all three with "valid", and each card's name, description and endpoint.
2. **Given** a host whose MCP server card names an endpoint, **When** entry point checks run, **Then** SpiderLens sends at most one discovery request to that endpoint and records whether it answered as an MCP server, which protocol versions it reports, and whether it requires sign-in. It never calls the server's tools.
3. **Given** a page with `<link rel="alternate" type="text/markdown">`, **When** the crawl ends, **Then** the Markdown version is listed for that page and checked for status and content type.
4. **Given** a host where robots.txt disallows a well-known entry point path for SpiderLens's user agent, **When** entry point checks run with default settings, **Then** the file is not requested and is listed as "blocked by robots.txt" with the rule.
5. **Given** entry point checks are turned off, **When** a crawl runs, **Then** no extra request beyond the normal crawl is made, and the host view says the checks were off.

---

### User Story 4 - One agent readiness score I can explain to a client (Priority: P2)

The consultant needs one number per site and per page to put in a report, and they need to defend it. The Agents summary shows an Agent readiness score from 0 to 100 for the crawl and for each page, broken into four parts: Access (can agents reach it), Discovery (can they find the entry points), Reading (can they read it without JavaScript) and Acting (can they complete actions). Clicking a part lists the rules behind it, each with its weight and the pages it affects.

**Why this priority**: Users asked for a score in every AEO/GEO tool review, but a score without the rules behind it is marketing. It is P2 because it is a summary of User Stories 2, 3 and 5 and adds nothing they do not already find.

**Independent Test**: For a fixture crawl with known findings, compute the expected score by hand from the documented weights and compare with the stored score for the crawl and for sample pages.

**Acceptance Scenarios**:

1. **Given** a crawl, **When** the user opens the Agents summary, **Then** they see the crawl score, the four part scores, and for each part the rules that cost points, ordered by points lost.
2. **Given** any page, **When** the user opens its details, **Then** its page score is shown with every rule that changed it.
3. **Given** a check that could not run (for example JavaScript-only forms in a crawl without rendering), **When** the score is computed, **Then** that rule is listed as "not checked" and is left out of the score rather than counted as passed or failed, and the score says how many rules were not checked.
4. **Given** the same crawl file, **When** the score is computed in the desktop app, from the command line and through the MCP server, **Then** all three give the same numbers.

---

### User Story 5 - Do agents get blocked at the door? (Priority: P3)

A site owner suspects their firewall blocks AI agents that their customers use. They turn on the user agent probe from spec 015 (off by default, labelled as intended for sites they control), choose agents from spec 015's registry, and SpiderLens requests a small sample of pages (spec 015's default sample) once more with each chosen agent's published user agent. This feature shows those results for the agents that act for a user. The Agents summary shows, per agent, how many sample pages returned the same status and content as SpiderLens's own requests, and which were blocked, challenged or served different content.

**Why this priority**: It answers a real and common question, but it sends extra requests with someone else's user agent, so it must be a deliberate choice, on a small sample, never part of a normal crawl. The mechanism is spec 015's; this story only adds the agents and the agent view.

**Independent Test**: Run a fixture host that blocks or challenges some agent user agents, turn the test on for three agents, and check the result per agent and the exact request count in the fixture log.

**Acceptance Scenarios**:

1. **Given** the test is on for ChatGPT agent and Google-Agent and the host returns 403 to one of them, **When** the crawl ends, **Then** the summary shows that agent as blocked on the affected pages, with status and response size side by side with SpiderLens's own result.
2. **Given** the test is on, **When** it runs, **Then** its requests count towards the host's speed limits, honour Retry-After and crawl-delay, run after the main fetch of each sampled page and never delay the main crawl.
3. **Given** the test is off (default), **When** a crawl runs, **Then** no request uses another product's user agent.

---

### User Story 6 - Let my assistant start a crawl (Priority: P3)

A developer working in Claude Code wants their assistant to re-crawl the staging site after a deploy and report new 404s. In Settings, next to the read access switch, they turn on "Allow AI assistants to start crawls" (off by default). The assistant can then start a crawl with a saved profile, watch its progress, pause or stop it, and query the result. It cannot change settings, use a profile it was not allowed to use, or override robots.txt.

**Why this priority**: Useful for automation, but it lets software start network activity on the user's behalf, so it is opt-in and after the read-only server proves itself.

**Independent Test**: With the setting off, check every crawl-starting tool is refused. With it on, start a crawl on a fixture site through a scripted MCP client, follow its progress to completion, and compare the stored result with the same crawl started from the command line.

**Acceptance Scenarios**:

1. **Given** the setting is off, **When** an assistant asks to start a crawl, **Then** it is refused with a message naming the setting.
2. **Given** the setting is on, **When** an assistant starts a crawl with an allowed profile, **Then** the crawl runs exactly as if started from the command line with that profile, appears in the desktop app's recent crawls marked "started by an AI assistant", and can be paused or stopped from the app.
3. **Given** the setting is on, **When** an assistant asks for a profile that ignores robots.txt or that the user did not allow for assistants, **Then** it is refused.

---

### Edge Cases

- **Crawled content is untrusted.** Page text, titles, `llms.txt` files, agent cards and HTML can contain instructions aimed at AI assistants ("ignore previous instructions…"). The MCP server returns crawled content only in fields marked as content from the crawled site, never as part of a tool's description or instructions, and its tool descriptions tell the assistant so. Text that matches known prompt injection patterns is flagged in the Agents tab ("Text aimed at AI agents") but returned unchanged.
- **Huge pages and huge answers.** Every MCP answer has a size limit. Lists are paged, long text is cut with the cut marked, and HTML is only returned on request.
- **Crawl file from an older version.** The MCP server opens it with the same compatibility rules as the app (spec 006). Agent readiness findings that were not stored are reported as "not checked; re-run checks", and the user can re-run them from stored data without crawling again, if the crawl stored page HTML.
- **No stored HTML.** Form and action checks need the parsed form data that spec 005's extraction stores during the crawl. If a crawl stored neither the form data nor HTML, those checks are "not checked".
- **Several assistants at once.** Each assistant starts its own MCP server process; they read the same files safely and independently.
- **The desktop app is not running.** The MCP server works without it: it is the same program started in command-line mode.
- **A file the assistant names is not a crawl file, or is outside the allowed folders.** Refused with a clear reason; nothing outside the allowed crawl files and folders is read.
- **An entry point that redirects, is huge, or is slow.** Entry point requests follow the same redirect, size and timeout rules as page fetches (spec 004), with a smaller size cap (default 5 MB).
- **An MCP endpoint on the crawled site that requires sign-in.** Recorded as "requires sign-in". SpiderLens never signs in to it and never sends it credentials.
- **Agent conventions change.** WebMCP, AI Catalog and server cards are drafts in 2026. Each convention SpiderLens checks is listed with the version it was checked against, and unknown versions are reported as "unrecognised version", not as errors.

## Requirements *(mandatory)*

### Functional Requirements

**Agent readiness: reading and acting (from stored data)**

- **FR-001**: The system MUST record, for each HTML page, every form with its method, target address, fields (name, type, required, label or accessible name, description) and submit control, marked as present in raw HTML, rendered HTML or both. This data MUST be extracted in the same single parse as spec 005's other page data and MUST NOT add a fetch or a second parse of the raw HTML.
- **FR-002**: The system MUST record, for each HTML page, interactive elements that act as links or buttons without a usable address or accessible name, up to a bounded number per page plus the total count.
- **FR-003**: The system MUST detect known CAPTCHA and bot challenge widgets on forms from a versioned list of signatures, and MUST read spec 014's "Possible bot challenge" flag for rendered pages.
- **FR-004**: The system MUST detect WebMCP declarative tool attributes on forms and their fields, record each declared tool's name, description and parameter descriptions, and flag declared tools with a missing description or undescribed fields.
- **FR-005**: The system MUST read Schema.org actions (`potentialAction`, including `SearchAction`) from the structured data stored by spec 005, list them per host, and check that each action's URL template, filled with a sample value, resolves to a URL inside the crawl's scope.
- **FR-006**: The system MUST read spec 014's JavaScript dependence values and flag pages whose main content, forms or navigation exist only after rendering, using spec 014's threshold.
- **FR-007**: All checks in FR-001 to FR-006 MUST run from stored data, off the fetch path, and MUST be re-runnable on a saved crawl without crawling again.

**Agent readiness: entry points (per host, off the fetch path)**

- **FR-008**: For each internal host, the system MUST look for these agent entry points: an AI Catalog at its well-known address, MCP server cards and A2A agent cards linked from the catalog or from pages, an API catalog at its well-known address (RFC 9727), service descriptions linked from pages or response headers (`service-desc`, `service-doc`), and Markdown alternates of pages. The `llms.txt` and `llms-full.txt` results MUST be read from spec 015, not fetched again.
- **FR-009**: Entry point checks MUST be on by default, MUST be one switch in the crawl configuration, MUST send no more than a documented fixed number of requests per host (default at most 8, plus one per linked card), MUST honour robots.txt, the host's speed limits, Retry-After and crawl-delay, and MUST run on a separate queue that never delays page fetching.
- **FR-010**: For each entry point the system MUST store its address, how it was found, status, content type, size, whether it parses against the version of the convention SpiderLens knows, and its problems.
- **FR-011**: For an MCP endpoint named in a server card, the system MUST send at most one discovery request, MUST record whether it answered as an MCP server, the protocol versions it reports and whether it requires sign-in, and MUST NOT call any tool, read any resource or send any credentials.
- **FR-012**: For pages with a Markdown alternate, the system MUST check its status and content type. Checking whether pages answer `Accept: text/markdown` with Markdown MUST be a separate option, off by default, applied to a sample of pages per host (default 10).

**Agent readiness: access by agent user agents**

- **FR-013**: The system MUST read spec 015's AI agent registry and robots.txt evaluation and show, per host, which registered agents (purpose "acts for a user") robots.txt allows, without keeping its own list.
- **FR-014**: "Test as AI agents" MUST be spec 015's optional user agent probe (FR-012 there), not a second mechanism: same switch, defaults, sample and politeness rules. This feature MUST add the registry's user-requested fetch agents to the probe's choices and MUST show the probe's results for those agents in the Agents summary and in the Access part of the score.

**Score and views**

- **FR-015**: Every agent readiness check MUST be a rule in the shape defined by spec 009 (FR-001 there): a stable unique id, name, category "Agents", priority (High, Medium, Low or Opportunity), scope, condition with optional threshold, the crawl data it needs (missing data makes the rule unavailable), kind (per page or site-wide), explanation, how to fix and evidence per URL. Each MUST appear in the Issues view and share one definition with its Agents tab filter. The score part (Access, Discovery, Reading, Acting) and weight of each rule MUST be kept in this feature's own versioned score table, keyed by rule id, so spec 009's rule shape is not changed.
- **FR-016**: The system MUST compute an Agent readiness score from 0 to 100 for each page and for the crawl, and a score for each of the four parts, from the rule weights. Rules that spec 009 marks unavailable for a page or crawl MUST be excluded from the score and counted as "not checked". The weights and formula MUST be documented in the app and in the user documentation, and MUST be stored with the crawl so a reopened crawl shows the score it had.
- **FR-017**: The system MUST show an **Agents** tab in the results with one row per HTML page and default columns Address, Agent readiness score, Forms, Forms only after JavaScript, Unlabelled fields, Actions without a link, Challenges, Declared agent tools and JavaScript dependence, and with one filter per rule, each showing its count, refreshed as defined by spec 007.
- **FR-018**: The system MUST show an **Agents summary** with the crawl score, the four part scores, the rules that cost the most points, the entry points per host (FR-008), the agent access results (FR-013, FR-014) and the "not checked" count with the reason for each.
- **FR-019**: The detail pane MUST show, for the selected page, its score with the rules that changed it, its forms and fields, its actions without a link, its declared agent tools and its Markdown alternate.

**SpiderLens as a tool for agents (local MCP server)**

- **FR-020**: The system MUST provide a local MCP server, started by the user's AI assistant as a command of the SpiderLens command-line program, communicating only over the standard input and output of that process. It MUST NOT open a network port, and MUST NOT require the desktop app to be running.
- **FR-021**: Read access MUST be off until the user turns on "Allow AI assistants to read my crawls" in Settings, or passes an explicit option on the command line. Settings MUST show, for each supported assistant (at least Claude Desktop, Claude Code, Cursor and a generic MCP client), the exact configuration snippet to copy, and a way to test that the server starts.
- **FR-022**: The server MUST offer read-only tools to: list the allowed crawls (recent crawls from spec 006 and crawl files in folders the user allowed); get a crawl's summary; list the tabs and filters with counts; query pages by tab, filter, column conditions, search text and sort; get one page's details; get one page's stored raw or rendered HTML; list issues and the pages of one issue, in spec 009's rule catalogue format (no second format); get the Agents summary and a page's agent readiness findings; and get the host entry points. Every query MUST use the same definitions and give the same results as the desktop app and the command-line interface.
- **FR-023**: Every answer MUST be bounded in size, with documented limits. Lists MUST be paged with a total count and a continuation token. Text and HTML MUST be cut at a documented limit, with the cut marked and a way to read the next part.
- **FR-024**: Content that comes from crawled pages MUST be returned in fields marked as crawled content, never inside tool names, descriptions or instructions. Tool descriptions MUST state that crawled content is untrusted data.
- **FR-025**: The server MUST read only crawl files that are in the recent crawls list or in folders the user allowed, MUST refuse any other path, and MUST NOT write any file in read-only mode.
- **FR-026**: When the user turns on "Allow AI assistants to start crawls" (off by default), the server MUST additionally offer tools to start a crawl from a profile the user allowed for assistants, read its progress, pause, resume and stop it. Assistants MUST NOT be able to change settings, create or edit profiles, override robots.txt, or start crawls with a profile that ignores robots.txt. Crawls started this way MUST be marked as started by an AI assistant in the crawl file and in the app.
- **FR-027**: The server MUST NOT return or store credentials. Request headers and passwords that spec 002 keeps only for the session are never written to the crawl file and therefore never reachable; the crawl configuration returned to an assistant MUST show them as "set" or "not set" only.
- **FR-028**: The desktop app MUST show a local log of recent assistant activity (time, assistant name as it introduced itself, tool, crawl, result size, refusals), kept on the user's computer only, with a clear button.
- **FR-029**: Queries from the MCP server against a crawl that is running MUST read committed data only, MUST NOT take locks that block the crawl's writes, and MUST label the answers as from a crawl in progress.

**Parity, storage and speed**

- **FR-030**: Every agent readiness result, the score and the Agents summary MUST be available from the command-line interface with the same definitions, including a command that prints the Agents summary and one that re-runs the checks on a saved crawl.
- **FR-031**: All agent readiness settings MUST be stored with the crawl's configuration. Crawl files without agent readiness data MUST keep opening, with the Agents tab explaining that the checks were not run and offering to run them from stored data.
- **FR-032**: Crawl file format changes MUST follow spec 006 (lazy migration with backup; older files keep opening).

### Key Entities *(include if feature involves data)*

- **Page form**: Per page: form position, method, target, raw/rendered/both, challenge widget if any, declared agent tool name and description if any.
- **Form field**: Per form: name, type, required, label or accessible name, description, declared parameter description.
- **Action without a link**: Per page: element kind, visible text, position (bounded list plus count).
- **Agent entry point**: Per host: kind (AI Catalog, MCP server card, A2A agent card, API catalog, service description, Markdown alternate, plus the `llms.txt` results read from 015), address, how found, status, content type, size, parse result, convention version, problems; for MCP endpoints, discovery result, protocol versions, sign-in required.
- **Agent access sample**: Spec 015's user agent probe results, read for the agents that act for a user (not stored twice).
- **Agent readiness finding**: Per page or host: rule id, part, outcome (pass, fail, not checked with reason), detail.
- **Agent readiness score**: Per page and per crawl: total and four part scores, rules not checked, the weight table version.
- **Assistant settings**: Read access on or off, allowed folders, start crawls on or off, profiles allowed for assistants. Stored in app settings, not in crawl profiles.
- **Assistant activity entry**: Time, assistant name, tool, crawl, result size, refused or not. Stored locally, never in crawl files.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: With entry point checks on and every other agent option at its default, the 100,000-page benchmark from spec 008 stays within 2% of the throughput and within 5% of the peak memory of a build without this feature, and the fixture log shows no more than the documented number of extra requests per host.
- **SC-002**: Running all agent readiness checks from stored data on a saved 100,000-page crawl takes under 60 seconds on the reference machine from spec 008, with memory flat as the crawl grows.
- **SC-003**: On the 100,000-page crawl, 95% of MCP queries for a filtered page list or a page's details answer in under 300 ms, and no answer exceeds its documented size limit.
- **SC-004**: While an assistant runs one query per second against a crawl that is in progress, the crawl's throughput stays within 2% of the same crawl without queries.
- **SC-005**: On a fixture set covering every Agents tab filter and every entry point kind, 100% of pages and hosts appear under exactly the expected filters with the expected values, with rendering off and on.
- **SC-006**: For the same crawl file, the agent readiness score and every query result are identical in the desktop app, the command-line interface and the MCP server.
- **SC-007**: A user who has never used MCP can turn on read access, add SpiderLens to Claude Desktop with the snippet and get an answer about their crawl in under 3 minutes in usability checks.
- **SC-008**: In a test suite of crawled pages, `llms.txt` files and agent cards containing prompt injection text, 100% of that text is returned only inside crawled-content fields, and no tool description or instruction text from the server contains crawled content.
- **SC-009**: With "start crawls" off, 100% of attempts to start, resume or change a crawl through the server are refused and nothing is written.

## Assumptions

- **Scope**: Milestone M6. Spider and List modes. Desktop app and command line only; no web or mobile version.
- **Out of scope**: SpiderLens calling an LLM provider itself (scoring pages with a model, custom AI prompts per page); submitting forms or performing actions on the crawled site; signing in to the crawled site's MCP or API endpoints; calling the crawled site's MCP tools; checking WebMCP tools registered by scripts at run time (only declarative attributes are visible to a crawler); an MCP server reachable over the network; acting as an A2A agent; signing SpiderLens's own requests with Web Bot Auth (its key would have to be stored and published, which this product does not do yet).
- **LLM scoring**: The backlog entry for 016 mentions LLM scoring. This spec assumes the user's own assistant does any model-based judgement through the MCP server, so SpiderLens stores no provider keys and makes no model calls. Built-in AI prompts with the user's own key, if wanted, belong to a later spec. (Decision for the owner, see research.md.)
- **Conventions checked** (versions recorded in research.md): AI Catalog (`/.well-known/ai-catalog.json`, draft), MCP Server Cards (SEP-2127, final, extensions track), A2A agent cards, API catalog (RFC 9727), `service-desc` and `service-doc` link relations (RFC 8631), Markdown alternates and `Accept: text/markdown`, WebMCP declarative attributes (Chrome origin trial, W3C Community Group draft). Drafts are expected to change; the signature and parser lists are versioned data, updated with app releases.
- **Score weights** are starting values documented in research.md and tuned during `/speckit-clarify`. The score is a transparent sum of rule weights, not a model.
- **Assistants supported** in the Settings snippets: Claude Desktop, Claude Code, Cursor and a generic MCP client snippet. Others work if they speak MCP over standard input and output.
- **Ownership with other specs**: the AI bot and agent registry, `llms.txt` parsing and AI robots evaluation are spec 015's; JavaScript dependence is spec 014's; the rule table and Issues view are spec 009's. If one of those has not landed when this spec is built, the dependent rules are "not checked", not reimplemented here.
- **Crawl file format** changes follow spec 006; the schema step number is agreed with the storage owner at plan time.
