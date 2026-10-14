# Feature Specification: External Data Integrations

**Feature Branch**: `024-external-integrations`

**Created**: 2026-10-13

**Status**: Approved (2026-10-13)

**Input**: User description: "External integrations: Google Search Console (clicks, impressions, index coverage per URL; orphan URLs from Search Console), Google Analytics 4 (sessions per landing page), PageSpeed Insights and the Chrome UX Report (Core Web Vitals per URL, rate limited), link-metrics providers (Ahrefs, Majestic, Moz), and export to Google Sheets and Looker Studio. Desktop only, no SpiderLens servers, secrets never stored in crawl files, every call made after the crawl from stored URLs with per-provider rate limits and caching, every integration opt-in and clearly labelled."

Provider facts (quotas, scopes, plans) with sources are in [research.md](research.md). They are recorded there because they change; this spec only depends on them where a limit is visible to the user.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Search Console data next to every crawled URL (Priority: P1)

An SEO specialist has finished crawling their site. They connect their Google account once, SpiderLens suggests the Search Console property that matches the crawled site, and with one action the user fetches clicks, impressions, click-through rate and average position for every crawled page over a date range they choose. The numbers appear as columns in the results table and in a dedicated Search Console tab, with filters such as "Indexable, no impressions" and "Has clicks, not indexable". Pages that Google shows traffic for but the crawl never found are listed as orphan candidates.

**Why this priority**: Search Console is the integration nearly every user of a desktop crawler connects. Joining "what the crawler found" with "what Google shows" is the single most valuable enrichment, and it is free for the user.

**Independent Test**: Against a local fake of the Search Console service holding known rows for the fixture site, connect a test account, fetch the last 28 days, and check every crawled URL shows the expected numbers, unmatched rows are listed as orphan candidates, and no request was made while the crawl was running.

**Acceptance Scenarios**:

1. **Given** a finished crawl of a site and no Google account connected, **When** the user opens Search Console data, **Then** they see what will be sent to Google (the list of crawled URLs is not sent; only property and date range queries are), a "Connect Google account" action that opens the system browser, and nothing is fetched until they connect and confirm.
2. **Given** a connected account with access to several properties, **When** the user starts a fetch, **Then** SpiderLens pre-selects the property that covers the crawled host (a domain property or the longest matching URL-prefix property) and the user can change it.
3. **Given** a chosen property and date range (default: last 28 days of final data), **When** the fetch completes, **Then** each crawled URL shows clicks, impressions, CTR and average position, and URLs with no Search Console rows show zero clicks and impressions, distinguished from "not fetched".
4. **Given** Search Console rows for pages the crawl did not find, **When** the fetch completes, **Then** those URLs appear in an "Orphan candidates" list with their clicks and impressions, and the user can add them to a new list crawl.
5. **Given** the user's site has more pages with data than fit in one response, **When** the fetch runs, **Then** all pages are retrieved (paging until no rows remain) and the result states how many rows were fetched and how many matched a crawled URL.
6. **Given** the user requests query-level data for one page in the inspector, **When** they open the page's Search Console section, **Then** they see that page's top queries for the same date range, fetched on demand.

---

### User Story 2 - Index status from URL Inspection (Priority: P2)

For the pages that matter, the user wants Google's own view: is the page indexed, which canonical Google chose, when Google last crawled it, and whether robots.txt blocked it. Google allows a limited number of inspections per property per day, so SpiderLens lets the user pick which pages to inspect, shows how much of today's allowance is left, and continues the next day where it stopped.

**Why this priority**: Index coverage per URL is a major reason SEOs connect Search Console, but the daily limit makes it a deliberate, partial operation rather than a whole-site enrichment.

**Independent Test**: With a fake service that enforces a small daily allowance, inspect more pages than the allowance, confirm the run pauses with the remaining count and a "resumes after" time, advance the fake clock and confirm it continues without re-inspecting finished pages.

**Acceptance Scenarios**:

