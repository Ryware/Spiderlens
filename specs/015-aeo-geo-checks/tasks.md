---
description: "Task list for spec 015, AEO and GEO checks"
---

# Tasks: AEO and GEO Checks

**Input**: [plan.md](./plan.md), [spec.md](./spec.md), [research.md](./research.md), [data-model.md](./data-model.md), [contracts/](./contracts/), [quickstart.md](./quickstart.md)

**Tests**: required. Every acceptance scenario maps to an automated test against the local AI fixture site (constitution VII). Every task that touches `ui/` includes its Vitest tests in the same task.

**Format**: `- [ ] T### [P] [USn] Description`. `[P]` = can run in parallel with the other `[P]` tasks of the same phase. "Depends on" names prerequisites that are not simply the previous task.

**Order**: the first code PR covers Phases 1 to 5 and 8 (everything that needs no rendering). Phase 6's rendered path waits for spec 014's `render_comparisons` table; Phase 9's T060 waits for spec 009's rule type.

## Phase 1: Setup

- [ ] T001 Create the `spiderlens-aeo` crate (`crates/aeo/Cargo.toml`, `src/lib.rs` with the public types of contracts/aeo-crate.md); add it to `members` and `default-members`; extend the CI "engine has no UI dependencies" check to it.
- [ ] T002 [P] AI fixture site `crates/test-fixtures/src/ai.rs`: hosts with robots.txt variants (GPTBot only, wildcard, ClaudeBot path, Google-Extended, Applebot fallback, unavailable 503, conflicting groups, Content-Signal, Content-Usage, License lines), llms.txt variants (valid, no H1, HTML, broken links, redirects, blocked), TDMRep file, article pages written to pass or fail each content check, an SPA shell, author and about pages; `src/bin/ai-site.rs` for quickstart.
- [ ] T003 [P] Baseline benchmark: `specs/015-aeo-geo-checks/benchmarks.md`, 100,000-page bench on the merge base, 7 alternating runs, median and spread of time, URLs/s and peak RSS (SC-001 baseline for T070).

## Phase 2: Foundational

- [ ] T004 AI crawler list `crates/config/src/ai_bots.toml` and `ai_bots.rs` (contracts/ai-bots.md, research R1), with tests for unique tokens and indexes, docs URLs, purposes; custom entries merged; `spiderlens aeo bots`.
- [ ] T005 [P] Scoring profile `crates/config/src/aeo_profile.rs` and `CrawlConfig.ai_checks` (`auto`, `site_files`) with serde defaults so older configs load; CLI flags `--no-ai-checks`, `--no-site-files`.
- [ ] T006 robots.txt additions in `crates/engine/src/robots.rs` (plan D6): rule line numbers, `other_lines`, `for_agent_with_fallback`; existing tests unchanged, new tests for each addition.
- [ ] T007 Migration step in `crates/engine/src/crawl/migrate.rs` with every table of data-model.md (additive); test that a format-6 file upgrades and that `read_compat` is unchanged. Version = latest on main plus one at merge.
- [ ] T008 Site files at crawl time (plan D3) in `runner.rs` and `store.rs`: low-priority per-host fetches after robots.txt, 1 MB cap, `site_files` rows; tests: requests appear once per host in the fixture log, never before the host's robots.txt, never while a page waits for the slot; skipped when `site_files = false`.
- [ ] T009 `aeo::analyze` skeleton (plan D4): writer open with migration and backup, three passes, chunked workers with read-only connections, bounded channel, batched writer, progress, cancel; `aeo_site.versions`; determinism test (two runs, identical rows except `ran_at` and `duration_ms`).

## Phase 3: US1 Access (P1)

- [ ] T010 [US1] `access.rs`: per host and crawler verdicts, home page, blocked share, deciding rules with line numbers, unavailable robots.txt, `robots may not apply` labels; `aeo_bot_access` rows. Tests: acceptance scenarios US1.1 to US1.6 and SC-003 (at least 40 robots.txt fixtures, 100% match).
- [ ] T011 [US1] Page-level access: `blocked_bots` bitmask, meta and header directives that name AI crawlers, snippet limits (`nosnippet`, `max-snippet`, `noarchive`, `nocache`, `data-nosnippet`, `data-snippet`, `noai`, `noimageai`) with per-vendor meaning (FR-013). Test: US1.7.
- [ ] T012 [P] [US5] Content usage and licence signals (FR-014) from `other_lines`, `Content-Usage` and `tdm-*` headers and TDMRep file. Test: US5.5.

## Phase 4: US6 Scores, first version (P1)

- [ ] T013 [US6] `rules.rs` with the local `RuleDef` (spec 009 FR-001 fields) and `AeoRule` (part, points, applies_to, basis with source URL); every rule documented in `docs/ai-checks.md` generated from the rule list by a test (doc stays in sync).
- [ ] T014 [US6] `score.rs`: integer page score, not-applicable and not-available handling, cap 20, provisional, site score 70/30, top fixes with stable order. Tests: US6.1 to US6.4, SC-007 (formula equals stored value for every fixture page).

## Phase 5: US3, US4, structured data (P2, no rendering needed)

