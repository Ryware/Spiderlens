# Implementation Plan: Fetching Pages, Errors and Retries

**Branch**: `004-errors-retries` | **Date**: 2026-10-13 | **Spec**: [spec.md](./spec.md)

## Summary

Today a crawl makes exactly one request per URL. Whatever comes back is
final: a 503 during a short overload, a reset connection or a 429 is stored
as the page's result, and a host that goes down keeps receiving requests
until its queue is empty. The engine already classifies transport failures
into eight classes (spec 001), adapts per-host speed and honours
`Retry-After` as a pause for the host (spec 008), and the results crate
already detects redirect loops and over-limit chains at read time
(spec 007).

This feature adds what is missing between those pieces:

1. **Retry policy** (US2): transient failures wait with exponential backoff
   and jitter (or `Retry-After`), outside the queue, then rejoin it at the
   back, up to the configured attempts. Pure decision function, no I/O.
2. **Host circuit breaker** (US4): a host with 5 transient failures in a row
   is paused, probed with one request, paused again with doubling waits, and
   given up after 5 failed probes. Other hosts keep their full speed.
3. **Complete classification** (US1, US3): sub-reasons (connect or read
   timeout, truncated or over-size body, redirect loop, over-limit or invalid
   target), attempts per URL, transient or permanent per class, HTTP classes
   derived from the status code.
4. **Local problems** (FR-015, FR-029): a dead network or a full disk pauses
   the crawl instead of failing thousands of URLs.
5. **Manual retry** (US5): re-queue only transient failures of a paused or
   finished crawl, in the app and in the CLI.
6. **Visibility** (FR-035 to FR-037): a crawl log table, summary counts,
   paused hosts in the status bar with "Resume now", error filters and the
   attempts column in the results table.
7. **Left over from spec 008**: speed and retry settings changed while a
   crawl runs apply to it at once (the stored configuration already records
   such changes; nothing could make them).

Crawl speed is the owner's top priority, so the guiding constraint is: **a
healthy crawl pays nothing**. Everything new runs only when a request
fails, or is an in-memory counter update per response.

## Technical Context

- Rust 2024, Tokio multi-thread runtime, reqwest 0.12 (manual redirects),
  SQLite via rusqlite in WAL mode with one write transaction committed every
  200 ms (spec 008 D2).
- The crawl loop is an actor: one coordinator task owns the crawl file and
  every scheduling decision (`crawl/runner.rs`); fetch tasks report back over
  a channel. Retry and host-health state therefore need no locks.
- The queue is the `urls` table ordered by `(depth, queue_seq)` with an
  in-memory window of 4,096 rows (`crawl/window.rs`).
- Redirects are not followed inside one request in a crawl: each hop is its
  own URL row linked by `redirect_to` (spec 003 R8). Chains, loops and
  over-limit chains are computed at read time by `spiderlens-results`
  (`temp.redirects`).
- New dependency: `fastrand` (jitter). Already in the lock file through
  `tempfile`; 0 new crates downloaded. Justified in D3.
- Reference machine: 4-core cloud container, release build, bench site on
  the same machine. Baseline on main `f3a4d3d` measured before any change
  (T001).

## Constitution Check

| Principle | How this plan complies |
|---|---|
| I. Spec before code | Spec approved 2026-10-11; this plan and tasks go to the owner in their own docs PR. |
| II. Local-first | No new network use beyond the retries the spec asks for. |
| III. Engine independent of shell | Policy, breaker, log and manual retry live in `spiderlens-engine`; the CLI gets `spiderlens retry` and the same summary counts (FR-039). |
| IV. Respectful crawling | Retries obey per-host limits, crawl-delay and `Retry-After`; a failing host is paused, not hammered (FR-020, FR-024). |
| V. Correct and complete data | Every URL ends in exactly one final state with class, sub-reason, message and attempt count (FR-011, FR-013); a URL is never failed because of a local problem (FR-015). |
| VI. Scales by design | Waiting retries live on disk (`state = 'waiting'`) with an in-memory timer heap of ids only; host health is a few integers per host. Performance measured before and after (D10). |
| VII. Test-first | A fixture server with scripted failures maps every acceptance scenario to a test (tasks.md); UI behaviour is covered by Vitest tests. |
| VIII. Simplicity | One tiny dependency; no new crate; redirect chain detection reuses spec 007's read-time logic instead of a second copy in the engine. |

