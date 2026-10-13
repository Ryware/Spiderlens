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
