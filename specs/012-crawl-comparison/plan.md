# Implementation Plan: Crawl Comparison and Change Detection

**Branch**: `012-crawl-comparison` | **Date**: 2026-10-14 | **Spec**: [spec.md](./spec.md)

## Summary

Compare a previous and a current crawl file without touching either: which
URLs are New, Added, Removed, Missing or Not reached for every results
filter, which tracked elements changed on pages present in both, how the
009 issues moved, and, with URL mapping, how a migration went. Same numbers
in the desktop app and in `spiderlens compare`.

The comparison is a **workspace**: a temporary SQLite file on disk that
holds one compact row per URL per side and the match between them. Each
crawl file is opened read-only as the `main` schema of its own connection,
exactly as the results reader opens it today, so every tab, filter, column
and rule definition from 007 and 009 runs unchanged; the workspace is
attached to that connection as `cmp`, and each side writes its rows into it
with one `INSERT … SELECT`. Nothing crosses into Rust row by row and nothing
is held in memory. After both sides are in, every count, list and diff is a
query over `cmp` alone.

## Technical Context

- Rust 2024, rusqlite (bundled SQLite), Tokio, Tauri 2, React 18 +
  TypeScript, Tailwind v4 tokens, Base UI components (spec 017), ts-rs.
- New crate: `crates/compare` (`spiderlens-compare`). Depends on
  `spiderlens-results` (tab definitions, SQL functions, reader connection
  setup) and `spiderlens-config` (comparison settings). New dependency:
  none (the workspace already has `regex`, `tempfile`, `serde_json`).
- Storage: crawl files (spec 006) read only; the workspace is a file in the
  OS temp folder, deleted on close, cancel or the next start.
- Testing: Rust unit and integration tests on fixture crawl pairs built by
  `crates/test-fixtures`, CLI tests, Vitest and Playwright with the fake
  transport, a 100,000 and 1,000,000 URL benchmark built with the existing
  `gen_crawl` example.

## Constitution Check

| Principle | How this plan complies |
|---|---|
| I Spec before code | Spec approved 2026-10-13 (Ryware), with the six recommended decisions. |
| II Local-first | Two local files in, one temporary local file used, nothing fetched (FR-036). |
| III Engine independent of the shell | All comparison logic lives in `spiderlens-compare`; the app and the CLI call the same functions (FR-029, SC-005). |
| IV Respectful crawling | Not affected: no network. |
| V Correct and complete data | Crawl files are opened with SQLite `mode=ro`, so a write is impossible, not just avoided (SC-004 is a test). Data a file lacks is "not available", never "changed" (Edge case). Incomplete crawls produce "Not reached", not Missing (FR-015). |
| VI Scales by design | One row per URL per side on disk, counts as single SQL passes, lists through on-disk view tables like 007. Memory is SQLite's bounded page cache per connection. Performance impact on crawling: none, the comparison never runs in the crawl process path and only reads files (FR-037, SC-008 is measured). |
| VII Test-first | Every acceptance scenario maps to a test in quickstart.md; fixture pairs with known changes per scenario. |
| VIII Simplicity | Filters, columns and rules are reused as SQL text from 007 and 009, not reimplemented. One crate, no new dependency, no schema change to crawl files. |

## Design decisions

### D1. A workspace file, crawl file as `main`, workspace attached as `cmp`

Each side gets a connection opened on `file:<crawl>?mode=ro` (read-only
URI) with the results crate's SQL functions registered, and then
`ATTACH '<workspace>' AS cmp`. Because the crawl is `main`, every existing
SQL fragment (`urls u`, `pages p`, `temp.redirects`, 009's
`temp.issue_hits`) resolves unchanged. Writes go only to `cmp.*`; a write to
`main` fails with "readonly database" (verified in research R1). The two
sides fill the workspace one after the other; the diff phase uses either
connection and reads only `cmp`.

Rust analogy: two shared borrows (`&Crawl`) and one owned scratch buffer
(`Vec` on disk); there is no `&mut` to either crawl anywhere in the type
signatures, since the connection itself cannot write them.

Rejected: streaming rows from two readers into a third connection (moves
millions of rows through Rust), or attaching both crawls to one connection
(every 007 and 009 fragment would need a schema prefix, a second copy of
the definitions, which FR-012 forbids).

### D2. One compact row per URL per side

`cmp.side_rows(side, id, url, key, …)` (data model) holds for each URL of a
crawl: its id in that file, its URL, its match key (URL after mapping on
the previous side, the URL itself on the current side), flags (internal,
HTML OK, reached), the **filter bits** and the **tracked element values**.
It is filled by one `INSERT INTO cmp.side_rows SELECT … FROM urls u` per
side. Element values reuse the 007 column expressions (`c::TITLE.expr`,
`c::INDEXABILITY.expr`, …) where one exists, and a few new expressions for
sets and fingerprints (D5).

