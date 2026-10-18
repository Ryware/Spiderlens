# Benchmarks: JavaScript Rendering

## Baseline (T004, SC-001)

**Date**: 2026-10-13. **Machine**: 4-core cloud container, 16 GB, no swap,
bench site on the same machine (not the reference laptop). **Code**: `main`
at 5e6f2ff (this branch at 48d802f differs from it only in `specs/`).
Release builds (`lto = "thin"`, `codegen-units = 1`, stripped).

Site: `bench-site --pages 100000` (spec 008). Crawl: `spiderlens crawl
http://127.0.0.1:8700/p/0 --speed fast --out FILE --quiet --json`. Each run
starts with an empty crawl file and the site's counters reset; runs are
separated by 30 s of idle time. Time is wall time of the `spiderlens`
process, URLs per second is the site's own request count (100,000 in every
run) divided by that time, and peak memory is the process's maximum
resident set size (`wait4` rusage).

| Run | Time | URLs/s | Peak RSS | Crawl file |
|---|---|---|---|---|
| 1 | 35.12 s | 2,847 | 32.0 MB | 336.0 MB |
| 2 | 36.92 s | 2,709 | 31.8 MB | 335.8 MB |
| 3 | 32.69 s | 3,059 | 31.7 MB | 335.8 MB |
| 4 | 35.74 s | 2,798 | 32.2 MB | 335.9 MB |
| 5 | 35.67 s | 2,804 | 32.9 MB | 335.7 MB |
| 6 | 36.57 s | 2,735 | 32.1 MB | 335.6 MB |
| 7 | 34.72 s | 2,880 | 31.9 MB | 335.4 MB |
| **Median** | **35.67 s** | **2,804** | **32.0 MB** | 335.8 MB |
| Spread (min to max) | 32.69 to 36.92 s (11.9% of the median) | 2,709 to 3,059 (12.5%) | 31.7 to 32.9 MB (3.8%) | |

The spread between single runs (about 12% here, 7% in spec 005) is far
larger than the 2% throughput budget of SC-001, so T080 compares medians of
7 alternating runs, as the plan says. Even medians of 7 runs on this
container probably move by a few percent between sessions; T080 should run
the baseline binary and the new one alternately in the same session rather
than compare against these numbers alone.

Release binary sizes on Linux x86-64 (stripped):

| Binary | Size |
|---|---|
| `spiderlens` (CLI) | 13,358,904 bytes (12.7 MiB) |
| `spiderlens-desktop` (built with `--features tauri/custom-protocol`, as `tauri build` does) | 19,007,352 bytes (18.1 MiB) |

To reproduce: build both binaries and `bench-site` from the merge base with
`cargo build --release`, then run the crawl command above 7 times with an
idle pause between runs.

## Results tabs with rendered values (T030, research R11)

**Date**: 2026-10-14. **Machine**: the same 4-core container (Intel Xeon
2.8 GHz), shared with other work, so single timings move by 10 to 30%.
**Code**: this branch after 6f6bfde plus the T030 changes. Release build of
`crates/results/examples/bench.rs`.

**Files**: `gen_crawl 100000 FILE rendered` (and `1000000`). The synthetic
file is the spec 007 one (about 10 links per page, base data only) plus,
for each of its 71,870 internal 200 HTML pages, a `rendered_pages` row
(a third of the titles changed, one page in twenty noindex), a `renders`
row, one link found only after rendering, and, on one page in four, one
raw link gone after rendering. The raw crawl column is the same generator
without rendered data. Each mode ran 3 times alternately; the table shows
medians in milliseconds. "Raw values" is the rendered file read with
`values = raw` (no temporary views, the raw expressions): the comparison
the 20% rule is about. Rows read at 90% of a view (1 to 2 ms in every
mode) are left out.

