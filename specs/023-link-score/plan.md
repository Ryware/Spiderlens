# Implementation Plan: Link Score and Internal Link Analysis

**Branch**: `023-link-score` | **Date**: 2026-10-14 | **Spec**: [spec.md](./spec.md)

## Summary

Link Score and its companion counts are computed after the crawl, from the stored `links` and `urls` tables, by two passes of the shared Crawl Analysis phase that spec 009 owns (009 plan D12): `link_positions` (optional, reads stored HTML) and `link_metrics` (the score, rank and counts). The output is one table per pass in the crawl file. The results crate reads it through ordinary column expressions, so the Internal tab, the new Links tab, the detail pane, CSV export, the CLI and the 009 rules all see the same values. Nothing touches the fetch path.

Because the 009 runner (Pass trait, `PASSES`, meta `analysis`) and the 006 migration steps are not on `main` yet, the work ships in two stages. Stage 1 computes the same tables into the reader's `temp` schema on demand when a finished crawl is opened, with no file change, so the feature is usable and testable at once. Stage 2, after 009's runner and 006's `migrate.rs` land, registers the passes and persists the tables; the column expressions do not change (D5).

## Technical Context

- **Language**: Rust 1.97 (workspace), TypeScript + React 18 (UI)
- **Crates touched**: `spiderlens-results` (computation, tabs, detail), `spiderlens-config` (position rules and weights), `spiderlens-engine` (migration step in stage 2 only), `spiderlens-cli`, `app/src-tauri`, `ui/`
- **New dependencies**: none. HTML parsing and CSS selectors reuse `scraper` / `html5ever` (already in the engine); stored bodies use `zstd` (already in the engine).
- **Storage**: SQLite crawl file (spec 006); new tables `link_metrics` and `link_positions` (stage 2 migration step, number taken at merge).
- **Testing**: `cargo test` (unit + fixture crawl files), a reference implementation check, `synth` 100k and 1M benchmarks, Vitest + Playwright for the UI.
- **Performance goals**: SC-002 under 10 s for 100k pages / 5M links; SC-003 under 3 min for 1M / 50M; SC-004 memory under 100 MB / 500 MB.

## Constitution Check

| Principle | How this plan complies |
|-----------|------------------------|
| I. Spec before code | Spec 023 approved 2026-10-13. |
| II. Local-first | All computation is local; no network. |
| III. Engine independent of the shell | Everything lives in `spiderlens-results`; the CLI exports the Links tab and runs the analysis with the same code. |
| IV. Respectful crawling | Not affected (no fetching). |
| V. Correct and complete data | Results record the run (version, completeness, weights, rules); pending and stale states are never shown as zero. |
| VI. Scales by design | Graph held as compact integer arrays (12 bytes per edge at peak, D3), not rows; per-URL output streamed to disk; performance stated below. |
| VII. Test-first | Every acceptance scenario maps to a test (Testing). |
| VIII. Simplicity | No new dependency, no new process; one table per pass; tabs and columns are data like the existing ones. |

## Design decisions

### D1. Two passes under the 009 contract

```text
link_positions   after: []                needs: [RawHtml]   budget: 5 s / 100k pages
link_metrics     after: [orphans?, link_positions]  needs: [Base]   budget: 5 s / 100k pages
```

`link_metrics` lists `orphans` (spec 011) in `after` only when that pass is registered, so the order rule in the coordinator note holds without a hard dependency. `link_positions` runs only when the file has stored HTML; without it the pass is skipped with its reason and every link counts as Unknown (spec US4 scenario 6). Each pass is a plain function `fn run(conn, schema, settings, progress, cancel) -> Result<Outcome>`; stage 2 wraps it in the `Pass` trait, stage 1 calls it directly (D6).

### D2. The graph: what is a node and what is an edge

One SQL scan over the `links_from` index, ordered by `from_id`:

```sql
SELECT l.from_id, l.to_id, l.pos
FROM links l JOIN urls t ON t.id = l.to_id
WHERE l.kind = 'anchor' AND l.in_head IS NOT 1
  AND l.nofollow = 0 AND l.page_nofollow = 0 AND l.sponsored = 0 AND l.ugc = 0
  AND t.internal = 1 AND l.to_id <> l.from_id
ORDER BY l.from_id
```

