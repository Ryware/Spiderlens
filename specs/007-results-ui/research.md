# Research: Results Interface

Measurements for plan.md, taken 2026-10-13 in a 4-core cloud container with
Python's `sqlite3` (SQLite 3.45). The table mirrors the real `urls` schema's
relevant columns; 1,000,000 URLs (10% external, ~1% without a title, random
titles from a small vocabulary so duplicates are common), then 15,000,000
links. File: 694 MB. Scripts: kept in the thread scratchpad, rewritten in
Rust as the `gen-crawl` fixture (T003) and the perf harness (T040).

## R1. Paging a 1M-row view

| Operation | Time |
|---|---|
| First 100 rows sorted by title (`ORDER BY … LIMIT 100`, top-k) | 0.40–0.75 s |
| Materialise full order sorted by title into a temp table | 1.5–2.5 s |
| … sorted by status code | 0.8 s |
| Index the temp table by id (for `view_locate`) | 0.7 s |
| Read 60 rows at position 600,000 | 0.001 s |
| Position of a given id | < 0.001 s |
| Search `ci`-contains over URL, materialised (53k matches) | 0.45 s |
| `COUNT(*)` of internal URLs | 0.12 s |

**Decision**: first screen via top-k, full order materialised in the
background (plan D4). OFFSET paging rejected: cost grows with the position.

## R2. Filter counts and duplicates at 1M

| Operation | Time |
|---|---|
| Distinct titles occurring more than once (`GROUP BY … HAVING`) | 1.4 s |
| Missing / over / below counts in one pass | ~0.3 s |
| Duplicate count via a correlated subquery per row | 5.2 s (rejected) |
| Duplicate count via `LEFT JOIN` of a grouped CTE | 6.0 s (rejected) |

**Decision**: build a temp set of duplicated normalised values once per
data version, then count and filter against it (plan D6). Note for spec 005:
a stored hash of the normalised value with an index would make this
sub-second.

## R3. Links at scale (15M links)

| Operation | Time |
|---|---|
| Inlink counts for 60 visible rows (`links_to` index) | < 0.001 s |
| Inlink count of a URL with 1,000,007 inlinks | 0.045 s |
| Inlink counts for every URL (`GROUP BY to_id`), for sorting by Inlinks | 1.2 s |

**Decision**: inlinks and outlinks are computed per visible row; sorting by
them builds the grouped counts into the view's temp table. No counters on
the hot write path.

## R4. Scroller height cap and windowing

Spec 017 research: Chromium and WebKitGTK cap a scroller at 33,554,428 px
(1,198,372 rows at 28 px). **Decision**: own windowing with a scale factor;
cap physical height at 2^24 px for margin; wheel and keys move in rows when
scaled. TanStack Virtual has no scaled mode (checked v3 options:
`estimateSize`, `scrollMargin`, `rangeExtractor`; none maps offset to index
non-linearly).

## R5. Live refresh cost

At 100k rows a full rebuild of a sorted view is ~0.1–0.2 s (linear
extrapolation from R1), so a 1 s refresh costs at most 20% of one core on
the reader thread while crawling; the crawl runs on its own runtime and
cores (spec 008). Adaptive back-off (interval ≥ 3× build time) bounds the
1M live case. To be verified against SC-007 (≥ 95% of CLI speed) in T040.

## R6. Unicode search and natural sort

SQLite `LIKE`/`lower()` fold ASCII only; Hebrew, Greek, Cyrillic, accented
Latin need Unicode folding. rusqlite `functions` feature registers scalar
functions and collations from Rust. **Decision**: `ci_contains` using
`str::to_lowercase` on both sides (simple folding; full case folding such
as German ß → ss is not needed for "contains" in M1), a `natural` collation
comparing digit runs numerically and the rest case-insensitively, `norm_ws`
for the FR-011 rule. Cost: a Rust callback per row; within R1 numbers in
the Rust harness, to be confirmed in T040.

