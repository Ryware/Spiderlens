# Implementation Plan: Sitemaps and Orphan Pages

**Branch**: `011-sitemaps-orphan-pages` | **Date**: 2026-10-14 | **Spec**: [spec.md](spec.md)

## Summary

The engine already fetches and parses sitemaps (spec 003) but keeps only the `<loc>` list in memory and one summary row per file. This feature changes three things:

1. **Write side (engine)**: the sitemap reader becomes a streaming entry parser that records every entry (location, `lastmod`, `changefreq`, `priority`, images, hreflang alternates) in batches through the existing writer, plus richer per-file rows. New sources (default locations, URLs the user adds, local files, a sitemap as a List mode source) feed the same reader. The setting splits into "Read sitemaps" and "Crawl URLs found only in sitemaps".
2. **Read side (`spiderlens-results`)**: two Crawl Analysis passes (spec 009 plan D12), `sitemap_audit` and `orphans`, write their own tables after the crawl; the Sitemaps rules join the spec 009 catalogue as one rule file; a `sitemaps` results tab, a Sitemap files view and an inspector section read them.
3. **Generation (`spiderlens-results`)**: a streaming sitemap writer that selects pages with one SQL query, writes files in URL order and splits by count and size, shared by the app and the CLI.

Nothing new runs in the page fetch path. The only crawl-time cost is reading the sitemap files themselves and storing their entries, measured by SC-002 and SC-003.

## Technical Context

- **Language**: Rust 1.97 (workspace), TypeScript 5 with React 19 (UI). No new crates. `quick-xml` and `flate2` move from engine-only to workspace dependencies used also by `spiderlens-results` (writer side). `time` is already in the tree through `rusqlite`; W3C datetime parsing is a 40-line function (research R3), not a new dependency.
- **Storage**: the crawl file (SQLite, spec 006). One migration step adds three tables and the pass outputs (number taken at merge, Storage rule). Reads through `CrawlStore::open_read_only` and the results `Reader`.
- **Where**: `crates/engine/src/crawl/sitemap.rs` (parser, sources, ingestion), `crates/engine/src/crawl/runner.rs` (wiring), `crates/engine/src/crawl/store.rs` (writes), `crates/engine/src/crawl/migrate.rs` (step), `crates/config/src/model.rs` (settings), `crates/results/src/sitemaps/` (passes, views, generation), `crates/results/src/issues/rules/sitemaps.rs` (rules), `crates/cli/src/sitemap_cmd.rs`, `app/src-tauri/src/results.rs`, `ui/src/sitemaps/`, `ui/src/settings/`.
- **Testing**: parser unit tests and property tests; a sitemap fixture site in `crates/test-fixtures` with every seeded problem of User Stories 1 to 4 and 6; one test per acceptance scenario; generated files checked by a structural validator in Rust on all OSes and by `xmllint --schema` against the bundled protocol XSDs in the Linux job; Vitest and Playwright with the fake transport; the 1,000,000-entry benchmark.
- **Performance goals**: SC-002 to SC-005 (unchanged crawl with sitemaps off, at most 2% slower with them on; 1M entries stored in 30 s and 50 MB peak; passes in 20 s for 1M URLs; generation of 1M pages in 60 s).

## Constitution Check

| Principle | How this plan meets it |
|---|---|
| I Spec before code | Spec approved 2026-10-13. |
| II Local first | Sitemaps are fetched from the crawled site only; local files are read from disk. No other network use. |
| III Engine independent of the shell | Ingestion is in the engine; passes, rules, views and generation are in `spiderlens-results`, used by both the app and the CLI. |
| IV Respectful crawling | Sitemap requests use the crawl's user agent, honour robots.txt for the sitemap host and count against the per-host rate limit like any other request. The default locations are tried only when robots.txt lists none. |
| V Correct and complete data | Every entry is kept with its raw text; every file records its outcome; rules on unfetched URLs say "not checked"; older files say "not available". |
| VI Scales by design | Entries stream into the file in batches of 1,000; the parser holds one file's bytes at most (the 50 MB protocol limit); passes and generation are SQL scans that write as they go. Impact numbers below. |
| VII Test first | Each acceptance scenario maps to a task with its test in [tasks.md](tasks.md). |
| VIII Simplicity | One parser for every source, one migration step, two passes on the shared phase, rules in the shared catalogue, the Sitemaps tab as a 007 `TabDef`. No new crate, no new dependency. |

