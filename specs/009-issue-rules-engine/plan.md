# Implementation Plan: Issue Rules Engine

**Branch**: `009-issue-rules-engine` | **Date**: 2026-10-13 | **Spec**: [spec.md](spec.md)

## Summary

Issues are computed on the read side of a crawl file, in the `spiderlens-results` crate that the desktop app and the CLI already share (spec 007). Every rule is an entry in a static catalogue: a SQL condition over `urls u` (with thresholds read through a settings function), the scope it applies to, the data it needs, the columns and evidence to show, and its texts. The 007 filters that test the same thing become references to rules, so filter and issue cannot drift. Evaluation fills one temporary table of hits per data version, incrementally while a crawl runs, never touching the engine's write path. The rule set version and the issue settings used go into two `meta` keys. The only schema change is for readability (FR-031): two nullable columns on `pages`, filled by the extraction walk, as one migration step.

## Technical Context

- **Language**: Rust 1.97 (workspace), TypeScript 5 with React 19 (UI). No new dependencies.
- **Storage**: the crawl file (SQLite, spec 006). Reads through `CrawlStore::open_read_only` / the results `Reader`; temporary tables live in the reader connection's `temp` schema.
- **Where**: `crates/results/src/issues/` (catalogue, evaluation, settings resolution), `crates/config` (issue settings in `CrawlConfig`), `crates/cli` (`spiderlens issues`, `spiderlens rules`), `app/src-tauri/src/results.rs` (commands), `ui/src/issues/` (sidebar group, issue header, inspector section, Settings › Issues).
- **Testing**: Rust unit tests per rule against a fixture crawl file, an issues fixture site in `crates/test-fixtures`, CLI tests, Vitest and Playwright (fake transport), benchmark on the 100,000-page fixture.
- **Performance goals**: SC-002 (crawl speed at least 95% with rules on), SC-003 (100k final in 3 s, 1M in 30 s), SC-004 (2 s after a setting change).

## Constitution Check

| Principle | How this plan meets it |
|---|---|
| I Spec before code | Spec approved 2026-10-13. One amendment folded in before merge: mixed content covers images until scripts and styles are extracted. |
| II Local first | Everything runs on the crawl file; no network. |
| III Engine independent of the shell | Catalogue and evaluation live in `spiderlens-results`, used by both the app and the CLI; the UI only renders what the crate returns. |
| IV Respectful crawling | No fetching. |
| V Correct and complete data | Rules whose data is missing report "unavailable", never zero (FR-010); targets not fetched are "not checked" (FR-011). |
| VI Scales by design | Hits are a temporary table on disk, counts are SQL aggregates, views reuse the 007 windowing. Nothing loads the crawl into memory. Performance impact stated below. |
| VII Test first | Every rule has a positive and a negative fixture case; acceptance scenarios map to tests in [tasks.md](tasks.md). |
| VIII Simplicity | Rules reuse the 007 filter mechanism (SQL conditions as data) instead of a new rule language. One new temp table, no new crate, no new dependency. Readability adds two counters to an existing walk rather than a text-analysis library. |

## Design decisions

**D1. A rule is data, the 007 filter shape grown up.** In Rust terms a rule is a `struct` in a `static` slice, the way 007 defines `TabDef` and `FilterDef`; think of it as a typed JSON schema the compiler checks. Fields (data-model.md): `id`, `name`, `category`, `priority`, `scope` (one of a few named eligibility clauses: internal URLs, pages, images on pages, pages with hreflang, ...), `cond` (SQL boolean over `urls u`), `threshold` (`Option<Threshold { key, unit, default, min, max }>`), `need: &[Need]`, `derived: &[Derived]`, `kind` (PerUrl or SiteWide), `columns` (ids of existing 007 columns to show), `evidence` (SQL expression producing the evidence text), `standards` (optional references such as a WCAG 2.2 success criterion, used by spec 026), `why`, `fix`. Categories live in one file each (`issues/rules/titles.rs`, ...), and `CATALOGUE` concatenates them. Specs 011, 014, 015 and 016 add a file and a line, nothing else.

**D2. One definition for a filter and a rule (FR-005).** A 007 `FilterDef` gains an optional `rule: Option<&'static str>`; when set, its condition, need, derived tables and threshold come from the rule. The Page titles > Missing filter and the "Page title: missing" issue are then the same SQL. A test walks every filter that names a rule and asserts identical URL sets on the fixture crawl (SC-007).