## R7. CSV

Excel needs a UTF-8 byte order mark to read non-Latin text; LibreOffice and
Numbers accept it. RFC 4180 quoting (quote when a value has a comma, quote,
CR or LF; double inner quotes). Formula neutralising per OWASP CSV
injection: prefix `'` when a value starts with `=`, `+`, `-`, `@`, tab or
CR. A hand-written writer (~40 lines, property-tested) is simpler than
configuring the `csv` crate to do the prefixing and BOM, and keeps one
writer for TSV copy too.

## R8. Page titles tab (T039)

Measured 2026-10-14 in the same 4-core container (shared with other builds,
load average 4–5, so ±30%). File: `gen_crawl --pages` (synthetic crawl
with page content; 71,870 Page titles rows per 100k URLs). Harness:
`cargo run --release -p spiderlens-results --example bench -- <file>
page-titles`. Each figure is what the table waits for when a tab, sort or
filter is picked: first screen, full order and the filter counts.

| 100k URLs, release | Before | After |
|---|---|---|
| Open the tab (All), data unchanged | 254 ms | 66 ms |
| Sort by title / title desc | 383 / 453 ms | 136 / 155 ms |
| Sort by title length / occurrences | 407 / 582 ms | 77 / 73 ms |
| Filters (missing … multiple) | 220–309 ms | 22–47 ms |
| Any of the above right after a crawl commit (worst) | 699 ms | 260 ms |
| Live refresh of a title-sorted view + counts | 471 ms | 153 ms |
| First open of the file (cold cache, builds `temp.titles`) | 409 ms | 385 ms |
| Debug build (`tauri dev`), worst case | 2.0–2.7 s | 0.3–0.4 s (1 s cold) |

At 1M URLs (release): filters 2.5–4.1 s → 0.4–0.8 s, sorts 4.3–6.6 s →
1.3–2.0 s, live refresh + counts 5.8 s → 2.4 s; the first open (full build)
stays ~4 s.

The `pages_title_hash` / `pages_md_hash` indexes made no difference either
way (a finished file with them measured the same as a live one without):
the results reader never used them; it normalised titles itself (FR-011).

**Findings**: every query of the tab called `content_kind`, `norm_ws` (up to
seven times per row in the counts) and `natkey` per row, and probed `pages`
twice per row for Same as H1 and Multiple; sorting by Occurrences looked up
the normalised title per row. The 2.1–2.7 s seen in the app match the debug
build, where these Rust callbacks and the bundled SQLite run unoptimised.

**Decision**: one derived table per crawl file, `temp.titles(id, t, len, k,
n, same_h1, count)` plus `temp.title_occ(v, n)`, holding exactly the tab's
rows. The tab's view, counts and export read it (`TabDef.rows`) instead of
scanning `urls`, and sort by title on the stored `natkey` (`ColumnDef.key`).
It is built once and then **appended to** on each data version: the engine
writes a response, with its page content in the same transaction, only to a
URL in flight, so a listed page never changes; new pages are found with one
primary-key probe per URL and their titles' counts updated in place
(a window function was 4× slower than group-and-look-up). Results are
identical: `tests/page_titles.rs` checks rows, values, every filter, count
and sort against rules computed from the raw file, before and after a live
update, with and without page content, and the export.

**Not changed**: the on-disk schema. An engine-side alternative (a stored
normalised title or its hash, indexed only when a run ends as now) would
save the first build (~0.3 s at 100k, ~4 s at 1M) but not the per-query
work this fixes. Optional for development: `[profile.dev.package."*"]
opt-level = 2` in the workspace would make `tauri dev` timings close to
release ones.

Still to measure on Ryware's Mac (T036–T038, T040): scrolling in the real
WebKit view and its frame times, memory at 100k and 1M (SC-008), and the
live crawl against CLI speed (SC-007) with this tab open.
