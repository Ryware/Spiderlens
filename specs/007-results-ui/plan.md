# Implementation Plan: Results Interface

**Branch**: `007-results-ui` | **Date**: 2026-10-13 | **Spec**: [spec.md](./spec.md)

## Summary

Turn the app from "a crawl with a progress page" into "a crawl you can
browse": a dense, Headroom-style results table with the eleven M1 tabs,
filters with live counts, sort, search, column choice, a detail inspector,
copy and CSV export, smooth at one million rows and during a live crawl.

The table never holds the crawl. Rust owns every query (tab, filter, search,
sort) against the crawl file and serves the UI one screen of rows at a time.
The UI owns only the column layout, the scroll position and a small cache of
recently seen rows.

One fact shapes the delivery: **most of the data some tabs show is not
collected yet.** Today the crawl file stores status, content type, redirect
target, title, depth, links (without anchor text) and error class. Meta
descriptions, H1/H2, headings, images, canonicals, directives, word count,
size, response time, headers and raw HTML arrive with spec 005 (extraction)
and spec 004 (attempts). This plan therefore builds the whole interface now,
with every tab, column and filter declared, and each one stating which data
it needs. What the file lacks is shown as "not collected yet" with the reason,
using the same mechanism the spec already requires for older crawl files
(Edge cases: "Older crawl files"). When spec 005 lands, those tabs light up
without UI work. See D3 and the delivery table at the end.

## Technical Context

- Rust 2024, rusqlite (bundled SQLite, WAL), Tokio. Tauri 2 commands and
  events. React 18 + TypeScript, Tailwind v4 with OKLCH tokens, Base UI
  components from spec 017, ts-rs for shared types (`ui/src/generated`).
- New UI dependency: `@tanstack/react-table` (column model only), already
  chosen in spec 017 research. **No** TanStack Virtual (see D8).
- New Rust dependency: none beyond what the workspace has (`csv` writing is
  done by hand, about 40 lines, to control quoting and formula neutralising
  exactly; see research R7).
- Numbers below were measured in this container (4 cores, SQLite 3.45 via
  Python, 1,000,000 URLs, 15,000,000 links, 694 MB file). Rust with rusqlite
  is the same SQLite engine; the reference laptop (FR spec assumptions) is
  expected to be at least as fast. Details in [research.md](./research.md).

## Constitution Check

| Principle | How this plan complies |
|---|---|
| I Spec before code | Spec approved 2026-10-11. One spec wording change (D13, FR-003 window 10 s → 5 s) was approved with the plan on 2026-10-13 and applied to spec.md. |
| II Local-first | Everything reads the local crawl file. |
| III Engine independent of shell | Tabs, filters, queries, detail and CSV export live in a new library crate `spiderlens-results`. The desktop app and the CLI (`spiderlens export`, FR-031) call the same functions. |
| IV Respectful crawling | Not affected. |
| V Correct and complete data | Counts come from the same SQL as the rows, so a filter's count and its rows cannot disagree. Missing data is labelled, never shown as empty-and-fine. |
| VI Scales by design | Rows are paged from disk; the UI keeps at most ~3,000 rows; view order lives in an on-disk temp table, not in memory. Performance impact on the crawl: the crawl writer is unchanged except one meta value per commit (D13); all reads use a separate read-only connection, which WAL never blocks. Measured in T040. |
| VII Test-first | Every acceptance scenario maps to a test (quickstart.md). New fixture site seeded with known issues; synthetic 1M-URL crawl file generator for scale tests. |
| VIII Simplicity | A tab is data (columns and filters as SQL fragments), not code paths. One windowing component serves the main table and the detail lists. |

## Design decisions

### D1. A results crate, separate from the engine

`crates/results` (`spiderlens-results`) is the read side of the crawl file:
tab definitions, view queries, counts, URL detail, CSV export. The engine
remains the only writer. The desktop app and the CLI depend on it.

*Why not in the engine?* The engine's job is crawling; the read side grows
with every audit feature (M2 issues, M5 AEO views) and must not drag the
crawler's compile time or its hot loop along.
*Analogy*: CQRS. The engine is the command side, this crate is the query
side, the crawl file is the shared store.

### D2. Tabs are data, defined once in Rust

