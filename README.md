# SpiderLens

A desktop SEO crawler for Windows, macOS and Linux, with answer engine
optimization, generative engine optimization and AI agent support planned.

Everything runs on your machine. There is no SpiderLens server.

## Layout

| Path | What it is |
| --- | --- |
| `crates/engine` | Crawl engine library. No UI dependencies. |
| `crates/results` | Read side of a crawl file: result tabs, filters, sorting, URL detail, CSV export. Shared by the app and the CLI. |
| `crates/cli` | `spiderlens` command-line tool |
| `crates/test-fixtures` | Local fixture website used by tests |
| `app/src-tauri` | Desktop app (Tauri 2) |
| `ui` | Desktop UI (React + TypeScript) |
| `specs` | Feature specs, plans and tasks ([Spec Kit](https://github.com/github/spec-kit)) |

## Quick start

```sh
cargo run -p spiderlens -- check example.com
cargo run -p spiderlens -- crawl example.com --out example.spiderlens
```

## Results

A crawl is saved as it runs to one `.spiderlens` file. In the app, the
result tabs in the sidebar (Internal, External, Response codes, URL, Page
titles and the rest) open a table of its URLs with filters, search, sorting
and a detail pane; ⌘O (Ctrl+O) opens a saved file. The same views export
from the command line:

```sh
spiderlens export example.spiderlens --list-tabs
spiderlens export example.spiderlens --tab response-codes --filter client_error --out 4xx.csv
spiderlens export example.spiderlens --tab page-titles --filter duplicate --sort title --columns address,title
```

The CSV starts with a byte order mark so spreadsheet apps read non-Latin text
correctly, and cells that a spreadsheet would run as a formula start with `'`.

Building, testing and running the desktop app: [docs/CONTRIBUTING.md](docs/CONTRIBUTING.md).
Opening an unsigned test build: [docs/unsigned-builds.md](docs/unsigned-builds.md).
Signing in to password-protected sites, cookies and proxies: [docs/site-login.md](docs/site-login.md).