## Design decisions

**D1. One streaming parser, one ingestion path.** `sitemap::parse` today returns `Vec<String>`. It becomes `sitemap::read(bytes, &mut impl FnMut(Entry)) -> Result<FileFacts, FileError>`, where `Entry { pos, kind: Url | Sitemap, loc_raw, lastmod_raw, changefreq, priority, images: Vec<String>, alternates: Vec<(lang, href)> }` and `FileFacts { root, compressed, bytes, entries, line_of_error }`. Think of it as a SAX callback rather than building a DOM: memory is the file bytes plus one entry. The runner sends `Msg::SitemapEntries { file, batch }` every 1,000 entries, so the writer thread (the only one that touches SQLite) stores them in its normal batched transaction (spec 008, 200 ms commits). Every source (robots.txt, default locations, user URLs, local files, List source) produces a `SitemapJob { location: Remote(Url) | Local(path, base_url), source, parent, level }` processed by the same loop.

**D2. Entry URLs are matched at ingestion, not later.** Each entry's `loc` is normalized with the crawl's `ScopeRules::normalize` (spec 003) and passed to `discover()` as today, which returns the `url_id` (existing row or new one). The entry row stores that id, so every later question ("which sitemaps list this URL", "is this sitemap URL linked") is an indexed join, never a string match over a million rows. Entries whose `loc` fails normalization store `invalid` with the reason and no id.

**D3. Storage: three new tables, the old one kept.** The base `sitemaps` table (url, parent, url_count, error) keeps being written so an older SpiderLens can still read the file (additive step, spec 006 D4). New: `sitemap_files` (one row per file with source, status, final URL, compression, size, kind, counts, error class, error line, read time), `sitemap_entries` (one row per entry, D2), `sitemap_refs` (images and hreflang alternates per entry). Pass outputs: `sitemap_problems` and `orphans`. Details in [data-model.md](data-model.md). A new `data_features` id `sitemap_entries` tells the reader which files have the data (results `Need::SitemapEntries`); older files fall back to the base table for the Sitemap files view and report rules as unavailable (US6 scenario 4).

**D4. The setting split, with old profiles mapped.** `ScopeConfig.use_sitemaps: bool` becomes `ScopeConfig.sitemaps: SitemapSettings { read, crawl_sitemap_only, urls: Vec<String>, files: Vec<LocalSitemap { path, base_url }> }`. Deserialization accepts the old `use_sitemaps` key and maps it to both flags (FR-001), the same `#[serde(alias)]`-plus-`From` pattern spec 002 uses for profile versions. Defaults: both flags on for new profiles (spec Assumptions). Profiles and crawl files that have the old key keep its value, so no stored crawl changes behaviour on resume. Paths of local files are stored with the crawl (constitution V) but never their content beyond the entries.

**D5. "Found only in sitemap" is a new not-crawled reason that can upgrade.** When `crawl_sitemap_only` is off, `discover()` records sitemap-sourced internal URLs with `NotCrawledReason::SitemapOnly`; `can_upgrade()` returns true for it, so the first followed link to that URL queues it (the same path `Nofollow` and `OverDepthLimit` use today, spec 003 FR-039). The reason joins the spec 003 reason list at the end, keeping its FR-038 order test.

