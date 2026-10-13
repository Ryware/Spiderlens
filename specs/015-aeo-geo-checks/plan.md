# Implementation Plan: AEO and GEO Checks

**Branch**: `015-aeo-geo-checks` | **Date**: 2026-10-13 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `specs/015-aeo-geo-checks/spec.md` (approved 2026-10-13 with the recommended defaults) and [research.md](./research.md).

## Summary

Add an AI search area that answers, per page and per site: can AI crawlers reach it, can they read it without JavaScript, and is it easy to quote and trust. Everything is computed by a new analysis step that reads the finished crawl file and writes its own tables. Nothing runs in the fetch loop except three small per-host file fetches.

The approach:

- A new library crate, `crates/aeo`, holds the analysis: the AI crawler access matrix, llms.txt and other site files, content checks on stored HTML, dates, authors, entities, structured data validation, rules and scores. It has no Tauri dependency; the app and the CLI both call `aeo::analyze`.
- The AI crawler list is a data file in `crates/config` (`ai_bots.json`), versioned and embedded at build time. It is the one list SpiderLens has; spec 016 reads it from there.
- Every check is a rule in the spec 009 rule shape plus three fields of our own (score part, points, basis). The score is a pure integer function of rule outcomes and a scoring profile, so the same file always gives the same bytes (SC-006).
- Spec 014 is used only through its `render_comparisons` table. When the table or a page's row is missing, the readability checks fall back to raw-HTML evidence and say so.

Speed first: the crawl pays three extra requests per internal host (`/llms.txt`, `/llms-full.txt`, `/.well-known/tdmrep.json`), issued after robots.txt through the normal host limiter and never in front of page fetches. robots.txt gains line numbers and a list of non-rule lines, which is parse-once-per-host work. No per-page work is added to fetch, extraction or the group commit. Task T070 checks SC-001 on the 100,000-page benchmark.

## Technical Context

**Language/Version**: Rust 2024 (toolchain from `rust-toolchain.toml`, workspace `rust-version` 1.97), TypeScript 5 with React 18 for the UI.

**Primary Dependencies**: existing only: rusqlite, scraper/html5ever, zstd, serde/serde_json, regex, jiff, url, ts-rs, Tauri 2. No new crates (research R6, constitution VIII). Parallel workers use `std::thread::scope`, not a thread-pool crate.

**Storage**: the crawl file (SQLite). One additive migration step (expected Step 7; the number is taken at merge time, latest on main plus one).

**Testing**: `cargo test --workspace`; a new AI fixture site in `crates/test-fixtures` (robots.txt variants, llms.txt variants, article pages written to pass or fail each check); golden JSON results for SC-004, SC-006, SC-007; Vitest for every UI change and one Playwright flow for SC-009; the 100,000-page benchmark from spec 008 for SC-001 and SC-002.

**Target Platform**: Windows 10+, macOS (Intel and Apple Silicon), mainstream Linux desktops; desktop app and CLI.

**Project Type**: desktop app (Tauri) plus CLI over shared Rust libraries.

**Performance Goals**: crawl throughput and peak memory within normal run-to-run variation with the feature on (SC-001); analysis of a 100,000-page crawl with stored HTML in at most 60 s on the reference machine (SC-002).

**Constraints**: memory during analysis within the crawl's own peak; the app stays responsive (analysis on worker threads, results written in batches); output byte-identical across platforms.

**Scale/Scope**: up to 1,000,000 URLs per crawl file; about 25 built-in AI crawlers plus user entries; about 45 checks.

## Constitution Check

