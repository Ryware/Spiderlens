# Research: Page Data Extraction

Measurements: 4-core container, release builds, scratch benchmark crate
(scraper 0.24, lol_html 2, zstd 0.13, flate2 1). Sample pages: a bench-site
page (3 KB), the npm `config` docs page (76 KB), the Go language spec
(293 KB). Real public sites are not reachable from the build container.

## R1. HTML parser

- **Decision**: html5ever through `scraper` (already a dependency), one
  parse and one depth-first walk per page, scripting disabled.
- **Rationale**: implements the WHATWG tree construction rules browsers use,
  required by FR-006 and User Story 4. Parse + walk: 50 µs / 2.3 ms / 4.1 ms
  on the three samples.
- **Alternatives**: `lol_html` streaming (about 2x faster: 27 µs / 1.1 ms /
  1.7 ms) rejected because it does not run tree construction, so malformed
  pages differ from browsers. `tl` and other fast tokenizers rejected for
  the same reason. A custom `TreeSink` deferred (plan D1).

## R2. Raw HTML compression

- **Decision**: `zstd` level 3, with a reused compressor per worker thread.
- **Rationale**: 15% / 26% / 28% of original size, 0.36 ms for 76 KB.
  Meets SC-005 (at most 30%).
- **Alternatives**: deflate level 6 via `flate2`: same ratio, 7 to 9 times
  slower (2.4 ms / 11 ms). zstd level 1: about 10% faster, 2 to 3 points
  worse ratio. Brotli: better ratio, far slower to compress. A trained
  zstd dictionary: much better on templated sites, deferred.

## R3. Stable hashes

- **Decision**: `xxhash-rust` (xxh3-64), stored as signed 64-bit INTEGER.
- **Rationale**: specified, stable output on every platform and version;
  hashes are persisted in crawl files and compared across them later.
- **Alternatives**: `std` `DefaultHasher` is explicitly not stable across
  releases. SHA-256 is stable but 10x slower and needs a BLOB column.

## R4. Where extraction runs

- **Decision**: `tokio::task::spawn_blocking` with a byte-budget semaphore
  (64 MB of HTML in extraction at once).
- **Rationale**: keeps Tokio's I/O workers free for downloads; bounds peak
  memory when large pages arrive together. Response time is measured
  before the hand-off.
- **Alternatives**: a `rayon` pool (new dependency, no gain over Tokio's
  blocking pool); extracting in the coordinator (serialises parsing with
  SQLite writes, the existing bottleneck).

## R5. Storage shape

- **Decision**: summary columns on `urls` (any response) and a 1:1 `pages`
  table (HTML body facts), child tables for lists, references in the
  existing `links` table, raw HTML in `bodies`.
- **Rationale**: tab queries read only rowid-joined summary rows; lists are
  read only by filters and the detail pane; no extra row per link.
- **Alternatives**: JSON per page (tab filters would parse JSON per row);
  key/value table (self-join per column); separate `canonicals`,
  `hreflang`, `images` tables (one more insert per reference and the same
  URL bookkeeping duplicated three times).

## R6. Encoding

- **Decision**: keep `title::detect_encoding` (BOM, then HTTP charset, then
  `<meta>` prescan of the first 1024 bytes, then UTF-8; an undeclared body that is not valid UTF-8 is decoded as windows-1252, the browser fallback in Western locales), decode with
  `encoding_rs`, use its "had errors" flag for `encoding_errors`.
- **Rationale**: same order as the WHATWG encoding sniffing algorithm
  (FR-007); already tested for windows-1255, Shift_JIS, BOM precedence.

## R7. Browser truth for the malformed corpus

- **Decision**: headless Chromium via Playwright (preinstalled in the dev
  container), driven by `scripts/browser-golden.mjs`; output committed as
  `golden.json`; Rust tests compare against it without a browser.
- **Rationale**: SC-002 is about agreeing with browsers, so a browser is the
  oracle. Committing the output keeps CI fast and deterministic.

## R8. Word count

- **Decision**: text nodes under `<body>`, excluding `script`, `style`,
  `template`, `noscript` and comments; split into words on Unicode
  whitespace and punctuation (characters that are not letters, digits or
  marks), so "don't" counts as 2 and "e-mail" as 2.
- **Rationale**: FR-014's definition; SC-006 within 1%.
