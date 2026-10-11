<!--
Sync Impact Report
- Version change: template → 1.0.0 (initial ratification draft)
- Principles added: I–VIII (all new)
- Sections added: Product Constraints, Development Workflow, Governance
- Templates requiring updates: none (default Spec Kit templates are compatible)
- Ratified by the owner on 2026-10-11
-->

# SpiderLens Constitution

SpiderLens is a desktop SEO crawler for Windows, macOS and Linux, in the spirit of
Screaming Frog, with answer engine optimization (AEO), generative engine
optimization (GEO) and AI agent support added in later milestones.

## Core Principles

### I. Spec Before Code (NON-NEGOTIABLE)

No production code is written for a feature until its `spec.md` has been reviewed
and approved by the project owner. Specs describe what and why, never how.
Technology choices belong in `plan.md`. When behaviour changes, the spec is updated
first and the code follows.

### II. Local-First, No Servers

The product runs entirely on the user's machine. No feature may require a
SpiderLens-operated server, account system or cloud database. Shipping and updates
use static release hosting only. Calls to third-party services (for example an LLM
provider or Google APIs) are opt-in, use the user's own credentials, and the app
remains fully usable without them.

### III. Engine Independent of the Shell

The crawl engine, audit rules and storage are libraries with no dependency on the
desktop shell or the UI. Everything the desktop app can do with a crawl must also be
possible from the command-line interface. This keeps a later web or mobile version
possible without a rewrite.

### IV. Respectful Crawling by Default

Default settings honour robots.txt, crawl-delay, `Retry-After` and per-host rate
limits, and identify the crawler with an honest user agent. Overriding any of these
is an explicit user action, clearly labelled, intended for sites the user controls.

### V. Correct and Complete Data

Every discovered URL ends in exactly one recorded final state. Failures are never
silent: each records an error class, attempt count and message. Every crawl stores
the configuration and app version that produced it, so results can be explained
and reproduced. Reports state how complete the crawl was.

### VI. Scales by Design

Crawl data lives on disk, not in memory, so memory use stays flat as a crawl grows.
No component may load an entire crawl into memory. The UI only requests the data it
displays. Each feature that touches the crawl pipeline states its performance
impact in its plan.

### VII. Test-First Quality Gates

Every acceptance scenario in a spec maps to at least one automated test. Crawl
behaviour is tested against local fixture websites, never live third-party sites.
A feature is done only when all its acceptance criteria pass in CI on Windows,
macOS and Linux, and `/speckit-converge` reports Converged.

### VIII. Simplicity

Build the simplest thing that satisfies the approved spec. New dependencies,
abstractions or settings must be justified in the plan. Prefer configuration of
existing mechanisms (for example a new tab defined as a query) over new code paths.

## Product Constraints

- Platforms: Windows 10+, macOS (Intel and Apple Silicon), mainstream Linux
  desktops.
- Engine language: Rust. Desktop shell: Tauri 2. UI: React with TypeScript.
  Changing any of these requires a constitution amendment.
- One crawl is stored as one self-contained file the user can save, reopen and
  share. Older crawl files must keep opening after upgrades.
- Privacy: no telemetry or crash reporting is sent without explicit opt-in.
- Naming: the product is SpiderLens; the repository and package prefix is
  `spiderlens`.

## Development Workflow

- Each feature lives in `specs/NNN-short-name/` with `spec.md`, `plan.md` and
  `tasks.md`.
- Flow: specify → clarify (recommended) → plan → tasks → analyze (recommended) →
  implement → converge.
- The project owner approves every spec and every pull request. Commits and pushes
  to the repository are made by, or explicitly on behalf of, the owner only.
- Each task is a small, reviewable change that includes its tests.

## Governance

This constitution overrides any other project practice. Every spec, plan and pull
request is checked against it. Amendments require the owner's approval, a version
bump and a note in the Sync Impact Report: MAJOR for removing or redefining a
principle, MINOR for adding one, PATCH for wording.

**Version**: 1.0.0 | **Ratified**: 2026-10-11 | **Last Amended**: 2026-10-11
