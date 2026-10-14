# Benchmarks: Issue Rules Engine

**Date**: 2026-10-14. **Machine**: 4-core cloud container, bench site on the
same machine (not the spec 008 reference laptop). **Branch**:
`009-issue-rules-engine` at 75815a6. Release builds.

Files:

- **100k real**: `bench-site --pages 100000`, then `spiderlens crawl
  http://127.0.0.1:8700/p/0 --speed fast --out FILE --quiet`. 100,000 pages,
  2.5 million links.
- **1M synthetic**: `cargo run --release -p spiderlens-results --example
  gen_crawl -- 1000000` (1,000,000 URLs, this crate's scale generator).

Figures from `cargo run --release -p spiderlens-results --example
issue_bench -- FILE` and `time spiderlens issues --all --json FILE`.

| Measure | Target | 100k real | 1M synthetic |
|---|---|---|---|
| Final issues after the crawl ends (SC-003) | 3 s / 30 s | **3.0 s** (`spiderlens issues`), 3.4 s with opening the file | **12.1 s** |
| Threshold change, count and list updated (SC-004) | 2 s | **0.08 s** | 0.8 s |
| First screen of the largest issue (SC-005) | 1 s | 0.08 s (100,000 URLs) | 0.07 s (484,034 URLs) |
| Peak memory of the evaluating process (SC-006) | 1M at most +20% | 91 MB | 101 MB (+11%) |

Crawl speed, 100k real, Fast, two runs each (SC-002, target 95%):

| Run | Time |
|---|---|
| Crawl alone | 31.8 s, 30.1 s |
| Crawl with all rules evaluated every 3 s by a second process (`live_issues` example, as the app does) | 30.4 s, 29.1 s |

Live evaluation does not slow the crawl: it reads through its own
connection in WAL mode and never touches the writer. Each live evaluation
took 0.5 to 3.2 s, growing with the crawl.

Extraction with readability counting: the same 100k crawl took 28.3 s on
this machine earlier the same day before the counters were added, and 29.0
to 31.1 s in the spec 005 benchmark. 30 to 32 s now is within the run to
run spread on this container; the counting is one pass over text that the
walk already builds.

## How the final pass got from 8.4 s to 3.0 s (100k real)

| Change | Time |
|---|---|
| First version: one `INSERT ... SELECT` per scope with a CASE per rule | 8.4 s |
| Page rules read `pages` through one join instead of a lookup per field | 7.4 s |
| Link rules test bits of `temp.page_flags`, built by one sequential scan of `links` in Rust | 5.2 s |
| One row per URL with a column per rule, title and heading checks in the same flags pass | 3.7 s |
| The `links` scan split by rowid over four read-only connections | 3.0 s |

Every step was checked against the first version's full output
(`spiderlens issues --all --json`) on the 100k file and the summary on the
1M file: identical.

## Not met or not measured

- **SC-003 partial counts within 2 s on 1M**: counts appear when the whole
  evaluation ends (12 s). The interface stays responsive because evaluation
  runs on the reader thread, but there is no early partial count yet.
- **Memory** is the process peak (VmHWM), including SQLite's page cache
  (bounded at 32 MB per connection) and the hit list; not measured on macOS
  or Windows.
