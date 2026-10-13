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
