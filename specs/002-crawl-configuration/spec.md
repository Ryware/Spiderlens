# Feature Specification: Crawl Configuration

**Feature Branch**: `002-crawl-configuration`

**Created**: 2026-10-11

**Status**: Approved (2026-10-11)

**Input**: User description: "Crawl configuration: the user starts a crawl in Spider mode (one start URL) or List mode (paste or upload URLs), and can set limits (max URLs, max depth), include/exclude patterns, user agent (presets and custom), custom request headers, speed settings (polite default and fast presets, max concurrent requests, max URLs per second), robots.txt behaviour, subdomain and external link handling, and retry settings. Configurations can be saved as reusable profiles, and every crawl stores the exact configuration used."

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Start a Spider crawl from one URL with safe defaults (Priority: P1)

An SEO specialist opens SpiderLens, types a website address into the start field, keeps Spider mode selected and presses Start. The crawl begins straight away using the default configuration: polite speed, robots.txt honoured, the honest SpiderLens user agent, only the start URL's own subdomain treated as internal, no URL or depth limit, and the default retry policy. The user never has to open a settings screen to get a correct, respectful crawl.

**Why this priority**: This is the shortest path to the first usable crawl (milestone M1). Every other configuration option refines this flow; without it nothing can be crawled.

**Independent Test**: On a local fixture site, enter the fixture's home page, press Start without changing any setting, and confirm the crawl runs with every default value listed in this spec, and that the stored configuration of the finished crawl matches those defaults exactly.

**Acceptance Scenarios**:

1. **Given** the app is open with no settings changed, **When** the user enters a valid start URL in Spider mode and presses Start, **Then** the crawl begins using the default configuration and the start URL is the first URL requested.
2. **Given** the user types an address without a scheme (for example `example.com`), **When** they press Start, **Then** the app uses `https://example.com/`, shows the completed address in the start field, and begins the crawl.
3. **Given** the user enters text that is not a valid web address (for example `ht!tp//bad`), **When** they press Start, **Then** the crawl does not begin and a message next to the start field explains what is wrong.
4. **Given** the start URL redirects to another host on the same site (for example `example.com` to `www.example.com`), **When** the crawl begins, **Then** the redirect target's host is treated as internal for the rest of the crawl and this decision is visible in the stored configuration.
5. **Given** a crawl was started with defaults, **When** the user looks at the crawl's configuration afterwards, **Then** they see every setting value that was used, including those they never touched.

---

### User Story 2 - Crawl a supplied list of URLs (List mode) (Priority: P1)

A user has a list of URLs (from a sitemap export, analytics report or migration plan) and wants each one checked without following links. They switch to List mode, then either paste URLs into a text box or upload a text or CSV file. The app shows how many valid, duplicate and invalid entries it found, lets them review invalid lines, and crawls only the valid, unique URLs.

**Why this priority**: List mode is the second M1 crawl mode and is essential for audits of known URL sets such as redirect checks after a migration. It shares the same configuration and stands alone from Spider mode.

**Independent Test**: Paste a list containing valid URLs, duplicates, blank lines and malformed lines; confirm the counts shown, confirm only the unique valid URLs are crawled, and confirm no discovered links are crawled.

**Acceptance Scenarios**:

1. **Given** List mode is selected, **When** the user pastes 10 lines of which 7 are valid unique URLs, 2 are duplicates of those and 1 is malformed, **Then** the app reports 7 URLs to crawl, 2 duplicates removed and 1 invalid line, and lets the user see the invalid line.
2. **Given** List mode is selected, **When** the user uploads a plain text file with one URL per line, or a CSV file, **Then** every URL found in the file is loaded and the same counts are shown.
3. **Given** a list has been loaded, **When** the crawl runs, **Then** only the listed URLs are requested; links found on those pages are recorded but not crawled.
4. **Given** List mode is selected and the list has no valid URLs, **When** the user presses Start, **Then** the crawl does not begin and the app explains that the list is empty or invalid.
5. **Given** a list contains URLs from several different sites, **When** the crawl runs, **Then** each URL is requested, and robots.txt and speed limits are applied per site.