| Step | Raw crawl | Rendered crawl, raw values | Rendered crawl, rendered values | Change |
|---|---|---|---|---|
| setup: first Internal view | 0 | 0 | 93 |  |
| Internal, default order: first screen | 1 | 0 | 1 | (under 10 ms) |
| Internal, default order: full order | 69 | 53 | 42 | -21% |
| Internal, sort by title: first screen | 52 | 68 | 78 | +15% |
| Internal, sort by title: full order | 160 | 156 | 179 | +15% |
| Internal, sort by title desc: first screen | 49 | 53 | 75 | +42% |
| Internal, sort by title desc: full order | 160 | 147 | 180 | +22% |
| Internal, sort by status: first screen | 19 | 19 | 27 | +42% |
| Internal, sort by status: full order | 59 | 61 | 61 | +0% |
| Internal, sort by inlinks: first screen | 143 | 159 | 255 | +60% |
| Internal, sort by inlinks: full order | 233 | 196 | 289 | +47% |
| Internal, search 'shoes': first screen | 1 | 1 | 1 | (under 10 ms) |
| Internal, search 'shoes': full order | 61 | 63 | 89 | +41% |
| Internal, search 'חנות' sorted by title: first screen | 79 | 92 | 101 | +10% |
| Internal, search 'חנות' sorted by title: full order | 92 | 96 | 111 | +16% |
| Page titles, duplicate: first screen | 144 | 141 | 187 | +33% |
| Page titles, duplicate: full order | 100 | 80 | 98 | +22% |
| Page titles, sort by occurrences: first screen | 183 | 160 | 190 | +19% |
| Page titles, sort by occurrences: full order | 215 | 193 | 237 | +23% |
| Response codes, 4xx: first screen | 29 | 28 | 43 | +54% |
| Response codes, 4xx: full order | 19 | 18 | 29 | +61% |
| Response codes, redirect chains: first screen | 18 | 17 | 24 | +41% |
| Response codes, redirect chains: full order | 18 | 18 | 19 | +6% |
| URL, non-ASCII: first screen | 0 | 0 | 0 | (under 10 ms) |
| URL, non-ASCII: full order | 36 | 35 | 39 | +11% |
| Internal, sort by outlinks: first screen | 149 | 133 | 227 | +71% |
| Internal, sort by outlinks: full order | 168 | 165 | 263 | +59% |
| Images, sort by found on: first screen | 191 | 205 | 205 | +0% |
| Images, sort by found on: full order | 187 | 223 | 230 | +3% |
| Internal, sort by indexability: first screen | 0 | 0 | 1 | (under 10 ms) |
| Internal, sort by indexability: full order | 39 | 39 | 42 | +8% |
| counts internal | 85 | 88 | 83 | -6% |
| counts external | 21 | 20 | 21 | +5% |
| counts response-codes | 54 | 47 | 49 | +4% |
| counts url | 210 | 131 | 124 | -5% |
| counts page-titles | 184 | 220 | 189 | -14% |
| counts images | 29 | 27 | 30 | +11% |
| detail of URL 1 (most inlinks) | 0 | 0 | 1 | (under 10 ms) |
| export Internal to CSV | 819 | 766 | 1046 | +37% |


1,000,000 URLs (one run each, same generator; 899,705 internal URLs):

| Step | Raw values | Rendered values | Change |
|---|---|---|---|
| setup: first Internal view | 0 | 702 |  |
| Internal, default order: first screen | 1 | 1 | (under 10 ms) |
| Internal, default order: full order | 450 | 395 | -12% |
| Internal, sort by title: first screen | 492 | 904 | +84% |
| Internal, sort by title: full order | 2343 | 2941 | +26% |
| Internal, sort by title desc: first screen | 479 | 1071 | +124% |
| Internal, sort by title desc: full order | 2579 | 2695 | +4% |
| Internal, sort by status: first screen | 173 | 214 | +24% |
| Internal, sort by status: full order | 781 | 796 | +2% |
| Internal, sort by inlinks: first screen | 1676 | 2581 | +54% |
| Internal, sort by inlinks: full order | 2415 | 3483 | +44% |
| Internal, search 'shoes': first screen | 1 | 1 | (under 10 ms) |
| Internal, search 'shoes': full order | 624 | 1191 | +91% |
| Internal, search 'חנות' sorted by title: first screen | 777 | 1385 | +78% |
| Internal, search 'חנות' sorted by title: full order | 1188 | 1644 | +38% |
| Page titles, duplicate: first screen | 1943 | 2221 | +14% |
| Page titles, duplicate: full order | 1267 | 1507 | +19% |
| Page titles, sort by occurrences: first screen | 2551 | 2709 | +6% |
| Page titles, sort by occurrences: full order | 3096 | 3627 | +17% |
| Response codes, 4xx: first screen | 342 | 359 | +5% |
| Response codes, 4xx: full order | 156 | 159 | +2% |
| Response codes, redirect chains: first screen | 168 | 166 | -1% |
| Response codes, redirect chains: full order | 184 | 162 | -12% |
| URL, non-ASCII: first screen | 0 | 0 | (under 10 ms) |
| URL, non-ASCII: full order | 374 | 342 | -9% |
| Internal, sort by outlinks: first screen | 1521 | 2566 | +69% |
| Internal, sort by outlinks: full order | 1807 | 3374 | +87% |
| Images, sort by found on: first screen | 3307 | 3485 | +5% |
| Images, sort by found on: full order | 3145 | 3697 | +18% |
| Internal, sort by indexability: first screen | 1 | 1 | (under 10 ms) |
| Internal, sort by indexability: full order | 432 | 394 | -9% |
| counts internal | 865 | 963 | +11% |
| counts external | 193 | 200 | +4% |
| counts response-codes | 484 | 479 | -1% |
| counts url | 1332 | 1329 | -0% |
| counts page-titles | 1850 | 2206 | +19% |
| counts images | 250 | 326 | +30% |
| detail of URL 1 (most inlinks) | 0 | 0 | (under 10 ms) |
| export Internal to CSV | 10741 | 14509 | +35% |


### What was rewritten, and why

The first measurement (research R11's design with the urls overlay and
per-expression variants) found three things, now fixed:

