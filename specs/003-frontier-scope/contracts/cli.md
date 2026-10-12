# CLI Contract: crawl, report, scope test

## New shared crawl options

Added to the shared options of spec 002 (accepted by `crawl`, `scope test` and
`config show|validate`). Defaults are the same as the desktop app (FR-042).

| Flag | Setting | Default |
| --- | --- | --- |
| `--list-depth <N>` | `limits.list_depth` | 0 |
| `--stay-in-folder` | `scope.stay_in_folder` | off |
| `--max-url-length <N>` | `scope.max_url_length` | 2000 |
| `--query-params keep\|remove-all` | `scope.query_params.mode` | keep |
| `--remove-param <NAME>` (repeatable) | `scope.query_params.mode = remove`, adds a name | none |
| `--sort-params` | `scope.query_params.sort` | off |
| `--follow-nofollow internal\|external\|both` | nofollow handling | neither |
| `--use-sitemaps` | `scope.use_sitemaps` | off |

`--follow-nofollow` without a value keeps its spec 002 meaning (internal).

## `spiderlens crawl`

```text
spiderlens crawl <URL> [options] [--out FILE] [--json] [--quiet]
spiderlens crawl --list <FILE> [options] [--out FILE] [--json] [--quiet]
spiderlens crawl --resume <FILE> [--json] [--quiet]
```

- `--out` defaults to `<host>-<UTC timestamp>.spiderlens` in the current folder.
  An existing file is never overwritten (exit 2).
- The configuration is validated first; any error issue exits 2 before a request
  is made.
- Progress goes to stderr about once a second unless `--quiet`.
- On finish, stdout gets the summary: status, crawled, not crawled with counts per
  reason, external checks, robots.txt outcome per host and sitemap problems.
  `--json` prints it as one object (contracts/crawl-summary.schema.json).
- Ctrl-C pauses: in-flight requests finish, the file is saved with status
  `paused`, and the resume command is printed. A second Ctrl-C exits at once.
- `--resume` uses the configuration stored in the file; options other than
  `--json` and `--quiet` are refused (exit 2).

| Exit | Meaning |
| --- | --- |
| 0 | crawl completed (whatever the pages returned) |
| 1 | internal error (for example the crawl file cannot be written) |
| 2 | usage or validation error, or start URL blocked or excluded before any page was fetched |
| 5 | paused by the user; resumable |

## `spiderlens report`

```text
spiderlens report <FILE> [--not-crawled] [--crawled] [--reason <REASON>]... [--json]
```

Lists URLs of a crawl file, one per line: state, reason, reason detail, depth,
source and the URL, tab-separated. With no filter, prints the summary only.
`--json` prints one JSON object per line (JSON Lines) for URL listings.
`<REASON>` uses the values of NotCrawledReason in data-model.md.

## `spiderlens scope test`

```text
spiderlens scope test <URL>... --start <START_URL> [options] [--fetch-robots] [--json]
```

For each URL prints `would crawl` or `would not be crawled: <reason> (<detail>)`,
using the same function as the crawler (FR-016). The URL is treated as a link
found on the start page (depth 1). With `--fetch-robots`, the robots.txt of each
host is fetched first and applied; otherwise robots.txt is reported as "not
checked". Exit 0.