- Nodes are internal URLs (`urls.internal = 1`), indexed densely by `url.id` (ids are an integer primary key, so a `Vec` indexed by id, sized to `max(id)+1`, is the id map; 4 bytes per id).
- Rows arrive grouped by source, so duplicates per (source, target) are removed with a small per-source sort, and the edge list is built directly in CSR form (offsets + targets), never as a hash map.
- Redirecting URLs (`urls.redirect_to` not null) get exactly one out-edge, to their redirect target, with weight 1; loops are detected with the existing `temp.redirects` derived table (`is_loop`) and pass nothing (spec edge case).
- Error, blocked, uncrawled and non-HTML URLs have no anchor rows from them, so they are dangling by construction.
- With position weighting on, each edge carries the highest weight among the links that formed it (FR-025), as an `f32` per edge; off, edges are unweighted and no weight array is allocated.

### D3. The score: power iteration, sequential and deterministic

Classic damped PageRank, d = 0.85, uniform teleport, dangling mass spread uniformly:

```
r_next[v] = (1-d)/N + d * ( Σ_{u→v} r[u] * w(u,v)/W(u) + D/N ),  D = Σ_{dangling u} r[u]
```

- Stop when the L1 change is below `1e-9` or after 100 rounds (fixed, so files match across machines).
- **Determinism (FR-005, SC-008)**: one thread, edges visited in CSR order, `f64` sums. IEEE-754 addition in a fixed order gives identical bits on every platform; no parallel reduction, no fused multiply-add (`f64::mul_add` is not used, and Rust does not contract `a*b+c` on its own).
- **Cost**: 5M edges x up to 100 rounds is at most 5 x 10^8 multiply-adds, well under the budget; typical convergence is 40-60 rounds. At 50M edges the iteration is about 30 s; the SQL scan dominates (D7).
- **Memory**: offsets `u32` per node, targets `u32` per edge, two `f64` rank vectors per node, out-weight per node. 100k / 5M: about 22 MB. 1M / 50M: about 220 MB, plus 200 MB for weights when weighting is on, so weighting on very large files streams weights from disk instead (D7 fallback).

**Scale (FR-004)**: `score = 100 * ln(r / r_min) / ln(r_max / r_min)`, where `r_min = (1-d)/N` (the value of a page with no inlinks) and `r_max` the highest rank in the crawl; clamped to 0..100, stored as `REAL` and rounded only for display (whole number in the table, one decimal in the detail pane and exports). One URL alone scores 100. Rank is competition ranking on the stored value rounded to one decimal (1, 2, 2, 4), so ties are what the user sees as ties.

### D4. Counts in the same pass

From the CSR arrays: unique outlinks (out-degree before redirect substitution), unique followed inlinks (in-degree). From one more scan of `links_to` grouped by `to_id`: all inlinks (every anchor occurrence including nofollow) and unique inlinks (`COUNT(DISTINCT from_id)`), unique external outlinks (`COUNT(DISTINCT to_id)` over external targets). `% of pages linking` = unique inlinks / number of internal HTML pages x 100. All written in one `INSERT` stream per 10,000 URLs.

### D5. Output tables and how columns read them

```sql
CREATE TABLE link_metrics (
  url_id INTEGER PRIMARY KEY, score REAL NOT NULL, rank INTEGER NOT NULL,
  unique_inlinks INTEGER NOT NULL, followed_inlinks INTEGER NOT NULL, all_inlinks INTEGER NOT NULL,
  unique_outlinks INTEGER NOT NULL, external_outlinks INTEGER NOT NULL, pct_linking REAL NOT NULL
);
CREATE TABLE link_positions (
  from_id INTEGER NOT NULL, pos INTEGER NOT NULL, position INTEGER NOT NULL, rule INTEGER,
  PRIMARY KEY (from_id, pos)
) WITHOUT ROWID;
```

Column expressions name the tables **unqualified**, for example `(SELECT score FROM link_metrics WHERE url_id = u.id)`. SQLite resolves an unqualified name in `temp` first, then `main`, so a reader that computed fresh results into `temp` (stage 1, read-only files, stale files) shadows the stored table, and a reader on an analysed file reads `main` with no copy. This is the one trick that lets stage 1 and stage 2 share every column, filter and rule.

`position` is a small integer (0 Unknown, 1 Navigation, 2 Header, 3 Content, 4 Sidebar, 5 Footer); `rule` is the index of the custom rule that matched, or NULL for a built-in one.

### D6. Stage 1: on-demand computation in the reader