**D6. Two Crawl Analysis passes (spec 009 D12).**
- `sitemap_audit` (`after: []`, needs `SitemapEntries`): computes the problems that need the whole crawl or Rust logic, into `sitemap_problems(rule, file_id, entry_id, url_id, detail)`: location rule (entry host and path against the file's final URL, cross-host allowed when that host's robots.txt lists the file, research R4), duplicates within and across files, `lastmod` validity and future dates (parsed at ingestion into `lastmod` seconds or `lastmod_invalid`), "all identical", "older than the page" (joins the stored `Last-Modified` header with `header_value()`), index loops and depth, empty files, "no sitemap listed in robots.txt", "no sitemap found". Budget: 2 s per 100,000 entries.
- `orphans` (`after: []`, needs `SitemapEntries`): classifies every internal URL that has a non-link source (`in_sitemap = 1` now; spec 024 adds sources) into `orphans(url_id, class, sources)`. "Linked" is computed in SQL: followed `anchor` links (which include `<area>`, spec 005) from crawled internal HTML pages to a different URL, then extended over `redirect` and `meta_refresh` link targets (and spec 014 JavaScript redirects once present) until no row is added, bounded by the crawl's redirect limit. Budget: 3 s per 100,000 URLs. Link Score (023) lists `orphans` in its `after`.
Both write into the schema the phase gives them (`main`, or `temp` for read-only files) and replace their tables in one transaction. Per-URL rules that only compare a sitemap URL with its own crawl data (redirect, error, noindex, canonicalised, blocked, not crawled, not in sitemap) are plain live rules, not pass output, so they update during the crawl like other rules.

**D7. Rules are one catalogue file.** `crates/results/src/issues/rules/sitemaps.rs` adds the 27 rules of the spec table with ids `sitemap.*` in the spec 009 rule shape. Per-URL rules are conditions over `urls u` (for example `u.in_sitemap = 1 AND u.status BETWEEN 300 AND 399`), or `u.id IN (SELECT url_id FROM sitemap_problems WHERE rule = '...')` for pass output; rules that read pass output declare the pass in `need` so they show "available when the crawl ends" while running. File and entry problems with no URL row (invalid `loc`, file errors) are site-wide rules whose affected list is the Sitemap files view filtered by rule; this needs one addition to the 009 rule shape, an optional `detail: Option<&'static str>` naming a view id, agreed with the 009 thread through the coordinator (research R8).

**D8. The Sitemaps tab is a 007 `TabDef`.** Eligible rows: URLs with `in_sitemap = 1` plus internal HTML 200 pages (so "Not in a sitemap" lives in the same tab). Filters from FR-026, each pointing at its rule (spec 009 D2) so filter and issue are one definition. Columns: Address, Status, Indexability, Sitemaps (count of entries, derived per data version like 007 inlink counts), Lastmod (latest), Orphan class, Discovery source. A `ViewRequest.sitemap_file: Option<i64>` narrows the tab to one file's entries (FR-027). The inspector gets a "Sitemaps" section listing the files and `lastmod` for the selected URL.

**D9. Generation streams from one query.** `sitemaps::generate(reader, &GenerateOptions, &dyn Sink) -> GenerateReport`. Selection is one SQL statement built from the options (all conditions are fixed SQL with bound parameters; patterns are compiled by the spec 002 matcher and applied in Rust while streaming, never spliced into SQL), ordered by URL. A `FileSplitter` writes entries with `quick_xml::Writer` into the current file, tracking bytes and count, and starts the next file before either limit would be passed (50,000 or the user limit; 50 MB minus the closing tags). Images come from `links kind = 'image'` with status 200 joined per page (capped at 1,000), alternates from `links kind = 'hreflang'` restricted to selected pages through a temp table of selected ids. Output is deterministic: URL order, fixed attribute order, no timestamps in content except `lastmod`, gzip with a fixed header (mtime 0, no name) so `.gz` output is byte-identical too (SC-006). The `Sink` is a folder writer that writes `name.tmp` and renames on success, so a failed run never leaves a half file. A dry run (`Sink::Count`) gives the sheet's live counts (FR-024) without writing. Preview of 1M pages is a count query, not a generation.

**D10. Desktop UI.** In the sidebar's Crawl results group, the 007 tab list gains "Sitemaps"; under it, "Sitemap files" opens a tree panel (indexes with children, virtualised, keyboard navigable, `aria-expanded`). "Generate XML sitemap" opens a sheet (spec 017 sheet pattern, like Settings) with option groups, live counts, a folder picker (`tauri-plugin-dialog`, already used) and a replace confirmation. Settings › Scope gains a "Sitemaps" group: two switches, a URL list editor (reusing `HeadersEditor`'s row pattern) and local files with base URL. All colours from existing tokens; a new `--status-orphan` token only if contrast tests pass, otherwise the existing warning token. Every component has Vitest tests; Playwright covers the tab, the files tree, the generate sheet and the settings group with the fake transport.