**D3. Thresholds are a SQL function, not string formatting.** Conditions read `thr('title.over')`. The reader registers `thr` (not deterministic) over an `Arc<RwLock<ResolvedSettings>>`, so changing a threshold changes the next query with no SQL rebuilt and nothing user-typed ever reaches SQL text. Filter labels with a threshold become templates ("Over {n} characters") rendered from the same settings.

**D4. Evaluation fills `temp.issue_hits(rule INTEGER, url_id INTEGER, PRIMARY KEY (rule, url_id)) WITHOUT ROWID`, plus an index on `url_id`.** Rules are grouped by scope; each group is one scan of `urls` with `INSERT INTO temp.issue_hits SELECT r.idx, u.id FROM urls u JOIN temp.rule_list r ON r.scope = ? WHERE <scope> AND CASE r.idx WHEN 1 THEN (cond1) WHEN 2 THEN (cond2) ... END`. Turned-off rules are left out of `rule_list`. Counts are `GROUP BY rule`; a page's issues are one index lookup; an issue view is the 007 view machinery over `u.id IN (SELECT url_id FROM temp.issue_hits WHERE rule = ?)`. Evidence is computed per visible row by the rule's `evidence` expression, so it is never stored (FR-012 "shown without re-evaluating" holds because it is a single-row expression on rows already selected).

**D5. Incremental while a crawl runs (FR-007, FR-019).** The reader keeps a watermark (`max(fetched_at)` seen at the last pass). Every refresh (at most every 5 s, on the same timer as 007 counts): PerUrl rules are re-evaluated only for URLs fetched after the watermark (delete their hits, insert new ones), which covers retries updating a row in place; SiteWide rules (duplicates, broken links, hreflang return links, canonical and pagination targets, noindexed with many inlinks) are recomputed in full from their derived tables, which 007 already rebuilds per data version. When the crawl ends a final full pass runs and the provisional mark is dropped.

**D6. Rust-computed derived tables for the few rules SQL expresses badly.** Non-sequential headings (needs heading order from `page_text`), invalid hreflang codes (BCP 47 shape check) and non-descriptive anchor text (a phrase list) use SQL scalar functions registered in `sql.rs` (`headings_skip(url_id)` reads levels in order; `lang_code_ok(text)`; `weak_anchor(text)`). Hreflang return links use a derived table `temp.hreflang_pairs(from_id, to_id, lang)` built from `links WHERE kind = 'hreflang'`. Security headers use `has_header(headers, name)` over the stored header JSON.

**D7. Settings live in the profile and travel with the crawl (FR-021 to FR-024).** `CrawlConfig` gains `issues: IssueSettings { rules: BTreeMap<String, RuleOverride { enabled: Option<bool>, priority: Option<Priority>, threshold: Option<f64> }> }`, `#[serde(default, skip_serializing_if = "is_empty")]`, so only overrides are stored and old profiles load unchanged. Validation of ids and ranges happens in `spiderlens-results` (the catalogue owner); unknown ids are reported as notices and ignored. Issue settings, like speed and retry settings (spec 004), stay editable while a crawl runs, because they never affect fetching.

**D8. What the crawl file records (FR-013).** Two `meta` keys, written by the writer at run end and by the app when settings change on a writable file: `rules_version` (integer) and `issue_settings` (JSON of the overrides used). Meta is key/value, so **no migration step is needed**; read-only files are never written. Opening a file whose `rules_version` is older simply re-evaluates (evaluation is always live), and the Issues section shows "Rule set v{n}".

**D9. Which settings apply.** The desktop app evaluates with the active profile (spec US4 scenario 4). The CLI uses, in order: `--issues <file>` or `--profile <name>`, else the file's stored `issue_settings`, else defaults. Documented in the CLI contract.

**D10. UI.** A new "Issues" group at the top of the sidebar (above Crawl results) shows four priority rows with totals; expanding a priority lists its issues with counts (virtualised, keyboard navigable as a tree, `aria-expanded`). Selecting an issue opens the results table in an `issues` tab whose filter is the rule id: the 007 grid, column menu, search, copy and export work unchanged. An `IssueHeader` above the grid shows name, priority badge, threshold, why and fix (collapsible). The inspector gets an Issues section first in its list. A new optional "Issues" column (highest priority badge and count) is available on every tab. Settings › Issues joins the profile categories: search, rules grouped by category, a switch, a priority select and a threshold field per rule, "Reset to defaults". Priority colours come from new tokens in `tokens.css` (`--priority-high`, `--priority-medium`, `--priority-low`, `--priority-opportunity`) checked by the existing contrast test; colour is never the only signal (text label on every badge).

