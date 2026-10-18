---
description: "Task list for spec 016, AI agents"
---

# Tasks: AI Agents

**Input**: [plan.md](./plan.md), [spec.md](./spec.md), [research.md](./research.md), [data-model.md](./data-model.md), [contracts/](./contracts/), [quickstart.md](./quickstart.md)

**Tests**: required. Every acceptance scenario maps to an automated test against local fixtures (constitution VII). Every task that touches `ui/` includes its Vitest tests in the same task; Playwright tests are listed separately.

**Format**: `- [ ] T### [P] [USn] Description`. `[P]` = parallel with other `[P]` tasks of the same phase. "Depends on" names tasks from earlier phases only when it is not the previous task.

**External dependencies**: tasks marked **(needs 009)**, **(needs 014)** or **(needs 015)** start only after that spec's code is on main. Until then the rules they implement are reported as not checked, which the earlier tasks already handle.

## Phase 1: Setup

- [x] T001 Add `rmcp = { version = "=3.5.1", default-features = false, features = ["server", "transport-io", "macros"] }` to `[workspace.dependencies]`; create `crates/mcp` (`Cargo.toml`, `src/lib.rs` with `pub fn serve_stdio(opts: ServeOptions) -> anyhow::Result<()>` returning after stdin closes, no tools yet); add to `members`; extend the CI "engine has no UI dependencies" check to `spiderlens-mcp`.
- [ ] T002 [P] Baselines in `specs/016-ai-agents/benchmarks.md`: 100,000-page bench (Fast, 7 runs alternated, median and spread of time, URLs/s, peak RSS) on the merge base, and extraction time per page on the spec 005 corpus (criterion-free: a `#[ignore]` timing test printing the median of 20 runs).
- [ ] T003 [P] Agents fixture site `crates/test-fixtures/src/agents_site.rs` and `src/bin/agents-site.rs` (port 8720) with every page and well-known file listed in quickstart.md step 1, request log at `/__stats`, and a second host for cards.

## Phase 2: Foundational

- [x] T004 `extract::AgentData` and its types (data-model.md) in `crates/engine/src/extract/agents.rs`; `PageData.agents` defaulting to empty; serde for tests. No collection yet.
- [x] T005 Migration step (number = latest on main + 1) in `crates/engine/src/crawl/migrate.rs` creating `page_agent`, `page_forms`, `form_fields`, `page_actions`, `agent_entry_points`, `agent_scores` with their indexes; `data_features` gains `agents`. Test: a format-2 file and a file at the previous step both open and migrate; the step test for no gaps passes.
- [x] T006 Store agent rows in the page batch (`store.rs`), one row per page and source in `page_agent`, detail rows capped as in data-model.md. Test: write a page with 2 forms, 60 actions; read back counts and 50 capped actions.
- [ ] T007 [P] `agent_settings` in the crawl configuration model (`crates/config`), stored with the crawl, with defaults; `spiderlens crawl` flags from contracts/cli.md under the "Agents" heading; `config show` prints them. Tests in `crates/cli/tests/config.rs`.
- [x] T008 [P] `crates/config/src/assistants.rs`: `AssistantSettings` (`assistants.json`), load and atomic save in the app config directory, resolved the same way as `recent.rs`; validation (folders exist, profiles exist and do not ignore robots.txt). Unit tests.
- [ ] T009 [P] `Need::Agents` and `Need::AgentScores` in `crates/results/src/tabs.rs` with unavailable reasons ("Agent readiness was not checked in this crawl; run agent checks." and "Agent checks have not been evaluated yet.").

**Checkpoint**: the file format holds agent data; nothing collects it yet.

## Phase 3: User Story 1, ask my AI assistant about my crawl (P1) 🎯 MVP

