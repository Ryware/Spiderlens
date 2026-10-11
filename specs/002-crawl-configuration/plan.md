# Implementation Plan: Crawl Configuration

**Branch**: `002-crawl-configuration` | **Date**: 2026-10-11 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `/specs/002-crawl-configuration/spec.md`

## Summary

Spec 002 defines every setting a crawl can have, how it is validated, saved as a
profile, shared as a file, stored inside a crawl, and changed mid-crawl (speed
and retry only). The behaviour those settings drive (frontier, fetching,
storage, rate limiting) belongs to specs 003, 004, 006 and 008.

The approach is a new library crate, `spiderlens-config`, that owns the
configuration as one typed model: defaults, presets, validation, URL-pattern
matching, List-mode input parsing, the profile file format and a profile store.
The engine, CLI, desktop app and (later) the crawl store all depend on it, so
there is exactly one definition of a setting and one validator. The UI never
re-implements validation: it calls the Rust validator through the transport.
TypeScript types are generated from the Rust types.

**Delivery split.** This feature delivers the configuration model, validation,
patterns and tester, List-mode parsing, user agent and speed presets, profiles,
CLI options, the settings screen, and the "every setting stored" contract.
Acceptance scenarios that need a running crawl (for example "exactly 100 URLs
are crawled", "Polite never exceeds 5 URLs per second", "the stored
configuration is reopened from the crawl file") are implemented with the spec
that runs the crawl: 003 (scope, limits, depth, List mode), 004 (retries,
headers on requests, robots modes), 006 (stored configuration and resume) and
008 (speed). Each of those plans must list the 002 scenarios it closes; the
traceability table is in [quickstart.md](./quickstart.md).

## Technical Context

**Language/Version**: Rust 1.97 (edition 2024); TypeScript 5.

**Primary Dependencies** (new):
- `regex` (patterns, linear-time matching), `psl` (registrable domain from
  the compiled-in Public Suffix List), `csv` (List-mode files),
  `serde_ignored` (lists unknown settings on import), `secrecy` (secret header
  values), `ts-rs` (TypeScript types from Rust), `jiff` (timestamps in the
  change log).
- Desktop: Tauri path API for the profile folder (in the desktop crate only).

**Storage**: profiles are JSON files, one per profile, in the user's app
config folder (`%APPDATA%\SpiderLens\profiles`, `~/Library/Application
Support/com.ryware.spiderlens/profiles`, `~/.config/spiderlens/profiles`).
The stored crawl configuration is the same JSON, written into the crawl file
by spec 006.

**Testing**: unit tests per module; property tests for normalization and
round-trips (`proptest`); CLI contract tests; UI component tests (vitest) for
warnings and the "Custom" preset label; a 1,000,000-line List-mode benchmark.

**Target Platform**: as spec 001.

**Project Type**: library crate plus CLI and desktop changes.

**Performance Goals**: parse and count 100,000 List-mode lines in under 5 s
(SC-002; target under 0.5 s); 1,000,000 lines parsed off the UI thread with
progress events, UI stays responsive (about 100 ms).

**Constraints**: secret header values never reach disk or logs (FR-038);
profiles from newer versions load with unknown settings listed (FR-035); no
network use for the suffix list (Principle II).

**Scale/Scope**: about 40 settings in 9 groups; up to 1,000,000 List-mode URLs.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principle | How this plan complies | Status |
| --- | --- | --- |
| I. Spec before code | Spec 002 approved 2026-10-11. | Pass |
| II. Local-first | Profiles on disk only; suffix list compiled in; sharing by file export. | Pass |
| III. Engine independent of shell | `spiderlens-config` has no Tauri dependency; profile folder path is passed in by the shell. CLI accepts the same profile file and options (FR-036). | Pass |
| IV. Respectful crawling | Defaults are Polite, Respect robots, SpiderLens UA; every less-respectful choice is flagged by the validator as a `Warning`, which the UI must display (SC-009). | Pass |
| V. Correct and complete data | Full configuration, including untouched defaults, serialized with the app version; mid-crawl changes logged with timestamps. | Pass |
| VI. Scales by design | List parsing streams line by line; dedupe uses a hash set of normalized URLs (about 100 MB for 1M URLs, documented); spec 006 moves the list to disk. | Pass |
| VII. Test-first | Every scenario mapped to a test or to the spec that closes it. | Pass |
| VIII. Simplicity | One new crate, justified below; no plugin system for settings; preset name is derived, not stored separately. | Pass, see Complexity Tracking |

Re-check after Phase 1 design: Pass.

## Project Structure

### Documentation (this feature)

```text
specs/002-crawl-configuration/
├── plan.md
├── research.md
├── data-model.md
├── quickstart.md
├── contracts/
│   ├── profile.schema.json   # Exported profile file format
│   ├── cli.md                # spiderlens config ... and shared crawl options
│   └── desktop-commands.md   # Tauri commands the settings screen uses
└── tasks.md
```

### Source Code (repository root)

```text
crates/
├── config/                   # NEW: spiderlens-config
│   └── src/
│       ├── lib.rs
│       ├── model.rs          # CrawlConfig, its groups and built-in defaults
│       ├── presets.rs        # user agent and speed presets
│       ├── validate.rs       # issues (errors + warnings) with field paths
│       ├── patterns.rs       # include/exclude matcher + tester
│       ├── url_norm.rs       # normalization used by List mode (003 extends)
│       ├── site.rs           # registrable domain, internal-host rules
│       ├── list_input.rs     # paste/text/CSV parsing, counts, invalid lines
│       ├── secret.rs         # secret header values
│       ├── profile.rs        # file format, import/export, unknown settings
│       ├── store.rs          # ProfileStore over a directory
│       └── stored.rs         # StoredCrawlConfig + change log
├── engine/                   # depends on config; CheckOptions built from it
└── cli/                      # `spiderlens config ...`, shared crawl options
app/src-tauri/src/
└── settings.rs               # commands: validate, presets, profiles, list parse
ui/src/
├── generated/                # TS types from ts-rs (committed, checked in CI)
├── settings/                 # settings screen, pattern tester, list input
└── transport.ts              # new methods
```

**Structure Decision**: configuration gets its own crate because four
consumers (engine, CLI, desktop, store) need it and none of them should own it.
The engine depending on the config, and not the other way round, keeps the
dependency graph acyclic.

## Complexity Tracking

| Addition | Why needed | Simpler alternative rejected because |
| --- | --- | --- |
| New crate `spiderlens-config` | Shared by engine, CLI, desktop and store | Putting it in the engine would force the store and UI type generation to depend on the whole HTTP stack |
| Generated TypeScript types (`ts-rs`) | About 40 settings would otherwise be typed twice | Hand-written TS types drift silently; CI fails if generated files are stale |