---

### User Story 3 - Control scope: limits, patterns, subdomains and external links (Priority: P2)

A user crawling a large site wants to limit the crawl to the part they care about. They set a maximum number of URLs, a maximum link depth, include patterns (for example only `/blog/`) and exclude patterns (for example anything with `?sort=`). They choose whether other subdomains of the same site count as internal, and whether external links are status-checked. Before starting, they test their patterns against a few sample URLs to be sure they behave as expected.

**Why this priority**: Large sites cannot be crawled usefully without scope control, but a first crawl of a small site works without it.

**Independent Test**: On a fixture site with known structure, set each scope option in turn and confirm the set of crawled URLs exactly matches the expected set, and that URLs left out are recorded with the reason they were not crawled.

**Acceptance Scenarios**:

1. **Given** max URLs is set to 100 on a site with 1,000 pages, **When** the crawl runs, **Then** exactly 100 URLs are crawled, the crawl ends with the status "stopped at URL limit", and remaining discovered URLs are recorded as not crawled because of the limit.
2. **Given** max depth is 2, **When** the crawl runs, **Then** the start URL (depth 0) and pages up to 2 clicks away are crawled, and pages at depth 3 are recorded but not crawled, with the reason "beyond max depth".
3. **Given** an include pattern matching `/blog/` and an exclude pattern matching `/blog/tag/`, **When** a page links to `/blog/post-1` and `/blog/tag/news`, **Then** `/blog/post-1` is crawled and `/blog/tag/news` is recorded as excluded by configuration.
4. **Given** the user enters a pattern with invalid syntax, **When** they leave the field or press Start, **Then** the field is marked invalid with an explanation and the crawl cannot start until it is fixed.
5. **Given** the user opens the pattern tester and pastes sample URLs, **When** they run the test, **Then** each sample URL is shown as included or excluded together with the pattern that decided it.
6. **Given** the default subdomain setting, **When** the start URL is `www.example.com` and a page links to `blog.example.com`, **Then** the `blog.example.com` link is treated as external; **Given** "crawl all subdomains" is on, **Then** it is treated as internal and crawled.
7. **Given** external link checking is on (the default), **When** a page links to another site, **Then** the external URL is requested once to record its status but its links are never followed; **Given** external link checking is off, **Then** the external URL is recorded without being requested.

---

### User Story 4 - Choose identity, headers, speed, robots and retry behaviour (Priority: P2)

A user needs to crawl a staging site that requires a custom header, wants to see the site as a mobile search engine bot would, and wants the crawl to go faster because they control the server. They pick a user agent preset (or type a custom one), add request headers, switch from the "Polite" to the "Fast" speed preset or set concurrency and URLs per second by hand, choose how robots.txt is treated, and adjust retry attempts. Any setting that makes the crawl less respectful is clearly labelled.

**Why this priority**: Needed for staging sites, bot-specific checks and fast crawls of owned sites; defaults already cover public sites.

**Independent Test**: On fixture sites that echo request headers, enforce robots.txt and return controlled errors, change each setting and confirm the observed requests, request rate, robots behaviour and retry counts match the configuration.

**Acceptance Scenarios**:

