# Research: Content Checks (spec 027)

Date: 2026-10-14. Crate data from crates.io on this date; measurements on the cloud build container (x86-64, release build), so laptop numbers will differ but not by orders of magnitude.

## R1 (A) How to measure pixel width without system fonts

- **Decision**: a committed, generated table of advance widths from open fonts metric-compatible with what search results draw (Liberation Sans for Arial; Noto Sans Hebrew and Noto Sans Arabic), title at 20 px and description at 14 px, no kerning; per-script fallback widths for anything missing.
- **Rationale**: identical results on every OS (system font measuring would differ on Linux), no font files shipped, cheap enough to run as a SQL function per displayed row. Advance widths are facts about a font; Liberation Sans is designed to match Arial's widths exactly.
- **Alternatives**: measure in the webview with canvas `measureText` (only works in the UI, not in the CLI or rules, and depends on installed fonts); ship a font and a shaping engine such as `rustybuzz` (correct kerning and ligatures, but a large dependency for a ±2% gain).
- **Limits** (defaults, user-adjustable via 009 thresholds): title 600 px desktop, 580 px mobile; description 990 px desktop, 680 px mobile; short title under 200 px. These are the widths SEO tools commonly use in 2026; search engines change them, which is why they are thresholds.

## R2 (B) Where to evaluate segments

- **Decision**: in Rust, into a temp table on the reader connection.
- **Rationale**: keeps user-written patterns out of SQL (009 plan D3), reuses the `regex` crate and its size limits, and one scan is fast: a rough estimate of 2 to 4 µs per URL gives 2 to 4 s for 1M URLs, inside SC-B1's 30 s.
- **Alternatives**: SQL `CASE` built from user input (injection risk and slow `REGEXP`); a stored `segment` column in `urls` (needs a schema step and a rewrite on every edit, and segments are a view, not a crawl fact).

## R3 (C) PDF reading in Rust

| Option | Licence | Notes |
|---|---|---|
| `lopdf` 0.45 | MIT | Mature (24M downloads), parses structure, Info dictionary, XMP, annotations, encryption flags. |
| `pdf-extract` 0.12 | MIT | Text extraction on top of `lopdf`; known to panic on some malformed files, so calls run under `catch_unwind`. |
| `pdfium-render` | MIT/Apache + PDFium binary | Best text quality but needs a native PDFium library per platform: rejected (shipping and signing cost). |
| `mupdf` bindings | AGPL | Rejected (licence). |

- **Decision**: `lopdf` + `pdf-extract`, per-page extraction with a deadline check, `catch_unwind`, watchdog for the per-document time limit.
- **Size cap** 20 MB by default: most web PDFs are under 5 MB; memory per read is bounded at about 3 to 4 times the file size (SC-C3).

## R4 (D) Offline spelling, including Hebrew

Measured with `spellbook` 0.4.2 and the Hunspell dictionaries packaged in `wooorm/dictionaries`:

| Test | Result |
|---|---|
| Load English (`en`, 552 KB) | 12 ms |
| Load Hebrew (`he`, 5.7 MB `.dic`, 79 KB `.aff`) | 132 ms |
| English: `recieve`, `acommodate` | flagged; first suggestions `receive`, `accommodate` |
| English: `colour` with `en_US` | flagged (British spelling needs `en_GB`; dialect follows `lang="en-GB"`) |
| Hebrew prefixes: ושלום, והשלום, ובבית, שבבית, כשהלכתי, ומהמחשבים | all accepted |
| Hebrew misspellings: שלוםם, מחשבב | flagged |
| Hebrew both spellings: ספרייה, ספריה | accepted |
| Hebrew with niqqud: בָּיִת | **flagged**: niqqud must be stripped before lookup (FR-D05) |
| Speed, English | 180,000 word checks in 6 ms (about 35 ns per word) |
| Speed, Hebrew | 130,000 word checks in 11 ms (about 80 ns per word) |

- So 10,000 pages of 500 words is about 5M checks, well under 1 s of checking: SC-D2's 2 minutes is dominated by reading and decompressing stored HTML, not by the dictionary.
- **Decision**: `spellbook` (MPL-2.0: file-level copyleft, compatible with an MIT app as an unmodified dependency). Dictionaries as separate data files.
- **Licences of dictionaries**: English (`en`, SCOWL-based) is permissive and bundled. Hebrew comes from Hspell and is **AGPL-3.0**: it is downloaded on request as a data file with its licence shown, never linked or compiled into the app. Each other language's licence is listed in the release manifest; any with unclear terms are left out of the list.
- **Alternatives**: `zspell` (MIT/Apache, but last release 2024 and slower); `hunspell` C library via FFI (system dependency on three OSes, `unsafe`, against `unsafe_code = "forbid"` in spirit); LanguageTool (Java runtime or a server: rejected by constitution II).

## R5 (D) Grammar

- **Decision**: `harper-core` 2.11 (Apache-2.0), English only.
- **Measured**: on "This is an book. I went to the the store. Their is a problem." it reports the article, the repetition and their/there.
- **Why English only**: no offline, permissive grammar checker covers Hebrew or most other languages at usable quality (LanguageTool needs Java or a server; `nlprule` covers English/German/Spanish and is unmaintained). The app says so per page (FR-D10).

## R6 (E) AMP validation

- **Decision**: native core checks on the DOM already parsed by 005; no official validator.
- **Rationale**: the official validator is a JavaScript program meant for Node or a browser; bundling it would add a JS runtime path for a feature the spec ranks lowest. The core checks catch the faults that make AMP pages fail most often (missing boilerplate or runtime, custom scripts, oversized CSS, broken canonical pairing).
- **Alternative**: run the validator in the 014 headless browser when JS rendering is on: possible follow-up, not in this spec.

## R7 Dictionary distribution

- **Decision**: a `dictionaries.json` manifest built in CI from a pinned commit of `wooorm/dictionaries`, published with each release; dictionaries uploaded as release assets; the app downloads by URL from the manifest and verifies SHA-256.
- **Rationale**: constitution II allows static release hosting; pinning makes downloads reproducible; hashes stop tampered files.
- **Users without network**: point to `.aff`/`.dic` files on disk (macOS and Linux often have Hunspell dictionaries installed by office suites; Linux under `/usr/share/hunspell`), FR-D04.
