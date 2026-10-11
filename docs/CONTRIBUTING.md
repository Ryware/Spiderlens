# Contributing

Every feature starts as a spec in `specs/NNN-name/` and is approved before code
is written. See `.specify/memory/constitution.md`.

## Prerequisites

All systems:

- [rustup](https://rustup.rs). The repository's `rust-toolchain.toml` selects
  the Rust version automatically.
- Node.js 22 and pnpm 10 (`corepack enable` installs pnpm).

Desktop app only (the CLI does not need these):

| System | Install |
| --- | --- |
| Windows 10/11 | Microsoft C++ Build Tools ("Desktop development with C++"). WebView2 is already part of Windows 11 and current Windows 10. |
| macOS | `xcode-select --install` |
| Ubuntu / Debian | `sudo apt install libwebkit2gtk-4.1-dev libgtk-3-dev librsvg2-dev build-essential` |
| Fedora | `sudo dnf install webkit2gtk4.1-devel gtk3-devel librsvg2-devel` |

## Build and run

```sh
pnpm install                          # once
cargo run -p spiderlens -- check example.com
pnpm dev                              # desktop app with live reload
pnpm build                            # desktop installer for this system
cargo build --release -p spiderlens   # CLI only
```

## Test

One command runs everything:

```sh
pnpm --filter spiderlens-ui build && cargo test --workspace && pnpm test
```

Notes:

- `cargo test` without `--workspace` skips the desktop crate, so the engine and
  CLI can be tested on a machine without webview libraries.
- Tests only use the local fixture website in `crates/test-fixtures`. No test
  contacts a live site.
- Before sending a change: `cargo fmt --all` and
  `cargo clippy --workspace --all-targets -- -D warnings`.

## Continuous integration

`.github/workflows/ci.yml` runs on every pull request and every push to `main`:

1. **Test** on Ubuntu, Windows and macOS (Apple Silicon): format, the engine
   independence check, clippy, Rust tests, UI tests.
2. **Build** for Windows x64, macOS Apple Silicon, macOS Intel and Linux x64:
   desktop package and CLI, then a launch check that starts both and compares
   their build identity.

Packages are attached to the workflow run as artifacts for 90 days.

## Build identity

Every build reports its product version, engine version and build id
(`spiderlens version`, or the desktop start screen). In CI the build id is
`<first 7 characters of the commit>.<run number>`; locally it is the short
commit, or `dev` outside git.
