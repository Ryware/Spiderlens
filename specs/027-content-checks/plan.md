# Implementation Plan: Content Checks (SERP Preview, Segments, PDF, Spelling, AMP)

**Branch**: `027-content-checks` | **Date**: 2026-10-14 | **Spec**: [spec.md](./spec.md)

## Summary

Five independent parts, five code pull requests, in this order: A SERP preview, B Segments, C PDF, D Spelling and grammar, E AMP. Each PR can be merged, postponed or dropped without the others (FR-000a).

The plan rests on one idea per part, chosen so the default crawl path does not change (FR-000b, SC-000):

| Part | Central decision | Crawl-time cost with defaults | File format change |
|---|---|---|---|
| A | Pixel width is a **SQL function** over stored text (`px_width`), like 009's `thr()`: no new column, works on old files | none | one additive step (proposals table) |
| B | Segment membership is a **derived temp table** built in Rust from the profile's segments, like 009's `temp.occ_*` | none | none (segments travel inside the stored config) |
| C | PDF bodies go to a **separate bounded reader pool**, never through HTML extraction | none (off) | one additive step |
| D | Spelling is a **post-extraction analysis pass** over stored text at low priority, runnable on any crawl | none (off) | one additive step |
| E | AMP checks run on the **already parsed DOM** of pages that declare AMP | none (off) | one additive step |

Every finding is a rule in the 009 catalogue (FR-000d): each part adds one file under `crates/results/src/issues/rules/` and one line in the catalogue, the same extension point 015 and 016 use.

## Technical Context