- **The `NOT INDEXED` hint does not pass through a view.** Through the
  `urls` view SQLite used the state index and sorted, so the Internal first
  screen in id order went from 1 ms to 53 ms. The view now reads
  `main.urls NOT INDEXED` itself (lookups by id still use the rowid).
- **Inlinks and Outlinks** read every link row to filter out raw links gone
  after rendering (`live_raw!`): sorting by Inlinks took 3,459 ms instead of
  155 ms. They now count raw links from the index alone, subtract the gone
  links (`temp.gone_to` for inlinks, a derived table built from the small
  `raw_only_links`; a count of `raw_only_links` by page for outlinks) and
  add the `rendered_links` index count.
- **Every rendered column of every row was computed before a first-screen
  sort kept 60 rows.** In rendered mode the first screen now picks the 60
  ids first and reads their columns after (sort by status at 1M: 623 ms to
  214 ms). Raw mode is unchanged.

Also: the "Values" flag and the rendered variants share the view's single
`rendered_pages` lookup (`u.rendered_row`) instead of a second lookup, and
filter counts of a tab whose conditions read no rendered field count from
`main.urls`, because SQLite keeps the view's LEFT JOIN in an aggregate even
when nothing from it is read.

`crates/results/tests/rendering.rs` checks the rewritten link counts
against their plain definition for every URL of a synthetic rendered file,
the two-step first screen against the stored order, and the counts against
the listed rows.

### What is still more than 20% slower

Anything that sorts or searches on a rendered value pays one extra
primary-key lookup per row (the `rendered_pages` row, or the gone and
rendered link counts), and the result is still above the 20% budget:

| Step (100,000 URLs) | Raw values | Rendered values |
|---|---|---|
| Internal, sort by Inlinks / Outlinks, first screen | 159 / 133 ms | 255 / 227 ms |
| Internal, sort by Title, first screen | 53 to 68 ms | 75 to 78 ms |
| Internal, search, full order | 63 ms | 89 ms |
| Export Internal to CSV | 766 ms | 1,046 ms |

At 1,000,000 URLs two steps cross a spec 007 target that raw values meet:
the Inlinks and Outlinks sorts show their first screen after about 2.6 s
(1.5 to 1.7 s raw; SC-004 asks for 2 s), and the Page titles filter counts
take 2.2 s (1.85 s raw; SC-003 asks for 2 s). Three steps were already
over their target with raw values on this machine and stay over: the Page
titles "Duplicate" first screen (1.9 s raw, 2.2 s rendered; 1 s), the Page
titles occurrence sort (2.6 s, 2.7 s; 2 s) and the Images "Found on" sort
(3.3 s, 3.5 s; 2 s). Everything else stays inside the targets. Setting up rendered values (the views
and `temp.gone_to`) costs 93 ms at 100,000 URLs and 702 ms at 1,000,000,
once per data version.

These are not fixable by rewriting expressions further: the cost is the
lookup itself. The options, for the owner:

1. Accept it: rendered values cost a lookup per row only when a sort,
   search or export reads them; the raw switch gives raw speed.
2. A derived table of the per-URL change in link counts (`d_in`, `d_out`),
   tried here: Inlinks/Outlinks first screen 196 / 211 ms instead of
   255 / 227 ms at 100,000 URLs, but its build grows with `rendered_links`
   (on a script-only site that is every link) and runs again at every
   refresh of a running crawl (setup 213 to 253 ms at 100,000 URLs instead
   of 93 ms).
3. Store the rendered inlink and outlink counts when the crawl writes them
   (a data-model change for a later task): the sorts then cost what raw
   ones do.

## Raw fetch rate while rendering (T038, SC-002 second half)

**To be measured.** The test is
`spa_raw_fetch_rate_while_rendering_is_not_lower` in
`crates/engine/tests/rendering.rs` (ignored). It starts `bench-site --spa
--pages 10000`, crawls it with rendering off and on in alternating rounds
(3 each), stops the rendering crawl once every page is fetched, and reads
the fetch rate from the site's own counters (`/__stats`: `pages`,
`first_page_ms`, `last_page_ms`). Two cases: capped at 300 URLs per second
(asserted: on at least 95% of off) and uncapped (printed only, because the
site, the crawler and the browser share one machine's CPUs). 10,000 pages
never reach the 50,000 queue mark, so this measures the fetch rate until
the mark.

```
SPIDERLENS_BROWSER=/path/to/chromium cargo test --release -p spiderlens-engine \
  --test rendering -- --ignored --nocapture spa_raw_fetch_rate
```

| Case | Rendering off | Rendering on | Change |
|---|---|---|---|
| capped, 300 URLs/s | to be measured | to be measured | |
| uncapped | to be measured | to be measured | |

An unrecorded earlier trial on the 4-core container (before the statement
cache fix in this batch) found rendering on 60% slower even when no page
was rendered; the cause was rusqlite's 16-statement cache being exceeded.
That is fixed in `store.rs` (128 statements). The SC-001 gate (T080) still
has to be run with 7 alternating runs.