Each tab is a static definition: which URLs are eligible (a SQL `WHERE`),
its columns (id, label, SQL expression, value kind, width, default visible),
its main text column for search, and its filters (id, label with threshold,
SQL `WHERE`). The UI asks for the definitions with `results_tabs` and renders
them; it never has its own copy of labels or thresholds. Same pattern as the
not-crawled reason labels in spec 003 (FR-037).

*Trade-off*: SQL fragments in Rust strings are not type-checked by the
compiler. Mitigation: a test opens a crawl file of every supported format
version and runs every tab × filter × sort column once, so a typo fails CI.

### D3. Every column and filter declares the data it needs

`Needs` is a small set: `Base` (exists today), `Extraction` (spec 005),
`Attempts` (spec 004), `Timing` (response time and size), `RawHtml`. When a
view opens, the crate checks which the file has (by schema introspection:
does the table or column exist, plus a `meta` flag written by the producing
spec). Missing ones come back as `unavailable { reason }`; the UI shows the
column empty with a tooltip and the filter disabled with the reason, never
an error. Today that means:

| Works now | Waits for spec 005 / 004 |
|---|---|
| Internal, External, Response codes, URL tabs (all filters), Page titles: All, Missing, Duplicate, Over 60, Below 30 | Page titles: Same as H1, Multiple; Meta description, H1, H2, Images, Canonicals, Directives tabs; Indexability, Word count, Size, Response time, Attempts columns |
| Detail: response basics, redirect chain (from stored targets), inlinks, outlinks (without anchor text) | Detail: headers, headings, images, anchor text and rel values, raw HTML |

*Alternative considered*: build spec 005 first, then 007. Rejected as the
default because the table is what makes every later spec visible and
testable in the app, and because the "data not available" path is required
anyway for older files. Ryware can still choose this order (see the reply).

### D4. Views are materialised order, paged by position

Opening a view (tab + filter + search + sort) runs on a dedicated reader
thread with its own read-only connection:

1. **First screen fast path**: the same query with `LIMIT 200` (SQLite keeps
   a top-k heap instead of sorting everything). Measured 0.4–0.75 s on 1M
   rows sorted by title. This is returned at once with the exact total when
   cheap, or "counting…" otherwise.
2. **Full order in the background**: `INSERT INTO temp.view_N(id) SELECT id
   … ORDER BY …` builds a table whose rowid is the row position. Measured
   1.5–2.5 s for 1M rows sorted by text, 0.8 s by number. After that any
   screen anywhere is `WHERE pos BETWEEN a AND b` joined to `urls`: 1 ms,
   and "where is row id X now?" is an index lookup: under 1 ms.

Until step 2 finishes, a jump deep into the table shows skeleton rows and
fills in as soon as the order is ready (SC-005 allows 200 ms after scrolling
stops; the background build is a one-off per view).

*Why not OFFSET paging?* `OFFSET 600000` re-walks 600,000 sorted rows every
time; dragging the scrollbar would be seconds per frame.
*Why not keyset paging?* It cannot jump to row 600,000 from a scrollbar drag.
*Why not keep the order in memory?* 1M ids is only 8 MB, but the constitution
forbids anything that grows with the crawl in memory, and the temp table
costs the same speed. *Analogy*: a database cursor materialised to a temp
table, as reporting tools do for "page 4,000 of 20,000".

### D5. Live crawls: refresh with anchors, adaptively

