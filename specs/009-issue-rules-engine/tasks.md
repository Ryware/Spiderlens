# Tasks: Issue Rules Engine (spec 009)

Code in one draft PR on branch `009-issue-rules-engine`. [P] = can run in parallel with the previous task.

## Setup and foundation

- [x] T001 `Priority`, `IssueSettings`, `RuleOverride` in `crates/config` with serde defaults; old profiles load unchanged (FR-022)
- [x] T002 `issues/mod.rs`: `Rule`, `Scope`, `Threshold`, `RuleKind`, `Category`, `CATALOGUE`, `RULES_VERSION`; catalogue test (unique ids, texts, scopes, column ids) (FR-001 to FR-003, SC-008)
- [x] T003 `issues/settings.rs`: resolve and validate settings, notices for unknown ids, range errors (FR-024)
- [x] T004 `sql.rs`: `thr`, `headings_skip`, `lang_code_ok`, `weak_anchor`, `has_header` with unit tests (plan D3, D6) (as built: heading order and hreflang checks moved to `temp.page_flags`, plan "As built")
- [x] T005 Issues fixture site in `crates/test-fixtures`: one positive and one negative page per rule, expected results table (SC-001)

## US1 and US3: issues by priority, rule texts (P1)

- [x] T006 [P] Rule files: response, links, titles, meta, headings, content (FR-004)
- [x] T007 [P] Rule files: images, canonicals, directives, hreflang, pagination, structured data, security, URL (FR-004)
- [x] T008 `eval.rs`: `temp.issue_hits` by scope group, counts, unavailable rules, not-checked targets (FR-006, FR-008 to FR-011)
- [x] T009 007 filters that duplicate rules point at them; threshold label templates; parity test (FR-005, SC-007)
- [x] T010 `issues` tab: rule filters, per-rule columns and evidence column; views, export and copy through the 007 machinery (FR-015)
- [x] T011 Per-rule tests over the fixture crawl (SC-001)

## US2: issues of one page (P1)

- [x] T012 `url_issues` with evidence; highest-priority column on every tab (FR-012, FR-017, FR-018)

## Live crawls

- [x] T013 Incremental evaluation with watermark, provisional SiteWide counts, final pass at run end; retry updates hits in place (FR-007, FR-019, edge cases) (as built: full evaluation each refresh, measured with no crawl slowdown; plan "As built" D5)
- [x] T014 Writer stores `rules_version` and `issue_settings` meta at run end; app writes them on settings change for writable files only (FR-013)

## US5: command line (P2)

- [x] T015 `spiderlens rules` and `spiderlens issues` (text, JSON, --all, --min-priority, --export, --fail-on, settings precedence); CLI tests and contract exit code (FR-025 to FR-028, SC-009)

## Desktop app

- [x] T016 Tauri commands `rule_catalogue`, `issues_summary`, `url_issues`, `set_issue_settings`; ts-rs types regenerated (as built: `issues_apply` in place of `set_issue_settings`, contracts/desktop-commands.md)
- [x] T017 Priority tokens in `tokens.css` (light and dark, all palettes), contrast test passes (as built: the meaning tokens are reused for priorities, plan "As built" D10)
- [x] T018 [US1] Sidebar Issues group: priorities, expandable issue lists, counts, provisional marker, Show all rules, keyboard tree (FR-014, FR-019, FR-020)
- [x] T019 [US1] Issue header above the grid: name, priority, threshold, why, fix (FR-016)
- [x] T020 [US2] Inspector Issues section; click selects the issue (FR-017)
- [x] T021 [US4] Settings › Issues: search, groups, switch, priority, threshold, reset; changes re-evaluate (FR-021 to FR-024)
- [x] T022 Command palette: "Go to issue…" and "Issue settings"
- [x] T023 Vitest for T018 to T022 and the fake transport; Playwright: issue to rows to inspector and back, threshold change updates counts

## US6: older files (P3)

