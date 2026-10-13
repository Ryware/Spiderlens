# Benchmarks: Page Data Extraction

**Date**: 2026-10-13. **Machine**: 4-core cloud container, bench site on the
same machine (not the spec 008 reference laptop). **Before**: `main` at
3e8fb45 (after PR #11). **After**: this branch. Release builds, runs
alternated before/after.

Site: `cargo run --release -p spiderlens-test-fixtures --bin bench-site --
--pages 100000`, then `spiderlens crawl http://127.0.0.1:8700/p/0 --speed
fast --out FILE --quiet`. Each bench page is about 3 KB with ~25 links.

| Measure | Before | After |
|---|---|---|
| 100,000 pages, Fast (SC-003, target 500 URLs/s; plan D9 target 3,100) | 24.4 to 24.9 s, ~4,050 URLs/s | 29.0 to 31.1 s, ~3,350 URLs/s |
| Peak memory (RSS), 100,000 pages | 18 MB | 31 to 33 MB, flat after the first seconds |
| Crawl file size | 154 MB | 336 MB |
| Stored raw HTML vs received (SC-005, target 30%) | | 15.6% (45 MB for 289 MB) |
| One 50 MB page with 100,000 links (SC-004, target 10 s) | | 0.58 s |
| Malformed corpus vs Chromium (SC-002, target 98%) | | 59 of 60 pages (98.3%) |

Where the file size goes (after, 100,000 pages): `links` 144 MB plus its two
indexes 59 MB, `bodies` 50 MB, `urls` 15 MB, `headers` 12.5 MB, `page_text`
12 MB plus index 7.5 MB, `pages` 11 MB, duplicate hash indexes 3 MB.

Notes:
- The crawl still runs on one coordinator thread that owns SQLite; per page
  it now writes one `pages` row, ~3 `page_text` rows, a `bodies` row and a
  `headers` row on top of the link rows. Extraction itself runs off that
  thread and is not the limit.
- Two changes kept the cost down: a background thread runs passive WAL
  checkpoints so the writer never waits on them, and plain links (no rel,
  alt or size) use a narrow 10-column insert.
- The two duplicate-filter indexes on `pages` (random xxh3 keys) cost about
  5 s of the 100,000-page run when maintained live, because every insert
  dirties a different index page. They are built when a run ends (0.1 s for
  100,000 pages) and dropped when a run resumes, so a reader during a live crawl does
  not have them.
- Memory grows by the extraction budget in use (parse trees of the pages
  being extracted, bounded at 64 MB of bodies) and the blocking threads.
- The one corpus page that differs: `19-select-swallows.html`. Chromium now
  follows the 2025 HTML change that keeps `<a>` inside `<select>`; html5ever
  follows the earlier rule and drops it.
