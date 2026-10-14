# Research: Crawl Report and Exports

## R1. Excel writer

**Decision**: `rust_xlsxwriter` with `Workbook::set_constant_memory` (per-worksheet `constant_memory` mode).

**Why**: pure Rust (no C toolchain on Windows), writes XLSX only (what we need), streams rows to temporary files so memory stays flat for a 1,000,000-row sheet, supports frozen panes, number and date formats, and internal hyperlinks for the overview sheet. MIT or Apache-2.0.

**Alternatives**: `umya-spreadsheet` keeps the whole workbook in memory (fails SC-005); writing SpreadsheetML by hand is a zip of XML parts we would have to maintain; CSV-only would miss FR-019.

## R2. PDF

**Decision**: render the report HTML in a hidden web view and use the operating system's print-to-PDF.

**Why**: FR-017 requires Hebrew, Arabic and CJK text to render correctly. Rust PDF writers (`printpdf`, `genpdf`) place glyphs but do not shape complex scripts or reorder right-to-left text; doing that would mean bundling HarfBuzz-level shaping and fonts. The web view shapes text with system fonts and already renders the HTML report identically to the file the user can send. Every supported OS has a built-in PDF printer.

**Trade-off**: one OS dialog to confirm, and the CLI cannot produce a PDF (stated in the spec's Assumptions). Headless Chromium from spec 014 could later print PDFs from the CLI without a dialog once rendering is installed; noted as a follow-up, not built now.

## R3. Score formula

**Decision**: per-page worst-issue penalty, weights 1.0 / 0.5 / 0.2 / 0, score = 100 × (1 − mean penalty).

**Why**: bounded 0 to 100, explainable as "share of pages free of problems, weighted by severity", insensitive to how many rules happen to fire on one page (a page with ten title issues is not ten times worse for the site), and the effect of a fix is computable (gain). Sum-of-issues formulas penalise rule-rich categories and fall below zero on large sites; ratio-of-issues-to-rules formulas hide that most pages may be broken.

**Breakdown**: shares (equal split of each page's penalty among its worst-priority issues) add up exactly; gains (penalty without that issue) predict fix effects. Both are shown, top issues ordered by gain.

## R4. Report HTML without a template engine

**Decision**: a hand-written renderer over the `Report` AST with one escaping writer.

**Why**: the output is one page with a handful of block kinds; a template engine (askama, tera) adds a dependency and a second language for the same few loops. The escaping writer makes the safe path the only path; tests feed titles containing `<script>`, quotes and `</style>`.

## R5. Token colours in the report

**Decision**: `crates/results/build.rs` reads `ui/src/styles/tokens.css`, extracts the light theme custom properties, and emits them as a Rust string constant; the HTML style block uses `var(--…)` with those definitions.

**Why**: one source of colour (the spec 017 rule that only `tokens.css` holds colours), and printing needs the light theme regardless of the app's current theme. If the file layout changes, the build fails loudly instead of drifting.