- **Builds on**: 005 extraction (`pages`, `page_text`, `links`, `bodies` with zstd raw HTML), 007 results crate (`TabDef`, `ColumnDef`, `FilterDef` as SQL data, `sql::register` scalar functions), 009 rule catalogue (PR #52, `issues::rule(...)` builder, `thr()`, `Derived` temp tables), 006 storage (PR #27, `crawl/migrate.rs` additive steps). Parts A and B start from main after #52 and #27 merge; if #52 is late, A ships its columns and filters first and adds its rules in a follow-up commit (spec Assumptions).
- **Language/versions**: Rust 1.97 (edition 2024), React 19 + TypeScript, Tauri 2. No new language.
- **New dependencies** (constitution VIII, each justified in [research.md](./research.md)):
  - A: none at run time. `ttf-parser` as a **build-time tool only** (an `xtask`-style generator), to produce the width table from open fonts.
  - C: `lopdf` (MIT) for structure, metadata and link annotations; `pdf-extract` (MIT, on `lopdf`) for text.
  - D: `spellbook` (MPL-2.0, pure Rust, Hunspell-compatible); `harper-core` (Apache-2.0) for English grammar. Both only in a new `crates/spell` crate so the engine does not depend on them.
  - B, E: none.
- **Storage**: one SQLite crawl file (006). Schema steps are taken at merge time: latest step on main plus one, no gaps, nullable columns, no hand-written `ALTER` (project rule). This plan does not reserve numbers.
- **Testing**: Rust unit and fixture-site tests (`crates/test-fixtures`), CLI tests (`assert_cmd`), Vitest + Playwright in Chromium for every UI change (FR-000g), on Windows, macOS and Linux.
- **Performance goals**: SC-000 (default path within 2% on the 008 benchmark), SC-A2, SC-B1, SC-C2, SC-D2, SC-D3. Each part's PR includes its benchmark result.

## Constitution Check

| Principle | How this plan complies |
|---|---|
| I Spec before code | Spec 027 approved 2026-10-13; this plan adds no behaviour beyond it. |
| II Local-first | All checks run locally. The only network use is a user-requested dictionary download from the app's static release location (GitHub Releases), FR-D03. |
| III Engine independent of shell | Widths, segments, PDF reading, spelling and AMP live in crates (`results`, `engine`, `spell`); every feature has a CLI command ([contracts/cli.md](./contracts/cli.md)). |
| IV Respectful crawling | PDFs and AMP pages obey the same scope, robots and rate rules as pages. |
| V Correct and complete data | Partial PDF reads and unchecked pages are recorded with reasons; missing data shows as unavailable, never zero (FR-000f). |
| VI Scales by design | Widths computed per displayed row; segment table is one integer pair per URL; PDF memory bounded by the size cap; spelling streams page by page. |
| VII Test-first | Every acceptance scenario maps to a task with its test ([tasks.md](./tasks.md)). |
| VIII Simplicity | Reuses SQL-as-data tabs, rule builder, derived tables and migration steps; no new query language for segments. |

No violations.

## Design decisions

### D1 (A) Pixel width as a SQL function over a generated width table

- `crates/results/src/serp/` holds `widths.rs`, a **generated** table of advance widths per character for two styles: title (20 px) and description (14 px), each with desktop and mobile limits. A function `px_width(text, style) -> u32` sums advance widths after collapsing whitespace (as results display), with no kerning.
- The table is generated from **Liberation Sans** (SIL OFL; metric-compatible with Arial, the font search results use) for Latin, Greek and Cyrillic, and **Noto Sans Hebrew / Noto Sans Arabic** (OFL) for Hebrew and Arabic. Characters not in the table use a per-script fallback: CJK at 1 em, emoji at 1.25 em, anything else at the font's average width. The generator (`tools/serp-widths`, not a workspace member of the shipped binaries) is run by hand when metrics change; CI checks the committed table matches the generator output (data change, not code change, per spec Assumptions).
- Registered in `sql::register` as `px_width(text, 'title' | 'desc')` and `px_trunc(text, style, limit)` (returns the truncated display string). Columns in the Page titles and Meta description tabs use `px_width(norm_ws(u.title), 'title')`: computed only for rows the UI requests (constitution VI), so **old files get widths for free** (FR-A04) and nothing is added to the crawl path.
- Sorting and filtering on a computed column over 100k rows: measured target under 1 s (SC-A2 budget 3 s). If it misses, the fallback is a `Derived` temp table built once per open crawl, which 009 already supports.
- Limits are 009 thresholds (`title.px_over_desktop` etc.) read through `thr()`, so the user changes them in Settings › Issues (FR-A03).
- **One table, two readers**: the same table is exported to `ui/src/generated/serpWidths.json` (like ts-rs exports) so the preview editor measures as the user types without a round trip (SC-A3, 100 ms). A Rust test and a Vitest test check both sides return the same widths for the fixture strings.
- **Right-to-left**: width is order-independent; truncation cuts at the logical end, and the preview renders with `dir="auto"`.

### D2 (A) Proposals and SERP list mode

- Proposals live in a new table `serp_proposals` (data-model) in the crawl file, written through a desktop command; this is part A's schema step. Export uses the existing 007 CSV writer.
- SERP list mode reuses 007's list input parser (`config::list_input`) for CSV with URL, title, description. It builds an in-memory SQLite database with the same `urls`/`pages` columns used by the tabs, so the same tab definitions, columns and filters render it, and makes no network request (FR-A07). CLI: `spiderlens serp`.

### D3 (B) Segments as data in the profile, membership as a derived table

- `Segment { id, name, color, enabled, mode: All|Any, clauses: Vec<Clause> }` lives in `CrawlConfig` (crates/config), so it is saved in profiles, exported with them, and stored with the crawl by the existing `StoredCrawlConfig` (FR-B03, FR-000e). **No file format change.**
- `Clause` = field (host, path, query, url, status class, content type, indexability, language, depth, word count) + operator + value. Patterns compile with the `regex` crate already used by `config::patterns`, with the same size limits; validation reports the failing clause (FR-B07).
- Membership is computed **in Rust**, not in SQL: one scan of `urls` joined to the few `pages` facts, first matching enabled segment wins (FR-B03), written to `temp.url_segment(url_id INTEGER PRIMARY KEY, seg INTEGER)` on the reader connection. This keeps user patterns out of SQL text (009 plan D3 rule) and lets a 1M-URL rebuild stay within SC-B1 (a scan plus regex per URL; estimate 2 to 4 µs per URL).
- During a crawl the table is extended incrementally for `url_id > last_seen` at each live refresh; a segment edit rebuilds it fully.
- Tabs: a shared `Segment` column (`(SELECT seg FROM temp.url_segment s WHERE s.url_id = u.id)`, displayed by name and colour in the UI) is appended to every tab when segments exist; the segment filter adds `u.id IN (SELECT url_id FROM temp.url_segment WHERE seg = ?1)` with the id bound as a parameter. Sidebar counts and issue counts (009 `IssuesSummary`) take the same optional segment parameter (FR-B04, FR-B05).
- Scope tester (007) calls the same `segments::classify` on the test URL (FR-B08).

### D4 (C) PDFs: separate download policy and a bounded reader pool

- **Fetch**: `fetch.rs` already decides per content type whether to keep the body (`keep` closure). With `crawl_pdfs` on, `application/pdf` (or `%PDF-` in the first bytes when the type is missing or `application/octet-stream`) is kept up to `pdf_max_bytes` (default 20 MB), separate from the 50 MB HTML cap. Over the cap: the download stops, status "over size limit" (C1.5). With the setting off, nothing changes (C1.6).
- **Reading** happens off the fetch workers: the runner sends `(url_id, bytes)` to a `PdfReader` with `min(2, cores/4)` blocking threads and a bounded channel (capacity 8). When the channel is full the PDF bytes are **spilled to the crawl file's `bodies` table** and read later, so the fetchers never wait on PDF parsing (SC-C2). A slow PDF never blocks HTML extraction because they share no queue.
- **Time limit** (C3.2): text is extracted page by page with a deadline check between pages; a page that itself exceeds the budget cannot be interrupted safely in-thread, so each read also runs under `catch_unwind` and a watchdog marks the document "partly read: time limit" and returns the thread's result to a discard channel. Damaged files are caught the same way (C1.4).
- **Data**: `pdfs` table (metadata, page count, word count, flags, status), PDF text into `bodies` under the raw HTML retention rule (FR-C05), links from link annotations into the existing `links` table with `source = 'pdf'` and `pos = page number`, handed to discovery only if `follow_pdf_links` (FR-C03). Inlinks then show the PDF as a source with no UI change.
- **Tab**: a PDFs tab defined as a `TabDef` over `urls JOIN pdfs`; rules in `rules/pdf.rs` (FR-C07).

### D5 (D) Spelling as an analysis pass in a separate crate

- New crate `crates/spell`: dictionary manager (install, list, remove, load), tokenizer, checker and grammar adapter. The engine and results crates do not depend on it; the app and CLI do. This keeps the crawl binary path free of it (constitution III, VIII).
- **Engine of record**: `spellbook` (pure Rust Hunspell-compatible checker and suggester), which reads the common `.aff`/`.dic` format (FR-D04). It handles Hebrew prefixes through the dictionary's affix rules; niqqud is stripped before lookup (FR-D05; research R4 has the measured behaviour).
- **Dictionaries**: English (`en_US`, from the SCOWL-based Hunspell dictionary, permissive licence) is bundled. Others are listed in a static `dictionaries.json` manifest shipped with each release (language, file names, size, SHA-256, licence) and downloaded on request from the release's own GitHub Releases assets, verified by hash, stored in the app data folder (FR-D03). Copyleft dictionaries (Hebrew from Hspell is AGPL-3.0) are data files fetched at the user's request, not compiled or linked into the app; the licence is shown before download.
- **Pass**: a `SpellRun` reads `page_text` (title, description, headings) and body text recomputed from the stored HTML (005 visible-text routine, reused) page by page, in batches of 200, writing `spell_findings`. It runs on a single low-priority thread during a crawl, after extraction commits, and continues after the crawl ends (D4 story). On a finished crawl it runs on demand (FR-D07). Progress = pages checked / pages eligible.
- **Language** (FR-D02): page `lang` primary subtag, else the default language; runs of characters in another script (Unicode script property) go to an installed dictionary for that script or are skipped; a page whose text is mostly in a script that does not match its declared language gets a "language mismatch" finding (edge case).
- **Site-wide repeats** (FR-D09): text blocks whose xxh3 hash appears on over 50% of checked pages are checked once and listed site-wide; per-page counts still include them.
- **Grammar** (FR-D10): `harper-core` on English pages only, off by default, findings with `kind = 'grammar'`.
- **Skips** (edge case): tokens with digits, URLs, emails, `<code>`/`<pre>`, all-caps (setting), hyphen parts checked separately.

### D6 (E) AMP checks on the parsed DOM

- With `check_amp` on, the `amphtml` link from a page becomes a discovery source (`source = 'amphtml'`) even when it would otherwise only be stored, subject to scope (FR-E01).
- A page whose root has `amp`, `⚡`, `amp4email` is checked during extraction, on the DOM 005 already parsed, so there is no second parse: marker, runtime script, boilerplate, viewport, canonical, author scripts (any `<script>` without `custom-element`/`custom-template` or a JSON type), `<style amp-custom>` size up to 75,000 bytes, and a short disallowed list (`<frame>`, `<frameset>`, `<object>`, `<embed>`, `<base>` with target other than `_blank`/`_top`). Results to `amp_checks`.
- Pairing (canonical ↔ amphtml mutual) is a site-wide rule over stored links, evaluated by the 009 engine.
- The AMP tab shows the "core subset" note (FR-E04).

### D7 UI

- All new views use the 017 design system and 018 tokens; no raw colours (the token test enforces this). Segment colours come from the existing chart/accent token set.
- New screens: Search preview section in the Inspector (A), SERP list mode as a mode of the start screen (A), Settings › Segments and the segment filter in the FilterStrip (B), PDFs tab (C), Spelling tab, Inspector findings section and Settings › Spelling with the dictionary list (D), AMP tab (E). New settings on the crawl configuration sheet: Crawl PDFs, Follow PDF links, PDF size limit (C), Check spelling, Grammar, default language (D), Check AMP (E).
- Every PR touching `ui/` adds Vitest component tests and at least one Playwright flow through the fake transport (`ui/src/testing/fakeTransport.ts`).

## Project Structure

```text
crates/results/src/serp/{mod.rs,widths.rs}          # A: px_width, px_trunc, generated table
crates/results/src/segments.rs                      # B: classify, build temp.url_segment
crates/results/src/issues/rules/{serp,pdf,spelling,amp}.rs   # A, C, D, E rules
crates/config/src/segments.rs                       # B: Segment, Clause, validation
crates/engine/src/pdf/{mod.rs,reader.rs}            # C
crates/engine/src/extract/amp.rs                    # E
crates/spell/                                       # D: new crate
tools/serp-widths/                                  # A: width table generator (not shipped)
app/src-tauri/src/{serp.rs,segments.rs,spell.rs}    # desktop commands
crates/cli/src/{serp.rs,spell.rs}                   # CLI commands
ui/src/results/SearchPreview.tsx                    # A
ui/src/serp/SerpListMode.tsx                        # A
ui/src/settings/SegmentsSettings.tsx, ui/src/results/SegmentFilter.tsx   # B
ui/src/settings/SpellingSettings.tsx, ui/src/results/SpellingFindings.tsx # D
```

## Performance impact (constitution VI)

| Part | Crawl path | Open / query path | Benchmark in its PR |
|---|---|---|---|
| A | none | width per requested row; sort over 100k rows under 1 s | SC-A2 on the 100k fixture |
| B | none (incremental table extension on live refresh, under 10 ms per 1,000 new URLs) | rebuild 100k under 5 s, 1M under 30 s | SC-B1 |
| C | off: none; on: PDF bytes leave fetch workers immediately | none | SC-C2 on 10k pages + 1k PDFs |
| D | off: none; on: one low-priority thread | none | SC-D2, SC-D3 |
| E | off: none; on: checks on an already parsed DOM, only for AMP pages | none | SC-000 with E off |

## Cross-spec coordination

- **009** (rule catalogue): new categories `Serp`, `Pdf`, `Spelling`, `Amp`; `RULES_VERSION` bump per PR. Rule ids are prefixed (`serp.*`, `pdf.*`, `spell.*`, `amp.*`).
- **006** (schema owner): four additive steps (A, C, D, E), numbered at merge time.
- **007** follow-ups (results table thread): the Segment column and filter touch `FilterStrip` and `ResultsView`; coordinated through the coordinator before part B's PR.
- **013** custom extraction, **015** AEO/GEO, **016** agents: can read `temp.url_segment`, PDF text and spelling findings; no dependency in the other direction.
- **019** cleanup: "Remove stored HTML" also removes stored PDF text (same `bodies` table); spelling findings stay.

## Complexity Tracking

None. The one new crate (`spell`) exists to keep the engine free of the spelling dependencies.