No violations.

## Design decisions

### D1. Retry decision is a pure function

`crawl/retry.rs` holds one function that takes the outcome of an attempt
(status code or error class and sub-reason, `Retry-After`), the attempt
number within the current round and the retry settings, and returns either
`Record` or `Retry { wait, retry_after_used, capped }`. No clock, no I/O,
no randomness inside (the jitter factor is passed in), so every row of the
spec's classification table and every wait in US2 is a unit test.

*Analogy*: a reducer in Redux, or a pure domain service in DDD: the runner
is the impure shell that feeds it facts and carries out its decision.

### D2. Classification: transport classes stored, HTTP classes derived

`urls.error_class` keeps meaning "no usable response" (DNS, connection,
timeout, TLS, content, redirect problem, malformed URL, host unavailable).
A 404 or a final 503 keeps `error_class = NULL` and its status code; its
class ("HTTP 4xx", "HTTP 5xx", "HTTP 429", "HTTP other") and whether it is
transient are derived from the status by one function used by the engine
(retry decisions, summary) and by one SQL function in the results crate
(filters, columns).

*Why*: every existing filter in the results table ("Client error (4xx)",
"No response", ...) already relies on `error_class IS NULL` for HTTP
answers, and the status code is the more precise fact. Storing a second
copy of it as a class would let the two disagree.

*Alternative rejected*: one `final_class` column with everything. Simpler
to query, but duplicates the status and breaks spec 007's filters.

`ErrorClass` (Rust enum) gains `RedirectProblem` (replacing `RedirectLoop`
in new files; old files keep reading `redirect_loop`) and `HostUnavailable`,
and `CheckError` gains an optional sub-reason (`connect`, `read`,
`truncated`, `over_size`, `decompression`, `loop`, `over_limit`,
`invalid_target`). The enum is closed, so every `match` over it must handle
the new classes: the compiler finds every place to update.

*Analogy*: a sealed class hierarchy in Kotlin or a discriminated union in
TypeScript with `never` exhaustiveness checks.

### D3. Backoff with full control over jitter

Wait before retry *n* (n = 1 for the second attempt) is
`base * 2^(n-1) * (1 + j)` with `j` uniform in [-0.2, 0.2], capped at the
maximum wait (FR-017). Defaults: base 1 s, max 60 s. When a 429 or 503
carries `Retry-After`, the wait is `max(backoff, retry_after)`, capped at
the maximum wait; when the server asked for more than the cap, the log says
so (FR-018). The same capped value is applied as the host's pause, so no
other request goes to that host before the retry (US2.3). Spec 008 capped
host pauses at 24 h; this spec's maximum wait replaces that cap.

`fastrand` gives the jitter: a non-cryptographic generator, seeded per
crawl, which is all jitter needs. `rand` would pull in several crates for
the same thing.

### D4. Waiting retries live on disk, timers in memory

A URL that will be retried moves to a new state, `waiting`, with
`retry_at` (epoch milliseconds) stored on its row, and its id goes into an
in-memory min-heap ordered by due time. When it is due, the coordinator
gives it a new `queue_seq` (the back of its depth level), sets it `queued`
and hands it to the queue window. So:

- waiting URLs never block the queue (FR-019): they are not in it;
- a pause, quit or crash keeps them (FR-023): on resume, `waiting` rows are
  loaded back into the heap with their remaining wait; a stop turns them
  into "Crawl stopped" like other queued rows, and resume brings them back
  with their attempt counts;
- memory is one `(Instant, i64)` per waiting URL, bounded in practice by
  the circuit breaker.