| Principle | How this plan complies |
|---|---|
| I. Spec before code | Spec approved 2026-10-13. Plan, research and tasks go to the owner as a docs PR before code merges. |
| II. Local-first, no servers | Every check is offline and deterministic. The AI crawler list, Schema.org vocabulary and type rules ship inside the app. No API key, no account. The only network use is the crawl itself, the per-host site files and the opt-in probe. |
| III. Engine independent of the shell | `crates/aeo` depends on `spiderlens-engine` and `spiderlens-config`, never on Tauri. CLI parity: `spiderlens aeo run`, `summary`, `export`, `llms-txt`, `probe`, `bots` (contracts/cli.md). |
| IV. Respectful crawling | Site files go through the host limiter, crawl-delay and Retry-After like robots.txt, with SpiderLens's own user agent. The probe sends other vendors' user agent strings only after an explicit, labelled user action, honours the host limits and robots.txt for SpiderLens, and is capped at 10 pages per host by default. |
| V. Correct and complete data | Every check result is pass, fail, not applicable or not available with a reason; nothing is silently empty. Results carry the app version, crawler list version, vocabulary version and scoring profile. |
| VI. Scales by design | The page pass streams pages in id-ordered chunks with one read-only connection per worker; a single writer batches inserts. No step loads the crawl into memory. Performance impact below. |
| VII. Test-first | Every acceptance scenario maps to a fixture and test in tasks.md. UI tests are mandatory in every task that touches `ui/`. |
| VIII. Simplicity | One new crate, justified: the analysis is large, independent of crawling, and must be callable after the fact on any file. No new dependencies. The AI search tab is data in `tabs.rs` like every other tab. |

Result: no violations. Complexity Tracking records the new crate.

## Design decisions

### D1. `crates/aeo`: the analysis library

Public surface (contracts/aeo-crate.md):

```rust
pub fn analyze(file: &Path, opts: &AnalyzeOptions, progress: &dyn Fn(Progress)) -> Result<Summary>;
pub fn summary(file: &Path) -> Result<Option<Summary>>;     // read-only
pub fn page_result(file: &Path, url_id: i64) -> Result<Option<PageResult>>;
pub fn generate_llms_txt(file: &Path, opts: &LlmsTxtOptions) -> Result<String>;
pub async fn probe(file: &Path, opts: &ProbeOptions, progress: ...) -> Result<ProbeSummary>;
```

**It runs as a pass of spec 009's Crawl Analysis phase** (009 plan D12), not as a mechanism of its own. `crates/aeo` registers `AeoPass` implementing 009's `Pass` trait (`id = "aeo"`, `version`, `after = []` for now, `needs`, `budget`, `run(cx)`). The phase gives the connection, the target schema (main, or temp for a read-only file, which is then not saved), progress and cancel; it runs at crawl end before the writer lock is released, and on demand for old or stale files; `meta.analysis` records the pass version and data version, and a stale pass reruns fully. `analyze` above is the pass body (`run(cx)` calls it with `cx`'s connection); the AEO tables come from the pass's own migration step. The automatic run follows `ai_checks.auto` (FR-002, FR-003): when it is off the pass is registered but skipped until asked for. CLI: `spiderlens analyse` runs all passes, including this one; `spiderlens aeo run` runs only this pass. Until 009's phase code merges, the crawl's finish step calls `analyze_open` directly and task T061 moves it under the phase.

In Rust terms the crate is "functional core, imperative shell": every check is a pure function from a page's facts to an outcome (`fn(&PageFacts, &Profile) -> Outcome`), and only `analyze` touches SQLite. That keeps SC-004 and SC-007 testable with plain unit tests and fixtures.

### D2. The AI crawler list (`crates/config/src/ai_bots.rs`, `ai_bots.json`)

`ai_bots.json` holds `version`, `date` and one `[[bot]]` per crawler with the fields of FR-006 plus `fallback_tokens` (Applebot falls back to Googlebot) and `robots_may_not_apply`. It is embedded with `include_str!` and parsed once into `&'static [AiBot]` (a `LazyLock`, the Rust counterpart of a lazily initialised static in other languages). A unit test checks unique tokens, a documentation URL for every entry and a known purpose. User entries live in the app settings (`config::store`) and are merged at analysis time; built-in entries win on token clashes and the clash is reported. Each bot has a stable index inside a list version, used for the per-page bitmask in D5. The list's version is stored with the results, so a reopened file knows which bit means which crawler.

Spec 016 reads `config::ai_bots::all()`; the coordinator has told that thread.

### D3. Site files at crawl time (`crates/engine/src/crawl/runner.rs`)

When the coordinator learns a new internal host's robots.txt verdict (it already does this before the host's first page), it queues three site-file fetches for that host if `ai_checks.site_files` is on (default on). They are low-priority jobs in the same host allowance: a page fetch that is ready goes first; a site file never holds a slot a page is waiting for. Bodies are capped at 1 MB, decoded like pages, and stored in a new `site_files` table. A crawl that ignores robots.txt still fetches these files, since they are not rules.