1. **Given** the user selects the "Googlebot Smartphone" preset, **When** the crawl runs, **Then** every request carries that user agent string and robots.txt rules are matched for that bot's token.
2. **Given** the user enters a custom user agent, **When** they save it, **Then** they must also provide (or accept the suggested) robots.txt token used for matching robots rules.
3. **Given** the user adds a header `X-Staging-Key: abc`, **When** the crawl runs, **Then** every request to internal URLs includes that header.
4. **Given** the user tries to add a header the app controls itself (for example `Host` or `User-Agent`), **When** they save it, **Then** the app refuses with an explanation of where that value is set instead.
5. **Given** the Polite preset (default), **When** the crawl runs against a fixture, **Then** measured concurrency and request rate never exceed the Polite values, and the site's robots.txt crawl-delay is honoured.
6. **Given** the user selects the Fast preset or raises concurrency, **When** they confirm, **Then** the app shows a clear notice that this setting is intended for sites they control, and the crawl uses the new values.
7. **Given** the user edits any individual speed value after choosing a preset, **When** they look at the preset selector, **Then** it shows "Custom".
8. **Given** robots.txt is set to "Ignore", **When** the user selects it, **Then** a clearly labelled warning is shown, and during the crawl disallowed URLs are crawled but still marked as "disallowed by robots.txt".
9. **Given** retry attempts are set to 1, **When** a URL times out, **Then** it is not retried and its final state records 1 attempt; **Given** the default of 3, **Then** it is tried up to 3 times in total before being recorded as failed.

---

### User Story 5 - Save and reuse configuration profiles (Priority: P3)

A consultant audits the same kinds of sites every week. They save their configuration as a named profile ("Client A staging", "Mobile bot check"), choose which profile is the default for new crawls, and can load, rename, duplicate, delete, export and import profiles. An exported profile can be shared with a colleague or used by the command-line crawler.

**Why this priority**: Saves time and prevents mistakes for repeat users, but every crawl works without profiles.

**Independent Test**: Create a profile with non-default values, restart the app, load the profile, and confirm every value is restored; export it, import it on a clean install, and start the same crawl from the command line with it.

**Acceptance Scenarios**:

1. **Given** a configuration with non-default values, **When** the user saves it as a profile named "Client A", **Then** "Client A" appears in the profile list and remains after the app restarts.
2. **Given** the user saves a profile with a name that already exists, **When** they confirm, **Then** the app asks whether to overwrite or choose another name.
3. **Given** profiles exist, **When** the user marks one as default, **Then** new crawls start with that profile's settings.
4. **Given** a profile is loaded and then changed, **When** the user looks at the profile selector, **Then** it shows that the current settings differ from the saved profile, and offers to save, save as new, or revert.
5. **Given** an exported profile file, **When** it is imported on another machine or passed to the command-line crawler, **Then** the resulting configuration is identical to the original.
6. **Given** the user wants to undo all changes, **When** they choose "Reset to defaults", **Then** every setting returns to the built-in defaults.

---

### User Story 6 - Every crawl remembers exactly how it was configured (Priority: P2)

Weeks after a crawl, a user (or a colleague who received the crawl file) needs to know exactly how it was run: which mode, start URL or list, limits, patterns, user agent, headers, speed, robots and retry settings, and which app version ran it. They open the crawl and view its configuration read-only. They can start a new crawl with the same configuration, or save it as a profile.

**Why this priority**: Required by the constitution (Principle V) so results can be explained and reproduced; also needed for resume after pause or restart.

**Independent Test**: Run crawls with varied configurations, close and reopen each crawl file, and confirm the displayed configuration and app version match what was set, setting by setting.

**Acceptance Scenarios**:

1. **Given** a finished or paused crawl, **When** the user opens its configuration view, **Then** every setting used and the app version are shown and cannot be edited there.
2. **Given** a stored crawl configuration, **When** the user chooses "New crawl with this configuration", **Then** a new crawl is set up with identical settings, ready to start.
3. **Given** a paused crawl, **When** the user changes speed or retry settings and resumes, **Then** the new values take effect and the stored configuration records the change and when it happened; scope settings (mode, start URL or list, limits, patterns, subdomain and external handling) cannot be changed for that crawl.
4. **Given** a crawl file created by an older app version, **When** it is opened, **Then** its configuration is shown as it was recorded, and settings that did not exist in that version are shown as "not available in this version".

---

### Edge Cases

