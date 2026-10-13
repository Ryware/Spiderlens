# Implementation Plan: Crawl Speed and Performance

**Branch**: `claude/crawl-speed` | **Date**: 2026-10-12 | **Spec**: [spec.md](./spec.md)

## Summary

Make the crawler as fast as the user's caps and the site allow, and no
faster. Today the crawl loop itself is the bottleneck: on the local 100,000
page benchmark site the Fast preset reaches about 250 URLs per second while
the server never sees more than 2 requests open at once, and the crawler
process sits at one full CPU core. So the first job is to make the
coordinator cheap per URL; only then do concurrency settings matter.

Then add what the spec needs to go fast safely: a per-site allowance that
grows while a site responds steadily and shrinks on slowdowns, 429/503 and
timeouts (AIMD, the same rule TCP uses), Retry-After, and live speed figures
with an estimated time remaining.

## Technical Context

- Rust 2024, Tokio multi-thread runtime, reqwest 0.12 (rustls, HTTP/2),
  SQLite through rusqlite (WAL, `synchronous=NORMAL`).
- One coordinator task owns the crawl file and every scheduling decision;
  fetches run as separate tasks (actor pattern, spec 003 R7). This stays.
- Reference numbers in this plan come from a 4-core cloud container with the
  benchmark site on the same machine, not the FR-035 reference laptop.

## Where the time goes today (measured)

| Cost per URL in the coordinator | Why it is expensive |
|---|---|
| `dispatch()` rescans the whole 4,096-entry queue window after every response | For each entry it clones the host string, hashes it, parses the URL and checks robots.txt *before* checking whether that host already has its maximum open. With one site saturated, every response costs ~4,096 wasted iterations. |
| One SQLite transaction per response, plus one auto-committed write per request start | WAL commits are cheap but not free; batching amortises them. |
| A `GROUP BY state` count every 250 ms for progress (PR #5) | A full index scan that grows with the crawl. Counters kept in memory cost nothing. |

## Design decisions

### D1. Per-host queues inside the window (fixes the rescan)

The window keeps its entries in queue order, and also an index of keys per
host. `dispatch()` walks hosts, not URLs: skip a host in O(1) when it is at
its allowance, waiting for robots.txt, or inside a crawl-delay or
Retry-After; among the hosts that can start, take the one whose next URL is
earliest in the queue. Breadth-first order is preserved, cost per dispatch
is O(hosts in the window) instead of O(URLs in the window), and fairness
across sites (FR-010) falls out: a site with a huge backlog cannot hide
other sites' URLs behind it.

*Analogy*: a scheduler with one run queue per CPU instead of one global
list that every core scans.

### D2. Group commits to SQLite

Responses are written inside one open transaction that is committed every
200 ms (or 500 responses, or when the loop goes idle, pauses, stops or
ends). A crash loses at most the last fraction of a second (FR-024 allows
"the last few seconds"). The on-disk format does not change.

*Trade-off*: the desktop's own reads see results up to 200 ms later.

### D3. In-memory counters for progress

Queued and not-crawled counts are maintained as counters at the few places
a URL changes state, seeded once from the file on start and resume, and
cross-checked against the file in tests. This replaces the per-tick
`GROUP BY` from PR #5 without changing the event's fields.

### D4. Adaptive per-site allowance (AIMD)

Per host: `allowance` starts at 2 (or 1 if the cap is 1).
- **Grow**: after every `allowance` consecutive completed requests with no
  overload signal and a typical response time no worse than 1.5x the site's
  baseline, `allowance += 1`, up to the per-site cap.
- **Shrink on slowdown**: typical response time (EWMA of the last ~20) above
  2x the baseline, or a timeout: `allowance = max(1, allowance * 3/4)`.
- **Shrink on 429/503**: `allowance = max(1, allowance / 2)` immediately;
  with `Retry-After` (seconds or HTTP date), no new request to that host
  until then. Other hosts are unaffected.
- **Baseline**: the lowest typical response time seen while the allowance
  was small; it is what "normal" means for that site.
- **Adaptive off** (FR-018): allowance is the per-site cap, but 429/503 and
  Retry-After are still honoured.
- Crawl-delay keeps forcing 1 at a time, spaced (FR-017), as today.

*Trade-off*: a cautious start costs a few seconds on fast sites (reaching 32
from 2 takes about 30 x 32 / 2 = ~500 responses, well under a second on a
fast site); in exchange a struggling site is never hit with the full cap.

Retrying the 429/503 URL itself belongs to spec 004 (retries); until 004
lands the response is recorded as received.

### D5. HTTP client

- Enable `gzip` and `brotli` decoding in reqwest: real sites send 4-10x
  fewer bytes. On the local benchmark it changes nothing.
- HTTP/2 multiplexing is already negotiated over TLS (ALPN); one connection
  per site carries all of that site's parallel requests.
- Keep the connection pool unbounded per host (reqwest default) so idle
  keep-alive connections are reused instead of reconnecting.
- DNS: reqwest's default resolver runs `getaddrinfo` on a blocking thread;
  with connection reuse it is called once per host, so no change now. A
  caching async resolver is worth it only for list mode across thousands of
  hosts and is deferred.

### D6. Speed figures and ETA (FR-027, User Story 3)

`Progress` gains:
- `urls_per_sec`: rolling average over the last 5 seconds (spec assumption).
- `avg_urls_per_sec`: since the crawl (or resume) started.
- `eta_secs`: queued / current rate, or none while the rate is zero.

*Honesty caveat*: in Spider mode the total is unknown until the crawl ends,
so the ETA is "for the URLs found so far" and grows while discovery
outpaces crawling. The CLI and UI say so.

### D7. Benchmark site

`crates/test-fixtures/src/bin/bench-site.rs`: a synthetic site of N pages
(default 100,000), ~4 KB each, a tree of 10 children per page plus parent,
3 pseudo-random links and a 20-link menu, optional fixed delay. Binds to
127.0.0.1 only. `/__stats` reports requests, peak simultaneous requests and
peak requests per second as seen by the server, so caps are verified from
the server side (SC-003).

## Out of this round (later tasks in tasks.md)

Live cap changes during a crawl (FR-005), speed events in the crawl file
(FR-021), the per-site breakdown and limiting-factor reasons in the UI
(FR-028, FR-029), global-slowdown detection (FR-019), saving backpressure
(FR-023), the 1,000,000 URL memory run (SC-007) and the CI regression
benchmark (FR-034).

## Constitution Check

- Respectful by default: Polite stays the default; adaptation only ever
  lowers load below the user's caps; benchmarks hit local hosts only.
- Engine independent of UI: all changes are in `crates/engine`,
  `crates/config` and `crates/test-fixtures`.
- Tests first for behaviour: AIMD and Retry-After get unit tests; caps get
  server-side checks on the fixture site.
