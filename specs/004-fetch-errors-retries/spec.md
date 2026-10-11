# Feature Specification: Fetching Pages, Errors and Retries

**Feature Branch**: `004-fetch-errors-retries`

**Created**: 2026-10-11

**Status**: Approved (2026-10-11)

**Input**: User description: "Fetching pages, errors and retries: each URL is requested with the configured user agent and headers; redirects are followed and every hop recorded; every failure is classified into a named error class; transient failures are retried with growing waits and Retry-After honoured; permanent failures are recorded once; a host that keeps failing is paused while the rest of the crawl continues; at the end the user can retry only URLs that failed for transient reasons; the crawl log and summary show retries and error counts."

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Every URL ends in one clear, named outcome (Priority: P1)

An SEO specialist crawls a site. Some pages load, some redirect, some return 404, some time out, and one subdomain has an expired certificate. When the crawl ends, every URL in the results shows exactly one final state: either a successful response (with its status code) or a named error class with a plain-language message and the number of attempts made. Nothing is silently missing, and the specialist can filter the Response codes tab by error class to see, for example, all timeouts.

**Why this priority**: Correct and complete data is a core principle of the product. Without reliable classification, every downstream report (broken links, redirects, audits) is untrustworthy. This is the minimum viable slice of the feature.

**Independent Test**: Crawl a local fixture site that serves one example of each error class (and several successful pages). Verify each URL ends in exactly one final state, with the expected error class, message and attempt count, and that the Response codes tab can be filtered by each class.

**Acceptance Scenarios**:

1. **Given** a fixture site with pages returning 200, 404, 410 and 500, **When** the crawl completes, **Then** each URL shows its status code, the 404 and 410 pages are classed as "HTTP 4xx", the 500 page as "HTTP 5xx", and the 200 pages as successful.
2. **Given** a link to a host name that does not exist, **When** that URL is fetched, **Then** it is recorded once with the class "DNS (permanent)" and a message naming the host, and it is not retried.
3. **Given** a host whose certificate is expired or does not match the host name, **When** a URL on it is fetched, **Then** it is recorded with the class "TLS invalid" and a message stating the reason (for example "certificate expired").
4. **Given** a page whose server never sends a response within the configured timeout, **When** it is fetched, **Then** it is recorded with the class "Timeout" and the message says whether the connect or the read timed out.
5. **Given** a page that is larger than the configured maximum download size, cannot be decompressed, or declares an encoding that cannot be decoded, **When** it is fetched, **Then** it is recorded with the class "Content problem" and a message naming the specific problem.
6. **Given** a discovered link that cannot be parsed as a URL, **When** it is processed, **Then** it is recorded with the class "Malformed URL", with the original link text and the page it was found on, and no request is made.
7. **Given** a URL excluded by robots.txt or by the crawl configuration, **When** it is reached, **Then** it is recorded as "Blocked" with the reason (robots.txt rule or which configuration rule), and it is not counted as an error.
8. **Given** any completed crawl, **When** the user counts URLs by final state, **Then** the totals of all final states equal the total number of discovered URLs.

---

### User Story 2 - Transient failures are retried politely and automatically (Priority: P1)

During a crawl, a busy site occasionally answers 503, sometimes resets connections and once replies 429 with a Retry-After header of 30 seconds. The specialist does not want these momentary hiccups to show up as broken pages, and does not want the crawler to hammer the site. SpiderLens retries these URLs automatically, waiting a little longer each time, honouring Retry-After, and placing retries at the back of the queue so the rest of the crawl keeps moving. If a URL eventually succeeds, the result shows the success and notes how many attempts it took.

**Why this priority**: Without retries, real crawls of real sites produce false "broken page" reports, which destroys trust in the results. Honouring Retry-After is also required by the respectful-crawling principle.

**Independent Test**: Use a fixture server that fails a given URL a set number of times with each transient error type, then succeeds. Verify the number of attempts, the waits between them, Retry-After handling, and that other URLs continue to be fetched meanwhile.

**Acceptance Scenarios**:

