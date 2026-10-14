# Tasks: Site Visualisations

**Input**: [plan.md](./plan.md), [data-model.md](./data-model.md), [contracts/](./contracts/), [spec.md](./spec.md)

`[P]` = can run in parallel (different files). `[USn]` = user story. Stage 1 = derived tables in `temp`; stage 2 = passes after the 009 runner and 006 migrations land.

## Phase 1: Setup

- [ ] T001 Structure fixture site in `crates/test-fixtures/src/viz.rs` (known tree with a shorter path found later, redirects, sitemap-only page, blocked URL, 404 at depth 3, two link clusters, folders on two hosts, query-string pages, anchor texts in English and Hebrew)
- [ ] T002 [P] `--viz-*` tokens in `tokens.css` (light and dark) and contrast test extension (plan D8)
- [ ] T003 [P] `build.rs` in `spiderlens-results` generating `viz/palette.rs` from `tokens.css`; test that it matches the tokens

## Phase 2: Foundations

- [ ] T004 `viz/mod.rs`: `Scene`, `SceneNode`, `ViewState`, `NodeId`, ts-rs exports; `build_scene()` dispatch
- [ ] T005 `viz/tidy.rs` and `viz/radial.rs` with invariant tests (no overlap, parents centred, same node set)
- [ ] T006 `viz/lod.rs`: budgets, initial expansion, folding with summaries, paging; tests
- [ ] T007 Tauri `viz_scene` (binary response above 5,000 nodes) and `viz_cancel`; TS types
- [ ] T008 UI `ui/src/viz/VizCanvas`: renderer (batched paths, shapes, labels LOD), controller (pan, zoom, fit, hit grid, keyboard), Vitest tests for the maths and keyboard navigation
- [ ] T009 UI `VizView` shell: toolbar, legend, omitted notice, tooltip, sidebar group and command palette entries; Vitest tests

## Phase 3: US1 crawl tree (P1)

- [ ] T010 [US1] `viz/crawl_tree.rs`: parent rule (plan D2), "Found outside links" groups, counts, child order; `ensure_viz` stage 1 trigger; fixture tests (SC-001)
- [ ] T011 [US1] Crawl tree scenes in tree and radial layouts; colour and size options with fallbacks; golden scene test
- [ ] T012 [US1] Selection to inspector, Show in table, Copy URL, Open in browser; Playwright e2e (hover, click, expand, layout switch)

## Phase 4: US2 directory tree (P1)

- [ ] T013 [US2] `viz/dirs.rs`: folder build with roll-up counts, several hosts, query-string grouping; fixture tests
- [ ] T014 [US2] Directory tree scenes; folder "Show in table" as a scoped 007 view; count equality test; Playwright e2e

## Phase 5: US6 big sites (P2, gates Phases 3-4 being done)

- [ ] T015 [US6] `bench_viz` example on `synth` 100k / 1M; record in `benchmarks.md`; check SC-002 and SC-006
- [ ] T016 [US6] Playwright frame-time probe on a 20,000-node fake scene (SC-004); live-crawl snapshot and Refresh (FR-006)

## Phase 6: US3 link graph (P2)

- [ ] T017 [US3] `viz/force.rs`: seeded Barnes-Hut ForceAtlas2, pinned nodes, convergence, frame streaming and cancel; determinism test on all platforms
- [ ] T018 [US3] Graph selection by Link Score or inlinks, focus rings, nofollow and external-domain options; fixture test (two clusters, one joining link)
- [ ] T019 [US3] UI: frames, drag to pin, Reset layout, Focus controls, reduced motion; Vitest and Playwright tests
- [ ] T020 [US3] Force-directed layout option for the two trees

## Phase 7: US4 anchor text clouds (P2)

- [ ] T021 [US4] `viz/anchors.rs`: terms (words and whole anchors), stop word lists, language choice, spiral layout with width table; tests per language
- [ ] T022 [US4] `viz_term_links`; position filter when spec 023 positions exist; tests
- [ ] T023 [US4] UI `AnchorCloud` and term list, entry from the inspector's Inlinks; empty states; Vitest and Playwright tests

## Phase 8: US5 export (P2)

- [ ] T024 [US5] `viz/svg.rs` with legend and header; golden SVG tests on all platforms
- [ ] T025 [US5] `viz/png.rs` (`resvg`), scale and size limit; `viz/csv.rs` with the 007 CSV rules; tests
- [ ] T026 [US5] `viz_export` with progress and atomic write; UI Export menu (format, scale, theme); Playwright e2e
- [ ] T027 [US5] `spiderlens viz` (contracts/cli.md); test that CLI and desktop default-state SVG and CSV are identical (SC-007)

## Phase 9: Stage 2

- [ ] T028 Migration step for `viz_crawl_tree`, `viz_dirs`, `viz_dir_urls` (number at merge); passes registered with the 009 runner; running crawls still compute into `temp` on demand

## Phase 10: Polish

- [ ] T029 Light and dark screenshots of every view for the owner's review (`/mnt/project-files/design/022-screenshots/`)
- [ ] T030 `/speckit-converge` and quickstart walk-through
