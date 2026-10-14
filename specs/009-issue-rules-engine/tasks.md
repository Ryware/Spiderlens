# Tasks: Issue Rules Engine (spec 009)

Code in one draft PR on branch `009-issue-rules-engine`. [P] = can run in parallel with the previous task.

## Setup and foundation

- [ ] T001 `Priority`, `IssueSettings`, `RuleOverride` in `crates/config` with serde defaults; old profiles load unchanged (FR-022)
- [ ] T002 `issues/mod.rs`: `Rule`, `Scope`, `Threshold`, `RuleKind`, `Category`, `CATALOGUE`, `RULES_VERSION`; catalogue test (unique ids, texts, scopes, column ids) (FR-001 to FR-003, SC-008)
- [ ] T003 `issues/settings.rs`: resolve and validate settings, notices for unknown ids, range errors (FR-024)
- [ ] T004 `sql.rs`: `thr`, `headings_skip`, `lang_code_ok`, `weak_anchor`, `has_header` with unit tests (plan D3, D6)
- [ ] T005 Issues fixture site in `crates/test-fixtures`: one positive and one negative page per rule, expected results table (SC-001)

## US1 and US3: issues by priority, rule texts (P1)

- [ ] T006 [P] Rule files: response, links, titles, meta, headings, content (FR-004)
- [ ] T007 [P] Rule files: images, canonicals, directives, hreflang, pagination, structured data, security, URL (FR-004)
- [ ] T008 `eval.rs`: `temp.issue_hits` by scope group, counts, unavailable rules, not-checked targets (FR-006, FR-008 to FR-011)
- [ ] T009 007 filters that duplicate rules point at them; threshold label templates; parity test (FR-005, SC-007)
- [ ] T010 `issues` tab: rule filters, per-rule columns and evidence column; views, export and copy through the 007 machinery (FR-015)
- [ ] T011 Per-rule tests over the fixture crawl (SC-001)

## US2: issues of one page (P1)

- [ ] T012 `url_issues` with evidence; highest-priority column on every tab (FR-012, FR-017, FR-018)

## Live crawls

- [ ] T013 Incremental evaluation with watermark, provisional SiteWide counts, final pass at run end; retry updates hits in place (FR-007, FR-019, edge cases)
- [ ] T014 Writer stores `rules_version` and `issue_settings` meta at run end; app writes them on settings change for writable files only (FR-013)

## US5: command line (P2)

- [ ] T015 `spiderlens rules` and `spiderlens issues` (text, JSON, --all, --min-priority, --export, --fail-on, settings precedence); CLI tests and contract exit code (FR-025 to FR-028, SC-009)

## Desktop app

- [ ] T016 Tauri commands `rule_catalogue`, `issues_summary`, `url_issues`, `set_issue_settings`; ts-rs types regenerated
- [ ] T017 Priority tokens in `tokens.css` (light and dark, all palettes), contrast test passes
- [ ] T018 [US1] Sidebar Issues group: priorities, expandable issue lists, counts, provisional marker, Show all rules, keyboard tree (FR-014, FR-019, FR-020)
- [ ] T019 [US1] Issue header above the grid: name, priority, threshold, why, fix (FR-016)
- [ ] T020 [US2] Inspector Issues section; click selects the issue (FR-017)
- [ ] T021 [US4] Settings › Issues: search, groups, switch, priority, threshold, reset; changes re-evaluate (FR-021 to FR-024)
- [ ] T022 Command palette: "Go to issue…" and "Issue settings"
- [ ] T023 Vitest for T018 to T022 and the fake transport; Playwright: issue to rows to inspector and back, threshold change updates counts

## US6: older files (P3)

- [ ] T024 Fixture files format 1 and pre-extraction: unavailable rules with reasons; read-only files never written

## Crawl Analysis phase (plan D12)

- [x] T024c `analysis/` module: `Pass` trait, `PASSES`, runner with topological order, staleness from `analysis` meta, main or temp schema, progress and cancel; tests with two fake passes
- [x] T024d Run the phase at crawl end in the app and CLI before releasing the writer lock; on demand for opened files; `spiderlens analyse`; status bar progress; rules that read pass outputs report "available when the crawl ends"

## Readability (FR-031)

- [x] T024a Sentence and syllable counting in the extraction walk for English or undeclared pages, unit tests on known passages
- [x] T024b Migration step for `pages.sentence_count` and `pages.syllable_count` (number at merge), `Need::Readability`, Readability column on Internal, "Difficult to read" rule, unavailable on older files

## Polish

- [ ] T025 Benchmarks in `benchmarks.md`: 100k crawl with live evaluation (SC-002), final pass 100k and 1M (SC-003), setting change (SC-004), memory (SC-006); extraction speed with readability against the 005 benchmark
- [ ] T026 Screenshots of the Issues sidebar, header, inspector and settings in light and dark, `screenshots/`
- [ ] T027 `/speckit-converge` reports Converged
