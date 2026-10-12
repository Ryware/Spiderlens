# Implementation Plan: Crawl Frontier and Scope

**Branch**: `003-frontier-scope` | **Date**: 2026-10-11 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `/specs/003-frontier-scope/spec.md`

## Summary

Spec 003 turns SpiderLens from a single-page checker into a crawler. It decides,
for every discovered URL, whether it is crawled and when, and records a single
reason for every URL that is not crawled.

The approach adds a `crawl` module to `spiderlens-engine` with five parts that
can each be tested alone:

1. **Link normalization** (in `spiderlens-config`, next to the start-URL
   normalizer): one canonical form per URL, with the query-parameter rules.
2. **Scope rules**: one pure function that applies the FR-038 reason order. The
   crawler and the scope tester call the same function, so they cannot disagree.
3. **robots.txt policy**: our own RFC 9309 parser and matcher, replacing
   `texting_robots`, because the spec needs the exact rule line that blocked a
   URL (FR-026) and that crate does not expose it (research R4).
4. **Frontier store**: the queue, the seen-URL set, links and not-crawled
   reasons live in a SQLite crawl file from the first URL (FR-043). Memory holds
   only a bounded window of the queue.
5. **Scheduler and runner**: a single coordinator task owns the store and
   dispatches fetches in breadth-first order, subject to per-host concurrency,
   crawl-delay and the global rate cap. Workers fetch one URL each (no inline
   redirect following) and send results back over a channel.

The CLI gains `spiderlens crawl` (new and `--resume`), `spiderlens report` (the
"Not crawled" filter and per-reason counts) and `spiderlens scope test` (the
scope tester). No desktop UI is built in this feature: UI work waits for the
design direction thread, and the desktop commands are specified only.

## Technical Context

**Language/Version**: Rust 2024, toolchain 1.97 (unchanged).

**Primary Dependencies (new)**:
- `rusqlite` with `bundled` (SQLite compiled in, same version on every OS).
- `quick-xml` (sitemap parsing, streaming).
- `flate2` (gzip sitemaps).
- `tokio` features `sync`, `signal` (control channel, Ctrl-C in the CLI).
- Removed: `texting_robots` (replaced by `robots` module, research R4).

**Storage**: one SQLite file per crawl (`*.spiderlens`), WAL journal, one
transaction per processed page. Spec 006 owns the final file format and adds the
results tables; this feature creates the file and its frontier tables.

**Testing**: unit tests per module (normalization, scope order, robots matcher,
sitemap parser, frontier window), integration tests against an extended fixture
site with several host names resolved to 127.0.0.1, CLI contract tests.

**Target Platform**: unchanged (Windows, macOS Intel and Apple Silicon, Linux).

**Performance Goals**: no measurable cost of scope decisions at 500 URLs/s on
the fixture (SC-006); a bounded queue window so memory does not grow with the
number of discovered URLs (SC-007). Full-scale benchmarks (100k and 1M URLs)
are built in spec 008; this feature adds a smaller guard test (research R10).

**Constraints**: robots.txt honoured by default; never more than one request in
flight to a host with a crawl-delay; never two fetches of one normalized URL.

## Constitution Check

| Principle | How this plan complies | Status |
| --- | --- | --- |
| I. Spec before code | Spec 003 approved 2026-10-11. Three conflicts with spec 002 are resolved in research R12 and reported to the owner. | Pass |
| II. Local-first | The crawl file is a local SQLite file. No servers. | Pass |
| III. Engine independent of shell | All logic is in `spiderlens-engine` and `spiderlens-config`; the CLI is a thin layer. | Pass |
| IV. Respectful crawling | robots.txt before any request to a host, crawl-delay honoured with one request in flight, rate cap applied globally. | Pass |
| V. Correct and complete data | Every discovered URL ends in exactly one state; reasons follow one fixed order in one function. | Pass |
| VI. Scales by design | Queue and seen set on disk; bounded in-memory window; batched writes. | Pass |
| VII. Test-first gates | Each scenario mapped to a test in [quickstart.md](./quickstart.md); fixture site only. | Pass |
| VIII. Simplicity | One new storage dependency that spec 006 needs anyway; one crate removed. The robots matcher is our own code, justified in R4. | Pass |

## Project Structure

### Documentation

```text
specs/003-frontier-scope/
├── plan.md, research.md, data-model.md, quickstart.md, tasks.md
└── contracts/
    ├── cli.md                 # crawl, report, scope test
    ├── crawl-summary.schema.json
    └── desktop-commands.md    # specified only; UI on hold
```

### Source Code

```text
crates/config/src/
├── model.rs            # Scope gains query rules, nofollow, folder, sitemaps, URL length; Limits gains list_depth
└── url_norm.rs         # normalize_link, QueryRules (FR-001..FR-006)
crates/engine/src/
├── robots.rs           # RFC 9309 parser and matcher with matched rule (replaces texting_robots)
├── fetch.rs            # + fetch_once (no redirect following) for the crawler
└── crawl/
    ├── mod.rs          # public API: CrawlOptions, run, CrawlControl, CrawlEvent, CrawlSummary
    ├── reason.rs       # NotCrawledReason, DiscoverySource, LinkKind
    ├── scope.rs        # ScopeRules::classify, the FR-038 order; scope tester
    ├── links.rs        # discovery from HTML and headers (minimal; spec 005 owns full extraction)
    ├── sitemap.rs      # sitemap and sitemap index parsing, gzip
    ├── store.rs        # SQLite crawl file: urls, links, hosts, sitemaps, meta
    ├── window.rs       # bounded in-memory queue window
    ├── hosts.rs        # per-host slots, crawl-delay, robots state
    └── runner.rs       # coordinator loop
crates/test-fixtures/src/
└── site.rs             # multi-host link-graph site with request log
crates/cli/src/
├── main.rs             # crawl, report, scope test
└── options.rs          # new scope flags
```

## Complexity Tracking

| Item | Why | Simpler alternative rejected because |
| --- | --- | --- |
| Own robots.txt matcher | FR-026 and FR-016 need the rule line | `texting_robots` hides the matched rule; forking it is more code to own than a 250-line matcher |
| SQLite now rather than in 006 | FR-043 requires the queue on disk in this feature | An in-memory queue would be thrown away and would fail SC-007 |