This is the only crawl-time change. Its cost on the benchmark (one host) is three requests out of 100,000.

### D4. Three passes

1. **Hosts** (single thread, fast): for each internal host, parse the stored robots.txt with line numbers, evaluate every AI crawler's token with fallbacks for the home page, record content usage and licence lines, validate llms.txt (D9) and read TDMRep.
2. **Pages** (parallel): URL ids are split into chunks of 1,000. `N = available_parallelism() - 1` workers (at least 1) each open a read-only connection (`store::connect_read_only`), read a chunk's rows (`urls`, `pages`, `page_text`, `directives`, `structured_data`, `headers`, `bodies`, and `render_comparisons` when present), decompress the body, build `PageFacts`, run every page check and send a `PageRow` to one writer thread over a bounded channel (capacity 4 chunks, so memory stays flat). The writer inserts each chunk in one transaction. Robots matching for the access share runs here too: about 25 crawlers times 100,000 URLs is 2.5 million matches of a few microseconds each, spread over the workers.
3. **Site** (single thread): aggregate per-host blocked shares, site entity, about and contact pages, structured data coverage, score distribution, site score and top fixes from the page rows with SQL aggregates, not by loading pages.

Progress (`pages_done / pages_total`) is reported per chunk; the results tab reads rows as they are committed, so partial results show while the pass runs (FR-002).

### D5. Stored results (data-model.md)

- `site_files`: per host and path: status, final URL, content type, size, body, outcome.
- `aeo_hosts`: per host: robots availability, content signals and other lines as JSON, llms.txt validation as JSON, TDMRep.
- `aeo_bot_access`: per host and crawler: home page verdict, blocked count, crawled count, the deciding rule with line number.
- `aeo_pages`: per page: page type and reason, score, five part scores, number of checks used, provisional flag, `blocked_bots` (bitmask over the crawler list), `flags` (bitmask of failed checks that have a tab filter), version used per check group (raw or rendered), chosen published and modified dates, author, counts (question headings, direct answers, figures, quotations, citations, lists, tables, longest paragraph).
- `aeo_findings`: one row per failed or not-available check per page, with evidence JSON. Passes are not stored row by row; the explanation recomputes them from `aeo_pages` and the rule list, which keeps the table near the number of real problems.
- `aeo_site`: one row: site score, parts, distribution, entity summary, coverage and top fixes as JSON, plus the versions (app, crawler list, vocabulary, rules) and the profile used.
- `aeo_probe`: per host, crawler and URL: status, size, type, outcome compared with the crawl's response.

Filters in the results tab are SQL conditions on `aeo_pages.flags` and `blocked_bots` (`flags & 8 != 0`), one index-free integer test per row, the same cost as the existing filters.

### D6. robots.txt with line numbers and other lines (`crates/engine/src/robots.rs`)

`Rule` gains `line_no`. `RobotsTxt` gains `other_lines: Vec<OtherLine { line_no, key, value }>` for every non-empty line that is not `user-agent`, `allow`, `disallow`, `crawl-delay` or `sitemap` (so `Content-Signal`, `Content-Usage`, `License` and typos all show, FR-014). `for_agent_with_fallback(&[token, fallback...])` picks the first token that has its own group, else `*`. The crawl's own matching is unchanged; the existing robots tests keep passing and new tests cover the additions.