1. **Given** default settings and a URL that returns 503 twice then 200, **When** it is crawled, **Then** the final state is 200 with an attempt count of 3, and the waits before the second and third attempts were roughly 1 second and 2 seconds (within the allowed random variation).
2. **Given** a URL that returns 500 on every attempt, **When** it is crawled with default settings, **Then** it is attempted exactly 3 times in total and its final state is "HTTP 5xx" with attempt count 3 and the last error message.
3. **Given** a URL that returns 429 with "Retry-After: 30", **When** it is retried, **Then** the next attempt to that URL happens no sooner than 30 seconds later, and no other request is sent to that host before then.
4. **Given** a Retry-After value larger than the maximum retry wait (default 60 seconds), **When** the URL is scheduled for retry, **Then** the wait is the maximum retry wait, and the log records that the server asked for a longer wait.
5. **Given** a URL that fails with a connection reset, a DNS temporary failure, a timeout, or a truncated body, **When** it is crawled, **Then** it is retried in the same way as a 5xx response.
6. **Given** a URL waiting to be retried, **When** other URLs are queued, **Then** those other URLs are fetched while the retry is waiting, and the retry is placed at the back of the queue once its wait is over.
7. **Given** a URL that fails with a permanent class (DNS permanent, TLS invalid, HTTP 4xx other than 429, malformed URL, redirect problem, or a content problem other than a truncated body), **When** it fails, **Then** it is recorded once with attempt count 1 and is never retried automatically.

---

### User Story 3 - Redirects are followed and every hop is visible (Priority: P2)

A site migration has left chains of redirects (http to https, then to a trailing slash, then to a new path). The specialist needs to see the full chain for each starting URL, including each hop's status code and target, to find chains to shorten and loops to fix.

**Why this priority**: Redirect auditing is one of the most common reasons SEO specialists run a crawler. It depends on correct fetching (Story 1) but is valuable on its own.

**Independent Test**: Crawl a fixture site with single redirects, multi-hop chains, a chain longer than the limit, a loop, and a redirect to an external host. Verify recorded hops, final states and classes.

**Acceptance Scenarios**:

1. **Given** URL A redirects with 301 to B, which redirects with 302 to C, which returns 200, **When** A is crawled, **Then** A's record lists both hops in order (status code and target of each), the final destination C, and the number of hops (2).
2. **Given** a redirect chain of 11 hops with the default limit of 10, **When** the start URL is crawled, **Then** following stops at the limit, the start URL's final state is "Redirect problem: chain over limit", and the hops followed so far are recorded.
3. **Given** URL A redirects to B and B redirects back to A, **When** A is crawled, **Then** the loop is detected the first time a URL repeats, the final state is "Redirect problem: loop", and the hops are recorded.
4. **Given** a redirect whose target is excluded by robots.txt or configuration, **When** it is followed, **Then** the chain stops at that hop and the target is recorded as "Blocked" with its reason.
5. **Given** a redirect to a URL that has already been crawled, **When** the redirect is followed, **Then** the destination is not fetched a second time, and the redirecting URL's record links to the existing destination record.
6. **Given** a redirect response with a missing or unparsable target, **When** it is received, **Then** the URL is recorded as "Redirect problem: invalid target" with the raw value shown.

---

### User Story 4 - A failing host is paused without stopping the crawl (Priority: P2)

The specialist crawls a site that links to a media subdomain which goes down halfway through the crawl. Instead of the whole crawl stalling while every URL on that subdomain times out three times, SpiderLens notices that the host keeps failing, pauses requests to that host for a while, tells the user, and keeps crawling the other hosts. After the pause it sends a single test request; if the host has recovered, crawling of it resumes.

**Why this priority**: Prevents one bad host from wasting the user's time and from adding load to a struggling server. Important for multi-host crawls, but single-host crawls still work without it.

**Independent Test**: Use two fixture hosts; make one start refusing connections mid-crawl and later recover. Verify the failing host is paused, the other host continues at normal speed, the pause is visible, and the failing host resumes after recovery.

**Acceptance Scenarios**:

1. **Given** default settings, **When** 5 requests in a row to the same host fail with a transient error, **Then** that host is paused for 60 seconds, a notice appears in the crawl log and in the crawl status view naming the host, and requests to other hosts continue.
2. **Given** a paused host, **When** its pause ends, **Then** exactly one test request is sent to it; if it succeeds, normal crawling of that host resumes; if it fails, the host is paused again for twice as long, up to a maximum pause of 10 minutes.
3. **Given** a host that has been paused 5 times in a row without recovering, **When** the fifth test request fails, **Then** the remaining queued URLs for that host are recorded with the class "Host unavailable" (a transient class), the crawl continues with other hosts, and the log explains why.
4. **Given** a single-host crawl whose only host is paused, **When** the pause is in effect, **Then** the crawl status shows "Waiting for host" with a countdown rather than appearing frozen, and the user can still pause or stop the crawl.
5. **Given** a paused host, **When** the user chooses "Resume host now", **Then** the pause ends immediately and a test request is sent.
6. **Given** URLs in progress when a host is paused, **When** the pause starts, **Then** those URLs are returned to the queue without using up an extra attempt.

---

### User Story 5 - Retry only the transient failures at the end (Priority: P3)

After a long crawl, the summary shows 340 errors: 300 are 404s and 40 are timeouts on a server that was briefly overloaded. The specialist clicks "Retry failed URLs". Only the 40 transient failures are re-queued and fetched again; the 404s are left alone. The results update in place, and the summary reflects the new outcomes.

**Why this priority**: Saves the user from re-running a whole crawl to clean up temporary failures. Useful but only after the core fetch and retry behaviour exists.

**Independent Test**: Complete a crawl on a fixture site that produces both transient and permanent failures, then make the transient pages healthy and use the retry action. Verify only transient failures are re-fetched and their records are updated.

**Acceptance Scenarios**:

1. **Given** a finished (or paused) crawl with transient and permanent failures, **When** the user chooses "Retry failed URLs", **Then** the action states how many URLs will be retried, broken down by error class, before starting.
2. **Given** the user confirms the retry, **When** it runs, **Then** only URLs whose final state is a transient class (DNS temporary, connection, timeout, HTTP 429, HTTP 5xx, truncated body, host unavailable) are fetched again, each with a fresh set of attempts.
3. **Given** a retried URL that now succeeds, **When** the retry completes, **Then** its record shows the new result, its total attempt count across the crawl, and that it was recovered by a manual retry; links found on it are processed as normal.
4. **Given** a crawl with no transient failures, **When** the user opens the actions, **Then** "Retry failed URLs" is unavailable and explains that there is nothing to retry.
5. **Given** the command-line crawler, **When** the user runs the retry action on a saved crawl file, **Then** the same URLs are retried with the same results as in the desktop app.

---

### User Story 6 - Requests identify the crawler honestly and use the configured headers (Priority: P3)

The specialist sets a custom user agent and adds an extra header (for example a staging access token) in the crawl configuration. Every request carries those values. By default, requests use an honest SpiderLens user agent.

**Why this priority**: Needed for crawling staging sites and sites that vary content by user agent, and required by the respectful-crawling principle, but a default user agent is enough for first use.

**Independent Test**: Crawl a fixture server that records request headers, first with defaults and then with a custom user agent and header. Verify every request (including redirects, retries and robots.txt) carries the expected values.

**Acceptance Scenarios**:

1. **Given** default settings, **When** any request is sent, **Then** its user agent identifies SpiderLens, its version and a way to learn about the crawler.
2. **Given** a custom user agent and one or more custom headers in the configuration, **When** any request is sent, including redirect hops, retries and the robots.txt request, **Then** those values are sent on every request.
3. **Given** a custom header whose value is marked as secret, **When** the crawl configuration is saved in the crawl file or shown in the log, **Then** the value is masked in the log and is not written to the crawl file at all (only a placeholder is kept, per spec 006).
4. **Given** a custom user agent, **When** robots.txt rules are evaluated, **Then** the rules for the configured user agent are applied (as defined in the scope specification).

---

### Edge Cases

