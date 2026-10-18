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

## Comparing crawls

A comparison shows what changed between a previous and a current crawl of the
same site. For every filter of every tab it counts the URLs that are New,
Added, Removed, Missing or Not reached; it detects changes in the tracked
elements (titles, headings, canonicals, directives and the rest) on pages in
both crawls; and it lists issues as newly affected, fixed or gone. For a
migration, URL mapping rules rewrite the previous URLs before they are
matched. Crawl files are only ever opened read-only; the work happens in a
temporary file that is deleted afterwards, and the setup warns when the temp
folder is low on space.

In the app, start a comparison with Compare crawls… on the start screen or in
the command palette; the open crawl is pre-selected as the current one.

```sh
spiderlens compare old.spiderlens new.spiderlens
spiderlens compare old.spiderlens new.spiderlens --map 'https://old.example.com/=>https://www.example.com/'
spiderlens compare old.spiderlens new.spiderlens --map-regex '^https://old\.example\.com/blog/(.*)=>https://www.example.com/news/$1'
spiderlens compare old.spiderlens new.spiderlens --settings compare.json --json
spiderlens compare old.spiderlens new.spiderlens --export changes   # one CSV per non-empty group
spiderlens compare new.spiderlens --baseline schedule              # against the schedule's previous run
spiderlens report new.spiderlens --html report.html --compare-with old.spiderlens
```

`--issue-settings current|previous|app` picks whose issue settings both crawls
are evaluated with. For a pipeline, `--fail-on-missing N` (internal HTML pages
missing), `--fail-on-lost-indexable N` (pages that stopped being indexable) and
`--fail-on-new-issues PRIORITY` (an issue at that priority or higher with newly
affected URLs) make the command exit with code 8 and say which condition failed.
Details: [specs/012-crawl-comparison](specs/012-crawl-comparison/contracts/cli.md).

## Accessibility and mobile checks

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
Rendering pages with JavaScript, and the browser it downloads: [docs/javascript-rendering.md](docs/javascript-rendering.md).
