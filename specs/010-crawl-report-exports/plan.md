# Implementation Plan: Crawl Report and Exports

**Branch**: `010-crawl-report-exports` | **Date**: 2026-10-14 | **Spec**: [spec.md](spec.md)

## Summary

The report is a value computed on the read side, in the `spiderlens-results` crate that the app and the CLI share: an ordered list of sections built from the crawl file and the live issues of spec 009 (`temp.issue_hits`). The health score is a streaming pass over those hits. One `Report` value feeds four renderers: the React Report view, the CLI text and JSON, a self-contained HTML file, and (through the HTML) a PDF. Bulk exports stream named tables from one read snapshot into a CSV folder or one Excel workbook. Nothing runs in the fetch path, nothing is stored in the crawl file, so there is no migration step.

## Technical Context

- **Language**: Rust 1.97 (workspace), TypeScript 5 with React 19 (UI).
- **New dependency**: `rust_xlsxwriter` (pure Rust, MIT/Apache, constant-memory mode streams rows to disk). Justified in research.md R1; no other new dependency.
- **Storage**: the crawl file (SQLite, spec 006), read through the results `Reader` (WAL snapshot). Temporary tables only.
- **Depends on**: spec 009 code (PR 52: `issues` module, `temp.issue_hits`, `temp.issue_rules`, `IssuesSummary`, `rule_infos`). The code PR is stacked on that branch and retargets to `main` when 009 merges.
- **Where**: `crates/results/src/report/` (model, sections, score, html), `crates/results/src/bulk/` (tables, CSV folder and xlsx sinks), `crates/cli/src/report_cmd.rs`, `app/src-tauri/src/report.rs`, `ui/src/report/`.
- **Testing**: Rust unit and integration tests on the report fixture crawl and synthetic crawls (`results::synth`), CLI tests, Vitest and Playwright (fake transport), HTML checked offline in Chromium by Playwright.
- **Performance goals**: SC-002 (report 3 s after final issues at 100k, 10 s at 1M), SC-003 (report file under 10 s, under 10 MB), SC-004 (bulk Excel 60 s at 100k, all links CSV 120 s at 10M links), SC-005 (flat memory), SC-006 (crawl speed at least 95%).

## Constitution Check

| Principle | How this plan meets it |
|---|---|
| I Spec before code | Spec approved 2026-10-13 ("מאושר בכל"). |
| II Local first | Report and exports are computed and written on the user's machine; the HTML file references nothing external; PDF goes through the OS print system. |
| III Engine independent of the shell | Report, score, HTML and bulk exports live in `spiderlens-results`; the app and CLI are thin callers. PDF is the one shell-only step (it needs a web view), and it renders the same HTML the CLI writes. |
| IV Respectful crawling | No fetching. |
| V Correct and complete data | Completeness is a section of its own and leads an incomplete report (FR-007); unavailable sections and rules say why (FR-002, FR-012); the score is marked partial when rules could not run. |
| VI Scales by design | Every figure is a SQL aggregate; the score streams hits in `url_id` order holding one page at a time; exports stream rows to disk. Performance impact below. |
| VII Test first | Every acceptance scenario maps to a test in [tasks.md](tasks.md). |
| VIII Simplicity | Sections are values in a static list, like 007 tabs and 009 rules. Bulk tables reuse 007 tab definitions and the 009 issue view. One new dependency (xlsx), justified. No schema change. |

## Design decisions

**D1. The report is one value, rendered four ways.** `Report { meta, completeness, score, sections: Vec<Section> }` is a serialisable struct (ts-rs exported). A `Section { id, title, state: Available | Unavailable(reason) | Provisional, blocks: Vec<Block> }` holds blocks of four kinds: `Figures` (label, value, optional `Target`), `Table` (columns, rows, each cell optionally a `Target`), `Bars` (label, value, share, optional `Target`), `Text`. A `Target` is `{ tab, filter, scope }` or `{ issue }`, which the UI opens in the results table (FR-004) and the HTML renders as plain text. In Rust terms this is a small document AST, like a JSON schema the compiler checks; renderers are pure functions over it, so the Report view, the CLI JSON and the HTML file cannot disagree (FR-005, SC-001).

