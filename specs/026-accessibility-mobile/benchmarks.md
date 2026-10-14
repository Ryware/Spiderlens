# Benchmarks: Accessibility and Mobile Usability Checks

**Date**: 2026-10-14. **Machine**: 4-core cloud container, bench site on the same machine (not the reference laptop). Release builds of branch `026-accessibility-mobile-code`.

## Static checks (T011, SC-003: at most 3% slower than Off)

### Extraction only

`cargo run --release -p spiderlens-engine --example a11y_bench`: `extract_page` on 200 pages shaped like the bench site's and 50 heavier shop pages (60 product cards, forms, images). Off and on alternate 15 times; the fastest run of each is kept. Three runs:

| Pages | Off | On | Extra share of extraction time |
|---|---|---|---|
| bench-site shape | 60.8 to 61.3 µs/page | 64.7 to 65.2 µs/page | +6.3% to +6.5% |
| rich shop page | 626 to 635 µs/page | 655 to 664 µs/page | +3.4% to +4.7% |

The collector costs about 4 µs on a bench page and 25 µs on a rich page. Extraction is only part of the work a crawl does for each page. The slowdown is less than this share.

### Whole crawl

`bench-site --pages 30000`, then `spiderlens crawl http://127.0.0.1:8700/p/0 --speed fast`. Off is the branch as committed. On is the same build with the runner's `a11y` flag set to true (computed and dropped; storage comes in PR 2). Four alternating pairs:

| Run | Off | On |
|---|---|---|
| 1 | 9.18 s, 3,392 URLs/s | 9.17 s, 3,380 URLs/s |
| 2 | 9.72 s, 3,210 URLs/s | 8.91 s, 3,519 URLs/s |
| 3 | 9.64 s, 3,215 URLs/s | 9.22 s, 3,368 URLs/s |
| 4 | 9.16 s, 3,442 URLs/s | 9.15 s, 3,432 URLs/s |
| Median | 9.41 s | 9.16 s |

No measurable difference: the cost is below the run-to-run noise (about 6%). SC-003 holds on this machine, so the default stays "Static checks" (plan D4). The full 100,000-page run with peak memory (T002 and T040) is repeated in PR 2, once the findings are stored, because writing them is the part that can cost more.