### D7. Content checks on stored HTML (`crates/aeo/src/content/`)

The page body is parsed once with the same html5ever pipeline spec 005 uses, into a `scraper::Html`. From it:

- **Main content**: `main`, else the single `article`, else `body` without `nav`, `header`, `footer`, `aside` and elements with `role` navigation, banner, contentinfo or complementary.
- **Question headings**: H2 to H4 text ending with `?` or starting with a question word from `questions.json` (English, Hebrew, Spanish, French, German, Italian, Portuguese, Dutch), selected by the page's `lang`, else by the site's most common language.
- **Direct answer**: the first element after the heading in document order that is a paragraph or list; its first sentence (split on `.`, `?`, `!` and their Hebrew and CJK counterparts) must be at most 60 words.
- **Structure**: paragraph word counts, subheadings per 1,000 words, lists and tables in the main content.
- **Facts with sources**: figures by regular expressions (numbers with `%`, currency signs, units, four-digit years), quotations (`blockquote` with `cite` or a following attribution, quotation marks followed by a name pattern), external links inside the main content.
- **Q&A sections**: `details`/`summary`, `dl`, question-heading runs, and FAQPage or QAPage items; structured questions are compared with the visible text after normalisation.
- **Bylines and dates**: `rel=author`, `itemprop=author`, elements whose class or id contains `author` or `byline`, `<time datetime>`, and visible dates near the H1 parsed with per-language month names (`dates.json`) into `jiff::civil::Date`.
- **Snippet markers**: counts of `data-nosnippet` and `data-snippet` elements.
- **Raw-only JavaScript evidence** (FR-017): text-to-HTML ratio, script bytes share, empty application containers (`#root`, `#app`, `#__next`, `[data-reactroot]` with no text), framework markers.

Not-available rules (FR-025): when `pages.html_stored = 0`, the checks that need the DOM return `NotAvailable("page HTML not stored")`.

### D8. Structured data validation (`crates/aeo/src/schema/`)

A build script (`crates/aeo/build.rs`) reads a checked-in, trimmed Schema.org vocabulary file (`vocab/schemaorg-<version>.json`: types with parents, properties with domains and expected types, about 300 KB) and emits a compact `phf`-free table as Rust source: sorted arrays searched with `binary_search`, so lookups are cheap and no new dependency is needed. The required and recommended properties for the 20 types in research.md R4 live in `type_rules.json`. JSON-LD is parsed with `serde_json` (the raw text is already stored); `@graph`, nested items and `@type` arrays are followed; microdata items come from spec 005's stored items. Messages carry a JSON pointer to the value.

### D9. llms.txt validator and generator (`crates/aeo/src/llms.rs`)

The validator is a line-based reader of the llms.txt proposal: optional BOM, the first non-empty line must be an H1, an optional `>` block, body lines without headings, then H2 sections whose list items must be `- [name](url)` with an optional `: notes`; the H2 `Optional` is recognised. Every problem carries a line number. Each link is resolved against the file's URL and joined to `urls` to get its crawl status, canonical, indexability and robots verdicts; links not in the crawl are listed as "not crawled" and, when in scope, checked with one request each through the host limiter during the next run (FR-045). The generator groups indexable canonical pages by first path segment, keeps the most linked-to pages first (`links_to` counts), and writes the draft text.

### D10. Readability and spec 014 (`crates/aeo/src/readability.rs`)

```rust
pub struct RenderFacts { pub dependence: f64, pub dep_headings: Option<f64>, pub dep_internal_links: Option<f64>,
    pub dep_structured: Option<f64>, pub raw_has_title: bool, pub raw_has_description: bool,
    pub raw_has_canonical: bool, pub raw_has_directives: bool }
```

