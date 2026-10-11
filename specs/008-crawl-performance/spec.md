# Feature Specification: Crawl Speed and Performance

**Feature Branch**: `008-crawl-performance`

**Created**: 2026-10-11

**Status**: Approved (2026-10-11)

**Input**: User description: "Crawl speed and performance: the crawler keeps the network busy without overloading sites; it adapts the number of simultaneous requests per site to how fast the site responds, slows down on rate-limit or overload signals, respects the user's global speed caps, keeps memory flat on very large crawls, and never lets the interface or disk writes slow the crawl. Includes measurable benchmark targets against a local test site and a user-visible speed indicator."

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Crawl a large site quickly within my speed limits (Priority: P1)

An SEO consultant crawls a client's 100,000-page site. They pick the "Fast" speed preset (or keep the default "Polite" preset) and optionally set their own caps: the most requests allowed at the same time across the whole crawl, the most URLs per second, and the most requests at the same time to any single site. The crawler keeps as many requests in flight as those limits allow, so the crawl finishes as fast as the site and the limits permit, and it never goes over any limit the user set.

**Why this priority**: Speed is the headline expectation of a desktop crawler. Without it, large crawls are impractical, and without hard caps users cannot trust the tool on sites they do not control.

**Independent Test**: Crawl the local 100,000-page fixture site with the Fast preset and record throughput; repeat with a global cap of 50 URLs per second and 10 simultaneous requests and confirm, from the fixture site's own request log, that neither cap is ever exceeded.

**Acceptance Scenarios**:

1. **Given** the local 100,000-page fixture site responding instantly and the Fast preset, **When** the user crawls it, **Then** the crawl sustains at least 500 URLs per second on the reference machine and every page ends in exactly one final state.
2. **Given** a global cap of 50 URLs per second, **When** the user crawls the fixture site, **Then** no one-second window contains more than 50 requests started by the crawler.
3. **Given** a global cap of 10 simultaneous requests, **When** the crawl runs, **Then** the fixture site never sees more than 10 open requests from the crawler at the same moment.
4. **Given** a crawl in progress, **When** the user lowers or raises a speed cap, **Then** the new cap takes effect within 2 seconds without restarting or pausing the crawl.
5. **Given** a fixture site that adds 100 ms of delay to every response and a cap of 50 simultaneous requests, **When** the user crawls it, **Then** the crawler keeps close to 50 requests in flight at all times (the network is kept busy) rather than waiting for one batch to finish before starting the next.
6. **Given** the same speed settings, **When** the crawl is started from the command-line interface instead of the desktop app, **Then** the same caps, presets and adaptive behaviour apply.

---

### User Story 2 - Never overload the site being crawled (Priority: P1)

A user crawls a production site on modest hosting. As the site starts to struggle (responses get slower, or it answers "too many requests" or "service unavailable"), the crawler automatically reduces how many requests it sends to that site at once, waits as long as the site asks, and only speeds up again gradually once the site is responding normally. A site that responds quickly and consistently is allowed more simultaneous requests, up to the user's per-site limit.

**Why this priority**: Respectful crawling by default is a core principle. Overloading a client's site can cause outages, IP bans and lost trust, and it also produces bad data (timeouts and errors that are the crawler's own fault).

**Independent Test**: Crawl fixture sites that (a) return "too many requests" when more than 4 requests are open at once, (b) get slower as load rises, and (c) return "service unavailable" with a wait time, and confirm from the fixture logs that the crawler backs off, settles below the overload point and completes every URL.

**Acceptance Scenarios**:

