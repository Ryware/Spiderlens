# Implementation Plan: Project Foundation

**Branch**: `001-project-foundation` | **Date**: 2026-10-11 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `/specs/001-project-foundation/spec.md`

## Summary

Stand up the SpiderLens repository so every change builds and tests itself on
Windows, macOS and Linux, and produces a runnable desktop app and a command-line
tool. The only crawling behaviour in this feature is a single-page check in the
CLI: robots.txt check, fetch with manual redirect following, title extraction and
a typed error model with documented exit codes.

The technical approach is a Cargo workspace. A library crate (`spiderlens-engine`)
holds all crawl logic and knows nothing about Tauri or the UI. Two thin binaries
sit on top of it: the CLI (`spiderlens`) and the Tauri 2 desktop app, whose React
UI only shows identity and version in this feature. GitHub Actions builds and
tests the matrix and keeps artifacts for 90 days. Details and alternatives are in
[research.md](./research.md).

## Technical Context

**Language/Version**: Rust, edition 2024, toolchain pinned with
`rust-toolchain.toml` (stable 1.97). TypeScript 5 for the UI.

**Primary Dependencies**:
- Engine: `tokio` (async runtime), `reqwest` with `rustls` (HTTP, automatic
  redirects off), `url`, `scraper` (HTML5 parser for the title),
  `encoding_rs` (charsets), `texting_robots` (robots.txt), `thiserror`, `serde`.
- CLI: `clap` (derive), `serde_json`, `anyhow`.
- Desktop: `tauri` 2, React 18, Vite.
- Tests: `axum` (fixture website), `assert_cmd`, `predicates`.

**Storage**: N/A. The single-page check stores nothing (spec assumption).

**Testing**: `cargo test --workspace` (unit, integration and CLI contract tests
against a local fixture website on `127.0.0.1` with a random port), `vitest` for
the UI, plus a launch smoke check of each built package in CI.

**Target Platform**: Windows 10+ x64, macOS Intel (x86_64), macOS Apple Silicon
(aarch64), Linux x64 desktop and headless.

**Project Type**: Desktop app plus CLI sharing one library (Cargo workspace).

**Performance Goals**: single-page check against the fixture site under 2 s
(SC-004); desktop window visible within 5 s (SC-003).

**Constraints**: no network use at app start, no telemetry (FR-009); response body
read capped at 50 MB without buffering beyond the cap (FR-019); connect timeout
10 s, read timeout 30 s, configurable from the CLI (FR-018); CI results within
45 minutes (SC-001, SC-006).

**Scale/Scope**: one page per invocation. The workspace layout is sized for
specs 002 to 008 without restructuring.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principle | How this plan complies | Status |
| --- | --- | --- |
| I. Spec before code | Spec 001 approved 2026-10-11; this plan only covers its FRs. | Pass |
| II. Local-first, no servers | Delivery is GitHub Actions artifacts (static hosting). App makes no network calls at start. | Pass |
| III. Engine independent of shell | `spiderlens-engine` has no Tauri or UI dependency; enforced by a CI check on its dependency tree. CLI and desktop both call it. | Pass |
| IV. Respectful crawling | robots.txt checked before every fetch; honest user agent `SpiderLens/<version> (+https://github.com/Ryware/spiderlens)`. | Pass |
| V. Correct and complete data | Every check ends in exactly one of Fetched, BlockedByRobots, Failed; failures carry class, message, attempts. | Pass |
| VI. Scales by design | Body is streamed and capped; nothing else is retained. | Pass |
| VII. Test-first gates | Each acceptance scenario has a named test (table in [quickstart.md](./quickstart.md)); fixture site only; 3-OS matrix. | Pass |
| VIII. Simplicity | Every dependency is justified in research.md. No store, rules or export crates yet; they are added by the specs that need them. | Pass |

Re-check after Phase 1 design: Pass, no violations.

## Project Structure

### Documentation (this feature)

```text
specs/001-project-foundation/
├── plan.md              # This file
├── research.md          # Decisions, rationale, alternatives
├── data-model.md        # PageCheckResult, RedirectHop, ErrorClass, BuildInfo
├── quickstart.md        # Build, run, test; scenario-to-test map
├── contracts/
│   ├── cli.md           # Commands, flags, exit codes, human output
│   └── page-check.schema.json  # JSON output contract
└── tasks.md             # Created by /speckit-tasks
```

### Source Code (repository root)

```text
Cargo.toml                    # workspace: members, shared versions, lints
rust-toolchain.toml
crates/
├── engine/                   # spiderlens-engine (library, no UI deps)
│   ├── build.rs              # injects build id
│   ├── src/
│   │   ├── lib.rs            # public API: check_page, BuildInfo
│   │   ├── build_info.rs
│   │   ├── config.rs         # CheckOptions (timeouts, limits, user agent)
│   │   ├── error.rs          # ErrorClass, CheckError
│   │   ├── fetch.rs          # HTTP client, manual redirects, capped body
│   │   ├── robots.rs         # robots.txt fetch and decision
│   │   ├── title.rs          # charset detection + title extraction
│   │   └── result.rs         # PageCheckResult, RedirectHop, FinalState
│   └── tests/                # integration tests against the fixture site
├── cli/                      # spiderlens (binary)
│   ├── src/main.rs
│   └── tests/cli.rs          # contract tests with assert_cmd
└── test-fixtures/            # spiderlens-test-fixtures (dev-only library)
    └── src/lib.rs            # axum fixture website on a random port
app/
└── src-tauri/                # spiderlens-desktop (Tauri 2 binary)
    ├── Cargo.toml
    ├── tauri.conf.json
    └── src/main.rs           # one command: build_info
ui/                           # React + TypeScript (Vite)
├── package.json
└── src/
    ├── transport.ts          # the only place that talks to Tauri
    └── App.tsx               # identity screen
.github/workflows/ci.yml      # test + build matrix, artifacts, launch checks
docs/
├── CONTRIBUTING.md           # prerequisites, build, test per OS
└── unsigned-builds.md        # opening unsigned builds per OS (FR-004)
specs/                        # Spec Kit features
```

**Structure Decision**: a single Cargo workspace with library and binaries under
`crates/`, the Tauri shell under `app/src-tauri` and the UI under `ui/`. This is
the layout from the Desktop App Architecture plan. Crates for rules, storage,
rendering and export are not created until the spec that needs them (Principle
VIII). `ui/src/transport.ts` is the single seam a future web version would
replace.

## Complexity Tracking

No violations.