**D11. Command line.** `spiderlens sitemaps <file> [--json]`, `spiderlens orphans <file> [--format plain|csv|json]`, `spiderlens sitemap generate <file> --out <dir> --base-url <url> [options]`, plus crawl flags `--no-sitemaps`, `--no-crawl-sitemap-only`, `--sitemap-url <url>` (repeatable), `--sitemap-file <path>=<base-url>` (repeatable); `--use-sitemaps` stays as an accepted no-op alias. Details in [contracts/cli.md](contracts/cli.md).

## Performance impact

- **Sitemaps off**: no code runs; SC-002 checked by the spec 008 bench.
- **Sitemaps on**: per entry, one normalization and one `discover()` call (both already happen today for every `<loc>`), plus one row insert in the writer's batch. Measured target: 1,000,000 entries in 20 files in 30 s extra and 50 MB peak (SC-003); the writer batch size is shared with 008 so commits do not multiply.
- **Passes**: SQL over indexed joins; budgets in D6, verified at 1M (SC-004).
- **Generation**: one sequential scan plus per-page lookups for images and alternates via indexes; target SC-005.

## Structure

```text
crates/engine/src/crawl/sitemap.rs     streaming parser, Entry, FileFacts, SitemapJob, sources, lastmod parsing
crates/engine/src/crawl/runner.rs      sources, Msg::SitemapEntries, SitemapOnly reason, List source
crates/engine/src/crawl/store.rs       upsert_sitemap_file, insert_sitemap_entries, refs
crates/engine/src/crawl/migrate.rs     step: sitemap_files, sitemap_entries, sitemap_refs, sitemap_problems, orphans
crates/engine/src/crawl/reason.rs      NotCrawledReason::SitemapOnly
crates/config/src/model.rs             SitemapSettings, LocalSitemap, old-key mapping
crates/config/src/list_input.rs        ListInput::Sitemap
crates/results/src/sitemaps/
  mod.rs        Need::SitemapEntries wiring, files view, entries for a URL
  audit.rs      sitemap_audit pass
  orphans.rs    orphans pass
  generate.rs   GenerateOptions, selection query, FileSplitter, sinks
  xml.rs        writer helpers, escaping
crates/results/src/issues/rules/sitemaps.rs   the Sitemaps category
crates/results/src/tabs.rs             `sitemaps` TabDef
crates/cli/src/sitemap_cmd.rs          sitemaps, orphans, sitemap generate
app/src-tauri/src/results.rs           sitemap_files, sitemap_entries_for_url, sitemap_preview, sitemap_generate
ui/src/sitemaps/                       SitemapFiles, GenerateSheet, InspectorSitemaps
ui/src/settings/SitemapSources.tsx     Settings › Scope › Sitemaps
crates/test-fixtures/src/sitemaps.rs   the sitemap fixture site and the 1M-entry generator
specs/011-sitemaps-orphan-pages/xsd/   sitemap.xsd, siteindex.xsd, sitemap-image.xsd, xhtml-link.xsd (for xmllint in CI)
```

## Dependencies and order

- **Spec 006 storage (PR 27)** for `migrate.rs`; **spec 004 (PR 29)** for error classes on sitemap fetches. Code starts on main after both.
- **Spec 009 rules engine (code PR 52)** for the catalogue, the analysis phase and `header_value`. The code ships as two PRs: **011a** (parser, sources, storage, settings, Sitemap files view, generation, CLI `sitemaps` and `sitemap generate`) does not need 009; **011b** (passes, rules, Sitemaps tab filters, `orphans` CLI, inspector section) starts when 009's code is on main.
- **Spec 023** reads `orphans`; **spec 024** adds orphan sources by writing a `url_sources` table the orphans pass will read (agreed shape in data-model.md, so 024 needs no change here).

## Complexity tracking

None: no principle is bent.