1. **Given** a site that responds quickly and with steady response times, **When** the crawl runs, **Then** the number of simultaneous requests to that site rises step by step until it reaches the per-site limit or the global limit.
2. **Given** a site whose response times rise sharply as more requests are sent, **When** the slowdown is detected, **Then** the crawler reduces the number of simultaneous requests to that site within 10 seconds.
3. **Given** a site that answers "too many requests" (HTTP 429) or "service unavailable" (HTTP 503), **When** such a response arrives, **Then** the crawler immediately cuts the simultaneous requests to that site (at least in half, never below one) and retries the affected URL later according to the retry rules from the fetching and errors feature.
4. **Given** a 429 or 503 response that includes a "Retry-After" wait time, **When** it arrives, **Then** no new request is sent to that site until the wait time has passed, and requests to other sites continue normally.
5. **Given** a site whose robots.txt sets a crawl-delay, **When** the crawl runs with default settings, **Then** that site receives at most one request at a time, spaced at least the crawl-delay apart, regardless of the preset.
6. **Given** a site that was slowed down and is now responding normally again, **When** responses stay steady, **Then** the crawler raises its simultaneous requests again gradually (one step at a time), not straight back to the previous level.

---

### User Story 3 - See how fast the crawl is going and why (Priority: P2)

While a crawl runs, the user sees a speed indicator: the current URLs per second, the average since the start, the number of requests in flight, how many URLs are waiting, and, when the crawl is slower than the user's caps would allow, a plain-language reason (for example "Slowed down: example.com is returning 'too many requests'", "Waiting 30 s as requested by example.com", "Limited by your cap of 5 URLs/second", "Limited by robots.txt crawl-delay"). The user can open a per-site breakdown showing each site's current speed state.

**Why this priority**: Users need to know whether a slow crawl is caused by their settings, by the site, or by their own computer, so they can act (raise a cap, contact the site owner, or accept it). Without it, adaptive slowdowns look like bugs.

**Independent Test**: Run crawls against the fixture scenarios from Stories 1 and 2 and check that the indicator shows the expected values and reasons, updated at least once per second.

**Acceptance Scenarios**:

1. **Given** a crawl in progress, **When** the user looks at the speed indicator, **Then** it shows current URLs per second, average URLs per second, requests in flight, URLs completed and URLs waiting, updated at least once per second.
2. **Given** the crawl is being held back by one of the user's caps, **When** the user looks at the indicator, **Then** it names which cap is limiting the speed.
3. **Given** a site has been slowed down or paused, **When** the user opens the per-site breakdown, **Then** the site is listed with its state (speeding up, steady, slowed down, waiting, paused), its current simultaneous request allowance, its recent average response time and the reason for its state, including a countdown when it is waiting.
4. **Given** the crawl is limited by the user's own computer (processing or saving results cannot keep up), **When** the user looks at the indicator, **Then** it says so in plain language.
5. **Given** a crawl run from the command-line interface, **When** it runs, **Then** it prints periodic progress with the same figures and reasons, and an end-of-crawl speed summary.

---

### User Story 4 - Very large crawls stay smooth (Priority: P2)

A user crawls a site with over a million URLs on an ordinary laptop. Memory use stays roughly flat from the first thousand URLs to the last, the results table remains responsive while the crawl runs at full speed, and slow disk writes or heavy use of the interface (scrolling, sorting, filtering, switching tabs) do not slow the crawl down or cause it to fail.

**Why this priority**: Large sites are where a desktop crawler earns its keep, and running out of memory or freezing hours into a crawl destroys trust. It is P2 because Stories 1 and 2 deliver value on small and medium sites first.

**Independent Test**: Crawl a 1,000,000-URL fixture site while an automated script continuously scrolls, sorts and filters the results table; record memory, interface response times and crawl throughput, and compare throughput with an identical crawl where the interface is idle.

**Acceptance Scenarios**:

1. **Given** a crawl growing from 100,000 to 1,000,000 URLs, **When** memory is measured at both points, **Then** memory at 1,000,000 URLs is no more than 20% higher than at 100,000 URLs.
2. **Given** a crawl running at full speed, **When** the user scrolls, sorts, filters or switches tabs, **Then** each interaction responds within about 100 ms.
3. **Given** a crawl running at full speed, **When** the user interacts heavily with the interface, **Then** crawl throughput drops by no more than 5% compared with an idle interface.
4. **Given** the interface window is minimised, hidden or frozen, **When** the crawl runs, **Then** the crawl continues at full speed and the interface catches up when it is shown again.
5. **Given** the disk is temporarily slow at saving results, **When** the crawl runs, **Then** the crawler slows its fetching to match rather than holding an ever-growing backlog in memory, no results are lost, and the speed indicator explains that saving is the limiting factor.