Until the 009 runner exists, `Reader::ensure_analysis()` runs the two functions into `temp` when a view or command needs their output, the crawl's `meta.status` is not `running`, and `temp.link_metrics` is missing or older than the reader's data version. A running crawl gets the new `Need::Analysis` state "Calculated when the crawl ends" (FR-009), so nothing is computed while fetching (FR-029). The computation runs on the reader's own thread, reports progress through a callback the app forwards as `analysis_progress` (the event name 009 defines), and checks a cancel flag between batches.

Stage 2 replaces `ensure_analysis` with the 009 runner: passes write `main` at crawl end under the writer lock, `meta.analysis` records versions, and the reader only computes into `temp` for read-only or stale files, exactly as 009 D12 describes. The column definitions and UI are unchanged.

### D7. Reading 50M links fast enough

The scan in D2 is the cost centre at 1M URLs. It reads only covered columns through `links_from` and the `urls` primary key; measured on the `synth` 1M file in the benchmark task. If it misses SC-003, the fallback is a covering index `links(from_id, kind, to_id, pos)` added by the same migration step; it is not added up front because it grows every crawl file by roughly 15%.

### D8. Link positions: parse stored HTML after the crawl

For each internal HTML page with a stored body: decompress (zstd), parse with `scraper` (the same html5ever tree builder spec 005 uses, so anchors come out in the same document order and `pos` matches `links.pos`), walk the anchors and area elements in order, and classify each by its ancestors:

1. Custom rules in order: the first rule whose selector matches an ancestor (or the link itself) wins.
2. Built-in rules: nearest landmark ancestor: `nav` or `role=navigation` → Navigation; `header` or `role=banner` (not inside `article`/`section`) → Header; `aside` or `role=complementary` → Sidebar; `footer` or `role=contentinfo` (not inside `article`/`section`) → Footer; otherwise Content.

Selectors are compiled once per run (`scraper::Selector::parse`); invalid selectors are rejected in Settings with the parser's message (validation in `spiderlens-config`). Pages are processed in parallel with `std::thread::scope` over chunks of page ids (results written in id order, so the output is deterministic); each worker holds one decompressed page at a time. Budget: spec 005 measured about 3,300 pages/s for full extraction on one core; parse-and-walk is a subset, so 100k pages on 4 cores is expected under 10 s. Measured in the benchmark task; if it misses SC-009, the fallback is recording the nearest landmark during extraction (one byte per link, computed while the spec 005 walk is already on the element), which keeps custom rules on the post-crawl path only.

The rules tester (FR-022) runs the same classifier on one page and returns each link with its position and rule, without writing anything.

### D9. Settings

`CrawlConfig.links` (new, `#[serde(default)]`):

```rust
pub struct LinkAnalysisSettings {
    pub auto_analyse: bool,                 // default true (FR-012)
    pub position_rules: Vec<PositionRule>,  // { selector: String, position: Position }
    pub weighting: Option<PositionWeights>, // None = off (default); Some = on with these weights
}
```

Stored with the profile and in the crawl's stored config, so rules travel with crawl files (FR-021). Changing rules or weights on an open file marks `link_positions` / `link_metrics` stale (FR-023); in stage 1 that simply drops the `temp` tables.

### D10. Results crate additions

- `Need::Analysis` with two reasons: running ("Calculated when the crawl ends") and not calculated ("Not calculated yet: run Crawl Analysis").
- Columns: `link_score`, `link_rank`, `unique_inlinks`, `followed_inlinks`, `all_inlinks` (replaces the per-row `COUNT(*)` of the existing Inlinks column when available), `unique_outlinks`, `external_outlinks`, `pct_linking`. `Link Score` visible by default on Internal (FR-007).
- `links` tab (FR-010): scope internal HTML pages; filters as specified; `Crawl depth 4 or more` is `u.depth >= 4`; `No followed inlinks` excludes `u.source = 'start'`; `Nofollow internal outlinks` is an `EXISTS` on `links` with nofollow to an internal target; `Linked only from navigation, header or footer` is a `NOT EXISTS` over `link_positions` joined to inlinks with positions 3/4.
- `UrlDetail` gains `link_metrics: Option<LinkMetrics>` and inlink/outlink rows gain `position`.

### D11. UI

