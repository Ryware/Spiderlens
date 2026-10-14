# Implementation Plan: Accessibility and Mobile Usability Checks

**Branch**: `026-accessibility-mobile` | **Date**: 2026-10-14 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `specs/026-accessibility-mobile/spec.md` (approved 2026-10-13 with the recommended defaults) and [research.md](./research.md).

## Summary

Two levels of checks feed one set of tables and one set of rules. **Static checks** are one more collector inside the single DOM walk of spec 005 (`crates/engine/src/extract/walk.rs`), switched on by a flag in `PageInput`, so "Off" costs one branch per page. **Full checks** run axe-core, and a small layout script of our own, as two *page probes* in the render stage of spec 014 (`RenderJob.probes`, the hook the 014 thread agreed on 2026-10-13). Results are written to four new tables in one additive migration step. Every check becomes a rule in the spec 009 catalogue (`crates/results/src/issues/rules/accessibility.rs` and `mobile.rs`), and two new tabs (Accessibility, Mobile) are `TabDef`s over the same tables. The code arrives in four PRs so the static subset, which needs no browser, ships first.

## Technical Context

**Language/Version**: Rust 1.97 (workspace `rust-toolchain.toml`), TypeScript 5 with React 19 for the UI.

**Primary Dependencies**: existing `scraper`/`html5ever` (spec 005 walk), `rusqlite` (spec 006 store), `spiderlens-render` (spec 014). New: `axe.min.js` from axe-core 4.14.0 (MPL-2.0), vendored unmodified under `crates/engine/assets/axe/` with its `LICENSE`, embedded with `include_str!`. No new Rust crate.