- A server returns 200 but closes the connection before the full body arrives: classed as "Content problem: truncated", which is transient and retried.
- A server sends a body larger than the maximum download size: download stops at the limit, class "Content problem: over size limit", not retried.
- A Retry-After header is given as a date rather than seconds, is in the past, or cannot be parsed: a date is converted to a wait; a past date means no extra wait; an unparsable value is ignored and normal backoff applies (logged).
- A 503 response carries Retry-After: the same Retry-After rules apply as for 429.
- 429 responses also lower the request rate for that host (as defined in the speed specification); retries do not bypass the host's rate limit or crawl-delay.
- A redirect points from https to http, or to another host: the hop is recorded as normal; scope rules decide whether the destination is fetched.
- A redirect hop itself fails (for example the target times out): the start URL's final state reflects the failing hop's error class, and the hops before it are recorded.
- A URL redirects to itself with a different case or trailing slash that normalises to the same URL: treated as a loop.
- The user's own network drops for everyone (every host fails at once): this is detected as a local network problem rather than pausing each host one by one; the crawl pauses safely and tells the user, and resumes when the user chooses to.
- Local problems such as a full disk: the crawl pauses safely, no URL is marked as failed because of it, and the user is told what to fix.
- The user pauses or quits during a retry wait, or the app crashes: on resume, URLs waiting for retry keep their attempt counts and are re-queued; none are lost or double-counted.
- A URL is discovered again while it waits for a retry: it is not added a second time.
- Status codes outside the usual ranges (for example 999 or 0): recorded with the raw code, classed as "HTTP other", not retried.
- A 401 or 403 response: classed as "HTTP 4xx" (permanent) and not retried, even though some sites use 403 for temporary bot protection.
- Manual retry is started while the crawl is still running: it is not available until the crawl is paused or finished.
- The maximum attempts setting is set to 1: no automatic retries happen; transient failures are still marked transient so the manual retry still works.

## Requirements *(mandatory)*

### Functional Requirements

**Requests**

- **FR-001**: System MUST send every request (pages, redirect hops, retries, robots.txt) with the configured user agent and the configured custom headers.
- **FR-002**: The default user agent MUST honestly identify SpiderLens, its version and a way to learn about the crawler; changing it MUST be an explicit, clearly labelled user action.
- **FR-003**: Users MUST be able to add, edit and remove custom request headers in the crawl configuration, and mark a header value as secret so that it is masked wherever it is displayed or logged and is never written to the crawl file (spec 006, FR-034).
- **FR-004**: System MUST apply configurable connect and read timeouts (defaults: 10 seconds connect, 30 seconds read) and a configurable maximum download size per response (default: 50 MB).
- **FR-005**: System MUST accept compressed responses and decompress them, recording both the transferred size and the uncompressed size.

**Redirects**

- **FR-006**: System MUST follow redirect responses (301, 302, 303, 307, 308 and other 3xx responses with a target) up to a configurable hop limit (default 10).
- **FR-007**: System MUST record every hop of a redirect chain in order, with each hop's URL, status code, target and response time, and record the final destination and number of hops on the starting URL.
- **FR-008**: System MUST detect redirect loops the first time a URL repeats in a chain (after normalisation) and record the class "Redirect problem: loop".
- **FR-009**: System MUST record "Redirect problem: chain over limit" when the hop limit is reached, and "Redirect problem: invalid target" when a redirect has a missing or unparsable target.
- **FR-010**: System MUST apply scope and robots rules to each redirect target before fetching it, and MUST NOT fetch a destination that has already been fetched in the same crawl; the redirecting record links to the existing destination record instead.

**Error classification**

- **FR-011**: System MUST give every discovered URL exactly one final state: successful response, blocked (not an error), or one named error class.
- **FR-012**: System MUST classify failures into these named classes, each marked transient or permanent:
  - DNS (permanent): host name does not exist. Permanent.
  - DNS temporary: name lookup failed temporarily. Transient.
  - Connection: connection refused or reset. Transient.
  - Timeout: connect or read timeout (which one is recorded). Transient.
  - TLS invalid: certificate or secure-connection problem (reason recorded). Permanent.
  - HTTP 429: too many requests. Transient.
  - HTTP 5xx: server error. Transient, except 501 Not Implemented and 505 HTTP Version Not Supported, which are permanent.
  - HTTP 4xx: client error other than 429. Permanent.
  - HTTP other: status code outside the standard ranges. Permanent.
  - Redirect problem: loop, chain over limit, or invalid target. Permanent.
  - Content problem: encoding, decompression, over size limit (permanent), or truncated body (transient).
  - Malformed URL: the link cannot be parsed into a requestable URL. Permanent; no request is made.
  - Host unavailable: the host was given up on after repeated pauses. Transient.