**D11a. Readability is counted during extraction, not re-parsed (FR-031).** The spec 005 walk already visits every visible text node to count words; it also counts sentences (runs ending in `.`, `!`, `?` followed by space or end, minimum 1) and English syllables (vowel groups, minus a silent final `e`, minimum 1 per word), only when the page's `lang` is absent or starts with `en`. Two nullable integers, `pages.sentence_count` and `pages.syllable_count`, are added by a migration step (number taken at merge, expected 8); the score is a SQL expression, `206.835 - 1.015 * words / sentences - 84.6 * syllables / words`, clamped to 0..100, NULL under 100 words. Older files have NULL counts, so the column and rule report unavailable through a new `Need::Readability` feature id. Cost: one byte-class check per character already being scanned; the benchmark task verifies the 005 speed is kept.

**D11. Exit code.** `spiderlens issues --fail-on <priority>` exits with a new code `ISSUES_FOUND`. Codes 0, 2 to 6 are taken; the number is taken at merge time as the next free one (expected 7), the same way migration steps are numbered, and recorded in the CLI contract.

**D12. The Crawl Analysis phase (shared with specs 010, 011, 012, 015, 020, 022, 023).** Some checks need a global computation over the whole crawl that is too slow to repeat on every open or every 5 s: Link Score (an iterative graph computation), near duplicates (similarity clustering), orphan pages (sitemap against link graph), AEO aggregates, the health score. Instead of each feature making its own pass, there is one phase with registered passes, and this spec owns it because the rule catalogue reads every pass's output and runs last.

- **A pass** is a value implementing a small trait, the way 007 tabs are values:

  ```rust
  pub trait Pass: Sync {
      fn id(&self) -> &'static str;                 // "link_score", "near_duplicates", "orphans", ...
      fn version(&self) -> u32;                     // bump to make old files rerun this pass
      fn after(&self) -> &'static [&'static str];   // passes whose output it reads
      fn needs(&self) -> &'static [Need];           // crawl data it reads; missing = skipped with reason
      fn budget(&self) -> Budget;                   // target per 100k URLs, stated in its own spec
      fn run(&self, cx: &mut PassContext<'_>) -> Result<PassOutcome>;
  }
  ```

  `PassContext` gives a connection, the output schema (`main` or `temp`), a progress sink (`cx.progress(done, total, label)`), a cancel flag checked between batches, and the crawl's settings. Passes are listed in one static `PASSES` slice in `crates/results/src/analysis/`; a feature adds its pass file and one line.
- **Outputs** are tables the pass owns, named after it (`link_score`, `near_dup_groups`, ...), created by that feature's own migration step (number at merge, per the Storage rule) and replaced in one transaction per pass. Meta key `analysis` holds `{ pass_id: { version, data_version, ms } }`.
- **When it runs**: automatically when a crawl finishes or is stopped, by the process that holds the writer lock (app or CLI), before the lock is released; and on demand for an opened file whose `analysis` meta is missing a pass, has an older pass version, or was made before more URLs were crawled (resume). During a crawl, passes do not run; rules that read pass outputs report "available when the crawl ends" (shown like provisional, never as zero).
- **Order and rerun**: topological order of `after`, ties by `PASSES` order; always a full rerun of a stale pass (the computations are global), and a pass reruns its dependants. The issue rule evaluation (D4) is not a pass: it stays live on temp tables so settings changes apply in seconds, and it reads pass outputs like any other table.
- **Read-only viewers** (view-only files, spec 006): passes run into the reader connection's `temp` schema with the same table names, so the results are visible for the session but nothing is written; the Issues section says "Analysis not saved: file is read-only". If a pass's budget for that file size would exceed 60 s, the viewer offers the run instead of starting it.
- **Progress and cancel**: one progress event stream (`analysis_progress { pass, label, done, total }`) shown in the status bar as "Analysing: Link Score 40%"; starting or resuming a crawl cancels the phase, which leaves earlier completed passes in place. The CLI prints the same lines and has `spiderlens analyse <file>` to run stale passes.
- **Budgets**: each pass states its target in its spec; the phase logs the measured time per pass to the crawl log (spec 006 events) so slow passes are visible. This spec's evaluation stays within SC-003 on top of the passes.