---

### User Story 5 - Repeatable performance benchmarks (Priority: P3)

The project owner and contributors run a benchmark suite against local fixture sites (never live third-party sites) that measures throughput, memory, interface responsiveness and politeness behaviour, and compares results with the targets in this spec and with previous runs, so performance regressions are caught before release.

**Why this priority**: The targets in this spec are only meaningful if they can be measured the same way every time. It is P3 because it supports, rather than delivers, user-facing value.

**Independent Test**: Run the benchmark suite twice on the same machine and confirm both runs produce a results report with all metrics, a pass/fail verdict per target, and throughput figures within 10% of each other.

**Acceptance Scenarios**:

1. **Given** the benchmark suite, **When** it runs, **Then** it produces a report listing each scenario, its measured throughput, peak and end memory, interface response times, the number of limit violations seen by the fixture site, and a pass/fail verdict against each target in this spec.
2. **Given** a change that reduces throughput by more than 10% against the stored baseline, **When** the reduced benchmark runs in automated checks, **Then** the check fails and names the regressed scenario.
3. **Given** the same machine and build, **When** the full benchmark is run five times, **Then** throughput results vary by no more than 10% between runs.

---

### Edge Cases

- **Many sites at once** (list mode with URLs on thousands of different sites, or external link checking): the global limits are shared fairly so that no site is starved, and no single site gets more than its per-site allowance.
- **Only one site**: the per-site allowance, not the global limit, is usually what bounds the speed; the indicator says so.
- **Per-site limit set higher than the global limit**: the global limit wins; settings show the effective value.
- **Invalid caps** (zero, negative, non-numeric, absurdly high): rejected with a clear message; the highest values the settings accept are documented.
- **Noisy response times**: normal small fluctuations do not cause the allowance to swing up and down; adjustments are based on recent typical response times, not single responses.
- **Very slow site** (responses close to the timeout): the allowance drops to one simultaneous request; if failures continue, the per-site circuit breaker from the fetching and errors feature pauses the site and the rest of the crawl continues.
- **Very long Retry-After or crawl-delay** (for example one hour, or a crawl-delay of 60 seconds): it is honoured; the site is shown as waiting with a countdown and an estimate of how long that site will take; other sites continue. If the only remaining work is for waiting sites, the crawl shows that it is waiting, not stuck.
- **Retry-After given as a date, in the past, or malformed**: a past date means no extra wait; a malformed value is ignored and normal backoff applies.
- **429 or 503 without Retry-After**: the allowance is cut and the URL is retried with the standard backoff.
- **Redirects to another site**: the request to the target site counts towards that site's allowance and caps, not the original site's.
- **Same server under several host names** (for example `www` and non-`www`, or many subdomains on one server): each host name is treated as a separate site for allowances; global caps still bound the total.
- **The user's own internet connection is the bottleneck**: response times rise across all sites at once; the crawler does not keep adding requests that only make things worse, and the indicator reports a general slowdown.
- **Computer goes to sleep or loses network mid-crawl**: on wake, sites restart from a cautious allowance rather than resuming at full speed against stale measurements.
- **Pause and resume, or reopen and resume a saved crawl**: speed settings come back as saved; per-site allowances start again from the cautious starting level.
- **Huge pages or pages with tens of thousands of links**: memory per page in flight is bounded by the size limit; discovered URLs are queued on disk, so a frontier of millions of URLs does not grow memory.
- **Slow or older computer**: page processing, not the network, may limit speed; the crawl still completes, memory stays flat, and the indicator names processing as the limit.
- **Disk full**: the crawl pauses safely as defined in the fetching and errors feature; no results that were already reported as saved are lost.
- **Command-line crawls with no interface at all**: the same limits and behaviour apply; there is no interface to slow anything down.

## Requirements *(mandatory)*

### Functional Requirements

**Speed settings and caps**

