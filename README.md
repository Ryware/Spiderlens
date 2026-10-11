# SpiderLens

A desktop SEO crawler for Windows, macOS and Linux, with answer engine
optimization, generative engine optimization and AI agent support planned.

Everything runs on your machine. There is no SpiderLens server.

## Layout

| Path | What it is |
| --- | --- |
| `crates/engine` | Crawl engine library. No UI dependencies. |
| `crates/cli` | `spiderlens` command-line tool |
| `crates/test-fixtures` | Local fixture website used by tests |
| `app/src-tauri` | Desktop app (Tauri 2) |
| `ui` | Desktop UI (React + TypeScript) |
| `specs` | Feature specs, plans and tasks ([Spec Kit](https://github.com/github/spec-kit)) |

## Quick start

```sh
cargo run -p spiderlens -- check example.com
```

Building, testing and running the desktop app: [docs/CONTRIBUTING.md](docs/CONTRIBUTING.md).
Opening an unsigned test build: [docs/unsigned-builds.md](docs/unsigned-builds.md).
