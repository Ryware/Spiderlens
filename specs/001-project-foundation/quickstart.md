# Quickstart: Project Foundation

## Prerequisites

- Rust via rustup (the repo's `rust-toolchain.toml` selects the version)
- Node 22 and pnpm 10 (desktop UI only)
- Tauri 2 system dependencies: see `docs/CONTRIBUTING.md` (WebView2 on Windows,
  Xcode command-line tools on macOS, webkit2gtk-4.1 on Linux)

## Run the CLI

```sh
cargo run -p spiderlens -- check example.com
cargo run -p spiderlens -- check https://example.com --json
cargo run -p spiderlens -- version
```

## Run the desktop app

```sh
pnpm install
pnpm dev
```

## Run all tests (one command, US5 scenario 1)

```sh
pnpm --filter spiderlens-ui build && cargo test --workspace && pnpm test
```

## Scenario-to-test map (FR-028)

| Scenario | Test |
| --- | --- |
| US1-1 build per platform | CI job `build` matrix (4 targets) |
| US1-2/3/4 window shows identity | CI `launch-check` + `ui/src/App.test.tsx::shows_identity` |
| US1-5 distinct build ids | `engine::build_info::tests::build_id_from_env` + CI `launch-check` compares to `GITHUB_SHA` |
| US1-6 unsigned steps | `docs/unsigned-builds.md` (manual review) |
| US2-1 title 200 | `cli::check_ok_title` |
| US2-2 two redirects | `cli::check_two_redirects` |
| US2-3 404 is fetched | `cli::check_404_is_fetched` |
| US2-4 no title | `cli::check_missing_title`, `engine::title::tests::*` |
| US2-5 non-HTML | `cli::check_non_html` |
| US2-6 DNS error | `cli::check_dns_error` |
| US2-7 malformed URL | `cli::check_malformed_url` |
| US2-8 robots blocked | `cli::check_robots_blocked` |
| US2-9 JSON output | `cli::json_matches_schema_for_every_fixture` |
| US3-1/2 per-OS results | CI job `test` matrix |
| US3-3 no live sites | All tests use `spiderlens-test-fixtures`; CI `test` runs with no external hosts needed |
| US3-4 launch check | CI job `launch-check` |
| US3-5 traceability | This table |
| US4-1 same version | CI `launch-check` compares CLI and desktop `build_info` |
| US4-2 headless Linux | CI `test` on ubuntu runs CLI tests without a display |
| US4-3 CLI packages | CI job `build` uploads `spiderlens-cli-<target>` |
| US5-1 one command | This file |
| US5-2 specs folder | `specs/001-project-foundation/` |
