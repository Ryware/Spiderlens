# CLI contract: sitemaps

## Crawl flags (`spiderlens crawl`)

| Flag | Effect | Default |
|---|---|---|
| `--no-sitemaps` | `read = false`, `crawl_sitemap_only = false` | sitemaps read |
| `--no-crawl-sitemap-only` | read sitemaps, record sitemap-only URLs as `sitemap_only` | crawled |
| `--sitemap-url <url>` | add a sitemap (repeatable) | |
| `--sitemap-file <path>=<base-url>` | add a local sitemap file (repeatable) | |
| `--use-sitemaps` | old flag, turns both reading and crawling sitemap-only URLs on | |
| `--list-sitemap <url-or-path>[=<base-url>]` | List mode with a sitemap as the list | |

## `spiderlens sitemaps <file> [--json]`

Plain: one line per file as a tree (indent per level): location, source, status, entries, problems. JSON: `{ "files": [SitemapFileRow], "available": bool }`. Exit 0; exit 2 when the file is not a crawl file.

## `spiderlens orphans <file> [--format plain|csv|json] [--nofollow-only]`

Rows: address, class, sources, status, indexability, sitemap files. Prints the completeness note on stderr when the crawl did not reach every internal URL. In List mode crawls it prints the reason and exits 0 with no rows. Runs stale analysis passes first (read-only files: into temp, not saved).

## `spiderlens sitemap generate <file> --out <dir> [options]`

| Option | Maps to |
|---|---|
| `--base-url <url>` | `base_url` |
| `--include-noindex`, `--include-canonicalised` | |
| `--include <pattern>`, `--exclude <pattern>` | repeatable |
| `--lastmod none|header|crawl|<YYYY-MM-DD>` | `lastmod` |
| `--changefreq <value>` | |
| `--priority off|depth|<0.0-1.0>` | |
| `--images off|inline|separate`, `--image-host <host>` | |
| `--hreflang` | |
| `--max-urls <n>` | |
| `--gzip` | |
| `--prefix <name>` | |
| `--force` | replace existing files (otherwise exit 2 listing them) |
| `--dry-run` | print counts only |
| `--json` | print `GenerateReport` |

Output is byte-identical to the desktop app for the same crawl and options (FR-019, SC-006). Exit 2 on bad options or files that already exist, 3 when the crawl file cannot be read, 4 when no page matches.

## `spiderlens issues`

Sitemap rules are part of the spec 009 `issues` command under category `Sitemaps`; no new flags.
