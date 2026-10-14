# Benchmarks: Fetch Errors and Retries

**Date**: 2026-10-13. **Machine**: 4-core cloud container, bench site on the
same machine. **Before**: `main` at f3a4d3d. **After**: branch
`004-errors-retries`. Release builds, runs alternated before/after.

Site: `bench-site --pages 100000`, then `spiderlens crawl
http://127.0.0.1:8700/p/0 --speed fast --out FILE --quiet`. The flaky runs
add `--flaky-per-mille 20`: 2% of pages answer 503 the first time and 200
after that (new bench-site option).

| Measure | Before | After |
|---|---|---|
| 100,000 healthy pages (target: no slower) | 33.8 to 36.5 s | 33.2 to 34.6 s |
| 100,000 pages, 2% answer 503 once | 33.5 to 34.6 s | 34.1 to 36.8 s |
| Crawl file, 100,000 pages | 336.3 MB | 336.5 MB |

All 2,008 flaky pages were recovered by the automatic retry in every run
(summary: "2008 made, 2008 URLs recovered"); no host was paused.

Notes:
- A healthy crawl does no extra work per page: the retry decision is a pure
  function on data already in memory, the breaker is two counters per host,
  and the new columns are written in the same row update as before.
- The first version of the flaky run took 61 to 64 s. Each 503 fed the
  host's adaptive speed control as "overloaded" and halved its concurrency,
  so 2,000 scattered 503s kept one host near a single request at a time.
  Before this branch, retries happened inside the request and speed control
  only saw the final answer. Now a 503 that will be retried, without
  Retry-After, does not cut the speed; a 503 after the last attempt, any
  429 and any Retry-After still do, and a host that keeps failing is caught
  by the host breaker (plan D6).
- Before/after numbers on this machine vary by about 10% between runs; the
  ranges above are from alternated runs.