## Structure

```text
crates/results/src/
  analysis/         Pass trait, PASSES, runner (order, staleness, temp or main schema, progress, cancel)
  issues/
    mod.rs          Rule, Priority, Scope, Threshold, CATALOGUE, RULES_VERSION, lookups
    settings.rs     ResolvedSettings from IssueSettings; validation; notices
    eval.rs         temp.issue_hits, scope groups, incremental watermark, counts, per-URL issues
    rules/          one file per category: response.rs links.rs titles.rs meta.rs headings.rs
                    content.rs images.rs canonicals.rs directives.rs hreflang.rs
                    pagination.rs structured.rs security.rs url.rs
  tabs.rs           FilterDef.rule; filters that duplicate rules point at them; `issues` tab
  sql.rs            thr, headings_skip, lang_code_ok, weak_anchor, has_header
crates/engine/src/extract/text.rs  sentence and syllable counting (readability)
crates/engine/src/crawl/migrate.rs  step: pages.sentence_count, pages.syllable_count
crates/config/src/model.rs     IssueSettings, RuleOverride, Priority
crates/cli/src/issues_cmd.rs   spiderlens rules / spiderlens issues
app/src-tauri/src/results.rs   issues_summary, url_issues, rule_catalogue, set_issue_settings
ui/src/issues/                 IssuesNav, IssueHeader, InspectorIssues, IssuesSettings, priority.ts
crates/test-fixtures/src/issues.rs   the issues fixture site (one positive and one negative page per rule)
```

## Performance impact (constitution VI)

- **Crawl write path**: unchanged. Evaluation runs on the reader connection, on the UI refresh timer, in WAL mode, so it never blocks the writer. The cost is CPU on the same machine: measured by running the 100,000-page benchmark (spec 008) with a reader evaluating every 5 s; target at least 95% of the speed without it (SC-002). If it misses, the fallback is to evaluate SiteWide rules only every 15 s while running (counts stay marked provisional anyway).
- **After the crawl**: one scan per scope group (about 10 groups), each with PK probes into `pages`; expected under 1 s per 100,000 URLs on the 4-core container, measured in the benchmark task and recorded in `benchmarks.md`.
- **Memory**: the hit table is on disk in the temp schema (`temp_store = FILE`); about 16 bytes per hit.

## Testing

- **Per rule**: `issues/rules/*` each have a table-driven test over the issues fixture crawl: expected URLs and evidence for the positive pages, absence for the negative ones (SC-001). A catalogue test checks every rule has non-empty texts, a unique id, a valid scope and known column ids (SC-008).
- **Parity**: filter versus rule on the fixture crawl (SC-007); app command versus CLI JSON output (FR-009).
- **Live**: a test crawl stores pages in batches and checks provisional then final counts, and a retried URL changing its hits in place.
- **Older files**: fixture files of format 1 and pre-extraction format 2 show unavailable rules with reasons (US6).
- **CLI**: `rules`, `issues` text and JSON, `--issue` export parity with the app, `--fail-on` exit codes (SC-009).
- **UI**: Vitest for the sidebar group, header, inspector section, settings page, keyboard and screen-reader names; Playwright for the click-through from issue to rows to inspector and back, and a threshold change updating counts.
- **Bench**: SC-002 to SC-004 on the 100,000-page fixture and a synthetic 1,000,000-URL file (`results::synth`).

## Coordination

- **Schema**: one additive step for the two readability columns (D11a), number taken at merge as main's latest plus one, per the Storage thread's rule. Issues themselves need no schema (D8).
- **Spec 004** error classes are read through `u.error_class`; rules for "no response" use the classes as stored after PR 29.
- **Specs 011, 014, 015, 016** add rule files to `issues/rules/` with the shape in D1; ids are namespaced by category (`sitemap.*`, `render.*`, `aeo.*`, `agent.*`).
- **Exit code** number taken at merge (D11).
- **Analysis phase** (D12): this spec ships the runner with no passes of its own; specs 010, 011, 015, 020 and 022/023 add their passes. The app's status bar and the CLI `analyse` command come with the runner.
