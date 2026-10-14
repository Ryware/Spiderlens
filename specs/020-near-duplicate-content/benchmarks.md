# Benchmarks: Near Duplicate Content

**Date**: 2026-10-14. **Machine**: 4-core cloud container, 16 GB (not the
FR reference laptop). Release builds. Same machine and method as spec 008.

## Crawl speed (T008, SC-004: at least 98% of the speed before)

Site: `cargo run --release -p spiderlens-test-fixtures --bin bench-site --
--pages 100000`, then `spiderlens crawl http://127.0.0.1:8700/p/0 --speed
fast`. Before is `main` at this branch's base, after is this branch. Two
runs each, the CLI's own "took … at N URLs/s" line.

| | Run 1 | Run 2 |
|---|---|---|
| Before (main) | 2,655 URLs/s | 2,982 URLs/s |
| After (this branch) | 2,778 URLs/s | 2,659 URLs/s |

The spread between runs of the same build (about 10%) is larger than any
difference between the builds; the crawl loop, not page extraction, sets the
speed (spec 008 benchmarks). After the crawl, the analysis pass adds 5.7 to
6.0 s on this site (below).

Page extraction alone, one thread, a typical 9 KB page with navigation,
footer and 800 words of main content
(`cargo run --release -p spiderlens-engine --example extract_speed`):

| | µs per page |
|---|---|
| main | 153 to 157 |
| This branch, first version | 390 |
| This branch, after the fixes below | 185 to 188 |

The remaining 30 µs is the main content collector and the signature, on a
thread pool that the crawl does not wait on at these speeds.

Fixes made while measuring:

- Tokenising takes a byte path for ASCII (84 µs to 15 µs for 820 words).
- Word count, token hashes and the exact-content hash come from one pass;
  the main text is no longer collapsed (tokens ignore spacing).
- `word_count` itself takes the same ASCII byte path (it also counts words
  for the whole page, so every crawl gains).

## Analysis (T013, SC-001, SC-002, SC-005, SC-006)

### Synthetic signatures

`cargo run --release -p spiderlens-results --example near_dup_bench -- N [dense]`:
300 words per page, one page in ten in a family of five near copies, plus
an optional template group.

| Pages | Signatures (4 threads) | Analysis at 90% | Analysis at 80% (SC-006) |
|---|---|---|---|
| 100,000 | 0.8 s | 0.9 s | 0.9 s |
| 1,020,000 with one 20,000-page template group | 8.2 s | 20.8 s | 19.2 s |

Targets: 100k pages in about a minute (SC-001), 1M in 10 minutes (SC-002),
a 20k group finishes (SC-005). Changing the threshold re-runs from stored
signatures, without re-crawling (SC-006).

### Real crawl file, thin templated pages

The 100,000 bench-site pages above share their main text (repeated filler)
and differ in a few links: about 33% similar, so nothing is a near
duplicate, but LSH puts 90,000 of them in one dense bucket group. This is the
worst case seen so far (`cargo run --release -p spiderlens-results
--example near_dup_pass -- file.spiderlens`):

| Version | Pass time |
|---|---|
| First version | 31.3 s |
| Candidate de-duplication amortised (doubling limit) | 14.6 s |
| Vectorised bin comparison (194 ns to 36 ns per pair) | 9.3 s |
| Dense groups scored in cache-sized tiles | 5.7 s |

A 128-bin prefix check rules out most unrelated pairs before the full
comparison (pairs at the floor are ruled out about once in 10^8).