- Sidebar: `Links` joins Crawl results after `Internal` (icon `Network`).
- Status bar: "Analysing: Link Score 40%" with a cancel button, driven by `analysis_progress`.
- Column cells show a muted "pending" chip while running and "not calculated" with a `Run` action afterwards (FR-009); never 0.
- Inspector: a "Links" group in Response details (score with one decimal, "rank 12 of 4,812", depth, the counts).
- Inlinks/Outlinks lists: a Position column and a position filter chip row.
- Settings › Link analysis (new category in the settings sheet): automatic analysis switch, position rules editor (ordered list with selector, position select, drag to reorder, inline validation), "Test on URL" field showing the tester result, weighting switch with five weight sliders and Reset.
- Colours: none new; positions are text labels.

### D12. CLI

- `spiderlens export <file> --tab links [--filter …]` works through the existing tab machinery (FR-028).
- Stage 1: `spiderlens links analyse <file>` runs the computation and prints progress and a summary (URLs, links, rounds, ms); stage 2 folds it into 009's `spiderlens analyse`, and the subcommand stays as an alias.
- `spiderlens links positions <file> --url <URL>` runs the tester (FR-022).
- `spiderlens crawl` runs the analysis at the end unless `--no-analyse` (FR-028); exit code reflects the crawl only.

## Performance plan

| Step | 100k pages / 5M links | 1M / 50M |
|------|----------------------|----------|
| Link scan + CSR build | ~3 s | ~40 s |
| Iteration (50 rounds) | ~0.3 s | ~30 s |
| Counts scan + write | ~2 s | ~25 s |
| Positions (4 cores) | ~6 s | ~60 s |

Measured with `synth` files in the benchmark task and recorded in `benchmarks.md`. The crawl itself is unchanged: passes never run while fetching, and the 008 crawl benchmark job guards the crawl speed.

## Testing plan

- **Reference check (SC-001)**: a straightforward dense PageRank in the test (no CSR, no dedupe shortcuts) over the fixture graphs; stored scores must match to 1e-9 before scaling and exactly after rounding; rank order must match.
- **Fixture graphs** in `crates/test-fixtures/src/links.rs`: hub page, deep page, nofollow-only page, redirect chain, redirect loop, 404 target, image link, self link, duplicate links, sitemap-only page, single-page site, List mode with no links.
- **Determinism (SC-008)**: score bits hashed in a test that runs on all three CI platforms; the expected hash is committed.
- **Positions**: fixture pages per landmark, custom rule order, invalid selector, no stored HTML, `pos` alignment with spec 005's link order on malformed HTML.
- **Benchmarks**: `synth` 100k (5M links) and 1M (50M) analyse runs; time and peak RSS checked against SC-002 to SC-004; 1M run on the manual benchmark job like spec 008.
- **UI**: Vitest for pending / not calculated cells, Links tab filters, inspector group, settings editor (reorder, validation, tester), status bar progress and cancel; Playwright e2e opening the Links tab on fake data and running a filter.

## Project structure (changes)

```text
crates/results/src/analysis/        stage 1: ensure_analysis(); stage 2: passes registered in 009's PASSES
  link_metrics.rs                   graph build (D2), iteration (D3), counts (D4), writer
  link_positions.rs                 classifier (D8), tester
crates/results/src/tabs.rs          Need::Analysis, columns, `links` tab
crates/results/src/detail.rs        LinkMetrics in UrlDetail, position on link rows
crates/config/src/model.rs          LinkAnalysisSettings, PositionRule, PositionWeights (+ validation)
crates/engine/src/crawl/migrate.rs  stage 2: step creating link_metrics, link_positions
crates/cli/src/links_cmd.rs         analyse, positions; --no-analyse on crawl
crates/test-fixtures/src/links.rs   fixture graphs and pages
app/src-tauri/src/results.rs        run_analysis, cancel_analysis, test_positions; analysis_progress events
ui/src/results/                     Links tab wiring, pending cells, inspector group, position column
ui/src/settings/LinkAnalysisSettings.tsx
ui/src/shell/                       status bar analysis progress
```

## Dependencies and coordination

- **009** owns the Crawl Analysis runner (D12 there). This plan follows its contract; stage 2 waits for it.
- **006** owns the schema; the stage 2 migration step takes the next number at merge.
- **011** orphan pages: `link_metrics` runs after `orphans` when present.
- **022** reads `link_metrics` for node size and `link_positions` for the anchor cloud filter.
- **014** JS rendering decides which link set is stored; this plan reads whatever `links` holds.

## Complexity Tracking

None. The stage 1 / stage 2 split is temporary and removes no code at the switch: stage 2 only adds the `Pass` wrappers, the migration step and the at-crawl-end trigger.