- Start URL is excluded by an exclude pattern, or does not match any include pattern: the start URL is still crawled (it is the user's explicit choice) but a warning is shown before starting so the user can correct the patterns.
- Start URL redirects to a different site entirely (for example `old-brand.com` to `new-brand.com`): the redirect is recorded and the crawl does not follow into the other site; the user is told the start URL redirected off-site and can restart with the new address.
- Start URL robots.txt disallows everything: the crawl ends with the start URL recorded as blocked by robots.txt and a message explaining why nothing else was crawled.
- Max URLs or max depth set to zero, negative or non-numeric: rejected with an explanation; blank means no limit.
- Max depth set in List mode: not applicable; the field is disabled in List mode.
- Very large lists (1,000,000 URLs pasted or uploaded): accepted without freezing the app; counts appear once processing finishes and progress is shown meanwhile.
- Uploaded file in an unsupported format, empty, or with an unexpected text encoding: rejected or partly loaded with a clear message about what was read.
- CSV file with several columns: every cell that is a valid web address is loaded; other cells are ignored.
- The same URL differing only in letter case of the host, a trailing default port or a fragment: treated as duplicates after normalization.
- Custom header with an empty name, invalid characters, or a duplicated name: rejected with an explanation.
- Max URLs per second set lower than the robots.txt crawl-delay implies (or higher): the stricter of the two applies while crawl-delay is honoured.
- Retry-After from the server is longer than the maximum backoff: the server's value is honoured.
- Profile file imported from a newer app version containing unknown settings: unknown settings are ignored with a notice listing them; known settings load.
- Profile file that is corrupt or not a SpiderLens profile: refused with a message; existing profiles are untouched.
- Deleting the profile currently marked as default: the built-in defaults become the default again.
- Configuration includes a header carrying a secret (for example an authorization token): see FR-038.

## Requirements *(mandatory)*

### Functional Requirements

**Crawl modes and starting a crawl**

- **FR-001**: Users MUST be able to choose between Spider mode (one start URL, follow links) and List mode (crawl a supplied set of URLs only) before starting a crawl.
- **FR-002**: In Spider mode, the system MUST accept a single start URL, add `https://` when no scheme is given, show the completed address, and refuse to start with a clear message when the address is not a valid web address.
- **FR-003**: In Spider mode, if the start URL redirects to another host of the same registrable domain, the system MUST treat the redirect target's host as the internal host for the crawl and record this in the crawl's stored configuration.
- **FR-004**: In List mode, users MUST be able to supply URLs by pasting text or uploading a plain text file or a CSV file.
- **FR-005**: In List mode, the system MUST normalize the supplied URLs, remove duplicates, and report the number of URLs to crawl, duplicates removed and invalid entries, and MUST let the user view the invalid entries before starting.
- **FR-006**: In List mode, the system MUST request only the supplied URLs; links found on those pages MUST be recorded but not crawled.
- **FR-007**: The system MUST refuse to start a crawl whose configuration is invalid, and MUST identify each invalid setting with an explanation of how to fix it.

**Limits and scope**

- **FR-008**: Users MUST be able to set a maximum number of URLs to crawl (positive whole number, blank meaning no limit; default no limit). When reached, the crawl MUST end with the status "stopped at URL limit" and remaining discovered URLs MUST be recorded as not crawled because of the limit.
- **FR-009**: Users MUST be able to set a maximum crawl depth in Spider mode, where the start URL is depth 0 and depth is the fewest link clicks from it (whole number, blank meaning no limit; default no limit). URLs beyond the limit MUST be recorded as not crawled because of depth.
- **FR-010**: Users MUST be able to add any number of include patterns and exclude patterns, matched against the full normalized URL. When at least one include pattern exists, a discovered URL MUST match at least one include pattern to be crawled; a URL matching any exclude pattern MUST NOT be crawled, even if it matches an include pattern.
- **FR-011**: The system MUST validate pattern syntax as the user types and before start, and MUST provide a pattern tester that shows, for user-supplied sample URLs, whether each would be crawled and which pattern decided it.
- **FR-012**: URLs not crawled because of limits, patterns, subdomain or external rules MUST be recorded with the specific reason, as "blocked by configuration", which is a final state and not an error.
- **FR-013**: The start URL, and every URL supplied in List mode, MUST be crawled regardless of include and exclude patterns; the system MUST warn before starting if the start URL would otherwise be excluded.
- **FR-014**: Users MUST be able to choose subdomain handling: "this subdomain only" (default: only the start URL's exact host is internal) or "all subdomains" (every host of the same registrable domain is internal).
- **FR-015**: Users MUST be able to choose whether external links are status-checked (default on). When on, each external URL MUST be requested at most once to record its response and MUST NOT have its links followed. When off, external URLs MUST be recorded without being requested.

**User agent and request headers**

- **FR-016**: Users MUST be able to choose a user agent from presets: SpiderLens (default, an honest identifier including the app version), Googlebot Smartphone, Googlebot Desktop, Bingbot, Chrome desktop browser and Chrome mobile browser, or enter a custom user agent.
- **FR-017**: Each user agent MUST have an associated robots.txt token used to match robots rules; presets supply it automatically, and a custom user agent MUST require one (with a suggested value).
- **FR-018**: Choosing any user agent other than the SpiderLens default MUST show a short notice that the crawler will no longer identify itself as SpiderLens.
- **FR-019**: Users MUST be able to add, edit and remove custom request headers as name and value pairs, and choose for each whether it is sent to internal URLs only (default) or to all URLs.
- **FR-020**: The system MUST reject headers with empty or invalid names, duplicate names, and names the app sets itself (at least `Host`, `User-Agent`, `Content-Length`, `Connection`, `Transfer-Encoding`), explaining where the app-controlled value is set instead.

**Speed**

- **FR-021**: The system MUST provide two speed presets: "Polite" (default) and "Fast". Selecting a preset MUST fill in the individual speed values; editing any value MUST change the preset shown to "Custom".
- **FR-022**: Users MUST be able to set the maximum number of concurrent requests overall, the maximum concurrent requests per host, and the maximum URLs per second overall (blank meaning no rate cap).
- **FR-023**: The system MUST never exceed the configured maximum concurrent requests, per-host maximum, or URLs per second, and within those limits MUST adapt per-host concurrency, lowering it when a host slows down or returns 429 or 503 responses. Users MUST be able to turn adaptive adjustment off.
- **FR-024**: Selecting the Fast preset, or any value above the Polite values, MUST show a clearly labelled notice that faster settings are intended for sites the user controls.
- **FR-025**: The system MUST honour robots.txt crawl-delay by default; users MUST be able to turn this off, with a clearly labelled warning. When both crawl-delay and the URLs-per-second cap apply to a host, the stricter MUST win.
- **FR-026**: Users MUST be able to set the connect timeout (default 10 seconds) and read timeout (default 30 seconds) and the maximum number of redirect hops followed per URL (default 10).

**Robots.txt**

- **FR-027**: Users MUST be able to choose robots.txt behaviour: "Respect" (default: disallowed URLs are not requested and are recorded as blocked by robots.txt), "Ignore but report" (disallowed URLs are crawled and marked as disallowed), or "Ignore" (robots.txt is not fetched). Both non-default options MUST show a clearly labelled warning.
- **FR-028**: Users MUST be able to choose whether nofollow links are followed (default: not followed) and how query parameters are handled; the detailed behaviour of these and other scope rules is defined in the crawl frontier specification, but their values MUST be part of the configuration covered by this spec.

**Retries**

- **FR-029**: Users MUST be able to set the total number of attempts per URL for transient failures (whole number from 1 to 10; default 3) and the maximum wait between attempts (default 60 seconds).
- **FR-030**: Retries MUST apply only to transient failure classes (temporary DNS failure, connection failure, timeout, HTTP 429, HTTP 5xx, truncated body), use increasing waits with random variation, and honour a server's Retry-After value even when it exceeds the maximum wait. Users MUST be able to exclude HTTP 5xx and HTTP 429 from retries individually.

**Profiles**

- **FR-031**: Users MUST be able to save the current configuration as a named profile, and load, rename, duplicate and delete profiles. Profiles MUST persist across app restarts and upgrades.
- **FR-032**: Users MUST be able to mark one profile as the default for new crawls; when none is marked, the built-in defaults apply. Users MUST be able to reset the current configuration to the built-in defaults at any time.
- **FR-033**: The system MUST show when the current settings differ from the loaded profile and offer to save, save as a new profile, or revert.
- **FR-034**: A profile MUST hold every crawl setting except the crawl mode's input (start URL or URL list), which is stored separately so a profile can be reused across sites. Users MUST be able to choose to include a start URL in the profile.
- **FR-035**: Users MUST be able to export a profile to a file and import a profile from a file. Importing a profile with settings unknown to the current version MUST load the known settings and list the ignored ones; a corrupt or foreign file MUST be refused without changing existing profiles.
- **FR-036**: The command-line crawler MUST accept the same configuration, either from an exported profile file or as individual options, apply the same validation and defaults, and produce identical crawl behaviour to the desktop app for the same configuration.

**Stored configuration per crawl**

- **FR-037**: Every crawl MUST store, inside its crawl file, the complete configuration it used (every setting, including defaults the user did not change), the start URL or full URL list, the resolved internal host(s), and the app version, at the moment the crawl starts.
- **FR-038**: Custom header values and any other secret-bearing settings MUST be handled in stored crawl files and exported profiles as follows: users can mark a header value as secret; secret values are never written to crawl files or exported profiles (only the header name and a placeholder are kept), are masked wherever displayed or logged, and must be re-entered when a crawl is resumed or a profile is used (consistent with specs 004 and 006).
- **FR-039**: Users MUST be able to view any crawl's stored configuration read-only, start a new crawl with it, and save it as a profile.
- **FR-040**: While a crawl is paused or running, users MUST be able to change speed and retry settings only; each change MUST be applied from that point on and recorded in the crawl's stored configuration with the time it was made. All other settings MUST be locked for that crawl.
- **FR-041**: Resuming a crawl (after pause, quit or crash) MUST use its stored configuration, including any recorded speed or retry changes.
- **FR-042**: Opening a crawl file from an older app version MUST show its stored configuration as recorded, marking settings that did not exist in that version as not available.

### Key Entities

- **Crawl Configuration**: The full set of settings for one crawl: mode, mode input, limits, include and exclude patterns, subdomain and external handling, nofollow and query parameter handling, user agent and robots token, custom headers, speed values and preset name, robots.txt behaviour, crawl-delay handling, timeout, redirect limit, and retry settings.
- **Mode Input**: For Spider mode, the start URL and the resolved internal host(s); for List mode, the normalized, de-duplicated URL list plus the counts of duplicates and invalid entries and the invalid entries themselves.
- **Configuration Profile**: A named, saved Crawl Configuration without mode input (unless the user chooses to include a start URL); one may be marked as default. Can be exported to and imported from a file.
- **Stored Crawl Configuration**: The exact Crawl Configuration and Mode Input recorded inside a crawl file when the crawl starts, plus the app version and a timestamped log of any speed or retry changes made during the crawl. Read-only after the crawl starts except for that log.
- **User Agent Preset**: A named user agent string with its robots.txt token.
- **Speed Preset**: A named set of speed values (Polite, Fast); "Custom" when values have been edited.
- **Custom Header**: A name, a value, and whether it is sent to internal URLs only or to all URLs.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: A first-time user can start a Spider crawl with default settings within 30 seconds of opening the app, without opening any settings screen.
- **SC-002**: A list of 100,000 URLs can be pasted or uploaded and its valid, duplicate and invalid counts shown within 5 seconds, and a list of 1,000,000 URLs is accepted without the app becoming unresponsive (interactions respond within about 100 ms).
- **SC-003**: In automated tests on fixture sites, 100% of scope settings (max URLs, max depth, include and exclude patterns, subdomain handling, external handling) produce exactly the expected set of crawled and not-crawled URLs, with the correct reason recorded for each not-crawled URL.
- **SC-004**: With the Polite preset on a fixture site, the measured number of simultaneous requests and requests per second never exceed the configured values (zero violations across the test run), and robots.txt crawl-delay is honoured.
- **SC-005**: With the Fast preset (or equivalent custom values) on the local 100,000-page fixture site, the crawl sustains at least 500 URLs per second.
- **SC-006**: For 100% of crawls in tests, reopening the crawl file shows a stored configuration identical, setting by setting, to the one used, along with the app version; starting a new crawl from it produces an identical configuration.
- **SC-007**: A profile exported and imported on another machine, or passed to the command-line crawler, reproduces the original configuration with 100% of settings identical.
- **SC-008**: 100% of invalid settings (bad start URL, empty list, bad pattern, invalid limit, forbidden header) are caught before the crawl starts, each with a message naming the setting and how to fix it.
- **SC-009**: Every setting that makes crawling less respectful than the default (non-SpiderLens user agent, Fast or higher speed, ignoring robots.txt or crawl-delay) shows a visible label or warning in 100% of cases, verified by automated UI tests.
- **SC-010**: A repeat user can load a saved profile and start a crawl in 3 interactions or fewer.

## Assumptions

- Built-in defaults: Spider mode; no URL or depth limit; no patterns; this subdomain only; external links status-checked; SpiderLens user agent; no custom headers; Polite speed preset; robots.txt "Respect"; crawl-delay honoured; nofollow links not followed; connect timeout 10 seconds and read timeout 30 seconds (as in spec 004); up to 10 redirect hops; 3 attempts in total for transient failures, waits of about 1, 2 and 4 seconds with random variation, capped at 60 seconds unless Retry-After is longer.
- Speed preset values are owned by spec 008 (crawl performance): Polite = up to 8 simultaneous requests overall, up to 4 per site, at most 5 URLs per second; Fast = up to 64 overall, up to 32 per site, no URLs-per-second cap. Adaptive per-site adjustment is on for both.
- Include and exclude patterns use regular expression syntax matched against the full normalized URL, as is familiar from other SEO crawlers; the pattern tester mitigates the learning curve.
- Missing scheme defaults to `https://`; if that fails to connect, the normal error handling applies and the user can retry with `http://`.
- In List mode, include and exclude patterns and depth do not apply to the supplied URLs; robots.txt, speed, user agent, headers and retry settings do.
- "Registrable domain" follows the public list of domain suffixes, so `example.co.uk` and `blog.example.co.uk` are the same site while `a.github.io` and `b.github.io` are not.
- Profiles are stored on the user's machine only (Principle II); sharing happens by exporting files.
- Changing a setting mid-crawl is limited to speed and retry settings so stored results stay explainable (Principle V).
- Detailed behaviour of URL normalization, query parameter handling, nofollow handling, error classes, the per-host circuit breaker and the "retry failed URLs" action is defined in the crawl engine and frontier specifications; this spec defines the user-facing settings and their storage.
- Out of scope for this feature: Compare and SERP crawl modes (later milestones), JavaScript rendering settings (M4), authentication flows such as form login (beyond custom headers), proxy settings, scheduled crawls, sitemap-based crawl input, AEO/GEO and AI agent settings (M5, M6), and spoofing protection beyond the labels and warnings described here.
- Dependencies: the foundation spec (001) for the app and command-line crawler, the crawl engine and storage specs for applying the configuration and saving it in the crawl file.