- [x] T010 [US1] Server skeleton on rmcp: server info, protocol negotiation, tool registration from static descriptions with the untrusted-content sentence, `ServeOptions` (allow_read, folders, allow_crawl, profiles, settings path), access check returning the "read access is off" tool error. Test with an in-process client: tools listed with access off, every call refused.
- [x] T011 [US1] `crates/mcp/src/crawls.rs`: crawl ids, `list_crawls` from the recent list and allowed folders (one level, `*.spiderlens`), path canonicalisation and refusal (FR-025). Tests: symlink and `..` escapes refused, recent entry accepted.
- [x] T012 [US1] `crates/mcp/src/envelope.rs` and `bounds.rs`: the `crawled` envelope builder, text cut at 2,000 characters, 64 KB answer cap that drops rows and sets `next`, stateless cursor tokens. Property test: no answer over 64 KB for random rows; cursor round trip.
- [x] T013 [US1] Tools `crawl_summary`, `list_tabs`, `query_pages` (with `where` ops mapped onto spec 007 view queries), `page_details`, `page_html` over `spiderlens_results::Reader`; `in_progress` and `as_of` for running crawls. Tests compare each answer with the CLI's `report`, `page`, `html` and `export` output on the same fixture crawl (SC-006).
- [x] T014 [US1] `export_rows` (CSV text through `write_csv`, 1,000-row and 64 KB caps, never a file) and the refusal for any write-like request ("read-only" message naming the setting). Test.
- [x] T015 [US1] Activity log `crates/mcp/src/activity.rs` (JSON lines, no arguments or results, 2,000-line cap with rotation). Unit tests.
- [x] T016 [US1] `spiderlens mcp` subcommand (contracts/cli.md) including `--self-test`; app `main.rs` runs `serve_stdio` when `argv[1] == "mcp"` before building Tauri. Test: `assert_cmd` drives the CLI over stdio with a scripted JSON-RPC client through list → summary → query → paging.
- [x] T017 [US1] Prompt injection suite (SC-008): fixture pages, titles, anchors and HTML full of instruction text; assert that no tool description, schema or non-`crawled` field of any answer contains it.
- [ ] T018 [US1] Speed while querying (SC-003, SC-004): `#[ignore]` benches on the 100k bench crawl: p95 of 200 mixed queries under 300 ms; a crawl of the bench site with one `query_pages` per second stays within 2% of T002's median. Record in benchmarks.md.
- [x] T019 [US1] Desktop commands `assistants_get`, `assistants_set`, `assistants_test`, `assistants_activity`, `assistants_activity_clear` in `app/src-tauri/src/agents.rs`; snippets for Claude Desktop, Claude Code, Cursor and generic, built from `current_exe()`; ts-rs exports; fake transport handlers.
- [x] T020 [US1] `ui/src/settings/AssistantsSettings.tsx`: Settings › AI assistants page (app group, like Appearance): read switch with the plain-language risk note, allowed folders (add with the folder dialog, remove), snippets with copy buttons, Test with result, recent activity with Clear; crawl section disabled until T070. Vitest: every control, copy, test success and failure, empty activity, keyboard navigation, tokens only.
- [x] T021 [US1] Playwright: open Settings › AI assistants, turn read access on, copy the Claude Desktop snippet, run Test (fake transport success), clear activity.
- [ ] T022 [US1] CI: a job step on all three OS runs the release desktop executable with `mcp --self-test` and a stdio round trip (plan D8 Windows risk). If Windows fails, ship `spiderlens.exe` in the bundle and point snippets at it.

**Checkpoint**: assistants can read every crawl through MCP. Shippable on its own.

## Phase 4: User Story 2, can agents act without JavaScript (P1)