- **FR-001**: System MUST offer speed presets "Polite" (the default) and "Fast", plus "Custom", where the user sets each value directly.
- **FR-002**: Users MUST be able to set a global maximum number of simultaneous requests, a global maximum number of URLs per second (or no URL-per-second cap), and a maximum number of simultaneous requests per site.
- **FR-003**: System MUST never exceed any of the user's caps: the number of open requests MUST never be above the global or per-site maximum, and the number of requests started in any one-second window MUST never be above the URLs-per-second cap.
- **FR-004**: All requests the crawler makes MUST count towards the caps and per-site allowances, including retries, redirect hops, robots.txt fetches and external link status checks.
- **FR-005**: Users MUST be able to change speed settings while a crawl is running; changes MUST take effect within 2 seconds without pausing or restarting the crawl.
- **FR-006**: System MUST validate speed settings, reject invalid values with a clear message, and show the effective value when one limit is overridden by another (for example a per-site limit above the global limit).
- **FR-007**: The speed settings used MUST be saved with the crawl's configuration and restored when a saved crawl is reopened or resumed.
- **FR-008**: Every speed setting and preset MUST be available from the command-line interface with the same meaning and defaults as in the desktop app.

**Keeping the network busy**

- **FR-009**: System MUST start a new request as soon as a slot is free under all applicable limits and a URL is waiting, so the number of requests in flight stays close to the effective limit whenever work is available.
- **FR-010**: When URLs from several sites are waiting, System MUST share the global limits fairly across those sites, so a site with a large backlog does not prevent progress on others, while URLs of a site that is waiting or paused do not block URLs of other sites.

**Adapting to each site**

- **FR-011**: System MUST keep a separate simultaneous-request allowance for each site (identified by host name), starting at a cautious level and never above the per-site maximum or below one.
- **FR-012**: System MUST raise a site's allowance one step at a time while that site's recent response times stay steady and it returns no overload signals.
- **FR-013**: System MUST lower a site's allowance when that site's recent typical response time rises well above its own earlier typical level, or when requests to it time out.
- **FR-014**: On an HTTP 429 or 503 response, System MUST immediately cut that site's allowance at least in half (never below one) and retry the affected URL according to the retry rules of the fetching and errors feature.
- **FR-015**: When a 429 or 503 response includes a Retry-After value, System MUST send no new request to that site until the requested time has passed, while continuing with other sites.
- **FR-016**: After a slowdown, System MUST raise the allowance again only gradually, using the same step-by-step rule as FR-012.
- **FR-017**: By default, System MUST honour a robots.txt crawl-delay for the crawler's user agent by allowing at most one request at a time to that site, spaced at least the crawl-delay apart, under every preset. Ignoring crawl-delay MUST be a separate, explicit, clearly labelled user choice intended for sites the user controls.
- **FR-018**: Users MUST be able to turn adaptive adjustment off (for example on their own site), in which case each site uses the fixed per-site maximum; overload responses (429/503) and Retry-After MUST still be honoured.
- **FR-019**: System MUST treat a sudden slowdown across all sites at once as a sign of the user's own connection or computer being overloaded and stop adding requests globally, rather than reacting as if every site had a problem.
- **FR-020**: When a crawl resumes (after pause, reopen, sleep or loss of network), each site's allowance MUST restart from the cautious starting level.
- **FR-021**: System MUST record, in the crawl file, a summary of speed events per site (slowdowns, overload responses received, waits requested and their durations, pauses), so a completed crawl can explain why parts of it were slow.

**Resource use and isolation**

- **FR-022**: Memory use MUST stay flat as a crawl grows: waiting URLs, discovered URLs and results MUST be kept on disk rather than accumulating in memory, and the amount of page data held in memory at once MUST be bounded by the number of requests in flight and the page size limit.
- **FR-023**: Saving results MUST not hold up fetching under normal conditions. When saving falls behind, System MUST slow fetching to match (keeping the backlog of unsaved results bounded) rather than growing memory without limit, and MUST never drop results.
- **FR-024**: Unsaved results MUST be saved frequently enough that a crash loses at most the last few seconds of results, as required by the storage feature.
- **FR-025**: The crawl MUST continue at full speed regardless of what the interface is doing, including when the interface is busy, minimised, hidden or not responding; the interface MUST receive progress in a way that cannot block the crawl.
- **FR-026**: The interface MUST remain responsive during a crawl at full speed, responding to scrolling, sorting, filtering, tab switching and opening a URL's details within about 100 ms.