The intermediate failed attempt is not recorded as the URL's result: its
links and page data are dropped, so a 503 page that later succeeds does not
leave duplicate links (and a waiting URL shows "Waiting to retry", not a
failure). Only the final attempt is recorded, with the total attempt count.

*Alternative rejected*: keep waiting URLs in the window with a "not before"
time. The window is ordered by queue position per host, so a waiting head
would block every URL behind it on the same host.

*Analogy*: a delayed-message queue (SQS visibility timeout, a Sidekiq retry
set): the message leaves the main queue, sits in a scheduled set with a due
time, and is re-enqueued when due.

### D5. Attempts: total and per round

`urls.attempts` counts every request made for the URL during the crawl
(FR-022, FR-032). `urls.round_attempts` counts the requests in the current
round; a manual retry resets it to 0 so each re-queued URL gets a fresh set
of automatic attempts while the total keeps growing. Requests returned to
the queue because their host was paused, or because the crawl paused for a
local problem, do not count (FR-027).

### D6. Host circuit breaker, per host, in memory

`crawl/health.rs` is a small state machine per host:

```
Active --(5 transient failures in a row)--> Paused(60 s)
Paused --(time up)--> Probing (exactly 1 request allowed)
Probing --success--> Active (counters reset)
Probing --failure--> Paused(min(2x, 10 min)), or GivenUp after 5 failed probes
Paused --("Resume now")--> Probing
```

Only transient failures count; a 404 or a TLS error is the page's problem,
not the host's. A response with any status counts as the host being alive.
While paused, the host is skipped by the dispatcher in one step (like a
crawl-delay wait), and results of requests that were already in flight are
returned to the queue without using an attempt (US4.6). A given-up host's
remaining URLs, and any found later, are recorded as "Host unavailable"
(transient, so the manual retry picks them up) without a request.

Each transition writes one crawl log row and is shown in the status bar with
a countdown (FR-028). The state is not persisted: after a resume every host
starts healthy, which is the right default after a pause that may have
lasted hours.

*Analogy*: Hystrix / resilience4j circuit breaker (closed, open,
half-open), with exponential back-off on re-opening.

### D7. Local network problem detection (FR-015, FR-029)

A global counter of consecutive network failures (connection, DNS
temporary, connect timeout) with no successful response in between, across
any hosts. When it reaches 10 failures spread over at least 2 hosts, the
crawl pauses itself: failed requests go back to the queue without using an
attempt, the status becomes `paused` with `pause_reason = network`, and the
user is told. Resuming starts fresh. A single-host crawl cannot tell its
host from the network; there the host breaker applies and the status shows
"Waiting for host" (US4.4).

A full disk (`SQLITE_FULL`) or a read-only file ends the run with a
`LocalProblem` error: no URL is marked failed, in-flight rows stay
`in_flight` on disk and go back to the queue on resume (that path already
exists for crashes).

### D8. Redirects: stop at the limit, classify invalid targets, reuse 007 for chains

- Invalid target (missing scheme host, unparsable `Location`, unsupported
  scheme): the redirecting URL gets `redirect_problem / invalid_target` with
  the raw value in the message (US3.6).
- Over limit: when a redirect response is handled, the engine counts the
  redirect hops that led to it (walking `source_id` back through rows
  discovered by redirect; only for 3xx responses, at most limit + 1 reads).
  At the limit, the target is not queued; it is recorded with the new
  not-crawled reason "Redirect chain over limit" (US3.2).
- Loops and chain labels: computed at read time by spec 007's
  `temp.redirects`, which already finds loops on first repeat and chains
  over the limit, and shows hops in the detail pane. A loop cannot make the
  crawler spin because every URL is fetched once.

*Why not label chains in the engine*: hops of a chain are fetched out of
order and in parallel; the chain is only known when all of them are done,
and the read side already has the logic and its tests.

### D9. Crawl log table and summary

New table `crawl_log(id, at, kind, host_key, url_id, message)` with kinds
`retry`, `failure`, `retry_after_capped`, `host_paused`, `host_resumed`,
`host_given_up`, `local_problem`, `manual_retry`. Written inside the crawl's
batched transaction, so it costs nothing extra on a healthy crawl (no rows).

