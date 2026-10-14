# Benchmarks: Crawl Storage

**Date**: 2026-10-13. **Machine**: 4-core cloud container, bench site on the
same machine. **Before**: `main` at f3a4d3d. **After**: branch
`006-crawl-storage`. Release builds, runs alternated before/after.

Site: `bench-site --pages 100000`, then `spiderlens crawl
http://127.0.0.1:8700/p/0 --speed fast --out FILE --quiet` (the default
1 GB disk reserve is on in the "after" runs).

| Measure | Before | After |
|---|---|---|
| 100,000 pages, Fast | 33.5 to 35.6 s | 33.1 to 35.6 s |
| Peak memory (RSS) | 32 MB | 31 to 32 MB |
| Crawl file size | 320 MB | 320 MB |

The first baseline set (31.4 to 32.9 s) was taken earlier on a quieter
machine; the table compares runs taken back to back.

No measurable change, as planned: the per-page write path is untouched.
Added per run are a lock file, a few `events` rows and a free-space check
every 2 s.

Memory (SC-002): peak RSS is reached in the first seconds and stays flat to
100,000 URLs (32 MB in both), well under the 20% growth allowed.

Recovery (SC-001): `crates/cli/tests/storage.rs` kills a running crawl three
times in a row (at 200, 500 and 800 of 1,500 pages); every resume recovers,
and the finished crawl has each page exactly once.