**Speed indicator**

- **FR-027**: System MUST show a speed indicator during a crawl with current URLs per second (averaged over the last few seconds), average URLs per second since the start, requests in flight, URLs completed and URLs waiting, updated at least once per second.
- **FR-028**: When the crawl is slower than the user's caps would allow, the speed indicator MUST name the main limiting factor in plain language: a user cap, the per-site limit, a site slowdown or overload response, a requested wait (with countdown), a robots.txt crawl-delay, the user's connection, processing on the user's computer, or saving results.
- **FR-029**: Users MUST be able to open a per-site breakdown listing, for each site with work in progress, its state (speeding up, steady, slowed down, waiting, paused), current allowance, recent average response time, URLs completed and waiting, and the reason for its state.
- **FR-030**: At the end of a crawl, System MUST show a speed summary: total duration, average URLs per second, peak URLs per second, and the sites that were slowed down, waited on or paused, with reasons.
- **FR-031**: The command-line interface MUST print periodic progress with the same figures and limiting-factor reasons, and the same end-of-crawl speed summary.

**Benchmarks**

- **FR-032**: The project MUST include local fixture sites for benchmarking that can serve at least 1,000,000 pages and simulate: instant responses, a fixed added delay, response times that rise with load, "too many requests" above a set number of simultaneous requests, "service unavailable" with Retry-After, and a robots.txt crawl-delay. Fixture sites MUST record every request with its start and end time so caps and politeness can be verified independently of the crawler.
- **FR-033**: The project MUST include a benchmark suite that runs the scenarios needed to verify every success criterion in this spec and produces a report with measured values and a pass/fail verdict per target.
- **FR-034**: A reduced version of the benchmark suite MUST run in automated checks and fail when throughput regresses by more than 10% against a stored baseline or when any cap or politeness rule is violated.
- **FR-035**: The full benchmark MUST be run on a defined reference machine, and its headline throughput target applies to that machine: a mid-range laptop no more than four years old, with 4 to 8 CPU cores, 16 GB of memory and a solid-state disk, on each supported operating system (the same reference machine is used by every SpiderLens spec).

### Key Entities *(include if feature involves data)*