The summary gains: URLs by final class (HTTP classes derived), transient and
permanent error totals, automatic retries made, URLs recovered by retry,
hosts paused, and the manual retry runs (FR-036). `failed_by_class` stays
for compatibility.

### D10. Speed: a healthy crawl pays nothing

Per response the new work is: one comparison to classify the outcome, one
counter reset in the host's health, and one extra integer column in the
existing `UPDATE` (`attempts`). No new query per page. Measured with the
bench site (100,000 pages) before and after; target: within 3% of the
baseline (run-to-run noise on the container is about 2%). A second
benchmark with 2% of pages answering 503 once checks that retries do not
stall the crawl.

### D11. Queue window keeps other hosts moving

The queue window holds the first 4,096 queued rows. If a paused host owns
most of them, other hosts' URLs are not in the window and the crawl would
idle until the pause ends (SC-005). When no host in the window can start
and the window does not hold the whole queue, the window loads the next
batch, up to 64k rows. The same fix helps spec 008's long crawl-delay
hosts.

### D12. Manual retry: one engine call, two front ends

`CrawlStore::transient_failures()` counts re-queueable URLs by class (for
the confirmation, FR-031). `Crawler::retry_failed(path)` re-queues exactly
those (state back to `queued`, new `queue_seq`, `round_attempts = 0`,
`manual_retry = 1`, old links and page data of those rows removed) in one
transaction, writes a `manual_retry` log row, and returns a crawler ready
to `run()`. The desktop app asks for confirmation with the counts; the CLI
prints them and asks, or proceeds with `--yes`. Not available while a crawl
runs (FR-034): the app disables it with the reason, the engine refuses a
file whose status is `running` unless it was left by a crash.

### D13. Live settings changes (spec 008 leftover)

`Crawler::run` takes a command channel next to the control channel:
`ResumeHost(host)` and `Apply(Change)`. Speed changes update the global and
per-host caps at once (a host's adaptive allowance is clamped to the new
cap); retry changes apply to the next decision. Each change is recorded in
the stored configuration's `changes` list with its time (spec 002 FR-040),
so results stay explainable. The settings sheet keeps every other section
locked during a crawl; Speed and Network-and-retries become editable with
an "Apply to running crawl" button.

## Data model

See [data-model.md](./data-model.md). All changes are additive; a format-2
file from before this feature opens, and on resume its missing columns and
table are added (`ALTER TABLE ADD COLUMN`, cheap in SQLite: no rewrite).

## UI

- **Status bar**: "Waiting for host" with countdown when every remaining
  host is paused; a paused-hosts popover listing host, reason, time left and
  "Resume now" (FR-028, US4.4, US4.5); retries waiting count.
- **Crawl summary panel**: errors by class, transient/permanent totals,
  retries made, recovered, hosts paused, local-problem pause message.
- **Retry failed URLs**: toolbar menu and command palette; confirmation
  dialog with counts by class; disabled with a reason while running or when
  nothing is transient (FR-030, FR-031, FR-034).
- **Settings, Network and retries**: base wait, host pause thresholds next
  to the existing attempts and max wait; editable during a crawl with Speed
  (D13).
- **Results table** (coordinated with spec 007's owner): Attempts column,
  "Error class" with sub-reason, Response codes filters "Transient errors",
  "Permanent errors" and one per class, detail pane with class, sub-reason,
  message, attempts and "Recovered by manual retry".

Every UI change gets Vitest tests using the existing fake transport.

## Risks

- **Fixture timing tests flake on slow CI runners.** Waits are checked with
  a tolerance and the base wait is lowered in tests (`with_retry_base`
  already exists for robots.txt).
- **Results crate overlap with spec 007's open PR #15.** Agreed with that
  thread before touching `crates/results`; this branch rebases on #15.
- **Schema overlap with spec 006.** Columns and table agreed with the
  storage thread before pushing (data-model.md lists them).