- [ ] T015 [US3] Page type classification with reasons and URL-pattern overrides (FR-019).
- [ ] T016 [US3] `content/` DOM facts (plan D7): main content, question headings (`data/questions.toml`, eight languages), direct answers, structure, facts and sources, Q&A sections, snippet markers; `NotAvailable` without stored HTML. Tests: US3.1 to US3.6, SC-004 for these checks.
- [ ] T017 [P] [US4] Dates (`data/dates.toml`): structured data, `<time>`, visible near H1 or byline, `Last-Modified`, agreement, future dates, staleness. Tests: US4.1, US4.2, US4.5.
- [ ] T018 [P] [US4] Authors and site entity: bylines, `author` items, author pages, Organization or Person and WebSite, `sameAs`, name consistency, about and contact pages (FR-028 to FR-030). Tests: US4.3, US4.4.
- [ ] T019 [P] Structured data validation (plan D8): `build.rs` tables from the trimmed Schema.org file, `type_rules.toml` for the 20 types, JSON pointers in messages, coverage per page type, disagreement with visible content (FR-031 to FR-033). Tests: fixtures per type with expected errors and warnings; US3.5.

## Phase 6: US2 Readable without JavaScript (P1)

- [ ] T020 [US2] Raw-only evidence and "Possibly built with JavaScript" (FR-017). Test: US2.3.
- [ ] T021 [US2] `RenderFacts` from `render_comparisons` when present; rendered values for crawlers that run JavaScript (FR-015, FR-016, FR-018). Tests on a fixture crawl file with a hand-made `render_comparisons` table: US2.1, US2.2, US2.4, US2.5, SC-005. End-to-end test on a real rendered crawl added when spec 014's code merges.

## Phase 7: US5 llms.txt (P2)

- [ ] T022 [US5] Validator (plan D9) with line numbers and link statuses; `llms-full.txt` status, type, size and links. Tests: US5.1 to US5.4, SC-008 (15 files).
- [ ] T023 [P] [US5] Generator and `spiderlens aeo llms-txt`. Test: US5.6 (draft from the fixture crawl matches a golden file).

## Phase 8: Results, app, UI and CLI (US6, US7)

- [ ] T024 [US7] `crates/results`: `Need::AiChecks`, the `ai-search` tab, columns and filters as data (FR-041), `ai_overview` and `ai_detail` readers; export through the existing export path. Tests: filter counts on the fixture crawl; US7.2; old file shows "AI checks have not run".
- [ ] T025 [US7] CLI `spiderlens aeo run|summary|export|page|llms-txt|probe|bots` (contracts/cli.md). Tests: US7.1, US7.3, SC-010 (CLI JSON equals the app's summary for the same file).
- [ ] T026 Automatic run at crawl completion from the runner's finish step (`analyze_open`), skipped when `auto = false`; partial results visible while it runs (FR-002). Test: crawl the fixture and find results without any further call.
- [ ] T027 App commands `app/src-tauri/src/aeo.rs` (contracts/desktop-commands.md) with progress events and cancel.
- [ ] T028 [US6] UI: AI search overview in `ui/src/aeo/Overview.tsx` (site score and parts, access matrix with per-vendor sentence, llms.txt, signals, entity, coverage, distribution, top fixes), with Vitest tests for every section, empty and partial states, and keyboard access.
- [ ] T029 [US6] UI: AI search tab wiring and the detail pane section (score explanation with points and basis, access per crawler, raw or rendered marker, questions and answers, dates, author, structured data messages), with Vitest tests.
- [ ] T030 [P] UI: Settings › AI search (automatic run, site files, scoring profiles, custom crawlers), llms.txt draft sheet with save dialog, probe confirm dialog that states what it sends; Vitest tests.
- [ ] T031 Playwright flow for SC-009: open the fixture crawl, find blocked crawlers and the top three fixes; runs in the existing UI end-to-end job.

## Phase 9: Probe and issue rules

- [ ] T032 [US1] User agent probe (FR-012): explicit start, per-host limits, robots.txt for SpiderLens's token, Retry-After, 10 pages per host, outcome classes including 402 with `crawler-price`; `aeo_probe`; CLI requires `--yes`. Tests against fixture hosts that block by user agent, serve a challenge page and answer 402.
- [ ] T060 Spec 009 integration (FR-044): replace the local `RuleDef` with 009's type, register the AI rules in its rule slice, apply per-profile disable, priority and threshold. Depends on spec 009's code merge.

## Phase 10: Performance and polish

- [ ] T070 SC-001: 100,000-page bench with the feature on against the T003 baseline (median of 7); blocks the merge if outside normal variation.
- [ ] T071 SC-002: analysis of the 100,000-page crawl with stored HTML in at most 60 s, memory within the crawl's peak; record in benchmarks.md.
- [ ] T072 SC-006: run the analysis on the same fixture crawl file in CI on all three platforms and compare result hashes.
- [ ] T073 Help text and `docs/ai-checks.md`: what each check means, its basis and source, and the statement that no score guarantees citation (FR-038).
- [ ] T074 `/speckit-converge` until Converged.

## Dependencies

- T001 → everything in `crates/aeo`. T004, T006, T007 → T009 → Phases 3 to 7. T013, T014 → T024 to T029.
- T021's end-to-end test waits for spec 014's code; T060 waits for spec 009's code. Neither blocks the first code PR.
- T008 is the only task that changes the crawl path; T070 must pass before merge.