Read from `render_comparisons` when the table exists and has a row for the page; `None` otherwise. Checks that care take `Option<&RenderFacts>` and use the raw evidence of D7 when it is `None`. For crawlers that run JavaScript (Googlebot, bingbot, Applebot) the content checks use rendered values when the page was rendered (FR-016), by reading `rendered_pages` and `rendered_structured_data` instead of the raw tables. Until spec 014's code merges, the table does not exist and every page takes the raw path; tests for the rendered path use a fixture file with a hand-made `render_comparisons` table.

### D11. Rules and scoring (`crates/aeo/src/rules.rs`, `score.rs`)

Each check is a `static` entry:

```rust
pub struct AeoRule { pub rule: RuleDef /* spec 009 FR-001 shape */, pub part: Part, pub points: u16,
    pub applies_to: PageTypes, pub basis: Basis /* VendorDoc | Research | CommonPractice, with source URL */ }
```

`RuleDef` is spec 009's type once that code merges; until then a local struct with the same fields, swapped in by task T060. Points are integers out of the part maximum (Access 30, Readable 25, Answer readiness 20, Trust 15, Structured data 10). The page score is `round(100 × earned / possible)` over applicable, available checks, in integer arithmetic (hundredths), so every platform gives the same number. Blocked for every AI search crawler caps it at 20 (FR-035); fewer than half the checks available marks it provisional (FR-036). The site score is `(70 × mean page score + 30 × site checks score) / 100`, also in integers. Top fixes: for each failed check, `gain = points lost across pages × 70 / (100 × pages)` plus its site share; sorted by gain, then pages affected, then rule id for a stable order.

The scoring profile (weights per part, thresholds) is a struct in `crates/config` stored in settings and copied into `aeo_site` with the results (FR-040).

### D12. Results, app and CLI

- `crates/results`: a new `Need::AiChecks` ("AI checks have not run on this crawl. Run them from the AI search tab."), the AI search tab and its filters as data in `tabs.rs` (FR-041), and `ai_overview`/`ai_detail` readers for the overview and detail section.
- App (`app/src-tauri/src/aeo.rs`): commands `aeo_run`, `aeo_cancel`, `aeo_summary`, `aeo_page`, `aeo_llms_txt_draft`, `aeo_probe`, `ai_bots_list`, `ai_bots_save_custom` (contracts/desktop-commands.md). The run streams progress events like the crawl does.
- UI (`ui/src/aeo/`): AI search overview (site score, parts, access matrix, llms.txt, signals, entity, coverage, distribution, top fixes), the AI search tab in the results table, the detail section, the probe confirm dialog, the llms.txt draft sheet, and Settings › AI search (automatic run, site files, scoring profile, custom crawlers). OKLCH tokens only; access verdict colours come from existing status tokens.
- CLI: `spiderlens aeo` subcommands (contracts/cli.md), JSON and CSV output, `--fail-under <score>` exits with spec 009's quality-gate code (7 unless 009 picks another).

### D13. Issue rules and the analysis phase (spec 009)

Spec 009 owns the rule table and the issue list. When its code merges, task T060 replaces the local `RuleDef` with 009's type and registers the AI rules in its static slice, so failed AI checks appear in the issue list, reports and exports, and the user's per-profile changes there (disable, priority, threshold) apply. Until then the AI search tab and overview show the findings alone. The AEO computation is a registered pass of 009's Crawl Analysis phase (D1); the AI rules are rules, not a pass.

### D14. Crawl file change: one additive step

Tables `site_files`, `aeo_hosts`, `aeo_bot_access`, `aeo_pages`, `aeo_findings`, `aeo_site`, `aeo_probe`, as one additive `migrate::STEPS` entry (all new tables, so `read_compat` is unchanged and older apps still open the file). The version is set at merge time (latest plus one; expected 7). A data feature id `ai_checks` is added to the `data_features` meta list when the analysis completes.

## Performance impact (constitution principle VI)