**Storage**: one additive step in `crates/engine/src/crawl/migrate.rs` (spec 006 PR #27), number taken at merge (latest step on main plus one).

**Testing**: `cargo test` with the accessibility fixture site in `crates/test-fixtures` (local only, constitution VII); browser tests use spec 014's shared helper that skips locally without a browser and fails in CI (`SPIDERLENS_REQUIRE_BROWSER=1`); Vitest and Playwright for `ui/`.

**Target Platform**: Windows, macOS, Linux (desktop app and CLI).

**Performance Goals**: SC-002 (Off within 2% and 5% memory of the baseline), SC-003 (Static at most 3% slower than Off), SC-004 (Full plus mobile at most 25% of median render time).

**Constraints**: no second parse of a page (FR-028); no extra page request (FR-008); per-page probe cap (FR-005, shared with 014's total probe cap, default 5 s); stored nodes capped at 20 per check and 500 per page (FR-010).

**Scale/Scope**: 100,000-page crawls; about 15 static checks, about 100 axe checks, 6 mobile checks.

## Constitution Check

| Principle | How this plan complies |
|---|---|
| I. Spec before code | Spec approved 2026-10-13 (Ryware). This plan and tasks go to the owner before the code PRs are merged. |
| II. Local-first | axe-core is embedded and runs in the local browser; nothing is sent anywhere. Documentation links open in the user's browser only when clicked (FR-023). |
| III. Engine independent of the shell | Collector in `spiderlens-engine`, probes in `spiderlens-engine` over `spiderlens-render`, rules and tabs in `spiderlens-results`; the CLI has every capability (FR-032). |
| IV. Respectful crawling | No extra page request. Fonts and images loaded for checks (FR-012) still go through spec 014's host allowance, robots.txt and cache. |
| V. Correct and complete data | Every checked page records one state (checked, timed out, not checked with reason); totals are stored even when nodes are capped; "needs review" is never counted as a violation; a page that timed out stores no partial list (FR-013). |
| VI. Scales by design | Results streamed to the batched writer (spec 008, 200 ms commits); readers query by index; nothing loads a whole crawl. Performance table below. |
| VII. Test-first | Every acceptance scenario maps to a task with a test (tasks.md). The fixture site has one positive and one negative page per static and mobile check and per tested axe check. |
| VIII. Simplicity | Static checks reuse the existing walk; full checks reuse the 014 hook and axe's own metadata; rules and tabs are data in the existing catalogue. The only new abstraction, the probe, belongs to 014 and is shared with 015, 016 and 025. |

## Design decisions

### D1. Static collector inside the spec 005 walk (research R4)

`PageInput` gains `a11y: bool`. When true, `walk()` creates an `A11yCollector` (new `crates/engine/src/extract/a11y.rs`) and calls `collector.element(e, ctx)` from `Walker::enter` for each HTML element and `collector.text(t)` while a button, label or link is open. At the end of the walk, `collector.finish(&data)` resolves what needs the whole document (labels pointing at a later `id`, `aria-labelledby` targets, duplicate `accesskey`s) and returns `A11yStatic { findings: Vec<StaticFinding>, viewport: Option<Viewport> }`, stored in `PageData.a11y: Option<A11yStatic>`. With `a11y: false` the field is `None` and no collector exists.

A `StaticFinding` is `{ check: StaticCheck, count: u32, nodes: Vec<Node { selector, excerpt }> }`, nodes capped at 20 per check. `StaticCheck` is a `#[non_exhaustive]` enum whose `as_str()` ids are the rule suffixes in D5 (`form_label`, `button_name`, `link_name`, `frame_title`, `lang_invalid`, `element_lang_invalid`, `zoom_disabled`, `viewport_missing`, `viewport_width`, `tabindex_positive`, `accesskey_duplicate`, `meta_refresh_delay`, `deprecated_element`, `plugin_element`, `area_alt`, `input_image_alt`). The four checks spec 009 already has (missing alt, missing lang, non-sequential headings, missing title) are not collected again (FR-007): 009 already reads them from the spec 005 data, and their rules carry `standards` tags (009 plan D1).

Accessible-name rules follow the subset of the W3C accessible name computation that the HTML alone can decide: `aria-labelledby` (targets resolved at finish), `aria-label`, then the element's own content (text, `img alt` inside, `value` for input buttons), then `title`. Anything that needs CSS (hidden by a stylesheet) is left to full checks; when a full check ran on the page, its result wins for the same concept (spec edge case "Duplicate checks"), recorded through a `same_as` axe id on each static check.

Selectors are built from the walk's own ancestry (`html > body > main > form:nth-of-type(2) > input#email`), short form when an `id` is unique so far. Excerpts are the element's start tag, cut to 300 characters.

*Analogy*: like adding a visitor to an AST pass in a compiler. One traversal, several visitors; turning a visitor off costs nothing.

### D2. Two probes in the render stage (research R2, R3)

`crates/engine/src/crawl/render/a11y_probe.rs` builds, per render job when the level is Full: the `a11y` probe (`axe.min.js` source plus a short runner script with the configuration from research R3, budget = the page budget minus the mobile probe's share) and, when mobile checks are on and the window is mobile, the `mobile_layout` probe (`crates/engine/assets/mobile_layout.js`, about 100 lines: document `scrollWidth` against `innerWidth`, the widest element, computed font size of visible text nodes weighted by character count, interactive element boxes with the WCAG 2.5.8 spacing test, at most 20 examples). It also adds `font` and `image` to `RenderJob.load_types` (FR-012).

Order inside the shared cap: `mobile_layout` first (milliseconds), then `a11y` with what remains. `on_rendered` parses each `ProbeOutcome::Ok(json)` into `A11yFull` and `MobileLayout`; `TimedOut` becomes state `timed_out` with `spent` (FR-013); `TooLarge` or `Failed` becomes `not_checked` with the reason. Pages flagged as a possible bot challenge are not given probes (FR-014): the challenge check runs on the serialised HTML before the probes, so the stage can drop them for that page.

The axe runner converts results in the page, before they cross CDP: violations and incomplete only, nodes capped (20 per check, 500 per page), excerpts cut to 300 characters, `any`/`all`/`none` check data reduced to the fields the detail pane shows (contrast: foreground, background, ratio, expected ratio, font size and weight). This keeps each result well under the 4 MB probe cap.

### D3. Storage: one additive migration step (research R5, data-model.md)

Tables `a11y_audits`, `a11y_results`, `a11y_nodes`, `mobile_measures`, keyed by `url_id`, plus meta keys `a11y_engine`, `a11y_settings` and the `data_features` entries `accessibility_static`, `accessibility_full`, `mobile_layout`. The writer gets one `Msg::A11y { url_id, static_part, full_part, mobile }` handled in the same batched transaction as the page (static) or the render record (full). Re-running checks on an existing file (FR-031) deletes the file's rows of the chosen level in one transaction, then writes the new ones; refused on view-only files (spec 006) and on files without stored HTML.

### D4. Settings: `CrawlConfig.accessibility` (`crates/config/src/model.rs`)

`AccessibilitySettings { level: Off|Static|Full (default Static), wcag: { version: 2.0|2.1|2.2 (2.2), level: A|AA|AAA (AA) }, best_practice: bool (true), mobile: bool (true), budget_secs: 1..=30 (5), small_text_px: 12, small_text_share: 40, tap_target_px: 24, overflow_px: 5 }`, `#[serde(default)]`, ts-rs export. Validation codes follow spec 002 (`accessibility.budget_secs.range`). A notice when `level = Full` and rendering is off ("Full checks need JavaScript rendering; static checks will run", FR-002). The WCAG target is not used while crawling (all tags are run and stored, research R3); it only selects what counts, so changing it on an open crawl needs no new check (FR-020) and goes through 009's `thr()`-style settings function as `wcag_target()`.

Default fallback (owner decision 1): if T040's benchmark shows Static costs more than 3%, the default becomes Off in the same PR, and the spec Assumptions line is updated.

### D5. Rules in the 009 catalogue

Two rule files, following 009's shape (009 plan D1):

- `issues/rules/accessibility.rs`: one rule per static check (`a11y.static.<id>`) and one per axe check (`a11y.axe.<axe-rule-id>`). Condition: `EXISTS (SELECT 1 FROM a11y_results r WHERE r.url_id = u.id AND r.check_id = '<id>' AND r.kind = 'violation' AND wcag_in_target(r.wcag))`. Priority from impact (FR-018), `standards` from axe's WCAG tags (`wcag143` becomes "WCAG 2.0 1.4.3 AA"), texts from axe's `help` and `description`, `fix` pointing to the Deque rule page. The axe rules are generated, not hand-written: `scripts/gen-axe-rules.mjs` loads the vendored `axe.min.js` in Node, calls `axe.getRules()` and writes `issues/rules/accessibility_axe.rs`; a test fails if the generated file does not match the vendored version (`AXE_VERSION` constant in both), so an axe upgrade is one PR that regenerates and bumps `RULES_VERSION` (FR-019). Axe checks that duplicate a 009 rule (`image-alt`, `html-has-lang`, `heading-order`, `document-title`) are not given their own rule: the 009 rule's condition is extended to also match a full-check violation of that axe id, so a page is counted once.
- `issues/rules/mobile.rs`: `mobile.viewport_missing`, `mobile.viewport_width`, `mobile.zoom_disabled`, `mobile.plugins`, `mobile.content_wider` (Medium), `mobile.small_text`, `mobile.small_targets`, with thresholds through `thr()`.

`Need::AccessibilityStatic`, `Need::AccessibilityFull` and `Need::MobileLayout` (detected from `data_features`) make rules report "unavailable" with a reason on crawls without that data (009 FR-010).

### D6. Results: tabs, detail pane, exports, CLI

- **Accessibility tab** and **Mobile tab**: `TabDef`s in `crates/results/src/tabs.rs` with the FR-021 and FR-022 columns and filters as SQL over the new tables. Filters per WCAG level use the same `wcag_in_target` function with a fixed argument.
- **Detail pane** (`crates/results/src/detail.rs`): sections `accessibility` and `mobile`, grouped by check, ordered by impact, with nodes, check data and the documentation link. Outline on the screenshot (FR-024) uses the stored box when the page has a screenshot (spec 014 `screenshots` table).
- **Disclaimer** (FR-025): one constant string shown by the tab header, the detail section, the CLI summary and the CSV/JSON export header comment.
- **Exports** (FR-026): four export kinds in `crates/results/src/export.rs` (`a11y_violations`, `a11y_violations_level`, `a11y_needs_review`, `a11y_summary`), CSV and JSON.
- **CLI**: `spiderlens crawl --accessibility off|static|full --wcag 2.2-aa --no-a11y-mobile --a11y-budget 5`, `spiderlens accessibility <file> [--level 2.1-aa] [--needs-review] [--format text|csv|json]` and `spiderlens accessibility run <file> [--static|--full]` (FR-031). Exit codes: 009's `ISSUES_FOUND` through `spiderlens issues --fail-on`; nothing new. Contract in [contracts/cli.md](./contracts/cli.md).
- **UI** (`ui/src/accessibility/`): Settings › Accessibility page (level, WCAG target, best practice, mobile, budget, thresholds), the two tabs registered in the sidebar's Crawl results group, the two inspector sections, a "Run accessibility checks" item in the crawl menu. Impact badges reuse 009's priority tokens plus four impact tokens in `tokens.css` (`--impact-critical` ... `--impact-minor`), checked by the existing contrast test; impact is always also written as text.

### D7. Progress and summary (FR-029, FR-033)

The progress event gains `a11y_checked` and `a11y_avg_ms`; the end-of-crawl summary gains the FR-033 block, computed with one grouped query at run end, in the CLI and the app.

## Performance impact (constitution principle VI)

| Situation | Cost | How it is checked |
|---|---|---|
| Level Off | One `bool` test per page in `walk()`; no collector, no `Msg::A11y`, no rows. | T040: 100,000-page benchmark, median of 7 alternating runs, within 2% time and 5% peak memory of the baseline recorded in T002 (SC-002). |
| Level Static | A `match` on the element name for every element (most fall through), a few small vectors per page, one batched insert per page with findings. | T040: at most 3% slower than Off (SC-003). |
| Level Full | axe-core in the render slot: tens of ms to about 1 s per page *(estimate)*; the mobile probe a few ms. Lowers rendered pages per second, never the raw fetch rate (FR-029). | T060: render benchmark, median extra time per page at most 25% of the median render time (SC-004). |
| File size | Static: a few rows per page with findings. Full: about 1 to 5 KB per page with violations *(estimate)*, bounded by the node caps. | T060 records file size with Full on and off. |
| Binary size | `axe.min.js` about 0.5 MB, embedded once in `spiderlens-engine`. | T060 records binary sizes against spec 014's 8 MB budget. |

## Delivery in four PRs

1. **Static collector** (engine only): T001 to T012. `PageInput.a11y`, `extract/a11y.rs`, fixture pages, unit and integration tests, baseline and Static benchmarks. No storage yet: `PageData.a11y` is computed but dropped by the runner, so the PR changes nothing in crawl files.
2. **Storage, settings, rules, tabs, CLI** for static: T013 to T039. Needs spec 006 PR #27 (migrate.rs) and spec 009's catalogue on main. Includes UI and its tests.
3. **Full and mobile checks**: T041 to T060. Needs spec 014's render stage and probe hook on main.
4. **Existing crawls, exports, polish**: T061 to T072.

## Coordination

- **014 (JS rendering)**: owns the probe hook, `load_types` and the shared probe cap (agreed 2026-10-13, published in its `contracts/render-crate.md`). This feature only registers probes and reads outcomes.
- **009 (issue rules)**: rule files `accessibility.rs`, `accessibility_axe.rs`, `mobile.rs`; `standards` tags on 009's alt, lang, heading order and title rules; their conditions extended to match the axe ids (D5). The `wcag_in_target` SQL function lives next to `thr()` in `crates/results/src/sql.rs`.
- **006 (storage)**: one additive step, number at merge (latest on main plus one).
- **010 (report), 012 (compare), 016 (agents)**: read the four tables; the shapes are in data-model.md.
- **024 (integrations)**: the Lighthouse accessibility score from PageSpeed stays theirs; this feature shows no score.

## Project Structure

### Documentation (this feature)

```text
specs/026-accessibility-mobile/
├── spec.md
├── research.md
├── plan.md
├── data-model.md
├── quickstart.md
├── contracts/
│   ├── cli.md
│   └── probes.md
├── checklists/requirements.md
└── tasks.md
```

### Source Code (repository root)

```text
crates/engine/
  assets/axe/axe.min.js, LICENSE, VERSION   vendored axe-core 4.14.0, unmodified
  assets/mobile_layout.js                   mobile layout probe
  src/extract/a11y.rs                       static collector (D1)
  src/extract/walk.rs                       calls the collector
  src/crawl/render/a11y_probe.rs            probes, outcomes (D2)
  src/crawl/migrate.rs                      one step (D3)
  tests/a11y_static.rs, tests/a11y_full.rs  fixture tests
crates/config/src/model.rs, validate.rs     AccessibilitySettings (D4)
crates/results/src/
  issues/rules/accessibility.rs, accessibility_axe.rs (generated), mobile.rs
  tabs.rs, detail.rs, export.rs, features.rs, sql.rs
crates/cli/src/accessibility_cmd.rs
crates/test-fixtures/src/a11y_site.rs
scripts/gen-axe-rules.mjs
app/src-tauri/src/results.rs                 commands for the tabs, detail, run checks
ui/src/accessibility/                        settings page, tab views, inspector sections
```

## Complexity Tracking

| Addition | Why needed | Simpler alternative rejected because |
|---|---|---|
| Vendored `axe.min.js` (third-party JavaScript) | Full checks need computed styles and accessible names that only a browser has; axe is the engine users compare against. | Writing our own rendered checks would take months and disagree with other tools. |
| Generated rule file | About 100 axe checks with texts and WCAG tags must match the vendored version exactly. | Hand-written entries drift on every axe upgrade. |
