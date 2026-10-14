# Implementation Plan: Near Duplicate Content

**Branch**: `020-near-duplicate-plan` (docs), `020-near-duplicate-code` (code) | **Date**: 2026-10-14 | **Spec**: [spec.md](spec.md)

## Summary

The spec 005 extraction walk, which already visits every text node once, also collects the page's **main content** (D1) and turns it into a fixed-size **content signature**: a one-permutation MinHash over five-word shingles, 480 bins of 16 bits, 960 bytes per page (D2). It is stored in a new `content_sig` table in the same group commit as the page, so nothing is read back and the crawl's write path only grows by one prepared insert (D3). After the crawl, a **Crawl Analysis pass** `near_duplicates` (the spec 009 runner, plan D12) finds candidate pairs with banded locality-sensitive hashing over the signatures, scores each candidate by comparing signatures, groups pairs into clusters with a union-find, and writes three tables (D4, D5). The Content tab, the Clusters view, the Duplicates inspector section and the comparison view read those tables through the results crate (D7, D8); the comparison re-extracts main content from stored HTML on demand. The "Near duplicate content" rule is one entry in the spec 009 catalogue over the pass's table (D9). The CLI gets `spiderlens duplicates` (D10). No new dependencies.

## Technical Context

- **Language**: Rust 1.97 (workspace), TypeScript 5 with React 19 (UI). No new crates: hashing uses `xxhash-rust` (already used for `text_hash`), parallelism uses `std::thread::scope`, the word diff is a small LCS in the results crate.
- **Storage**: the crawl file (SQLite, spec 006). One additive migration step (D3), number taken at merge time as main's latest plus one.
- **Where**: `crates/engine/src/extract/` (main content, shingles, signature), `crates/engine/src/crawl/` (store insert, migration step), `crates/results/src/near_dup/` (LSH, scoring, clusters, the analysis pass, queries, diff), `crates/results/src/tabs.rs` (Content tab), `crates/config` (content settings), `crates/cli` (`duplicates`), `app/src-tauri/src/results.rs` (commands), `ui/src/results/` and `ui/src/content/` (tab, clusters, inspector section, comparison), `ui/src/settings/` (Settings › Content).
- **Testing**: unit tests for main content, shingling and signature (golden values, cross-platform stable), accuracy tests against exact Jaccard on generated pairs (SC-003), a near-duplicate fixture site in `crates/test-fixtures`, pass tests on fixture crawl files, CLI tests, Vitest and Playwright with the fake transport, benchmarks on the spec 008 100k fixture and a synthetic 1M signature file.
- **Performance goals**: SC-001 (100k in 60 s), SC-002 (1M in 10 min, under 2 GB), SC-004 (crawl at least 98% of today's speed), SC-005 (20k-page group), SC-006 (threshold change in 10 s), SC-007 (selector change from HTML in 3 min).

## Constitution Check

| Principle | How this plan meets it |
|---|---|
| I Spec before code | Spec approved 2026-10-13; two small amendments are listed below for the owner's word before code merges. |
| II Local first | Deterministic hashing on the user's machine; no model, no network. |
| III Engine independent of the shell | Signature in the engine, analysis and queries in `spiderlens-results`, shared by app and CLI; the UI only renders. |
| IV Respectful crawling | No extra requests. |
| V Correct and complete data | Every eligible page gets a status (compared, too short, not indexable, no data); missing signatures report "unavailable", never zero. |
| VI Scales by design | Signatures stream from disk in batches; the LSH index holds 8-byte band keys, not texts; pairs are capped per page; peak memory stated in D5. |
| VII Test first | Each acceptance scenario maps to a test in [tasks.md](tasks.md). |
| VIII Simplicity | One extra buffer in the existing walk, one table, one pass on the shared runner, one tab defined as data. No new dependency. |

## Design decisions

### D1. Main content is collected in the same walk, into three candidate buffers

The 005 walk already appends every visible text node to `visible`. It now also appends, under the same `ctx`, to up to three buffers: text inside the first `<main>` (`main`), text inside any `<article>` (`article`), and body text outside boilerplate (`fallback`). Boilerplate means `nav`, `aside`, `[role=navigation|complementary|search]` and `form[role=search]` everywhere, plus `header`, `footer`, `[role=banner|contentinfo]` when they are not inside `main`, `article` or `section` (the HTML accessibility mapping only gives them landmark roles there). When the walk ends, the first non-empty buffer of `main`, `article`, `fallback` is the main content, and its source is recorded. This is one more `push_str` per text node, and no second parse.

User settings (FR-003) are CSS selectors parsed once per crawl into `scraper::Selector` values (scraper is already the DOM). When a content selector is set, the walk checks it only until the first match and then collects that element's subtree into a fourth buffer; exclude selectors are checked on each element entered and mark the subtree excluded in `ctx`. With no user selectors (the default) no selector matching happens at all.

Paragraph breaks are kept in the main text (the walk's existing `Separate` frames for block elements become `\n` in the main buffers), so the comparison view (D8) can align by paragraph from the same function.

In Rust terms this is like adding a field to a visitor rather than writing a second visitor: the walk owns a small `MainCollector` struct, and `extract_page` returns `PageData.main: MainContent { text_hash, words, source, signature }`.

### D2. The signature: one-permutation MinHash, 480 bins of 16 bits

- **Tokens**: words by the 005 word rules (`text::word_count`), lowercased. Runs of Han, Hiragana, Katakana, Hangul or Thai characters are split into single-character tokens (FR-005), so a page in those scripts gets meaningful shingles.
- **Shingles**: every run of 5 consecutive tokens, hashed with xxh3-64 (specified output, identical on every platform). Pages with fewer than 5 tokens get one shingle of what they have; they are below the 50-word minimum anyway.
- **Why MinHash**: the share of shared shingles (Jaccard similarity) is exactly what the spec's similarity means (FR-007): moved, added and removed passages all lower it. MinHash is the standard unbiased estimator of it from a fixed-size summary. SimHash, the usual alternative, estimates cosine similarity of weighted features, is tuned for "almost identical", and has no clean mapping from bits to a percentage in the 80 to 95% range users set. Exact comparison (keeping every shingle) is accurate but costs 4 bytes per word per page and still needs a candidate index.
- **Why one permutation**: classic MinHash computes K hash functions per shingle (480 times the work). One-permutation hashing (Li, Owen and Zhang, 2012) hashes each shingle once, uses the top bits to choose one of K bins and keeps the minimum per bin; empty bins are filled by optimal densification (Shrivastava, 2017), which keeps the estimator unbiased. The cost per page is one hash per word plus 480 bin fills, about 5 µs for a 1,000-word page, against roughly 1.2 ms of CPU per page for the rest of extraction today.
- **Why 480 × 16 bits**: the estimate is the share of equal bins. Its standard deviation is √(J(1−J)/K): 1.4 points at J = 0.9 with K = 480. A prototype with exact Jaccard as reference (research R2) measured a 95th-percentile error of 2.0 points at 90% and 2.9 points at 80% similarity, inside SC-003's 3 points. Storing the low 16 bits of each bin's minimum (b-bit MinHash) adds a 1 in 65,536 chance of a false bin match, negligible, and keeps the signature at 960 bytes (SC-009).
- **Exact flag**: `main_hash`, xxh3-64 of the main text, decides FR-014; equal hashes mean identical main content.

### D3. Storage during the crawl: one table, one insert

One additive migration step adds:

- `pages.main_words INTEGER`, `pages.main_hash INTEGER`, `pages.main_source TEXT`;
- `content_sig(url_id INTEGER PRIMARY KEY, sig BLOB NOT NULL)`, 960 bytes.

Rows are written with the page in the existing 200 ms group commit (spec 008); `INTEGER PRIMARY KEY` keys arrive in increasing order, so the B-tree appends without random page splits (unlike the hash indexes spec 005 had to defer). The writer adds `near_dup` to `meta.data_features` so the results crate knows the file has signatures (`Need::ContentSig`). On the 100k benchmark this adds about 96 MB of writes to a 336 MB file; the measured target is SC-004.

The signature is computed for every HTML page; eligibility (indexable, 200, word minimum) is decided in the pass, so changing those settings never needs the HTML (spec edge case).

### D4. The analysis pass `near_duplicates`

A value implementing the spec 009 `Pass` trait (009 plan D12): `id = "near_duplicates"`, `after = []`, `needs = [Need::ContentSig]`, budget "60 s per 100k pages". It runs at crawl end before the writer lock is released, on demand for stale files, into `temp` for read-only files, with progress and cancellation from the runner.

Steps:

1. **Eligible pages** (FR-001, FR-004): one SQL query joins `urls`, `pages` and `content_sig` for internal HTML 200 pages, filtered by `indexable_only` and `min_words`. Pages left out get a status row with the reason.
2. **Banding** (LSH): the 480 bins are cut into bands of `r` bins; each band's bins hash to an 8-byte key. Two pages that share any band key are candidates. `r` and the number of bands `b` (with `b·r ≤ 480`) are chosen from the reporting floor `f` so that a pair at `f` is a candidate with probability at least 0.999: the largest `r` that meets it is used, which keeps unrelated pages out: for the default floor 80%, `r = 9, b = 53`; at 50%, `r = 4, b = 120`. Table in research R3. Keys are sorted per band (radix sort on `u64`), and runs of equal keys are the buckets; memory is 8 bytes × pages per band, processed one band at a time.
3. **Scoring**: each candidate pair is scored by counting equal bins (480 `u16` comparisons, vectorised by the compiler), kept if at or above the floor. A pair met in several bands is scored once (pairs are deduplicated per page with a small sorted set).
4. **Large buckets**: a bucket bigger than 256 pages is not expanded pairwise per band. Its members are merged into a union-find "dense group" instead. After all bands, each dense group is scored all-pairs once, in parallel across cores (`std::thread::scope`), which is what makes a 20,000-page group cost 2 × 10⁸ comparisons once (about 10 s on 4 cores) instead of once per band. Groups above 50,000 pages compare each member with a deterministic sample of 2,000 members and mark counts as lower bounds (spec amendment 2).
5. **Per page**: closest match and similarity (if at least the floor), number of pages at or above the threshold, top 50 matches (FR-011, FR-013).
6. **Clusters** (FR-012): union-find over pairs at or above the threshold; cluster number by size, then by smallest member id, so numbers are stable for the same data; representative is the member with most inlinks, then shortest URL; similarity range from the pairs found.
7. **Write**: the three output tables (data-model.md) are replaced in one transaction.

### D5. Performance budget (constitution VI)

| Step (100k pages, 4-core container) | Estimate | Peak memory |
|---|---|---|
| Read signatures (96 MB, sequential by rowid) | 1 to 2 s | 96 MB if kept for scoring |
| Banding, 53 bands × 100k keys, radix sort | 1 s | 1.6 MB per band |
| Scoring candidates (typical site: under 10 candidates per page) | 1 s | pairs vector |
| Write outputs | 1 s | |

For 1M pages the signatures are 960 MB, so above 300,000 pages the pass does not keep them all in memory: it keeps band keys (8 bytes × pages for the current band) and reads signatures for scoring through a memory-mapped read of `content_sig` ordered by `url_id`, caching the most recent 256 MB. That keeps SC-002's 2 GB ceiling. Measured numbers go to `benchmarks.md` in the code PR.

Threshold, floor and indexable-only changes rerun only steps 1 to 7 from stored signatures (SC-006). Selector changes rerun extraction of main content from `bodies` (zstd) in parallel, rewrite `content_sig` and `pages.main_*` for writable files (temp copies for read-only ones), then rerun the pass (SC-007); without stored HTML the settings apply from the next crawl (spec edge case).

### D6. Settings

`CrawlConfig.content: ContentSettings { threshold: u8 = 90, report_floor: u8 = 80, min_words: u32 = 50, indexable_only: bool = true, content_selector: Option<String>, exclude_selectors: Vec<String> }`, `#[serde(default)]`, so older profiles load unchanged. Validation in `crates/config/src/validate.rs`: ranges, floor ≤ threshold, selectors parse (scraper's selector parser, a crate already in the workspace). Content settings stay editable while a crawl runs, like issue settings, because they never affect fetching; selector changes apply to pages fetched afterwards and mark the analysis stale. The pass records the settings it used under its entry in `meta.analysis`.

### D7. Results: Content tab, Clusters view, inspector

- **Content tab** (FR-017): a `TabDef` like the 007 tabs, `eligible` = internal HTML 200 pages, columns `main_words`, `closest_sim`, `closest_url`, `near_count`, `cluster`, `exact`, filters All, Near duplicates, Exact duplicates (the 009 rule's condition, plan D2 of 009), Not compared, Low content. Conditions read `near_dup_pages` (or `temp.near_dup_pages`). Before the pass has run the columns show "Not analysed yet" and the filters explain why, through a new `Need::NearDupAnalysis`.
- **Clusters view** (FR-019): a second view of the Content tab (segmented control "Pages / Clusters"), rows from `near_dup_clusters`, windowed with the 007 grid; opening a cluster filters the pages view to that cluster.
- **Inspector** (FR-018): a "Duplicates" section listing `near_dup_pairs` for the selected page, sorted by similarity, with "Compare" buttons and "50 of N shown" when capped.
- **Export** (FR-021): the 007 export for the tab view; two new export kinds, `near_dup_pairs` and `cluster(n)`, through the same CSV writer.

### D8. Comparison view

`compare(a, b)` in the results crate reads both pages' `bodies`, re-runs the main content function from D1 with the crawl's settings (same code, so the text matches what was compared), splits into paragraphs, aligns paragraphs by an LCS over paragraph hashes, and then diffs aligned paragraphs word by word with a small LCS (paragraphs are short; a page pair is a few thousand words, well under the 1 s of SC-005). It returns paragraphs with spans marked equal, added, removed; the UI renders two columns with the design-system diff tokens. "Show full page text" returns the excluded parts too, marked. Files without stored HTML return `unavailable: "html_not_stored"` with the similarity (US2.5).

### D9. The rule

`content.near_duplicate` in `issues/rules/content.rs` (spec 009 catalogue): scope pages, priority Medium, threshold key `content.threshold` read from the content settings (not a second threshold), kind SiteWide, needs `[Need::NearDupAnalysis]`, condition `u.id IN (SELECT url_id FROM near_dup_pages WHERE near_count > 0)`, evidence from the same row (closest URL, similarity, count, cluster). While the crawl runs or before the pass has run it reports "available when the crawl ends" (009 D12).

### D10. Command line

`spiderlens duplicates <file> [--threshold N] [--min-words N] [--include-non-indexable] [--pairs | --clusters | --pages] [--format csv|json] [--output PATH]`. Overrides run the pass into `temp` (the file is not changed); without overrides it reads the stored results, running the pass first if stale (into the file if writable, as `spiderlens analyse` would). Output columns in [contracts/cli.md](contracts/cli.md). Same functions as the app, so SC-008 parity is a test over both.

### D11. UI

Follows spec 017/018: the Content tab is a normal results tab; the Clusters view uses the 007 grid; the comparison opens as a large sheet with two scrollable columns synchronised by paragraph, keyboard: `[` and `]` move between differences, `Esc` closes. Diff colours are two new tokens (`--diff-added`, `--diff-removed` with their text pairs) in `tokens.css`, checked by the contrast test; changes are also marked with underline and strike so colour is never the only signal. Settings › Content in the crawl settings sheet: threshold and floor sliders with numeric fields, minimum words, indexable only switch, content selector, exclude selectors list with inline validation.

## Spec amendments (for the owner's word)

1. **Reporting floor default 80% instead of 50%** (FR-011, US1.4). Finding every pair down to 50% needs four times as many bands and on template-heavy sites produces many candidates that are never shown above the threshold; 80% keeps the analysis inside SC-001 and still shows "close but under 90%" pages. Users can set it down to 50%.
2. **Exact counts for groups of up to 50,000 closely similar pages** (FR-013). Above that the count is shown as "at least N" from a sample, because an exact count is quadratic in the group size (a 200,000-page group is 2 × 10¹⁰ comparisons).

Both are already written into spec.md in this PR.

## Project Structure

```text
crates/engine/src/extract/
  main_content.rs   MainCollector (buffers, boilerplate rules, selectors), paragraphs
  shingle.rs        tokens (with no-space scripts), 5-word shingles, OPH signature, densification
  walk.rs           feeds MainCollector from the existing text and element hooks
crates/engine/src/crawl/
  store.rs          content_sig insert, pages.main_* columns, data_features += near_dup
  migrate.rs        one additive step (number at merge)
crates/config/src/
  model.rs          ContentSettings
  validate.rs       ranges and selector checks
crates/results/src/near_dup/
  mod.rs            public API: summary, pairs, clusters, compare
  lsh.rs            band parameters, keys, buckets, dense groups
  score.rs          signature comparison, top-k per page, union-find, clusters
  pass.rs           the Analysis pass (spec 009 D12)
  diff.rs           paragraph alignment and word diff
crates/results/src/tabs.rs     Content tab and filters; Need::ContentSig, Need::NearDupAnalysis
crates/results/src/issues/rules/content.rs   content.near_duplicate (after spec 009 lands)
crates/cli/src/duplicates_cmd.rs
crates/test-fixtures/src/near_dup.rs         fixture site with known groups
app/src-tauri/src/results.rs   near_dup_clusters, near_dup_pairs, compare_pages commands
ui/src/content/   ClustersView, InspectorDuplicates, CompareSheet, diff tokens use
ui/src/settings/  ContentSettings section
```

## Coordination

- **Spec 009** owns the Analysis runner and the catalogue. The pass and the rule are added as one file and one line each when PR 52 (009 code) is on main; until then the code PR runs the pass function directly from the crawl-end hook and the CLI, behind the same trait signature, and switches to the runner on rebase.
- **Spec 006** migration rule: one additive step, number at merge.
- **Spec 014**: when rendering is on, the walk over the rendered DOM feeds the same collector, so signatures come from the rendered page (FR-006); 014 code calls `extract_page` on rendered HTML already.
- **Spec 010** report reads the per-page and cluster tables for its duplicate section.

## Complexity Tracking

None. No new crate, no new dependency, no new process.