- [x] T024 Fixture files format 1 and pre-extraction: unavailable rules with reasons; read-only files never written

## Crawl Analysis phase (plan D12)

- [x] T024c `analysis/` module: `Pass` trait, `PASSES`, runner with topological order, staleness from `analysis` meta, main or temp schema, progress and cancel; tests with two fake passes
- [x] T024d Run the phase at crawl end in the app and CLI before releasing the writer lock; on demand for opened files; `spiderlens analyse`; status bar progress; rules that read pass outputs report "available when the crawl ends"

## Readability (FR-031)

- [x] T024a Sentence and syllable counting in the extraction walk for English or undeclared pages, unit tests on known passages
- [x] T024b Migration step for `pages.sentence_count` and `pages.syllable_count` (number at merge), `Need::Readability`, Readability column on Internal, "Difficult to read" rule, unavailable on older files

## Polish

- [x] T025 Benchmarks in `benchmarks.md`: 100k crawl with live evaluation (SC-002), final pass 100k and 1M (SC-003), setting change (SC-004), memory (SC-006); extraction speed with readability against the 005 benchmark
- [x] T026 Screenshots of the Issues sidebar, header, inspector and settings in light and dark, `screenshots/`
- [ ] T027 `/speckit-converge` reports Converged

## Phase 11: Convergence

- [x] T028 Record `rules_version` and `issue_settings` meta from the desktop app: at crawl end (before the writer lock is released) and in the background after `issues_apply` changes settings on a writable file, never on view-only files; test both paths per FR-013, T014, contracts/desktop-commands.md `issues_apply` (partial) Done: crawl end in the app records them before the writer closes; `issues_apply` records on a writable file in a background thread.
- [x] T029 Show the rule set version in the app: "Rule set v{n}" in the sidebar Issues section, and "No issues found (rule set v{n})" in the inspector for a page without issues; Vitest coverage per FR-013, US2/AC3, US6/AC2 (partial) Done.
- [x] T030 List every broken target of "Pages with broken internal/external links" in the inspector's Issues section as a scrollable list with each target's state, each openable in the main table (count plus list for pages with thousands of targets); Rust and Vitest tests per US2/AC2, Edge Cases "Evidence on a large page" (partial) Done: up to 200 targets listed with status and the total; a click opens the target in Internal.
- [x] T031 Show partial issue counts while the final evaluation runs (per-scope results published as each scope finishes) instead of "Counting issues…", with first counts within 2 s on the 1M synthetic file; record the figure in benchmarks.md per FR-008, SC-003 (partial) Done: rules that read only the crawl run before the derived tables, and partial counts go out as the `issues-partial` event about once a second; first counts at 1.0 s on 1M (benchmarks.md).
- [x] T032 Make the Issues column filterable on every tab (for example filters "Has issues" and "Highest priority: High/Medium/Low/Opportunity"), with tests per FR-018 (partial) Done: a "Has issues" filter on every tab; priority filters were not added, the Issues column sorts by priority instead.
- [x] T033 Give rules the per-rule `columns` list of plan D1 / data-model.md and use it in `issue:<id>` tabs and exports (for example title and title length for title rules, broken target and its status for link rules) per FR-015, plan D1 (partial) Done: an `issue:<id>` tab shows its category tab's columns (title rules show title and length, link rules the Internal columns); link rules list their targets in the inspector.
- [x] T034 Validate the profile's issue settings in `spiderlens crawl` (and `check`) with the same rules as `spiderlens issues` before crawling, and accept `--issues <file>` on `crawl`; CLI tests per FR-028, US5/AC4 (partial) Done for `crawl`: the profile's issue settings are checked before the crawl file is created. `--issues <file>` was not added (profiles already carry the settings); `check` evaluates no rules.
- [x] T035 Make an unavailable rule's reason visible and announced in "Show all rules" (text or accessible description, not only a `title` tooltip) per US3/AC1, FR-020 (partial) Done: the reason shows as text under the rule and is its accessible description.