**D2. Sections are registered, not hard-coded (FR-002).** `static SECTIONS: &[SectionDef]` with `SectionDef { id, title, needs: &[Need], build: fn(&mut Reader, &Ctx) -> Result<Section> }`, the same pattern as 009's `catalogue()` and the analysis `PASSES`. Specs 011, 012, 014, 015 and 016 add a file and one line. A section whose `needs` the file lacks returns `Unavailable` with the 007 `Need::unavailable_reason`; a section whose builder fails returns `Unavailable` with the error, never disappears. First set: summary, completeness, issues by priority, top issues, response codes, site structure, content (FR-003).

**D3. The score streams the hits (FR-008 to FR-012).** One query, `SELECT h.url_id, r.prio, h.rule FROM temp.issue_hits h JOIN temp.issue_rules r ON r.idx = h.rule JOIN urls u ON u.id = h.url_id WHERE <Pages scope> ORDER BY h.url_id`, walked in Rust one page at a time (memory is one page's hits). Weights: High 1.0, Medium 0.5, Low 0.2, Opportunity 0. Per page: penalty = weight of its worst hit. Score = round(100 × (1 − Σ penalty / N)), N = count of pages in the Pages scope (from the 007 count, not from hits, so clean pages count). Two per-issue numbers come from the same walk:
- **share**: each page's penalty split equally among the issues at its worst priority; the shares add up exactly to the points lost, per priority and in total (US2 scenario 2, SC-008). This is the breakdown.
- **gain**: penalty minus the penalty without that issue (the next-worst weight on the page); the points fixing that issue alone would return (US2 scenario 3). Top issues are ordered by gain.
`SCORE_VERSION = 1` is written into the report and the CLI JSON. When N = 0 the score is `None` with a reason (FR-012). When any enabled rule is unavailable on this file, `partial = true` and the breakdown lists those rules.

**D4. "Site-wide issue" in FR-010 means an issue whose result is not a set of pages.** Spec 009's `RuleKind::SiteWide` (duplicates, broken links) still produces per-URL hits and those count in the score; they are site-wide only in that their hits can change until the crawl ends. Rules with no page hits at all (none in rule set v1; later examples are a missing llms.txt from spec 015 or a robots.txt problem) are listed under their priority and excluded from the score. The report labels them "Site-level".

**D5. Live, never stored.** `Reader::report(opts)` builds the report from the reader's current snapshot; the Report view calls it on open and on the existing 5 s refresh when `Reader::version()` changed. While running, the report and score are `provisional` (FR-006). Building it reads only aggregates and the hits walk; there is no analysis pass and no table in the crawl file. If a later pass (spec 009 D12) produces data a section needs, the section reads the pass's table; this spec registers no pass.

**D6. The HTML file is rendered in Rust with no template engine.** `report::html::render(&Report, &FileOptions) -> String`: an `Escaper` writer, one `<style>` block, inline SVG for bars, no scripts, no external URL anywhere (a test parses the output and fails on any `src`, `href` to a scheme, `url(` or `@import`). Colours come from the light theme in `ui/src/styles/tokens.css`, read with `include_str!` and converted at build time by a small `build.rs` step into CSS custom properties, so the report follows the design tokens without copying them (FR-017). Every text cell gets `dir="auto"` so Hebrew and Arabic titles read correctly. Print CSS: A4 and Letter safe margins, `break-after: avoid` on headings, `break-inside: avoid` on figure groups, tables repeat their header row. Per-issue URL lists stop at the limit (default 100, 0 to 10,000) with "and N more, see the issues export" (FR-015).

**D7. PDF goes through the operating system's print-to-PDF.** The app writes the HTML to a temporary file, opens it in a hidden Tauri web view window and calls `print()`, which shows the system print dialog with Save as PDF selected where the OS allows it (macOS, Windows "Microsoft Print to PDF", GTK "Print to File"). Reason (research.md R2): a Rust PDF writer would need its own font shaping for Hebrew, Arabic and CJK text, and FR-017 requires those to render correctly; the web view already shapes text with the system fonts. Cost: the user confirms one OS dialog. The CLI writes HTML only (spec Assumptions).

**D8. Bulk tables are data too (FR-018 to FR-021).** `BulkTable { id, label, file_stem, rows: fn(&Snapshot, &mut dyn Sink) -> Result<u64> }`:
- `tabs`: every 007 tab with filter `all` and every column, through the existing `view::compile` path used by CSV export, so values match the single export (FR-020, SC-007);
- `issues`: one table per enabled rule with hits, through the 009 `issues` tab view with the rule's columns and evidence;
- `links`: `links` joined with `urls` twice (source, target) plus the target's status and final state, ordered by source;
- `images`: links of kind `img` with alt, missing or empty alt, and the target's status and size;
- `redirect_chains`: one row per chain from the 007 `redirects` derived table, hops as `url (status)` joined in order, final URL and state;
- `headers`: `json_each(headers.headers)` per URL, one row per header;
- `summary`: the report's figures as label and value rows.
The whole export runs in one read transaction, a WAL snapshot (FR-024).

**D9. Two sinks.** `Sink` has `begin_table(name, columns)`, `row(&[Value])`, `end_table()`. `CsvFolderSink` writes one file per table with the 007 CSV rules (BOM, quoting, formula neutralisation; the existing `csv_field` moves to a shared module). `XlsxSink` uses `rust_xlsxwriter` in constant-memory mode: an Overview sheet first (table, rows, and for issues priority, why, fix, linked to the sheet), numbers as numbers, the header row frozen, sheet names cut to 31 characters and made unique, continuation sheets `name (2)` after 1,048,575 data rows, cells over 32,767 characters cut with a trailing marker and counted in the overview. Excel cells are written as string cells, which a spreadsheet never evaluates, so text is stored exactly as in the app and FR-023 holds without the CSV quote prefix.

**D10. Atomic output.** Files and folders are written under a sibling `.<name>.partial` path and renamed into place at the end; cancel or error removes the partial path (FR-024). Existing targets are refused unless the caller passes `overwrite` (the app asks first, the CLI has `--force`) (FR-025). Progress is reported per table and per 10,000 rows through a callback; cancel is an `AtomicBool` checked per batch, as 007 export does.

**D11. CLI.** `spiderlens report` keeps its current list and summary behaviour; `--audit` prints the 010 report (text, or `--json` for the `Report` value) and `--html <out>` writes the report file with `--title`, `--note`, `--urls-per-issue`. `spiderlens export` gains `--bulk <tables>` (comma separated or `all`), `--format csv|xlsx` and `--out <path>`. Issue settings follow 009 D9 (`--issues <file>` or `--profile`, else the file's stored settings, else defaults). Exit codes are the existing ones (contracts/cli.md).

**D12. App and UI.** Tauri commands `report`, `export_report_html`, `print_report`, `bulk_export`, `cancel_export` with `export_progress` events (contracts/desktop-commands.md). UI in `ui/src/report/`: `ReportView` (score card with partial and provisional badges, breakdown, then each section by block kind), `ExportReportDialog` (format, title, note, URL limit), `BulkExportDialog` (table checklist, format, destination, progress, cancel). The sidebar gains a Report entry above the results tabs; when a crawl ends a toast offers "View report"; app setting `openReportOnFinish` (default off) switches automatically (FR-031). Score colours use new tokens `--score-good`, `--score-fair`, `--score-poor` in `tokens.css`, checked by the existing contrast test, always shown with the number and a word ("Good", "Needs work", "Poor") so colour is never the only signal (FR-032).

## Structure

```text
crates/results/src/
  report/
    mod.rs        Report, Section, Block, Target, ReportOptions; Reader::report
    sections.rs   SECTIONS and the first seven builders
    score.rs      Score, ScoreBreakdown, SCORE_VERSION, the streaming walk
    html.rs       render(&Report, &FileOptions); escaping; print CSS
  bulk/
    mod.rs        BulkTable, BULK_TABLES, run_bulk(opts, progress, cancel)
    sink.rs       Sink, CsvFolderSink, XlsxSink, atomic paths
  csvfmt.rs       csv_field and neutralise, shared by export.rs and bulk
crates/results/build.rs        light-theme tokens from ui/src/styles/tokens.css
crates/cli/src/report_cmd.rs   report --audit / --html
crates/cli/src/export_cmd.rs   export --bulk / --format
app/src-tauri/src/report.rs    commands, hidden print window, progress events
ui/src/report/                 ReportView, ScoreCard, SectionView, ExportReportDialog, BulkExportDialog
crates/test-fixtures/src/report.rs   report fixture site (known score, codes, depths)
```

## Performance impact (constitution VI)

- **Crawl write path**: unchanged. The report and exports read through the reader connection in WAL mode and never block the writer. The SC-006 benchmark runs the 100,000-page fixture with the Report view refreshing every 5 s and a bulk export running.
- **Report build**: about ten aggregate queries (most already computed for 007 counts) plus one walk over the hits of pages; on the 100k fixture expected well under 1 s after 009's final evaluation; 1M synthetic file measured in the benchmark task.
- **Exports**: sequential scans in index order, written through a buffered writer; xlsx in constant-memory mode keeps one row in memory. 10M links: SQLite returns about 1 to 2 million rows per second, so the 120 s target leaves room for the CSV encoding.
- **Memory**: one page's hits during the score walk, one row during export, the `Report` value itself (bounded: per-issue URL lists are capped by the limit; at most 10 top issues).

## Testing

- **Score**: unit tests for the formula, shares adding up, gain equal to the score change after removing an issue, N = 0, partial (SC-008, US2).
- **Sections**: each builder against the report fixture crawl; figures equal to 007 counts and 009 issue counts (FR-005, SC-001); unavailable sections on old fixture files (US6).
- **HTML**: no external references, every section present, escaping of hostile titles, RTL markers, size under 10 MB at 100k (SC-003); Playwright opens the file offline in Chromium and checks the text.
- **Bulk**: each table against the single export of the same tab or issue (SC-007), Excel round-trip read with `calamine` as a dev-only dependency, continuation sheets with a tiny row limit injected in tests, cancel and failure leave nothing, overwrite refused.
- **CLI**: `report --audit` text and JSON equal to the app command; `--html` and `export --bulk` parity with the app; exit codes.
- **UI**: Vitest for ReportView (blocks, links open the right tab or issue, badges), dialogs (validation, progress, cancel), the sidebar entry and the finish toast; Playwright click-through from score to top issue to rows, and export dialogs with the fake transport.
- **Bench**: SC-002, SC-004, SC-005, SC-006 on the 100k fixture and the 1M synthetic file, recorded in `benchmarks.md`.

## Coordination

- **Spec 009 (PR 52)** owns the issues, `temp.issue_hits`, rule texts and the analysis runner; this code is stacked on it. If 009's data shapes change before merge, only `score.rs` and the issues section move.
- **Spec 006 (PR 27)** owns read-only opening; reports on view-only files use the same reader and write nothing (US6 scenario 2).
- **Spec 012** compares scores by `SCORE_VERSION`; **specs 011, 014, 015, 016** add sections to `SECTIONS`.
- **No migration step and no new exit code.**

## Implementation notes

- Report file tokens are read from ui/src/styles/tokens.css with `include_str!` at compile time, so no build script is needed.
- Score colours use the existing meaning tokens (`--ok`, `--warn`, `--bad`) instead of new score tokens.
- The CSV quoting and formula guard stay in export.rs (`pub(crate)`) and are shared by the bulk sinks.
- CLI issue settings: `--issues FILE` (rule overrides JSON) or defaults; `--profile` waits for spec 009's stored settings.