- [x] T030 [US2] Collector in `extract/agents.rs` called from `walk.rs` `enter()`: forms (method, action, search detection, submit control), fields with accessible names (aria-labelledby, aria-label, label for and wrapped, title, placeholder flagged), nameless buttons, actions without a link (capped 50 + total), WebMCP declarative attributes, agent links from `<link>`; label and labelledby resolution after the walk. Unit tests per element case in `extract/tests.rs` (including `<template>`, hidden inputs, nested forms, label pointing at a missing id).
- [x] T031 [US2] Agent links from the `Link` header (`service-desc`, `service-doc`, `api-catalog`, markdown alternate) in `link_header.rs`, kept out of the frontier. Test.
- [ ] T032 [P] [US2] `crates/config/data/challenge_widgets.json` (version, id, name, script hosts, class names) and matching in the collector, attaching the widget to its form. Tests for each widget.
- [ ] T033 [US2] Extraction speed check: rerun T002's extraction timing; must be within 3% (plan D1). Record in benchmarks.md; optimise before moving on if not.
- [x] T034 [US2] Agents `TabDef` (columns from FR-017, filters per page-level rule condition, counts); shared SQL condition constants in `crates/results/src/agents/conditions.rs` that the rules (T051) will reuse. Tests: each fixture page appears under exactly the expected filters (SC-005, raw).
- [x] T035 [US2] `PageAgentFindings` in `crates/results/src/agents/detail.rs`, desktop command `agents_page`, MCP tool `page_agent_findings`. Tests.
- [x] T036 [US2] Inspector "Agents" section in `ui/src/results/Inspector.tsx` (forms with fields and raw/rendered badges, actions without a link, declared tools, markdown alternate, score placeholder until T053). Vitest.
- [ ] T037 [US2] Re-run collection from stored HTML for older files (`agents check`, `agents_run_checks`): parse stored raw (and rendered) HTML with `extract_page`, write only agent rows. Test on a file made before the migration step.
- [x] T038 [US2] **(needs 014)** Rendered agent rows from 014's rendered extraction; "Form only after JavaScript" condition (rendered form with no raw form of the same method and resolved action); filters with rendering on and off (acceptance scenario 1). Tests.
- [ ] T039 [US2] Playwright: Agents tab with fake data, select a filter, open a page, inspect its forms.

## Phase 5: User Story 3, machine-readable entry points (P2)

- [ ] T040 [US3] Host-file queue `crawl/host_files.rs` (shared with 015 FR-010; if 015 already built it, add paths to it instead): per internal host after robots.txt, robots.txt check per path, host rate allowance, Retry-After, crawl-delay, 5 MB cap, drained before the crawl completes, counted in the summary. Test with the fixture request log: exact request count per host (FR-009), page fetching not delayed.
- [ ] T041 [P] [US3] Parsers in `crawl/entry_points.rs`: AI Catalog (draft, version recorded), RFC 9727 API catalog (linkset JSON), A2A agent card, MCP server card (SEP-2127 schema snapshot), OpenAPI presence; problems with codes. Unit tests with valid, invalid, unknown-version and oversized inputs.
- [x] T042 [US3] MCP discovery request (plan D4): `server/discover`, fallback `initialize`, 401 handling, nothing else; never sends credentials. Test against the fixture's two endpoints and one that hangs (timeout).
- [ ] T043 [US3] Markdown alternates verification (max per host) and the opt-in `Accept: text/markdown` sample; `md_status` and `md_outcome`. Tests. Partly done: alternates are verified (max per host, `md_outcome`); the opt-in `Accept: text/markdown` sample is not built.
- [ ] T044 [US3] `host_entry_points` MCP tool, entry points in `AgentsSummary`, and the "checks off" state (acceptance scenario 5). Tests.
- [ ] T045 [US3] Speed with entry points on (SC-001): rerun the 100k bench, record in benchmarks.md.

## Phase 6: User Story 4, one score I can explain (P2)

