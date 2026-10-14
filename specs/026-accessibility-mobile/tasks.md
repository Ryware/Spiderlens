---
description: "Task list for spec 026, accessibility and mobile usability checks"
---

# Tasks: Accessibility and Mobile Usability Checks

**Input**: [plan.md](./plan.md), [spec.md](./spec.md), [research.md](./research.md), [data-model.md](./data-model.md), [contracts/](./contracts/), [quickstart.md](./quickstart.md)

**Tests**: required. Every acceptance scenario maps to an automated test against the local fixture (constitution VII); every task that touches `ui/` includes its Vitest tests, and user-visible flows get a Playwright test.

**Format**: `- [ ] T### [P] [USn] Description`. `[P]` = parallel with other `[P]` tasks of the same phase. `[USn]` = user story from spec.md.

**Browser in tests**: tests that need a browser use spec 014's shared helper (skips locally without a browser, fails in CI with `SPIDERLENS_REQUIRE_BROWSER=1`).

## Phase 1: Static collector (PR 1, engine only)

**Purpose**: the static subset, with no storage, UI or browser. Crawl files do not change.

- [ ] T001 [P] Fixture pages: `crates/test-fixtures/src/a11y_site.rs` with one failing and one passing page per static check of data-model.md (paths `/static/<id>` and `/static/<id>-ok`), plus `/clean/1` to `/clean/10`; export from `lib.rs`.
- [ ] T002 [P] Baseline benchmark: create `specs/026-accessibility-mobile/benchmarks.md`; on the merge base, run the 100,000-page bench (`bench-site --pages 100000`, `--speed fast`) 7 times alternating, record median and spread of time, URLs/s and peak RSS (SC-002 baseline).
- [ ] T003 Types in `crates/engine/src/extract/a11y.rs`: `StaticCheck` (ids, WCAG tags, impact, `same_as`, from data-model.md), `StaticFinding`, `A11yNode`, `Viewport`, `A11yStatic`; `PageData.a11y: Option<A11yStatic>` (`None` in `PageData::empty`); `PageInput.a11y: bool`, every existing construction site passes `false`.
- [ ] T004 Collector skeleton: `A11yCollector::{element, text_open, text, close, finish}` with ancestry-based selectors and 300-character excerpts; `walk()` creates it only when `a11y` is true. Unit test: `a11y: false` gives `None` and identical `PageData` otherwise (FR-027).
- [ ] T005 [US2] Names: `form_label` (label `for`, wrapping label, `aria-label`, `aria-labelledby` resolved at finish, `title`; `hidden`, `submit`, `button`, `image`, `reset` inputs excluded), `button_name` (text, `img alt`, `aria-*`, `title`, `value` for input buttons), `link_name` (anchors with `href`: text, image alt, `aria-*`, `title`), `area_alt`, `input_image_alt`. Unit tests per rule, positive and negative (US2.1).
- [ ] T006 [US2] Language: `lang_invalid` (root `lang` present but not a well-formed BCP 47 tag, reusing 009's `lang_code_ok` logic moved to `crates/config` or duplicated with a shared test vector) and `element_lang_invalid`. Unit tests (US2.2).
- [ ] T007 [US2] [US3] Viewport: parse the first `meta name=viewport` into `Viewport`; `zoom_disabled` (`user-scalable=no|0` or `maximum-scale` below 2), `viewport_missing`, `viewport_width` (no `width=device-width`). Unit tests (US2.3, US3.1).
- [ ] T008 [US2] Others: `frame_title` (`iframe`, `frame` without non-empty `title` and without `aria-*` name), `tabindex_positive`, `accesskey_duplicate` (at finish), `meta_refresh_delay` (from the existing `meta_refresh`, delay above 0 and below 72,000 s), `deprecated_element` (`blink`, `marquee`), `plugin_element` (`object`, `embed`, `applet`). Unit tests.
- [ ] T009 Caps and edge cases: 20 nodes per check with exact `count`; `noscript` content ignored; SVG and MathML ignored; malformed pages from the spec 005 malformed fixture never panic (run the collector over all of them).
- [ ] T010 [US2] Integration test `crates/engine/tests/a11y_static.rs`: run `extract_page` with `a11y: true` over every T001 page and compare with an expected table (check, count, first selector): 100% match on the static part of SC-001.
- [ ] T011 Static benchmark: the T002 run with the collector on (temporary flag in the bench binary `--a11y-static`, the runner computes and drops `PageData.a11y`); record in `benchmarks.md`. Fails the task if more than 3% (SC-003); then the default in T014 becomes Off (plan D4).
- [ ] T012 `cargo fmt`, `cargo clippy --workspace -- -D warnings`, `cargo test --workspace`; update `specs/026-accessibility-mobile/tasks.md` checkboxes.

**Checkpoint**: static checks computed in the walk, proven correct and cheap.

---

## Phase 2: Storage, settings, rules, tabs and CLI for static (PR 2)

**Depends on**: spec 006 PR #27 (`migrate.rs`) and spec 009's rule catalogue on main.

- [ ] T013 Migration step in `crates/engine/src/crawl/migrate.rs` with the four tables and indexes of data-model.md (all of them, so PR 3 needs no second step); number = latest on main plus one at merge. Test: new file has the tables; an older file migrates; `FORMAT_VERSION` follows.
- [ ] T014 [P] Settings `AccessibilitySettings` in `crates/config/src/model.rs` with defaults (data-model.md), `#[serde(default)]`, ts-rs export, validation in `validate.rs` with codes, the FR-002 notice; extend the config default and round-trip tests. Commit regenerated `ui/src/generated/*.ts`.
- [ ] T015 Runner: pass `a11y = level != Off` into `PageInput`; `Msg::A11y` written in the page's batch (`a11y_audits.static_state`, `a11y_results` with `source = static`, `a11y_nodes`, `mobile_measures` viewport columns); `data_features` gains `accessibility_static`. Test on the fixture crawl: rows match T010's table.
- [ ] T016 [US2] Off writes nothing: crawl with `level = off`, assert empty tables and no `data_features` entry (US2.5, FR-027).
- [ ] T017 `Need::AccessibilityStatic`, `Need::AccessibilityFull`, `Need::MobileLayout` in `crates/results/src/features.rs`.
- [ ] T018 [P] `wcag_in_target(tags)` SQL function in `crates/results/src/sql.rs` over the resolved settings (like `thr`); unit tests for every version and level, best practice on and off (FR-020).
- [ ] T019 Rules `crates/results/src/issues/rules/accessibility.rs` (static checks) and `mobile.rs` (viewport, zoom, plugins now; layout rules return unavailable until PR 3 data exists), registered in `CATALOGUE`; `RULES_VERSION` bumped. Table-driven test over the fixture crawl (SC-001 static part).
- [ ] T020 `standards` tags on 009's alt, lang, heading-order and title rules (if not already present) and their conditions extended with `OR EXISTS (… a11y_results … check_id = '<axe id>' …)`; parity test: a page is counted once.
- [ ] T021 [US2] Accessibility tab `TabDef` (FR-021 columns and filters) and Mobile tab `TabDef` (FR-022) in `tabs.rs`; counts test on the fixture crawl.
- [ ] T022 [US4] Detail sections `accessibility` and `mobile` in `detail.rs` (grouped by check, ordered by impact, nodes, documentation link, capped note per US4.2). Test.
- [ ] T023 [US2] Disclaimer constant (FR-025) used by tab header, detail section, CLI and exports; test that each surface includes it.
- [ ] T024 [US2] CLI flags on `crawl` and `spiderlens accessibility <file>` (text, `--summary`, `--mobile`, `--wcag`) per contracts/cli.md in `crates/cli/src/accessibility_cmd.rs`; CLI tests against the fixture (US2.4, FR-032).
- [ ] T025 Tauri commands in `app/src-tauri/src/results.rs` for the two tabs and detail sections (reuse the 007 view commands; only the tab ids are new).
- [ ] T026 [US2] UI: Settings › Accessibility page in `ui/src/accessibility/AccessibilitySettings.tsx` (level, WCAG target, best practice, mobile, budget, thresholds, the rendering notice); Vitest for every control and the notice.
- [ ] T027 [US2] UI: register the Accessibility and Mobile tabs in the sidebar; empty-state texts ("This crawl has no accessibility data", "Full checks need JavaScript rendering" with a button that opens Settings › Rendering); Vitest.
- [ ] T028 [US4] UI: inspector sections with impact badges (tokens `--impact-*` in `tokens.css`, contrast test passes, text label always shown) and documentation links opened through the shell; Vitest.
- [ ] T029 [US2] Playwright: crawl the fixture through the fake transport, open Accessibility, filter "Critical", select a page, see the inspector section.
- [ ] T030 End-of-crawl summary block (FR-033) for static in CLI and app; test.
- [ ] T031 `cargo fmt`, clippy, all Rust tests, `pnpm -C ui test`, `pnpm -C ui e2e`.

---

## Phase 3: Full and mobile checks (PR 3)

**Depends on**: spec 014's render stage and probe hook on main.

- [ ] T041 Vendor axe-core 4.14.0: `crates/engine/assets/axe/{axe.min.js,LICENSE,VERSION}` unmodified, SHA-256 recorded in `VERSION`; About › Licences lists it; a test checks the hash.
- [ ] T042 [P] `scripts/gen-axe-rules.mjs` and the generated `crates/results/src/issues/rules/accessibility_axe.rs` (FR-018, FR-019); test that `AXE_VERSION` matches the vendored `VERSION`; `RULES_VERSION` bumped.
- [ ] T043 [P] `crates/engine/assets/a11y_runner.js` (options of research R3, reduction and caps of contracts/probes.md) with a Node unit test on a jsdom-free fixture result object (pure reduction function).
- [ ] T044 [P] `crates/engine/assets/mobile_layout.js`; browser test on `/mobile/*` fixture pages (SC-005).
- [ ] T045 Fixture pages for full checks `/full/<axe-id>` and `-ok` for at least 30 axe checks, including script-added violations (US1.2) and `/full/contrast-over-image` (US1.5); mobile pages `/mobile/content-wider`, `/mobile/small-text`, `/mobile/small-targets`, `/mobile/ok`.
- [ ] T046 Probes in `crates/engine/src/crawl/render/a11y_probe.rs`: build per job, `load_types` adds font and image (FR-012), order and budget split (plan D2), no probes for bot challenge pages (FR-014), outcomes to `Msg::A11y` full part.
- [ ] T047 [US1] Store full results with de-duplication against static (`same_as`), `a11y_engine` meta, `data_features` entries; test (US1.1, US1.3, US1.6).
- [ ] T048 [US1] Timed out and not checked states (FR-013, edge cases): a fixture page with a script that blocks for 10 s; test no partial list stored and static kept.
- [ ] T049 [US1] FR-009 test: rendered HTML and screenshot identical with and without probes on the fixture.
- [ ] T050 [US3] Mobile layout stored and desktop window recorded "not measured" (US3.2 to US3.5); layout rules in `mobile.rs` become available.
- [ ] T051 [US1] WCAG target switch on an open crawl changes counts without checking again (US1.4, FR-020); app and CLI parity (SC-006).
- [ ] T052 [US4] Screenshot outline in the inspector when a box and a screenshot exist (FR-024); Vitest and Playwright.
- [ ] T053 Progress fields `a11y_checked`, `a11y_avg_ms` (FR-029) in CLI and app status bar; Vitest.
- [ ] T060 Render benchmark: median extra time per page with Full and mobile on versus off on the rendered fixture (SC-004), file and binary sizes; record in `benchmarks.md`.

---

## Phase 4: Existing crawls, exports, CI (PR 4)

- [ ] T061 [US5] `accessibility run <file> --static`: re-extract from stored HTML with the collector only; replace previous static rows in one transaction; equality test with a crawl made with static on (US5.1).
- [ ] T062 [US5] `--full` on an existing crawl through spec 014's render-later path; refused without stored HTML or view-only (US5.2, US5.3).
- [ ] T063 [US5] UI: "Run accessibility checks" in the crawl menu with confirmation; Vitest and Playwright.
- [ ] T064 [US6] Exports `a11y_violations`, `a11y_violations_level`, `a11y_needs_review`, `a11y_summary` in CSV and JSON (FR-026), app and CLI parity test.
- [ ] T065 [US6] CI scenario test: `crawl --render javascript --accessibility full` then `issues --fail-on high` exit code (US6.1, US6.2).
- [ ] T066 SC-007 check: Playwright path from an issue to a failing element in at most three clicks.
- [ ] T070 Final benchmark run of SC-002 and SC-003 on the merged result; update `benchmarks.md`.
- [ ] T071 Docs: README feature list and `docs/` page on accessibility checks, with the disclaimer.
- [ ] T072 `/speckit-converge` reports Converged.