| Where | Cost with the feature on | Measured by |
|---|---|---|
| Crawl, per page | none | T070, SC-001 |
| Crawl, per internal host | 3 small requests after robots.txt, low priority; robots.txt parse keeps line numbers | T070 |
| After the crawl | page pass in parallel: decompress and parse each stored body once (spec 005 measured about 3,300 pages per second per core for the same parse), plus checks | T071, SC-002 |
| Opening a file | none: results are stored | |

Estimate for SC-002 on the 8-core reference machine with 7 workers: about 4 s of parsing, about 6 s of checks and robots matching, about 5 s of writes, well under 60 s. The benchmark runs the page pass alone and in a full crawl.

## Spec points that need the owner's nod

None open: Ryware approved the spec with the recommended defaults for the four decisions in research.md (probe included and off by default; weights 30/25/20/15/10; llms.txt generator included; eight languages including Hebrew).

## Open risks

- **Visible date parsing** in eight languages can mislabel dates; mitigated by only using dates near the H1 or byline and preferring structured data and `<time>`, and by the "No visible date found" outcome rather than guessing.
- **Page type classification** drives which checks apply; wrong types skew scores. The reason is shown and users can override by URL pattern (FR-019).
- **The crawler list ages quickly.** It is a data file with its own version, so updates are one-file pull requests with a test.
- **Schema.org vocabulary size**: trimmed to types, properties and expected types; pending/attic terms are flagged as warnings rather than errors.

## Project Structure

### Documentation (this feature)

```text
specs/015-aeo-geo-checks/
├── spec.md
├── research.md
├── plan.md              # this file
├── data-model.md
├── quickstart.md
├── contracts/
│   ├── aeo-crate.md
│   ├── cli.md
│   ├── desktop-commands.md
│   └── ai-bots.md
├── checklists/requirements.md
└── tasks.md
```

### Source Code (repository root)

```text
crates/config/src/
├── ai_bots.rs           # AiBot, all(), merge with custom entries
├── ai_bots.json         # the built-in list (research R1)
└── aeo_profile.rs       # scoring profile and thresholds
crates/engine/src/
├── robots.rs            # + line numbers, other lines, fallback tokens
└── crawl/
    ├── runner.rs        # + site-file fetches per internal host
    ├── migrate.rs       # + one step
    └── store.rs         # + site_files writes
crates/aeo/              # new
├── build.rs             # Schema.org tables
├── vocab/               # trimmed vocabulary, type_rules.json
├── data/                # questions.json, dates.json
└── src/
    ├── lib.rs           # analyze, summary, page_result
    ├── access.rs        # robots per crawler, meta and header directives, signals
    ├── content/         # main content, questions, answers, structure, facts, Q&A, bylines, dates
    ├── readability.rs   # RenderFacts and raw evidence
    ├── schema/          # vocabulary, validation, coverage
    ├── entity.rs        # site entity, about and contact pages
    ├── llms.rs          # validator and generator
    ├── probe.rs         # user agent probe
    ├── rules.rs         # the AI rules in the 009 shape
    ├── score.rs         # page and site scores, top fixes
    └── store.rs         # reading facts, writing results
crates/results/src/      # Need::AiChecks, AI search tab, overview and detail readers
crates/cli/src/aeo_cmd.rs
crates/test-fixtures/src/ai.rs   # AI fixture site
app/src-tauri/src/aeo.rs
ui/src/aeo/              # overview, tab wiring, detail section, dialogs, settings
```

## Complexity Tracking

| Addition | Why needed | Simpler alternative rejected because |
|---|---|---|
| New crate `crates/aeo` | About 45 checks, a vocabulary table and a validator that run after the fact on any crawl file, from the app and the CLI | Putting it in `crates/engine` would grow the crawler's compile time and mix post-crawl analysis with the fetch path the owner wants kept fast; putting it in `crates/results` would make the read-only results crate write files |