1. **Given** a finished crawl and a connected property, **When** the user chooses "Inspect index status", **Then** they choose a set of URLs (default: indexable HTML pages, ordered by Search Console clicks when available, otherwise by crawl depth) and see the count against today's remaining allowance before starting.
2. **Given** an inspection run, **When** results arrive, **Then** each inspected URL shows Google's verdict, coverage state, Google-selected canonical, user-declared canonical, last crawl time, crawled-as device, robots.txt state and page fetch state, plus a mismatch flag when Google's canonical differs from the crawled canonical.
3. **Given** the daily allowance runs out mid-run, **When** the limit is reached, **Then** the run pauses with a clear message, the time it can continue, and every already-inspected result kept; resuming later (also after reopening the crawl file) inspects only the remaining URLs.
4. **Given** the user selects more URLs than the property's daily allowance, **When** they confirm, **Then** they are told the run will take more than one day.

---

### User Story 3 - Core Web Vitals from field data and lab tests (Priority: P2)

The user wants to know which pages are slow for real visitors. SpiderLens fetches field data (real-user Core Web Vitals: LCP, INP, CLS, plus FCP and TTFB) from the Chrome UX Report for each page, falling back to origin-level data when a page has too little traffic, and optionally runs a lab test through PageSpeed Insights for selected pages to get the performance score and the main opportunities. Each provider's rate limit is respected automatically and the user sees an estimate of how long the run will take.

**Why this priority**: Core Web Vitals are a ranking signal and a staple of technical audits. Field data is fast and cheap to fetch; lab tests are slow, so they are opt-in per URL selection.

**Independent Test**: With fake field-data and lab-test services, run both against 300 fixture URLs; confirm the request rate never exceeds the configured limits, pages without field data fall back to origin data and are labelled as such, and a cached result is reused on a second run the same day without new requests.

**Acceptance Scenarios**:

1. **Given** a finished crawl and a saved Google API key, **When** the user fetches field data, **Then** each HTML page shows the 75th percentile of LCP, INP, CLS, FCP and TTFB for phone and desktop with a good / needs improvement / poor rating per metric, and the 28-day collection period is shown.
2. **Given** a page without enough real-user data, **When** field data is fetched, **Then** it shows "No page-level data" and the origin-level values clearly labelled as origin data, never presented as the page's own numbers.
3. **Given** the user selects pages for a lab test and chooses mobile, desktop or both, **When** the run starts, **Then** each page shows a performance score, lab LCP, CLS, Total Blocking Time and Speed Index, and its top opportunities; accessibility, best-practices and SEO category scores are also stored for features that use them (see spec 026).
4. **Given** any of these runs, **When** requests are sent, **Then** the rate stays within the configured per-provider limit, and the user sees progress, an estimated finish time and the number of requests left in today's allowance.

---

### User Story 4 - Google Analytics 4 sessions per landing page (Priority: P2)

The user connects the same Google account and picks a GA4 property and web data stream. SpiderLens fetches sessions, engaged sessions, engagement rate, key events and total revenue per landing page for a date range, matches each landing page to crawled URLs, and lists landing pages with sessions that the crawl never found as orphan candidates.

**Why this priority**: Traffic per page tells the user which technical problems actually cost visits. Many SEOs connect Analytics alongside Search Console.

**Independent Test**: With a fake Analytics service returning known landing-page rows (paths with and without query strings), confirm matching to crawled URLs, unmatched rows as orphan candidates, and that the run stays under the hourly allowance shown to the user.

**Acceptance Scenarios**:

1. **Given** a connected account, **When** the user chooses Analytics data, **Then** they pick a property and web data stream (pre-selected when one stream's domain matches the crawled host) and a date range (default last 28 days).
2. **Given** a fetch, **When** it completes, **Then** each crawled URL shows the fetched metrics, landing pages that differ only by query string are combined by default (with an option to keep them separate), and the result states how many landing pages matched.
3. **Given** the property's allowance is exhausted, **When** the provider refuses further requests, **Then** the fetch pauses with the reason and the time it can continue, keeping what was fetched.

---

### User Story 5 - Link metrics from Ahrefs, Majestic or Moz (Priority: P3)

A user who already pays for a link-data provider pastes their API key, chooses which metrics to fetch (for example referring domains, backlinks, URL and domain authority scores), and fetches them for crawled URLs. Because these providers charge per unit or row, SpiderLens shows an estimate of the cost in the provider's own units before every run and never fetches more than the user confirmed.

**Why this priority**: Valued by power users, but it requires a paid third-party subscription and costs money per call.

**Independent Test**: With a fake provider that meters units, confirm the estimate shown before the run matches the units consumed within 10%, that the run stops at a user-set unit cap, and that results are cached so a second run on the same URLs consumes nothing.

**Acceptance Scenarios**:

1. **Given** a saved key for one provider, **When** the user starts a link-metrics fetch, **Then** they choose the URL set (default: indexable HTML pages) and metrics, see the estimated units or rows, and must confirm before any request is sent.
2. **Given** a running fetch, **When** the consumed units reach the user's cap, **Then** the run stops and reports how many URLs were covered.
3. **Given** link metrics fetched within the cache period, **When** the user fetches the same URLs and metrics again, **Then** cached values are reused without consuming units, and the fetch date is shown.

---

### User Story 6 - Send crawl data to Google Sheets and Looker Studio (Priority: P3)

The user exports the current tab, or the integration tabs, to a new Google Sheet in their own Drive, and optionally opens a Looker Studio report built on that sheet with one click. SpiderLens can only see the files it created, never the rest of the user's Drive.

**Why this priority**: Agencies share results with clients in Sheets and Looker Studio. It completes the integration story but is not needed for analysis inside the app.

**Independent Test**: With a fake Sheets service, export a 5,000-row tab, confirm the sheet holds the same rows and columns as the CSV export of that tab, and that the Looker Studio link produced points to that sheet and worksheet.

**Acceptance Scenarios**:

1. **Given** a connected Google account, **When** the user chooses "Export to Google Sheets" on a tab, **Then** a new spreadsheet is created in their Drive with the tab's visible columns and current filter, and the app shows a link to it.
2. **Given** an export larger than a spreadsheet can hold, **When** the user starts it, **Then** they are warned before anything is sent and offered to export the first rows that fit or to split across several sheets.
3. **Given** a finished export, **When** the user chooses "Open in Looker Studio", **Then** the system browser opens a new Looker Studio report connected to that sheet, under the user's own Google account.

---

### User Story 7 - Connections are opt-in, visible and safe to share (Priority: P1)

Every integration is off until the user turns it on. Settings › Integrations lists each provider, what it sends and where, whether it is connected and which account or key name it uses. Tokens and keys live only in the operating system's secure credential store, never in crawl files or exported profiles. A crawl file shared with a colleague carries the fetched data but no credentials. Disconnecting revokes access and deletes the stored credential.

**Why this priority**: The constitution requires third-party calls to be opt-in with the user's own credentials, and the product promises that sharing a crawl file never leaks secrets (spec 006). Every other story depends on this.

**Independent Test**: Connect a fake Google account and save a fake provider key, run fetches, save the crawl file and export the profile, then search both files byte by byte for the token and key values; then disconnect and confirm the credential store no longer holds them and the fake service received a revocation.

**Acceptance Scenarios**:

1. **Given** a fresh install, **When** the user crawls and views results, **Then** no request is sent to any integration provider and no integration UI asks for attention beyond a quiet "Connect data sources" entry.
2. **Given** a connected account and saved keys, **When** the user saves a crawl file or exports a profile, **Then** neither contains any token, key or the Google account's password, and the profile only names which provider and property to use.
3. **Given** the user disconnects Google or deletes a key, **When** they confirm, **Then** the credential is removed from the secure store, Google access is revoked, and already-fetched data in crawl files stays until the user removes it.
4. **Given** a crawl file received from someone else that contains integration data, **When** the user opens it, **Then** the data is visible with its source, account label and fetch date, and refreshing it requires the user's own connection.
5. **Given** a system without a usable secure credential store, **When** the user connects, **Then** they are told credentials will be kept only until the app quits, and nothing is written to disk in plain text.
6. **Given** any integration screen, **When** data is shown, **Then** it is labelled with its provider, date range or collection period and fetch time.

---

### User Story 8 - Integrations from the command line (Priority: P3)

Everything above can be done from the command line against a saved crawl file, using the same credentials the desktop app stored, so that integrations can be scripted and later scheduled (spec 021).

**Why this priority**: Constitution principle III requires command-line parity; scripting is how agencies run recurring audits.

**Independent Test**: Using the command line against a fixture crawl file and fake services, connect, fetch Search Console data and field data, export to a sheet, and compare the resulting crawl file with the one produced through the desktop flow.

**Acceptance Scenarios**:

1. **Given** a saved crawl file, **When** the user runs the command-line fetch for a provider, **Then** the crawl file gains the same data the desktop app would add, with progress printed and a non-zero exit code if the run is paused or fails.
2. **Given** no stored Google connection, **When** the user runs a Google fetch from the command line, **Then** it opens the system browser for consent (or prints the link when no browser is available) and stores the credential in the same secure store the desktop app uses.

---

### Edge Cases

- **URL matching**: Search Console groups data by the canonical URL Google chose; Analytics reports paths without the host. Matching uses the same URL normalisation as the crawl (spec 002), ignores fragments, treats `http`/`https` and trailing-slash variants as candidates only when the crawl holds exactly one of them, and never guesses across hosts. Unmatched rows are counted and listed, never silently dropped.
- **Wrong property**: The chosen property does not cover the crawled host: the user is warned before fetching and the match rate is shown afterwards.
- **No access**: The account has no access to any matching property, or access is revoked mid-run: the run stops with the provider's reason and keeps what was fetched.
- **Data delay**: Search Console final data trails by a few days and field data by about two days; the date range picker shows the latest available date, and "fresh" (incomplete) Search Console data is only included when the user opts in, labelled as such.
- **Quota exhaustion**: Per-minute limits slow the run down; per-day limits pause it with the reset time; the user can resume or cancel. A provider's "too many requests" answer honours any wait time it states (same policy as spec 004).
- **Provider errors**: Timeouts and server errors are retried with backoff (spec 004 policy); repeated failures pause the run with the error class and count. A failure for one URL never blocks the others.
- **Large sites**: A crawl of 1,000,000 URLs: Search Console and Analytics fetches page through all available rows without loading them all into memory; URL Inspection and lab tests are limited by the user's selection.
- **Cache**: A cached value is reused only if the provider, account or key, property, date range or settings and URL all match and the entry is younger than the provider's cache period; the user can force a refresh.
- **Re-crawl**: Integration data belongs to the crawl it was fetched for; a new crawl starts without it but reuses the cache, so fetching again is quick and free of quota for unchanged requests.
- **Cancel**: Cancelling a run keeps every result already stored.
- **Crawl file from an older version**: Opens normally with no integration data; the first fetch adds it (additive change, spec 006).
- **Offline**: Starting a fetch without network fails fast with a clear message; crawling is unaffected.
- **Clock**: Daily allowance resets follow the provider's own reset time (for Google, midnight Pacific Time), shown in the user's local time.

## Requirements *(mandatory)*

### Functional Requirements

**General**

- **FR-001**: Every integration MUST be off by default and enabled only by an explicit user action. With no integration enabled, the app MUST make no request to any integration provider.
- **FR-002**: Integration requests MUST NOT be made while a crawl is fetching pages, and MUST NOT share the crawl's connection limits, queues or threads. Integrations MUST read crawled URLs from the stored crawl data only. A profile option MAY start chosen integrations automatically once a crawl finishes; it is off by default.
- **FR-003**: Each integration run MUST show, before it starts, the provider, the account or key label, the property where relevant, what will be sent, the number of requests or units estimated and the share of the daily allowance it will use.
- **FR-004**: Each provider MUST have a request-rate limit and a concurrency limit set at or below the provider's published limit, editable by the user within that published limit.
- **FR-005**: Runs MUST show progress, an estimated finish time, and be pausable, resumable (also after reopening the crawl file) and cancellable, keeping every stored result.
- **FR-006**: When a provider's per-day allowance is exhausted, the run MUST pause with the provider's reset time; when the provider asks the client to wait, the run MUST wait at least that long.
- **FR-007**: Transient provider failures MUST be retried with backoff following the spec 004 retry policy; every URL that ends without data MUST record why (not requested, no data from provider, provider error with class and attempts, quota).
- **FR-008**: Responses MUST be cached on the user's machine outside crawl files, keyed by provider, credential identity, property, parameters and URL, with a per-provider cache period (defaults: Search Console and Analytics 24 hours, field data until the provider's next daily update, lab tests 24 hours, URL Inspection 24 hours, link metrics 7 days). Users MUST be able to force a refresh and clear the cache.
- **FR-009**: Fetched integration data MUST be stored in the crawl file with its provider, account or key label (never the credential), property, parameters, fetch time and the provider's own date range or collection period.
- **FR-010**: Integration data MUST appear as columns available in existing tabs (hidden by default except in the provider's own tab) and in a tab per provider, each with filters, sorting, search and CSV export like other tabs (spec 007).
- **FR-011**: The user MUST be able to remove one provider's data, or all integration data, from a crawl file.
- **FR-012**: Matching provider rows to crawled URLs MUST use the crawl's URL normalisation, MUST report how many rows matched and MUST list unmatched rows.

**Credentials**

- **FR-013**: OAuth tokens and API keys MUST be stored only in the operating system's secure credential store (macOS Keychain, Windows Credential Manager, Secret Service on Linux). They MUST be read into memory only while needed, and MUST NOT be written to crawl files, profiles, logs, caches, exports, reports or crash reports, and MUST be masked wherever displayed.
- **FR-014**: When no secure credential store is available, credentials MUST be held in memory only until the app quits, and the user MUST be told so.
- **FR-015**: Google sign-in MUST open the system browser (never an embedded web view), request only read-only scopes for Search Console and Analytics, and request write access only to files the app itself creates for the Sheets export. Each scope MUST be requested only when the user first uses a feature that needs it.
- **FR-016**: The app MUST support a built-in Google sign-in when a SpiderLens Google client is provided by the owner, and MUST always allow the user to use their own Google client ID instead. Without either, Google sign-in features MUST say what is needed and stay disabled; key-based providers remain usable.
- **FR-017**: Each provider MUST have a "Forget" action. For a Google account it MUST revoke access with Google and delete the token from the credential store; for a key it MUST delete the key from the credential store.
- **FR-018**: Several Google accounts MAY be connected; each run MUST state which account it uses.
- **FR-019**: Settings › Integrations MUST list each provider with: on/off state, connection or key status, account or key label, what data is sent to it, its rate and concurrency limits, today's usage where known, and a link to the provider's own terms.

**Search Console**

- **FR-020**: The app MUST fetch clicks, impressions, CTR and average position per page for a user-chosen property, date range (up to the provider's history limit, default the last 28 days of final data) and search type (default web), paging through all available rows.
- **FR-021**: The app MUST offer per-page top queries on demand from the inspector.
- **FR-022**: The app MUST support URL Inspection for a user-chosen URL set, store the fields listed in User Story 2, track the property's daily allowance and continue across days.
- **FR-023**: Search Console and Analytics rows that match no crawled URL MUST be stored as orphan candidates with their source and metrics, and offered to spec 011 (orphan pages) and as input for a new list crawl.

**Analytics**

- **FR-024**: The app MUST fetch, per landing page, sessions, engaged sessions, engagement rate, key events and total revenue for a chosen GA4 property, web data stream and date range, with the option to combine or separate query-string variants.

**Page experience**

- **FR-025**: The app MUST fetch page-level field data (LCP, INP, CLS, FCP, TTFB at the 75th percentile with good / needs improvement / poor rating) for phone and desktop, fall back to clearly labelled origin-level data, and store the collection period.
- **FR-026**: The app MUST run lab tests for a user-chosen URL set and device (mobile, desktop or both), storing performance score, lab metrics, top opportunities, and the accessibility, best-practices and SEO category scores and audits for use by spec 026.
- **FR-027**: Field data and lab tests MUST use the user's own Google API key, stored per FR-013.

**Link metrics**

- **FR-028**: The app MUST support Ahrefs, Majestic and Moz with the user's own key, fetching a user-chosen set of each provider's page-level and domain-level metrics for a chosen URL set, in the provider's batch form where one exists.
- **FR-029**: Before each link-metrics run, the app MUST show the estimated units or rows in the provider's own terms, require confirmation, and stop at a user-set cap (default: the estimate plus 10%).

**Export**

- **FR-030**: The app MUST export a tab (visible columns, current filter and sort) or the integration tabs to a new Google Sheet in the user's Drive, warn before exports exceed a spreadsheet's capacity, and offer to truncate or split.
- **FR-031**: The app MUST offer to open a new Looker Studio report connected to an exported sheet, under the user's own Google account, without any SpiderLens server.

**Command line**

- **FR-032**: Every integration action (connect, disconnect, list properties, fetch per provider, inspect, export, clear cache, remove data) MUST be available from the command line against a crawl file, sharing the desktop app's credential store and cache, and returning a distinct exit code when a run ends paused for quota.

### Key Entities

- **Integration Provider**: One external data source (Search Console, URL Inspection, Analytics, field data, lab tests, Ahrefs, Majestic, Moz, Sheets export) with its rate limits, daily allowance, cache period and the data it returns.
- **Connection**: A credential reference: a Google account (by email label) or a named API key for a provider. The secret itself lives only in the secure credential store.
- **Property Binding**: The provider-side target used for a crawl: Search Console property, GA4 property and data stream, or none for URL-based providers. Stored in the crawl file and optionally in a profile.
- **Integration Run**: One fetch for one provider and one crawl: parameters, URL set, state (queued, running, paused for quota, paused by user, finished, cancelled, failed), counts, usage consumed, timing.
- **Integration Result**: The data for one URL from one run (metrics plus provider date range or collection period), or the reason no data was stored.
- **Orphan Candidate**: A URL reported by a provider with traffic or impressions that matches no crawled URL, with its source and metrics.
- **Cache Entry**: A stored provider response on the user's machine, keyed per FR-008, with its expiry.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: Crawl speed is unchanged by this feature: on the 100,000-page fixture site, crawl time with every integration connected and set to run after the crawl is within 2% of the time with none (measured on the reference machine, spec 008).
- **SC-002**: Search Console page data for a property with 100,000 pages and a 28-day range is fetched and matched in under 2 minutes against a fake service with realistic response sizes, with app memory staying flat during the fetch.
- **SC-003**: Field data for 1,000 URLs on both devices finishes within the provider's per-minute limit and in under 15 minutes, and no run in the test suite ever exceeds a provider's configured rate.
- **SC-004**: 100% of test runs that save crawl files or export profiles after using credentials contain none of the credential values (byte search in CI).
- **SC-005**: A second identical fetch within the cache period sends zero requests to the provider.
- **SC-006**: Link-metrics estimates shown before a run are within 10% of the units consumed in the fake-provider tests, and no run consumes more than the user's cap.
- **SC-007**: A first-time user can connect Google and see Search Console columns for their crawl in under 2 minutes in a moderated test, without reading documentation.
- **SC-008**: Every acceptance scenario passes in CI on Windows, macOS and Linux against local fake provider services; no test calls a live provider.

## Assumptions

- **No SpiderLens servers**: Sign-in uses the desktop flow Google recommends for installed apps (system browser and a local loopback redirect). The built-in Google sign-in needs a Google Cloud project owned by the project owner, with a public home page and privacy policy, and passes Google's review for the read-only Search Console and Analytics scopes; until then users bring their own client ID, or use the owner's project as listed test users. This is an owner action outside the code (see research.md).
- **User-owned keys**: Field data, lab tests and link-metrics providers use the user's own keys and quotas. SpiderLens never ships a shared key, because a key embedded in a desktop app is public and its quota would be shared by every user.
- **Credential store versus specs 002 and 006 (decided by the owner on 2026-10-13)**: Those specs ask for crawl secrets to be re-entered on resume. Integration credentials are kept in the OS credential store instead (macOS Keychain, Windows Credential Manager, Secret Service on Linux), read into memory only while the app or command line runs, because sign-in tokens cannot reasonably be re-entered and scheduled command-line runs (spec 021) need them without a person present. The rule that crawl files, profiles, logs, exports and reports never contain secrets is unchanged, and site passwords and login cookies (spec 025) stay unstored as today.
- **Integration data in crawl files is not secret but may be confidential**: Clicks, traffic and revenue are business data. They are stored in the crawl file so it stays self-contained, are listed in the crawl's "what this file contains" summary (spec 006), and can be removed before sharing (FR-011).
- **Provider limits change**: Defaults come from the providers' published limits in October 2026 (research.md) and live in configuration, not in the spec.
- **Ownership across specs**: This spec owns all calls to these providers and the stored results. Spec 011 consumes orphan candidates; spec 026 consumes the lab-test accessibility and mobile data; spec 010 owns file export formats (CSV, Excel) while this spec owns Google Sheets and Looker Studio; spec 009 rules may use integration columns as inputs once present; spec 021 schedules command-line runs. Storage changes follow the spec 006 additive migration rule.
- **Out of scope**: Writing anything to Search Console or Analytics (sitemap submission, indexing requests), Bing Webmaster Tools, Semrush and other providers not listed, Looker Studio community connectors, Google Sheets import, keyword research, rank tracking, and data for URLs outside the crawl other than orphan candidates.