### D3. Filter membership as bit columns

Every (tab, filter) pair of the registry gets a fixed bit position for this
comparison. A side stores `fb0, fb1, …` INTEGER columns of 62 bits each,
computed in the same SELECT as
`((eligible AND cond) << k) | …`. About 110 filters fit in two columns.
Counts for all filters and all four groups (New, Added, Removed, Missing)
plus Not reached are **one pass** over the match table with generated
`SUM((fb >> k) & 1 …)` expressions. A group list ("Added to Page titles
› Missing") is a 007-style view over `cmp.matches` with the bit test as its
condition. Filters a file cannot evaluate (`Need` not available) are
reported as unavailable on that side, and their deltas as "not
comparable", never as changes.

Derived tables a filter reads (`temp.occ_title` and the others, plan D6 of
007) are built on each side's connection before its SELECT, by the same
`build_derived`.

### D4. Matching

`cmp.matches(prev_id, cur_id, key)` is built with two index joins on
`side_rows(side, key)`: matched pairs, previous-only, current-only.
Collisions (two previous URLs with one key) keep both rows and are counted
(FR-009). Groups per URL:

- **New**: current only. **Missing**: previous only, and the current crawl
  did not record the URL at all. **Not reached**: previous URL whose
  counterpart is in the current file but still queued, or was skipped by a
  limit (`reason` in the limit set), or is absent while the current crawl
  ended early (stopped, failed, limit reached) and the URL's previous depth
  is beyond the current crawl's deepest finished level. The last rule is a
  heuristic and is labelled "Not reached (crawl incomplete)".
- **Added / Removed** per filter: matched, bit 0→1 / 1→0.

### D5. Tracked elements and thresholds

Elements are a static list like 007 tabs: `id`, label, `expr` (SQL over
`urls u` on a side), `kind` (text, set, int, fingerprint), default
threshold, `need`. Comparison in the workspace is SQL: text with `norm_ws`
and case-sensitive equality (FR-017), sets stored as sorted `\x1f`-joined
strings so set equality is string equality, numbers with
`cthr('<element>')`, a registered threshold function (009 D3 pattern), so
changing a threshold re-runs only a count query, never the side scans
(FR-019). Structured data: sorted list of top-level `@type` values from
JSON-LD and microdata blocks plus a 64-bit hash of the blocks' raw text;
content: `pages.text_hash` (spec 005). Response time is in the list, off by
default (FR-020).

### D6. Issue deltas through 009's evaluator

On each side connection, after the side rows, call 009's evaluation with
the chosen issue settings (FR-023) into `temp.issue_hits`, then copy
`(rule stable id, url id)` into `cmp.issue_hits(side, rule, id)`. Newly
affected, fixed and gone are joins through `cmp.matches` (story 3).
Analysis passes (009 D12) the rules need are taken from the file when its
`analysis` meta matches the current pass versions, otherwise run into that
connection's `temp` schema first (FR-024a). Rules unavailable on either
side are "not comparable" (FR-025). **Dependency**: needs 009's evaluator
to accept a connection plus settings (asked of the 009 thread; its plan
already keeps evaluation on the reader connection, so this is a function
signature, not a design change). Until 009 code lands, the Issues view
says "Issues arrive with the issue rules feature".

### D7. URL mapping

Rules are an ordered list of `{ find, replace, regex: bool }`. Compiled once
in Rust (`regex` crate, the same as 002 patterns), exposed to SQL as
`map_url(url)` on the previous side's connection only, first matching rule
wins (FR-006). The pre-filled host rule (FR-007) is a plain-text rule
`https://staging.example.com/` → `https://www.example.com/`, built from the
two crawls' start URLs. The mapping tester (FR-008) calls the same compiled
rules on sample URLs and looks the results up in the current file
(`SELECT id, state FROM urls WHERE url = ?`), no workspace needed.

### D8. Progress, cancel, partial results

Phases: open (validate both files, read meta, configuration diff), side
previous, side current, match, counts. Each side's SELECT runs in batches
of 50,000 URL ids (`WHERE u.id BETWEEN ? AND ?`) so progress and cancel are
checked between batches, and SQLite's `interrupt` stops a running batch.
The Overview summary (FR-014) is available after "open"; filter counts
after "counts" (SC-002 targets: 5 s summary, 30 s full on 100,000 URLs).
Cancel deletes the workspace file.

### D9. Comparison settings

`CompareSettings { mapping: Vec<MapRule>, elements: Vec<ElementSetting>,
issue_settings: IssueSettingsChoice }` in `spiderlens-config`, serde JSON,
validated like profiles (regex compiles, thresholds in range). The app keeps
the last used settings in its settings store and can save or load a
`.spiderlens-compare.json` file; the CLI takes the same file with
`--settings` (FR-028, story 5 scenario 5).

### D10. Configuration differences

Both files' stored `config` meta (spec 002 `StoredCrawlConfig`) is diffed
field by field over a fixed list of fields that change what is collected
(mode, scope rules, subdomains, robots mode, user agent, rendering,
limits, external checks) and shown at the top of the Overview (FR-003,
FR-014). JSON field paths, not a generic JSON diff, so labels are readable.

### D11. Schedule baseline (FR-002a)

`Baseline::PreviousScheduleRun` resolves through 021's lookup (schedule id
from the current crawl's stored info, most recent earlier Finished or
Stopped-at-limit run in the schedule's output folder). The compare crate
takes a resolved path; the resolver lives with 021 and is called by the
CLI and the app. Until 021 code lands, the option is hidden in the app and
the CLI reports "scheduled crawls are not available in this build".

### D12. Report section (FR-033)

`ComparisonReport` (serde, ts-rs) holds exactly the section's content:
both summaries, config differences, totals by final state with change,
New / Missing / Not reached, top ten filters by absolute change, issue
deltas by priority, changed pages per element with up to 20 example URLs,
mapping rules. `spiderlens compare --json` prints it; 010 renders it.

### D13. Exit code

`spiderlens compare` failure conditions (FR-032) exit with a new
`COMPARE_FAILED` code, taken at merge time as the next free number
(expected 8: 006 took 6, 009 expects 7), as 009 D11 does.

### D14. Desktop

A comparison opens in the main window as a **comparison mode**: the
sidebar shows Overview, the result tabs (each with its filters and group
counts), Change detection, Issues and, with mapping, Migration. The table
reuses the 007 Grid with paired columns (previous | current) for the
tab's main values; the inspector shows the two URLs' details side by
side. Entry points: start screen card "Compare crawls", command palette,
File › Compare with…, and "Compare with…" on an open crawl (FR-001). Setup
is a sheet (017 pattern): two file pickers fed by the recent list (006),
swap, both crawls' info, configuration differences, mapping editor with
tester, Start. A comparison runs on its own thread with its own two
connections; the crawl thread is untouched.

## Project structure

```text
crates/compare/
  src/lib.rs          Comparison, open/run/cancel, public API
  src/workspace.rs    workspace file, side connections, ATTACH, cleanup
  src/side.rs         side_rows fill: filter bits, element values, batches
  src/registry.rs     filter bit positions from results::tabs()
  src/elements.rs     tracked elements, thresholds, cthr()
  src/matching.rs     map_url(), matches, groups, Not reached
  src/counts.rs       overview counts, element counts, issue deltas
  src/view.rs         group lists, paging, export (reuses results export)
  src/detail.rs       previous and current URL detail side by side
  src/config_diff.rs  configuration differences
  src/report.rs       ComparisonReport (FR-033)
crates/config/src/compare.rs   CompareSettings, MapRule, validation
crates/cli/src/compare_cmd.rs  spiderlens compare
crates/test-fixtures/src/compare.rs  fixture site v1/v2 and migration pair
app/src-tauri/src/compare.rs   commands and events (contracts/desktop-commands.md)
ui/src/compare/               setup sheet, comparison mode, views, tests
ui/src/testing/fakeCompare.ts  fake transport data
```

## Performance

- Crawl speed: unaffected; no code in the engine or the crawl runner
  changes. SC-008 is measured by running the 008 benchmark while a 1M
  comparison runs.
- Comparison cost: two sequential scans (one per side) with the filter
  conditions and element expressions, plus index joins. Targets from
  SC-002; measured on gen_crawl files in T029 and recorded in research.md.
- Memory: two read-only connections plus the workspace connection with
  bounded page caches (32 MB each, as the results reader), temp tables on
  disk. SC-003 measured in T029.
- Disk: the workspace is about 200 bytes per URL per side (≈400 MB for two
  1M crawls); the setup warns when free space in the temp folder is below
  twice the estimate.

## Delivery

| PR | Content |
|---|---|
| This docs PR | plan, research, data model, contracts, quickstart, tasks |
| Code PR 1 | compare crate, settings, CLI, desktop mode with UI tests: stories 1, 2, 4, 5, 6 |
| Follow-up with 009 code | issue deltas (story 3), analysis versions (FR-024a), `--fail-on-new-issues` |
| Follow-up with 010 / 021 code | report rendering hook, schedule baseline |

## Complexity tracking

None. No new dependency, no schema change to crawl files.