- [x] T050 [US4] **(needs 009)** Add the rules of data-model.md to 009's catalogue under category `agents`, in 009's shape, reusing T034's conditions; site-wide rules flagged provisional during a crawl. Catalogue version bump. Tests through 009's rule test harness.
- [x] T051 [US4] Score table `agents/score.rs` (version 1, weights from data-model.md) and the formula from plan D5 as a pure function. Unit tests with hand-computed cases (Independent Test of US4), including parts with no checked rule and not-checked counts.
- [x] T052 [US4] Evaluation pass `agents::evaluate` (set-based SQL, one transaction) writing `agent_scores`; run at crawl end by the runner hook 009 provides, by `agents check` and by `agents_run_checks`. SC-002 bench on 100k pages under 60 s; record.
- [x] T053 [US4] `AgentsSummary` in `crates/results/src/agents/summary.rs`; desktop command `agents_summary`; MCP `agents_summary`; CLI `agents summary` and `agents page`. Parity test: the three give identical numbers (scenario 4, SC-006).
- [ ] T054 [US4] `ui/src/agents/AgentsSummary.tsx` (score and parts, top rules by points lost with links to their filter, entry points per host, not-checked list with reasons, "Run agent checks" for files without scores) and sidebar entry; inspector shows the page score with rules. Vitest.
- [ ] T055 [US4] Playwright: open the Agents summary, follow a rule to its filtered tab, run checks on an older file.

## Phase 7: User Story 5, agents at the door (P3)

- [ ] T060 [US5] **(needs 015)** Read 015's registry filtered to user-fetch agents and 015's robots evaluation; add user-fetch agents to the probe's choices if missing (coordinate with the 015 thread); rules `agents.robots_blocks_agents` and `agents.probe_blocked` read 015's stored results. Tests with the fixture that blocks one agent. Partly done: both rules read 015's stored results; 015's probe itself is not built, so `probe_blocked` stays not checked.
- [x] T061 [US5] Agent access block in the Agents summary (UI and MCP), Vitest.

## Phase 8: User Story 6, let my assistant start a crawl (P3)

- [ ] T070 [US6] Crawl tools in `crates/mcp/src/crawl_tools.rs`, listed only when allowed (`ttlMs` 10 s), profile allow-list and robots refusal, crawl in-process through the CLI's engine call, recent entry and `events` row marked `assistant:<client>`, clean pause when stdin closes. Tests: refused when off (SC-009), full crawl through the scripted client equals a CLI crawl. Partly done: tools, allow-list, robots refusal, recent entry, meta `started_by`, pause on disconnect and refusal tests are built; no `events` row and no full-crawl comparison test.
- [ ] T071 [US6] App side: "started by an AI assistant" badge in recent crawls; pause and stop from the app through spec 006's lock-holder channel (or the meta stop request, decided after reading 006's merged code, plan D10). Vitest. Partly done: the badge is built; pause and stop from the app are not.
- [x] T072 [US6] Settings crawl section enabled: switch, allowed profiles picker (profiles that ignore robots.txt disabled with a reason). Vitest and a Playwright step.

## Phase 9: Polish

- [x] T080 Docs: `docs/ai-assistants.md` (setup per client, what assistants can and cannot do, the injection note), score formula and weights page linked from the summary.
- [ ] T081 Final speed run (SC-001, SC-002, SC-003, SC-004) and update benchmarks.md; `cargo clippy`, `cargo fmt`, full Vitest and Playwright on all three OS in CI.
- [ ] T082 `/speckit-converge` against spec.md; update spec Status only through the owner.

## Dependencies and order

- Phase 1 → Phase 2 → Phases 3 and 4 in parallel (different crates) → Phase 5 → Phase 6 (needs 009) → Phases 7 (needs 015) and 8.
- Code PRs: **A** = Phases 1 to 4 without T038 (MCP read server, forms and actions, Agents tab); **B** = Phase 5; **C** = Phase 6 when 009 is on main; **D** = Phases 7 and 8 and T038 when 014 and 015 are on main; Polish rides on the last one. Each PR rebases and takes the next free migration step only if it adds one (only A does).