The reader knows a data version (the crawl's committed result count). The
visible view rebuilds when the version moved, at most once per second and
never more often than 3× its own last build time (so a 1M live view backs
off instead of burning a core; a 100k view refreshes every second). The UI
sends the id of the top visible row and of the selected row; the reply
carries their new positions, and the table shifts its scroll offset so the
row under the user's eyes does not move (FR-008). The selected URL keeps
its detail pane even if it leaves the filter (Edge case).

Filter counts for the visible tab refresh every 5 s while crawling (FR-009),
on the same reader thread, so the UI never runs two heavy queries at once.

*Trade-off*: rows appear within about 1–2 s, not instantly. The spec allows
2 s. Pushing every row as an event would be faster on screen and far more
expensive at 2,500 URLs per second.

### D6. Counts and duplicates in one pass per tab

A tab's filter counts are one `SELECT COUNT(*), SUM(cond1), SUM(cond2) …`
over the eligible rows. Duplicate filters first build a temp table of
normalised values that occur more than once (`GROUP BY … HAVING COUNT(*) >
1`, measured 1.4 s on 1M), reused by the Duplicate filter, its count and the
Occurrences column. Total for the Page titles tab at 1M: about 2 s, inside
SC-003's "count within 2 s". Spec 005's storage should add a hash column for
normalised title and description with an index; that makes this sub-second
later and is noted for the 005 plan, not needed here.

### D7. Text rules as SQL functions registered from Rust

SQLite's `LIKE` and `lower()` only fold ASCII. The crate registers on the
read connection: `ci_contains(text, term)` (Unicode case folding, for
search, FR-016), a `natural` collation (case-insensitive, digit runs compare
as numbers, FR-015), and `norm_ws(text)` (trim + collapse whitespace, the
duplicate rule of FR-011). `length()` already counts characters, not bytes
(FR-012). Empty values sort last in both directions with `ORDER BY (v IS
NULL OR v = ''), v` (FR-015).

### D8. The grid: TanStack Table for columns, our own windowing for rows

TanStack Table manages the column model: visibility, order, width, and the
sort indicator. Column preferences persist per tab in the existing UI
storage (`ui/src/shell/storage.ts`) and apply to every crawl, with "Reset
columns" (FR-017).

Rows use a small windowing component of our own instead of TanStack
Virtual. Both browser engines cap a scrollable element at 33,554,428 px
(spec 017 research), which at 28 px rows is 1,198,372 rows. Above that the
scroller must be *scaled*: its height is capped and the scroll position maps
proportionally to a row index; wheel and keyboard then move in row units so
one wheel notch still moves about three rows. TanStack Virtual has no scaled
mode, so we would be overriding its core either way. One component, about
200 lines with its own tests, handles both cases (scale factor 1 below the
cap), and the detail pane's long lists reuse it (FR-025).

*Trade-off*: we own scrolling code that a library would otherwise own.
Accepted because the hard part (scaling) is ours in both cases and the rest
is small.

Rows come from a page cache in the UI: pages of 100 rows, at most 30 pages
(3,000 rows) kept, least recently used dropped. Requests carry the view
version, and replies for an outdated version are ignored. This keeps UI
memory flat between 100k and 1M (SC-008).

Visual design follows the approved direction and spec 017 tokens: 28 px
rows, tabular numbers, status code as a toned badge, filter chips with
counts above the table, search field and column menu in the same strip,
selected row in `bg-selection`, the inspector on the right. A mock-up goes
to Ryware before the grid task is merged (T019).

### D9. Transport: a handful of commands

| Command / event | Purpose |
|---|---|
| `results_tabs()` | tab, column and filter definitions |
| `crawl_open(path)` / dialog | open a crawl file for viewing (D14) |
| `view_open(path, tab, filter, search, sort)` | first screen + view id + version |
| `view_rows(view, from, count)` | a page of rows by position |
| `view_locate(view, ids[])` | current positions of anchor and selected rows |
| `view_counts(path, tab)` | filter counts for one tab |
| `view_close(view)` | drop its temp table |
| `url_detail(path, id)`, `url_list(path, id, kind, from, count)` | detail sections and their paged lists |
| `copy_rows(view, ranges, columns)` | TSV text with full values, ≤ 10,000 rows |
| `export_start(view, columns, dest)`, `export_cancel(job)`, event `export-progress` | CSV export |
| `scope_test(config, start_url, urls, fetch_robots)` | deferred from spec 003 T023 |

Rows travel as arrays of cell values (not objects) to keep a 100-row page
under about 50 KB of JSON. Contracts in
[contracts/desktop-commands.md](./contracts/desktop-commands.md).

### D10. Detail pane: lazy sections, paged lists

Response details load with the selection (one indexed row read plus counts;
SC-006 300 ms holds even for a URL with 1M inlinks: the count over the
`links_to` index measured 45 ms). Inlinks, Outlinks, Images and Headings
load when their section opens, paged through the same windowing component.
Redirects follow the stored `redirect_to` chain up to the hop limit and flag
loops; when spec 005 stores hops explicitly the section reads those instead.
Raw HTML shows "not stored by this crawl: page extraction arrives with spec
005" until then, and later streams in chunks (Edge case over 5 MB).

### D11. CSV export: snapshot, temp file, rename

Export runs on its own thread with its own read transaction, which in WAL
mode *is* a snapshot: rows committed after the export started are not seen
(FR-030) and the crawl keeps writing. It writes `<name>.csv.part` next to
the destination, then renames on success; cancel or failure deletes the
part file (no partial file). UTF-8 with a byte order mark (so Excel opens
non-Latin text correctly), RFC 4180 quoting, CRLF line ends, and values
starting with `=`, `+`, `-`, `@`, tab or carriage return prefixed with `'`
(OWASP CSV injection guidance). Progress events every 250 ms. The CLI's
`spiderlens export` uses the same writer and the same tab definitions
(FR-031). Target: 1M rows in under 60 s (SC-009); expected well under it,
the read side streams at hundreds of thousands of rows per second.

### D12. Copy

Copy asks Rust for TSV of the selected positions (full values, header row,
FR-026). Over 10,000 rows the UI does not call it and offers "Export
selection to CSV" instead (FR-027). Single-cell copy uses the cell's full
value from the cached row.

### D13. Progress area: close the gaps

Most of US1 exists (status bar, overview, pause/resume/stop). Missing:

- **Elapsed time across quit and reopen (FR-004)**: today it lives in UI
  memory. The engine will store active crawl milliseconds in the crawl
  file's `meta`, updated with each commit batch, and report it in progress
  events. Cost: one tiny write per 200 ms batch.
- **Completeness summary by error class (FR-005)**: add failures by error
  class to the summary.
- **Rolling speed window**: spec 008 (approved later) set 5 s; spec 007
  FR-003 says 10 s. Proposal: amend FR-003 to 5 s so both specs agree. The
  average speed for the whole crawl already exists.

### D14. Opening a saved crawl, minimal

The results interface must work the same for a reopened file (FR-033,
US1-4). This plan adds only "Open crawl…" (command palette and ⌘O): pick a
file, show its overview and tabs, Resume if it was paused. Recent crawls,
Save As, file association, recovery and locking remain spec 006.

### D15. Where the table lives in the window

The sidebar's result tabs (disabled today) become live once a crawl exists,
each showing its URL count. Overview stays the progress page. Choosing a tab
shows the filter strip and the table in the main area, with the inspector on
the right. Per-tab filter, search and sort are remembered for the session
(FR-014); column layout across restarts (FR-017).

## Project structure

```
crates/results/            new: spiderlens-results
  src/tabs.rs              tab, column, filter definitions (D2, D3)
  src/sql.rs               ci_contains, natural collation, norm_ws (D7)
  src/view.rs              open, first screen, materialise, rows, locate (D4, D5)
  src/counts.rs            filter counts and duplicate sets (D6)
  src/detail.rs            URL detail sections and paged lists (D10)
  src/export.rs            CSV and TSV writers, export job (D11, D12)
crates/engine/             active_ms meta, error-class summary (D13)
crates/cli/src/export_cmd.rs   spiderlens export (FR-031)
crates/test-fixtures/      issues site; gen-crawl (synthetic 1M crawl file)
app/src-tauri/src/results.rs   commands, reader thread, export jobs (D9)
ui/src/results/            ResultsView, Grid, windowing, FilterStrip,
                           ColumnMenu, usePagedRows, Inspector sections
```

## Delivery

| Step | What Ryware can see |
|---|---|
| 1 (US1) | Tabs in the sidebar; Internal, External, Response codes, URL, Page titles fill live during a crawl; Open crawl… |
| 2 (US2) | Filters with counts, sort, search, column menu |
| 3 (US3) | Inspector with response, redirects, inlinks, outlinks |
| 4 (US4) | Copy, Export CSV, `spiderlens export` |
| 5 (US5) | Measured 1M-row and live-crawl numbers, recorded in research.md |
| After spec 005 | Meta description, H1, H2, Images, Canonicals, Directives tabs and the remaining detail sections light up; their acceptance tests are enabled then |

Each step is a reviewable PR into the `007-results-ui` branch line, or one
PR per step into main, as Ryware prefers.

## Risks

- **1M sort on a slower disk**: the first-screen fast path covers SC-004;
  the full order may take 3–4 s on a slow laptop, affecting only deep jumps
  right after a sort. Measured on the Mac in T040.
- **Live 1M crawls**: refresh backs off (D5), so rows can lag more than 2 s
  at that size. SC-007 is defined on the 100k site, where refresh is about
  0.1 s.
- **WebKit (macOS, Linux) scroll performance** with the scaled scroller:
  spec 017's spike showed 31 ms p95 frames on WebKitGTK at 5M rows; checked
  again on the real grid in T040.
