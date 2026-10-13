# Implementation Plan: Page Data Extraction

**Branch**: `005-page-extraction` | **Date**: 2026-10-13 | **Spec**: [spec.md](./spec.md)

## Summary

Today the engine parses each HTML page once, only to find links, the title
and page-level nofollow (`crawl/links.rs`), and stores status, content type,
redirect target, title and bare link rows. This feature replaces that
partial pass with one complete extraction pass per page that produces
everything the spec lists (response facts, titles, descriptions, headings,
word count, directives, canonicals, hreflang, links with context, images,
structured data, compressed raw HTML), and stores it in new tables of the
crawl file that spec 007's results table reads directly.

The design goal besides correctness is speed: extraction must not
noticeably slow the crawl (spec SC-003 sets a floor of 500 URLs/s; we hold
ourselves to staying within 20% of today's benchmark).

## Technical Context

- Rust 2024, Tokio multi-thread runtime, reqwest 0.12, SQLite via rusqlite
  (WAL, group commits every 200 ms from spec 008).
- One coordinator task owns the crawl file; fetches run as separate tasks
  that already parse the page today (`links::extract` inside the fetch task).
- HTML parser already in the tree: `scraper` 0.24 (html5ever, the WHATWG
  tree builder used by Servo). Encoding detection already follows the
  browser order (BOM, header, `<meta>` prescan, default) in `title.rs`.
- New dependencies: `zstd` (raw HTML compression) and `xxhash-rust`
  (stable 64-bit hashes stored in the file). Both justified below.
- Reference numbers: 4-core cloud container, release build, bench site on
  the same machine. Baseline on main `5834f0a`: **100,000 pages in 25.6 s
  (about 3,900 URLs/s), crawl file 155 MB**.

## Constitution Check

| Principle | How this plan complies |
|---|---|
| I. Spec before code | Spec approved 2026-10-11; this plan and tasks go to the owner before code. |
| II. Local-first | Nothing leaves the machine; extraction uses only the served bytes (FR-035). |
| III. Engine independent of shell | All extraction and storage is in `spiderlens-engine`; the CLI gets the same features (`--no-store-html`, `spiderlens page`, `spiderlens html`). No UI file is touched. |
| IV. Respectful crawling | No extra requests: no scripts, no sub-resources fetched (FR-035). |
| V. Correct and complete data | Every HTML page ends with extracted data or a recorded content problem (SC-007); partial pages are flagged, not dropped. |
| VI. Scales by design | Data goes straight to disk per page; per-page memory is bounded by a byte budget (D3); performance impact measured before and after (D9). |
| VII. Test-first | Every acceptance scenario maps to a fixture page and test (tasks.md); the malformed-HTML corpus is checked against real Chromium output (D8). |
| VIII. Simplicity | Two new small dependencies, both justified; no new crate; no new setting except the one the spec requires (raw HTML on/off). |

No violations.

## Design decisions

### D1. One parse, one tree walk (html5ever), not a streaming rewriter

Each HTML page is parsed once into a DOM with html5ever (through
`scraper`) and walked once, depth first, collecting every field in the same
pass. Today's code runs five CSS selector passes plus a title pass over the
tree; one walk replaces them.

Measured per page (parse, then parse + full walk):

| Page | Size | Parse | Parse + walk | lol_html (streaming) |
|---|---|---|---|---|
| Bench site page | 3 KB | 55 µs | 50 µs | 27 µs |
| npm docs page | 76 KB | 2.1 ms | 2.3 ms | 1.1 ms |
| Go language spec | 293 KB | 3.4 ms | 4.1 ms | 1.7 ms |

*Alternative rejected*: `lol_html` (Cloudflare's streaming rewriter) is
about twice as fast, but it tokenizes without running the HTML tree
construction rules, so a `<meta>` after stray body text, a missing
`</title>` or misnested tags come out differently from a browser. FR-006
and User Story 4 require browser-equal structure, and the walk is not where
the time goes (D9), so correctness wins.

*Alternative deferred*: a custom html5ever `TreeSink` that extracts while
parsing without building the DOM. It would save the allocation of the tree
(maybe 30%), at the cost of reimplementing ancestor tracking. Only worth it
if D9's numbers miss the target.

*Analogy*: like using the browser's own DOM parser rather than a regex-based
scanner; slower than a scanner, but the answers match what users see.

**Parsing mode**: scripting disabled, so `<noscript>` content is parsed as
markup and its links and images are extracted and flagged (spec edge case).
Word count excludes noscript text, because most visitors have scripts on.

### D2. Extraction is a pure function; the crawl only moves its result

New module `crates/engine/src/extract/` exposes
`extract_page(&PageInput) -> PageData`: bytes, URL, headers in, plain data
out, no I/O, no store access. That makes it deterministic (FR-032),
testable without a server, usable by `spiderlens check`, and later by AEO
checks re-running over stored HTML. `crawl/links.rs` is removed; the runner
maps `PageData.links` to discovery exactly as it maps `PageLinks` today.

Header-derived facts (X-Robots-Tag, Link canonical/hreflang) are extracted
by a separate function for every response, HTML or not (FR-003, FR-004).

### D3. Extraction runs off the network threads, under a memory budget

Today parsing runs inside the fetch task on Tokio's worker threads. A 50 MB
page (SC-004) would block a worker for seconds and stall other downloads.
So the fetch task hands the body to `spawn_blocking`, guarded by a byte
budget semaphore: at most 64 MB of HTML is being extracted at once (a 50 MB
page takes most of the budget and waits for its turn; small pages flow
freely). The DOM costs several times the HTML size, so this is what keeps
peak memory bounded no matter how many large pages arrive together.

Response time is taken **before** extraction. Today `took` also includes
parsing time; that is fixed here (FR-001).

*Trade-off*: a thread hop per page (a few microseconds) against never
stalling network I/O.

### D4. Storage: summary columns for tabs, child tables for lists

Rule: **facts about any response go on `urls`; facts from the HTML body go
on a 1:1 `pages` row; repeated items go in child tables keyed by
`url_id`.** Tabs in spec 007 read `urls JOIN pages` by primary key and never
touch child tables except for a filter or the detail pane.

- `urls` gains response facts: response time, size, resource type, redirect
  kind, and the header/meta indexability summary (noindex, nofollow,
  canonical).
- `pages` (one row per extracted HTML page) holds the tab columns: meta
  description, first two H1 and H2, counts, word count, language, hashes,
  flags. A narrow `urls` keeps the frontier (the hot table during a crawl)
  fast; tab queries pay one rowid join.
- Child tables: `page_text` (every title, meta description and heading in
  order), `directives`, `structured_data`.
- **Links, canonicals, hreflang, next/prev, meta refresh and images are all
  rows of the existing `links` table**, extended with context columns
  (anchor text, alt, rel, position, source, head/body, noscript, size
  attributes). They are all edges from a page to a URL, the frontier already
  writes one row per edge, and keeping one table means one insert per
  reference: extraction adds columns, not rows. Non-crawlable links
  (mailto, fragment-only, self, malformed) get a row with no target id and a
  reason (FR-024), instead of being dropped as today.
- `bodies`: compressed raw HTML, one row per page, keyed by `url_id`, kept
  out of every other table so scans never read it.
- `headers`: response headers per fetched URL, for the 007 detail pane (not
  required by spec 005, requested by spec 007; about 0.3 to 1 KB per URL).

Full schema with every column: [data-model.md](./data-model.md). The file
format version goes from 1 to 2 and `meta.extraction_version = 1` marks
files that have extraction data.

*Alternative rejected*: one JSON blob per page. Cheapest to write, but every
tab filter ("missing H1", "noindex") would have to parse JSON per row; at
1M rows that breaks spec 007's 2-second budget.

*Alternative rejected*: a generic key/value table for all extracted items.
Flexible, but untyped and needs a self-join per column in tab queries.

### D5. Duplicate detection: stored 64-bit hashes

`pages.title_hash`, `pages.meta_description_hash` and `pages.text_hash`
(FR-015 fingerprint) are xxh3-64 of the stored, whitespace-normalised text.
Title and description hashes have partial indexes, so spec 007's Duplicate
filters become an index group-by instead of a 1.4 s temp-table build at 1M
rows. xxh3 is chosen because its output is specified and stable across
platforms and versions; Rust's built-in hasher is not, and hashes live in
files that must reopen in later versions.

### D6. Raw HTML: zstd, level 3, compressed on the extraction thread

| Page | zstd-3 | deflate-6 (flate2, already a dependency) |
|---|---|---|
| 3 KB bench page | 15% of original | 15% |
| 76 KB docs page | 26%, 0.36 ms | 25%, 2.4 ms |
| 293 KB spec page | 28%, 1.2 ms | 27%, 11 ms |

Same ratio, 7 to 9 times faster, so `zstd` earns its place. It meets SC-005
(at most 30%). Stored bytes are the body after transfer decoding (gzip or
brotli removed), not re-encoded text, so retrieval is byte-identical
(FR-030). Retention is on by default with `extraction.store_html = false`
to turn it off (FR-031), stored with the crawl configuration.

*Cost to be aware of*: a 1M-page crawl of 75 KB pages keeps about 20 GB of
compressed HTML. The spec chose "on by default"; a future improvement is a
per-crawl zstd dictionary trained on the first pages, which typically
shrinks templated sites another 2 to 3 times.

### D7. Truncated and badly encoded pages are extracted, not skipped

- Over the size limit: today the fetch fails before reading. Now the first
  `max_body_mb` are kept, the page is extracted from them, flagged
  `partial`, and still recorded with the spec 004 content problem class.
- Connection dropped mid-body: bytes received so far are kept the same way
  (spec 004 will retry it; a successful retry replaces the data).
- Invalid bytes for the detected encoding: replaced with U+FFFD and the page
  flagged `encoding_errors` (FR-008).
- A panic inside extraction is caught; the page keeps its response facts and
  gets `extract_error` (FR-033).
- No content type: HTML is sniffed from the first bytes with the existing
  `is_html` (FR-005). XHTML is treated as HTML.

### D8. Browser truth for malformed HTML comes from Chromium

SC-002 asks for 98% agreement with a standards-following browser on 50+
malformed pages. The corpus (`crates/test-fixtures/malformed/*.html`) is
run once through headless Chromium (already installed in CI images and
dev containers, via Playwright) by `scripts/browser-golden.mjs`, which
writes the browser's title, description, headings, canonical and link list
to a committed `golden.json`. The Rust test compares against that file, so
CI needs no browser.

### D9. Performance budget and measurement

Where the time goes today: the coordinator thread, mostly SQLite work for
about 34 link rows per bench page (spec 008 benchmarks). Extraction adds,
per page, one `pages` row, about 3 `page_text` rows, one `bodies` row and
one `headers` row, plus wider link rows. Parsing cost moves off the
coordinator entirely (it already runs in fetch tasks).

Targets, measured with `bench-site --pages 100000`, Fast preset, before and
after, recorded in `benchmarks.md`:
- at least 3,100 URLs/s on this container (within 20% of 3,900 today);
- peak memory flat across the run;
- SC-004: one 50 MB page with 100,000 links extracted in under 10 s.

If the coordinator becomes the bottleneck, the fallback is to move SQLite
writes to their own thread fed by a channel (the "saving backpressure"
item left from spec 008), not to drop data.

## Spec points that need the owner's nod

1. **Transferred size** (FR-001 "size as received and after decoding"):
   reqwest removes `Content-Length` when it decompresses gzip/brotli, so
   for compressed responses only the decoded size is known. The plan stores
   decoded size always and the transferred size when the server sent it
   uncompressed. Getting the compressed wire size needs our own
   decompression layer; proposed as a later task.
2. **Crawl files made before this feature** (format 1) open for viewing;
   resuming them is refused with a message to start a new crawl. Spec 006
   only promises compatibility from the first public release, and none has
   shipped.
3. **The settings sheet toggle** for raw HTML (FR-031) is a UI change; this
   branch adds the setting, CLI flag and command contract, and hands the
   one switch to the results-table work, which owns the UI files.

## Project Structure

```text
specs/005-page-extraction/
├── plan.md, research.md, data-model.md, quickstart.md
├── contracts/cli.md            # --no-store-html, `spiderlens page`, `spiderlens html`
├── contracts/crawl-file.md     # tables and columns spec 007 reads
└── tasks.md                    # next step

crates/engine/src/
├── extract/                    # NEW: pure extraction (D1, D2)
│   ├── mod.rs                  # extract_page(), PageData
│   ├── walk.rs                 # the single tree walk
│   ├── text.rs                 # whitespace, word count, hashes
│   ├── directives.rs           # meta robots + X-Robots-Tag (FR-016, FR-017)
│   ├── link_header.rs          # Link header (moved from crawl/links.rs)
│   ├── microdata.rs            # FR-029
│   └── body.rs                 # zstd compress/decompress (D6)
├── crawl/links.rs              # REMOVED (replaced by extract/)
├── crawl/runner.rs             # spawn_blocking + budget, timing fix, page writes
├── crawl/store.rs              # schema v2, record_page()
├── crawl/reason.rs             # LinkKind: meta_refresh, next, prev
├── fetch.rs                    # partial bodies, headers, sizes
└── title.rs                    # keeps encoding detection and is_html
crates/config/src/model.rs      # extraction.store_html
crates/cli/src/                 # flag + page/html commands
crates/test-fixtures/           # extraction site, malformed corpus + golden.json
scripts/browser-golden.mjs      # regenerates golden.json with Chromium
```

## Complexity Tracking

None.
