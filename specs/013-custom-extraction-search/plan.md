# Implementation Plan: Custom Extraction and Custom Search

**Branch**: `013-custom-extraction-search` | **Date**: 2026-10-14 | **Spec**: [spec.md](./spec.md)

## Summary

Users define two kinds of rules in the crawl configuration: **extractors**
(CSS selector, XPath, regex, JSON-LD path) that return values, and
**searches** (text or regex, contains or does not contain, in the HTML
source or the visible text) that count matches. The engine compiles the
rules once per crawl, evaluates them on the document spec 005 already
parsed for each page, and stores per page and rule a summary row plus the
values. The same evaluator runs afterwards over stored HTML ("Run on this
crawl"), behind the single-page tester, and from the CLI. The results
crate gets two dynamic tabs whose columns come from the rules stored in
the crawl file.

The design goal besides correctness is speed: zero cost with no rules
(SC-001, within 2%), at least 85% of throughput with a typical 30-rule set
(SC-002), and reruns of 100,000 pages in under 2 minutes (SC-003).

## Technical Context

- Rust 2024, Tokio, rusqlite (WAL, 200 ms group commits, spec 008).
- Extraction (spec 005) runs in `spawn_blocking` under a 64 MB byte budget:
  one html5ever parse via `scraper` 0.24, one iterative walk
  (`extract/walk.rs`). The walk already builds the visible text (for word
  count) and keeps JSON-LD blocks raw.
- `scraper` already brings the `selectors` crate (Servo's CSS engine,
  Selectors Level 4 for static documents) and `regex` is already a
  workspace dependency.
- Schema changes go through `crates/engine/src/crawl/migrate.rs` steps
  (spec 006, PR #27). Step numbers are taken at merge time (main's
  latest + 1), never reserved.
- Reference numbers: bench-site 100,000 pages on the 4-core container,
  29-31 s on main after spec 005 (about 3,300 URLs/s, `specs/005-page-extraction/benchmarks.md`).
  The baseline is re-measured on the branch's merge base before code
  changes (T001).

## Constitution Check

| Principle | How this plan complies |
|---|---|
| I. Spec before code | Spec approved 2026-10-13; this plan and tasks go to the owner before code. |
| II. Local-first | Rules run locally on the user's data. The only network use is the single fetch the user asks for when testing on a typed URL (FR-018). |
| III. Engine independent of shell | Compilation, evaluation, storage and reruns live in `spiderlens-engine`; the results tabs in `spiderlens-results`; the CLI gets `--rules`, `spiderlens rules test` and `spiderlens rules run`. |
| IV. Respectful crawling | No extra requests during a crawl or a rerun (FR-012, FR-029). |
| V. Correct and complete data | Every eligible page and rule ends with a summary row (match, no match, timed out, error, not available). Rule definitions and run metadata are stored in the file (FR-010). |
| VI. Scales by design | Results go to disk per page with the page's other rows; values are capped per page (FR-019); reruns stream pages in id order and commit in batches; the tabs read through primary-key lookups. |
| VII. Test-first | Each acceptance scenario maps to a fixture page and test (tasks.md); CSS and XPath results are checked against Chromium on a 50-page corpus (SC-005). |
| VIII. Simplicity | One new dependency family for XPath and one for JSONPath, both justified in research.md; no new setting outside the rule list itself. |

No violations.

## Design decisions

### D1. Rules ride on the spec 005 parse

`walk::walk` currently drops the parsed `Html` when it returns. It is
changed to `walk::walk_with(text, page, token, budget, hook)`, where the
hook receives the finished document view after the walk:

```rust
pub struct DocView<'a> {
    pub doc: &'a scraper::Html,     // the one parse
    pub source: &'a str,            // decoded HTML text
    pub visible: &'a str,           // the walk's visible text (word count)
    pub json_ld: &'a [StructuredBlock],
}
```

`extract_page` gains `rules: Option<&CompiledRules>`; with `None` the code
path is today's, byte for byte (no hook, no allocation): FR-027, SC-001.
With rules, `custom::apply(rules, &view)` runs inside the same blocking
task, so it shares the byte budget and never blocks network I/O.

Analogy for the owner: the parse is the AST; spec 005's walk and every
rule are visitors over the same AST. Nothing parses twice (FR-028).

### D2. Compile once per crawl

`custom::compile(&RuleSet) -> Result<CompiledRules, Vec<RuleError>>` turns
the configuration into ready-to-run matchers: `scraper::Selector` for CSS,
`regex::Regex` (with `(?i)` when ignore case) for regex rules and plain
text searches (escaped), a compiled XPath, a compiled JSONPath. The
crawler holds `Arc<CompiledRules>` and hands a clone to each fetch task.
Validation (FR-008) is the same function: `config validate`, the settings
sheet and crawl start all call it and report errors with positions.

### D3. XPath through `sxd-xpath` over a converted tree

html5ever's tree has no XPath. Options are compared in research.md R1. The
choice: when at least one rule uses XPath (an XPath extractor, or a search
limited by an XPath element), the parsed `scraper` tree is copied into an
`sxd-document` tree (elements in no namespace, so `//div` matches HTML
elements as in browsers; attribute names already lower case), keeping a
`HashMap<sxd Element, ego NodeId>` so Inner and Outer HTML are serialised
by `scraper` exactly like CSS results. Copying is linear in the node count
and done once per page, only for crawls that use XPath. `sxd-xpath`
implements XPath 1.0 including the function library, which is what
browsers' `document.evaluate` supports (FR-003).

Cost estimate (measured in T003 before committing to it): copying a
typical 2,000-node page costs a fraction of its parse.

### D4. JSON-LD path

Two forms (research.md R2):

- **Item path** (the default in the UI): an optional `@type` filter plus a
  dotted path such as `offers.price` or `author.name`. Items are the
  top-level objects of each parseable block, array elements and members of
  `@graph`. `@type` matches a string or any element of an array.
- **JSONPath** (expression starts with `$`): RFC 9535 JSONPath via
  `serde_json_path`, applied to each block.

Values that are objects or arrays are stored as compact JSON; strings as
is; numbers and booleans as text. Blocks that are not valid JSON are
skipped and flagged (spec edge case).

### D5. Visible text and element limits

Searches in visible text read `DocView::visible` (the same text as word
count, FR-007). A search limited to an element (US6) concatenates, for
every element the limit selects, the visible text of that subtree
(script, style, template and comments skipped, whitespace collapsed),
or its outer HTML when the search target is HTML. Absent element:
`element_not_found` flag and count 0.

### D6. Evaluation budget

Regex evaluation is linear in input size (`regex` never backtracks), CSS
matching is linear in nodes, JSONPath is linear in JSON size. XPath can be
quadratic (`//*[count(//*) > 1]`). Each page has a rule budget (default
2 s, research R4): the clock is checked between rules and between matches
collected; once exceeded, the current rule and remaining rules for that
page are recorded as `timed_out` and skipped. An XPath evaluation already
in progress cannot be interrupted by `sxd-xpath`; it holds one blocking
thread until it returns, while the crawl keeps fetching and extracting on
the other threads. SC-007 is tested with a pathological regex and CSS
rule; the XPath case is documented in the rule editor ("complex XPath can
be slow on large pages").

### D7. Storage (data-model.md)

Three tables, added in one additive `migrate.rs` step:

- `custom_rules(id, kind, name, def_json, pos, last_run_at, run_pages, run_state)`:
  the rules of this crawl and their last run (FR-010, FR-026).
- `custom_results(url_id, rule_id, count, flags, first)` WITHOUT ROWID,
  primary key `(rule_id, url_id)`: one row per eligible page and rule;
  `first` is the first value (extractors) for table columns and sort.
- `custom_values(url_id, rule_id, seq, value)` WITHOUT ROWID: every stored
  value (extractors, up to 100) or match in context (searches, up to 10).

Rows are written in the same transaction as the page's other rows
(`record_page`), with cached statements. Keying `custom_results` by
`(rule_id, url_id)` makes a rule's column and its filters range scans, and
lets a rerun replace one rule with `DELETE ... WHERE rule_id = ?`.

Rule ids are stable for the life of a crawl: a rename keeps the id; an
edited definition keeps the id and is marked stale until rerun.

### D8. Reruns from stored HTML

`custom::rerun(path, rule_ids, progress, cancel)` opens the crawl file for
writing (refused for read-only files, spec 006), deletes the rules'
results, then streams `bodies` in `url_id` order, decompresses (zstd),
decodes, parses and applies the rules on a worker pool (one thread per
core, bounded channel), committing every 2,000 pages. Progress and cancel
go through a callback and an atomic flag. Pages without a stored body get
`not_available`. A cancelled run leaves `run_state = 'partial'` with the
processed count (FR-014). Reruns parse with the same code path as the
crawl (`extract_page` with rules), so values are identical (FR-015).

Speed check: parse plus walk is about 3,300 pages/s on 4 cores during a
crawl; a rerun has no network, so 100,000 pages should take about
40-60 s, under the 2-minute target (SC-003). Measured in T020.

A rerun while the same file is being crawled is refused; while paused it
is allowed (FR-016).

### D9. Dynamic tabs in the results crate

Tabs are `&'static` definitions today. `TabDef`, `ColumnDef` and
`FilterDef` move their text fields to `Cow<'static, str>`, and
`Reader::tabs()` returns the static tabs plus two tabs built from the
file's `custom_rules`: **Custom extraction** (Address, then per extractor
`first` and a hidden "count" column) and **Custom search** (Address, then
per search its count). Filters: per search "Name: contains / does not
contain" on `flags & MEETS`; per extractor "Has value", "No match", "More
matches than stored". Expressions are correlated primary-key lookups into
`custom_results`, like the existing content columns. Columns are numeric
when every non-null value parses as a number (spec Assumptions), decided
once per data version. The detail pane gets a "Custom" section listing
all values and search contexts (FR-024). CSV export gains a
`--all-values SEP` option (FR-025).

This touches the results crate owned by the results thread; the change is
mechanical (`Cow` fields) and announced to that thread before code.

### D10. Configuration

`CrawlConfig` gains `custom: CustomRules { extractors: Vec<Extractor>,
searches: Vec<Search> }` with `#[serde(default)]`, so older profiles and
crawl files load with no rules. Names unique per kind, at most 100 each.
Rule files are the same JSON as `custom` (`spiderlens-rules` envelope with
a version). ts-rs exports the types to `ui/src/generated`.

### D11. Desktop app and UI

- Settings sheet: new category **Custom rules** with two lists
  (Extraction, Search), an editor row per rule with inline validation
  errors (FR-008), import and export of rule files, and a **Test** panel:
  pick a crawled page (stored HTML, no request) or type a URL (one fetch
  with the crawl's identity and robots setting), results per rule within a
  second (FR-018, SC-006).
- Rules are read-only while a crawl runs (FR-016); paused crawls show
  "Run changed rules on earlier pages".
- Results: the two tabs appear through the existing tab list; a **Run on
  this crawl** action with progress and cancel (FR-012, FR-014).
- Tauri commands in `app/src-tauri/src/custom.rs`
  (contracts/desktop-commands.md); the fake transport in
  `ui/src/testing/fakeTransport.ts` implements them for Vitest and
  Playwright.
- Colours only from `ui/src/styles/tokens.css`.

### D12. Command line (contracts/cli.md)

- `spiderlens crawl --rules FILE` adds rules to the profile's rules.
- `spiderlens rules validate FILE`, `spiderlens rules test FILE (--url U | --crawl F --page U)`,
  `spiderlens rules run CRAWL [--rules FILE] [--only NAME...]` with progress on stderr.
- `spiderlens export --tab custom_extraction|custom_search [--all-values SEP]`.
- Exit codes: invalid rules use the existing configuration error code;
  a run on a file without stored HTML exits with the "not possible" code.

### D13. Rendered HTML (spec 014)

Not in this feature. When 014 merges, a per-rule `source: raw | rendered`
field (default raw) will feed the rendered document to the same
`custom::apply`. The `DocView` type is designed so that is a call-site
change only.

## Performance plan

| Case | Target | How measured |
|---|---|---|
| No rules | within 2% of baseline URLs/s and 5% of peak RSS (SC-001) | bench-site 100k, 3 runs each, branch vs merge base |
| Typical set: 5 CSS, 5 XPath, 5 regex, 5 JSON-LD, 10 searches | at least 85% of baseline (SC-002) | same bench with `--rules bench-rules.json` |
| Rerun typical set on 100k file | under 2 minutes (SC-003) | `spiderlens rules run` on the bench crawl file |
| Tester on one page | under 1 s (SC-006) | unit timing in the app command test |

Results go into `specs/013-custom-extraction-search/benchmarks.md`. If the
typical set misses 85%, the first levers are: skip the XPath tree copy
when only CSS is used (already in D3), run the regexes as one
`RegexSet` pre-filter on the source, and cap visible-text searches at the
text already built.

## Risks

- `sxd-xpath` is stable but unmaintained since 2019 (research R1). It is
  pure Rust, small, and isolated behind `custom::xpath`; a replacement
  would only touch that module.
- The results crate change (D9) overlaps with the results follow-ups
  thread; coordinated before code.
- Migration step number is taken at merge time (spec 006 rule).