- **Speed settings**: The preset (Polite, Fast or Custom), global maximum simultaneous requests, global maximum URLs per second, per-site maximum simultaneous requests, whether adaptive adjustment is on, and whether crawl-delay is honoured. Saved with each crawl's configuration.
- **Site speed state**: Per host name during a crawl: current simultaneous-request allowance, recent typical response time and earlier baseline, state (speeding up, steady, slowed down, waiting, paused), reason, and the time a requested wait ends. Not saved in detail; restarts cautiously on resume.
- **Speed event**: A recorded occurrence for a site, such as a slowdown, an overload response, a requested wait or a pause, with time, reason and duration. Summarised in the crawl file.
- **Throughput snapshot**: The figures shown by the speed indicator at a point in time: current and average URLs per second, requests in flight, URLs completed and waiting, and the main limiting factor.
- **Benchmark scenario**: A named fixture configuration (site size, response behaviour, speed settings) with the targets it must meet.
- **Benchmark result**: The measured values for one scenario run (throughput, memory, interface response times, limit violations), the build and machine it ran on, and its pass/fail verdict.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: On the reference machine, a crawl of the local 100,000-page fixture site with instant responses and the Fast preset sustains at least 500 URLs per second, averaged over the whole crawl excluding the first 10 seconds.
- **SC-002**: On a fixture site that adds 100 ms to every response, with a cap of 50 simultaneous requests and adaptive adjustment off, throughput is at least 90% of the theoretical maximum (at least 450 URLs per second), showing the network is kept busy.
- **SC-003**: Across all benchmark scenarios, the fixture sites' own logs show zero instances of the crawler exceeding the global simultaneous-request cap, the per-site cap, or the URLs-per-second cap in any one-second window.
- **SC-004**: On a fixture site that returns "too many requests" whenever more than 4 requests are open, the crawler settles at or below 4 simultaneous requests within 10 seconds of the first such response; after settling, fewer than 2% of its requests receive that response, and 100% of URLs end in a successful final state.
- **SC-005**: In 100% of cases, no request is sent to a site before a Retry-After wait it requested has passed, and no request to a site with a crawl-delay starts sooner than the crawl-delay after the previous one (with default settings).
- **SC-006**: When a fixture site's response times triple under load, the crawler reduces its simultaneous requests to that site within 10 seconds; on a steadily fast fixture site, it reaches the per-site maximum within 30 seconds.
- **SC-007**: Memory use at 1,000,000 URLs crawled is no more than 20% higher than at 100,000 URLs crawled in the same crawl.
- **SC-008**: During a full-speed crawl, 95% of interface interactions (scroll, sort, filter, tab switch, open details) respond within 100 ms, and continuous heavy interface use reduces crawl throughput by no more than 5% compared with an idle interface.
- **SC-009**: With saving results artificially slowed in a benchmark scenario, the crawl completes with 100% of results saved, memory stays within 20% of the same crawl on a normal disk, and the speed indicator names saving as the limiting factor.
- **SC-010**: The speed indicator updates at least once per second, and its current URLs-per-second figure is within 10% of the rate measured by the fixture site over the same period.
- **SC-011**: Speed-setting changes made during a crawl are reflected in the fixture site's request rate within 2 seconds in 100% of test runs.
- **SC-012**: Five runs of the full benchmark on the same machine and build produce throughput results within 10% of each other.
- **SC-013**: In usability checks, at least 9 out of 10 users looking at a slowed-down crawl can correctly say from the speed indicator alone whether the cause is their own settings, the site, or their computer.

## Assumptions

- **Scope**: This feature covers milestone M1 (core crawl) in Spider and List modes, including external link status checks. JavaScript rendering (M4), compare crawls and SERP mode (M3), AEO/GEO checks (M5) and AI agents (M6) are out of scope; their performance needs will be specified with those features.
- **Out of scope**: bandwidth caps (bytes per second), per-site URLs-per-second caps set by the user, scheduled or time-of-day speed rules, distributed or multi-machine crawling, proxy rotation, and sharing limits across several crawls running at the same time.
- **Preset defaults** (to be confirmed during planning, adjustable by the user): Polite allows up to 8 simultaneous requests overall, up to 4 per site, at most 5 URLs per second, with adaptive adjustment on. Fast allows up to 64 simultaneous requests overall, up to 32 per site, with no URLs-per-second cap and adaptive adjustment on. Both presets honour robots.txt, crawl-delay, Retry-After and 429/503 responses; Fast is labelled as intended for sites the user controls or has permission to crawl quickly.
- **Cautious starting level**: each site starts at 2 simultaneous requests (or 1 if the per-site maximum is 1) and adapts from there.
- A "site" for allowance purposes is a single host name. Grouping several host names that share one server is not done in M1; the global caps bound the total load.
- Retry counts, backoff timing, the per-site circuit breaker, error classes and the "disk full" pause are defined by the fetching and errors feature (004); this feature defines how those signals affect speed.
- Crash-safety guarantees and the crawl file format are defined by the storage feature; this feature only requires that saving does not slow the crawl and never drops results.
- The 1,000,000-row interface responsiveness requirement is shared with the results table feature; this feature adds the requirement that it holds while a crawl runs at full speed.
- Benchmarks run only against local fixture sites served from the same machine, never live third-party sites, so measured throughput reflects the crawler and not the internet.
- "About 100 ms" for interface interactions is measured from the user's input to the visible update.
- The speed indicator's "current" rate is a rolling average over the last 5 seconds.
