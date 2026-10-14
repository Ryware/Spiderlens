# Benchmarks: Crawl Speed and Performance

**Date**: 2026-10-12. **Machine**: 4-core cloud container, 16 GB, bench site
on the same machine (not the FR-035 reference laptop). **Before**: branch
`003-desktop-crawl` (PR #5). **After**: this branch. Release builds.

Site: `cargo run --release -p spiderlens-test-fixtures --bin bench-site --
--pages 100000 [--delay-ms D] [--max-open N]`, then
`spiderlens crawl http://127.0.0.1:8700/p/0 ...`. Server-side figures come
from the site's own `/__stats`.

| Scenario | Before | After |
|---|---|---|
| 100,000 pages, instant answers, Fast | 458 s, 218 URLs/s | 37 to 42 s, 2,400 to 2,700 URLs/s |
| 100 ms per answer, 50 at once, adaptive off (SC-002, target 450/s) | 419 URLs/s | 438 URLs/s (server peak 500/s) |
| 100 ms per answer, Fast (32 per site, adaptive) | | 277 URLs/s (theoretical 320) |
| Site that answers 429 above 4 open requests, Fast (SC-004, target under 2%) | 2,539 of 3,000 requests rejected (85%) | 15 of 3,000 rejected (0.5%) |
| Cap 50 URLs/s (SC-003) | | server peak 47 per second |
| Cap 10 at once, 100 ms answers (SC-003) | | server peak 10 open |
| Peak memory, 100,000 pages | 16 MB | 17 MB |

Notes:
- Before, the crawler process sat at one full core while the server never
  saw more than 2 requests open: the scheduling loop, not the network, set
  the speed. After, the loop still uses most of one core at 2,500 URLs/s,
  mainly SQLite work for ~25 links per page; that is 5x the SC-001 target.
- The adaptive Fast run at 100 ms is below the fixed run because each site
  starts at 2 requests and climbs; it reaches 32 after about 500 answers.
- SC-002 measured 438/s over a 10,000 page run that includes the first
  seconds; the steady state on the server was 500/s.

## Round 2: 1,000,000 pages and the CI benchmark (2026-10-13)

Same machine and site. `main` at aab9013 before, this branch after.

| Scenario | Before | After |
|---|---|---|
| Memory at 100,000 pages crawled | 33.7 MB | 29.8 MB |
| Memory at 1,000,000 pages crawled (SC-007: at most 20% above) | 76.2 MB (+126%) | 31.1 MB (+4%) |
| Peak memory, whole 1,000,000 page crawl | 78.5 MB | 33.5 MB (final index build) |
| Write-ahead log while crawling | grows with every page: 3.6 GB at 250,000 pages | rewinds every second, 130 MB at most |
| 1,000,000 pages, Fast | 495 s, 2,020 URLs/s | 462 s, 2,165 URLs/s |
| 100,000 pages, Fast | 40.8 s | 35.5 s |

What grew: not the crawler's own memory (heap stayed at 18 to 22 MB the
whole way) but the crawl file's write-ahead log and its index, which SQLite
maps into memory. The background checkpointer copied the log into the file
but never reached its end, because the crawl starts its next transaction
the moment it commits, and SQLite rewinds the log only for a transaction
that starts once everything is copied. A passive checkpoint from the crawl
itself, between two transactions once a second, copies the little that is
left, so the next transaction writes from the start of the log again.

Saving backpressure (FR-023) holds by construction: a result is only sent
to the saving loop by a request that counts as in flight, and nothing new
starts until the loop has saved it, so unsaved results never exceed the
"at once" cap, and page extraction has its own memory budget.

### CI benchmark (FR-034)

`bench-check` (in `test-fixtures`) runs in the `Crawl benchmark` CI job:

- 20,000 pages Fast, five runs, alternating with a build of the base
  branch on the same machine; fails when the best run is more than 10%
  slower than the base's best. A stored number would not work: identical
  builds measured 8% apart on a median of five; the best of five agree
  within 0.5%.
- Write-ahead log under 200 MB on a 50,000 page crawl (63 MB now, 386 MB
  before the fix).
- Caps from the site's own counters: 100 URLs/s, 10 at once, 6 per site
  with adaptive off; under 2% rejected by a site that answers 429 above 4
  open requests.

Local run: 3,003 vs 3,145 URLs/s median (noise), best of five +5.0% and
+2.8% against `main`; every check passes.