- **FR-013**: For every failed URL, System MUST record the error class, the sub-reason where one exists, a plain-language message, the number of attempts, and the time of the last attempt.
- **FR-014**: URLs blocked by robots.txt or crawl configuration MUST be recorded as "Blocked" with the specific rule, and MUST NOT be counted as errors.
- **FR-015**: Local problems on the user's machine (for example a full disk, or loss of the user's own network connection) MUST NOT be recorded as failures of the URLs being fetched; instead the crawl MUST pause safely, tell the user the cause, and allow resuming without losing results.

**Automatic retries**

- **FR-016**: System MUST retry automatically only URLs that fail with a transient class, up to a configurable maximum number of attempts in total (default 3, minimum 1, maximum 10).
- **FR-017**: The wait before each automatic retry MUST grow exponentially (default about 1 second, then 2 seconds, then 4 seconds, and so on), with random variation of up to plus or minus 20%, capped at a configurable maximum wait (default 60 seconds).
- **FR-018**: When a 429 or 503 response includes Retry-After (in seconds or as a date), System MUST wait at least that long before the next request to the same host, up to the maximum wait; when the requested wait exceeds the maximum, System MUST use the maximum and record that in the log.
- **FR-019**: A URL waiting for a retry MUST NOT block other URLs; once its wait is over it MUST be placed at the back of the queue.
- **FR-020**: Retries MUST respect the same per-host rate limits, crawl-delay and concurrency limits as first attempts.
- **FR-021**: URLs failing with a permanent class MUST be recorded once, with attempt count 1, and never retried automatically.
- **FR-022**: When a URL succeeds after one or more retries, its record MUST show the success and the total number of attempts.
- **FR-023**: Pending retries and attempt counts MUST survive pausing, saving, quitting and reopening a crawl, and recovery after a crash, without losing or duplicating URLs.

**Host pausing (circuit breaker)**

- **FR-024**: System MUST pause a host when a configurable number of consecutive requests to it fail with a transient class (default 5), for an initial pause (default 60 seconds), while other hosts continue to be crawled.
- **FR-025**: After a pause ends, System MUST send a single test request to the host; on success crawling resumes normally; on failure the pause repeats with double the length, up to a maximum pause (default 10 minutes).
- **FR-026**: After a configurable number of consecutive failed pauses (default 5), System MUST record the host's remaining queued URLs as "Host unavailable" and continue the crawl.
- **FR-027**: URLs that were in progress or queued for a host when it is paused MUST be returned to the queue without using up an attempt.
- **FR-028**: System MUST show currently paused hosts, with reason and time remaining, in the crawl status view and the crawl log, and MUST let the user resume a paused host immediately.
- **FR-029**: When failures happen across all hosts at the same time, System MUST treat this as a possible local network problem (per FR-015) rather than pausing each host individually.

**Manual retry of failed URLs**

- **FR-030**: Users MUST be able to choose "Retry failed URLs" on a finished or paused crawl; the action MUST re-queue only URLs whose final state is a transient class.
- **FR-031**: Before starting, the action MUST show how many URLs will be retried, broken down by error class, and let the user cancel.
- **FR-032**: Each re-queued URL MUST get a fresh set of automatic attempts; its record MUST keep its total attempt count across the whole crawl and show that it was retried manually.
- **FR-033**: Results of a manual retry MUST update the URL's existing record in place (no duplicates), and links found on newly successful pages MUST be processed as in a normal crawl.
- **FR-034**: "Retry failed URLs" MUST be unavailable, with an explanation, when the crawl is running or has no transient failures.

**Log, summary and visibility**

- **FR-035**: The crawl log MUST record each failure, each retry (URL, attempt number, error class, wait chosen and whether Retry-After applied), each host pause and resume, and each local-problem pause.
- **FR-036**: The crawl summary MUST show counts of URLs by final state and by error class, the number of automatic retries made, the number of URLs recovered by retry, the number of hosts paused, and whether a manual retry was run.
- **FR-037**: Users MUST be able to filter the results by error class and by "transient" versus "permanent", and the detail pane for a URL MUST show its error class, message, attempt count and redirect hops.
- **FR-038**: All settings in this specification (timeouts, maximum size, redirect limit, maximum attempts, waits, host pause settings, user agent, headers) MUST be stored with the crawl so its results can be explained and reproduced.
- **FR-039**: Everything in this specification, including the manual retry and the summary counts, MUST also be available from the command-line crawler, with the same behaviour as the desktop app.

