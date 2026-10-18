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

### Sitemaps and orphan pages

```sh
spiderlens sitemaps example.spiderlens                       # the sitemap files the crawl read, as a tree
spiderlens sitemap generate example.spiderlens --out sitemaps # write XML sitemaps from the crawl
spiderlens orphans example.spiderlens                        # sitemap URLs that no crawled page links to
spiderlens orphans example.spiderlens --nofollow-only --csv  # pages linked only through nofollow links
```

`orphans` runs the crawl analysis first if it has not run (on a read-only file
the results live only for that command) and says on stderr when the crawl did
not reach every page. A List mode crawl follows no links, so it prints the
reason and no rows. Details: [specs/011-sitemaps-orphan-pages](specs/011-sitemaps-orphan-pages/contracts/cli.md).

### Accessibility and mobile checks

Every crawl checks each page's HTML for common accessibility problems and
mobile-friendliness. Results are in the Accessibility and Mobile tabs and in
the details pane. Full checks, which run an accessibility engine in a rendered
page, are optional. Static results can be refreshed on a saved crawl.

```sh
spiderlens crawl example.com --accessibility full --render javascript
spiderlens accessibility example.spiderlens --summary
spiderlens accessibility run example.spiderlens --static
```

Automated checks find only part of the accessibility problems. A page without
violations is not necessarily accessible. Details: [docs/accessibility.md](docs/accessibility.md).

Building, testing and running the desktop app: [docs/CONTRIBUTING.md](docs/CONTRIBUTING.md).
Opening an unsigned test build: [docs/unsigned-builds.md](docs/unsigned-builds.md).
Signing in to password-protected sites, cookies and proxies: [docs/site-login.md](docs/site-login.md).
Where crawls live and what cleanup does: [docs/crawl-storage.md](docs/crawl-storage.md).
Accessibility and mobile checks: [docs/accessibility.md](docs/accessibility.md).
