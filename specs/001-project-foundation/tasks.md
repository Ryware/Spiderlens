# Tasks: Project Foundation

**Input**: [plan.md](./plan.md), [research.md](./research.md), [data-model.md](./data-model.md), [contracts/](./contracts/)

`[P]` = can run in parallel (different files). `[USn]` = user story.

## Phase 1: Setup

- [x] T001 Create workspace `Cargo.toml`, `rust-toolchain.toml`, `.gitignore`, `rustfmt.toml`, workspace lints
- [x] T002 [P] Create `crates/engine` library skeleton with `build.rs`
- [x] T003 [P] Create `crates/cli` binary skeleton
- [x] T004 [P] Create `crates/test-fixtures` dev library skeleton

## Phase 2: Foundational

- [x] T005 `engine/src/build_info.rs`: BuildInfo + build id injection (R9), unit tests
- [x] T006 `engine/src/error.rs`, `result.rs`: ErrorClass, CheckError, FinalState, Title, RedirectHop, PageCheckResult (data-model.md)
- [x] T007 `engine/src/config.rs`: CheckOptions with defaults and user agent
- [x] T008 `test-fixtures/src/lib.rs`: axum fixture website on a random port (R10)

## Phase 3: US2 single-page check from the CLI (P1)

- [x] T009 [US2] Write failing CLI contract tests `crates/cli/tests/cli.rs` for US2-1..9
- [x] T010 [US2] `engine/src/title.rs`: charset detection + title extraction, unit tests
- [x] T011 [US2] `engine/src/robots.rs`: fetch and evaluate robots.txt (R6), unit tests
- [x] T012 [US2] `engine/src/fetch.rs`: client, manual redirects, capped streaming body, error classification
- [x] T013 [US2] `engine/src/lib.rs`: `check_page` orchestrating robots → fetch → title
- [x] T014 [US2] `cli/src/main.rs`: clap commands, human and JSON output, exit codes
- [x] T015 [US2] Engine integration tests for timeout, connection refused, TLS invalid, redirect loop, oversize, encodings

## Phase 4: US1 desktop app downloadable per change (P1)

- [x] T016 [US1] `ui/`: Vite + React + TS app, `transport.ts`, identity screen, vitest test
- [x] T017 [US1] `app/src-tauri`: Tauri 2 shell, `build_info` command, `--print-build-info` flag, CSP
- [x] T018 [US1] `.github/workflows/ci.yml` build job: 4 targets, bundles, CLI archives, 90-day artifacts
- [x] T019 [P] [US1] `docs/unsigned-builds.md`

## Phase 5: US3 tests on all three systems (P2)

- [x] T020 [US3] CI test job matrix (fmt, clippy, cargo test, UI tests) on ubuntu, windows, macos-14
- [x] T021 [US3] CI launch-check job: run each CLI and desktop package, compare build ids

## Phase 6: US4 shared engine (P2)

- [x] T022 [US4] CI engine independence check (R13)
- [x] T023 [US4] Launch check asserts CLI and desktop report identical BuildInfo

## Phase 7: US5 contributors (P3)

- [x] T024 [US5] `docs/CONTRIBUTING.md` and root `README.md`

## Phase 8: Polish

- [x] T025 Run fmt, clippy, full test suite locally; fix
- [x] T026 Consistency pass: quickstart commands, research notes and tasks updated

## Dependencies

Setup → Foundational → US2 → US1 → US3 → US4 → US5 → Polish. T016 and T017 can
start once T005 exists. CI tasks (T018, T020 to T023) can only be proven green
once the owner pushes to `Ryware/spiderlens`.

## Status (2026-10-11)

Done locally: 51 tests pass (20 engine unit, 15 engine integration, 14 CLI
contract, 2 UI), fmt and clippy clean, engine independence check passes, and the
launch check passes on Linux against the real AppImage under a virtual display.
Not yet proven: the CI workflow itself (T018, T020 to T023) runs only after the
owner pushes to `Ryware/spiderlens`.