### Key Entities

- **Fetch attempt**: One try at requesting a URL. Has attempt number, start time, duration, outcome (status code or error class with sub-reason and message), and the wait chosen before the next attempt, if any.
- **Redirect hop**: One step in a redirect chain. Has position in the chain, URL, status code, target and response time. Belongs to the starting URL's record.
- **URL final state**: The one outcome a URL ends with: successful response, blocked (with rule), or error class (with transient/permanent flag, message, total attempts, and whether a manual retry recovered it).
- **Error class**: A named category of failure with a fixed transient or permanent flag and optional sub-reasons (for example Timeout: connect or read).
- **Host health**: Per host: consecutive failure count, paused or active, pause count, pause end time, and whether it has been given up (host unavailable).
- **Fetch settings**: User agent, custom headers (with secret flag), timeouts, maximum download size, redirect hop limit, maximum attempts, base and maximum retry wait, and host pause thresholds. Saved with the crawl.
- **Crawl log entry**: A timestamped event (failure, retry, host paused/resumed, local problem, manual retry started/finished) with the URL or host it concerns.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: On the error-classes fixture site, 100% of URLs end in exactly one final state, and 100% of failures carry the expected error class, a message and an attempt count.
- **SC-002**: On a fixture where pages fail transiently up to 2 times before succeeding, at least 99% of those pages end as successful with default settings, and none are reported as errors.
- **SC-003**: No URL with a permanent failure class is ever requested more than once during automatic crawling (0 extra requests measured by the fixture server).
- **SC-004**: When a server sends Retry-After, 100% of following requests to that host respect the requested wait (up to the maximum wait), as measured by the fixture server.
- **SC-005**: When one of two hosts goes down mid-crawl, the other host's crawl rate stays within 10% of its rate before the failure, and the failing host receives no more than 5 failed requests before it is paused.
- **SC-006**: The manual retry re-fetches 100% of transient failures and 0% of permanent failures.
- **SC-007**: Redirect chains on the fixture site are recorded with 100% of hops in the correct order, and every loop and over-limit chain is identified as such.
- **SC-008**: Summary error counts match the per-URL results exactly (0 discrepancies) in both the desktop app and the command-line crawler.
- **SC-009**: After a crash or quit during a crawl with pending retries, reopening and resuming loses no pending retries and double-counts no attempts.
- **SC-010**: A user can find all URLs of a given error class, and see why each failed, in under 30 seconds from the end of the crawl.
- **SC-011**: The added fetch, retry and host-pausing behaviour does not reduce throughput below the project target of 500 URLs per second on the local 100,000-page fixture site when no failures occur.

## Assumptions

- This feature belongs to milestone M1 (core crawl). It builds on the crawl frontier and scope rules (URL normalisation, robots.txt, include/exclude rules), the speed controls (adaptive per-host concurrency, rate limits, crawl-delay, presets), page extraction, storage, and the results table, each defined in its own specification; this spec only defines how those interact with fetching and errors.
- Default values follow the product decisions already made: 3 attempts in total, waits of about 1 s, 2 s and 4 s with random variation, a 60-second maximum wait, a 10-hop redirect limit, and retries placed at the back of the queue.
- Defaults chosen here where none were given: 10 s connect timeout, 30 s read timeout, 50 MB maximum download size, ±20% wait variation, host paused after 5 consecutive transient failures, 60 s initial pause doubling to a 10-minute maximum, and the host given up after 5 consecutive failed pauses. All are configurable.
- 401 and 403 responses are treated as permanent ("HTTP 4xx") and not retried; users can retry them by re-crawling. 501 and 505 are treated as permanent because retrying cannot help.
- A truncated body counts as a content problem but is transient, matching the product decision list.
- Only GET requests are used for crawling pages; HEAD-only checking of external links, if any, is defined in the scope specification.
- Custom headers apply to all hosts in the crawl; per-host header rules are out of scope for M1.
- Out of scope: logins and forms-based authentication, cookie settings, proxies, JavaScript rendering (M4), retrying URLs across separate crawls or comparing crawls (M3), scheduled crawls, and exports of error reports (M2).
