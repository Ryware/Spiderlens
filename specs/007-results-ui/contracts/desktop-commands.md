# Desktop Commands: Results Interface

All commands run on the results reader thread (one read-only connection per
open crawl file) except export, which has its own thread. Errors are plain
messages. Types are generated with ts-rs into `ui/src/generated`.

| Command | Arguments | Returns |
|---|---|---|
| `results_tabs` | – | `TabDef[]` (columns, filters, labels, default widths) |
| `crawl_open` | `path` | `CrawlReport` (summary, as `crawl_summary`), `features` |
| `view_open` | `path`, `tab`, `filter`, `scope?`, `search`, `sort? { column, desc }` | `RowPage` for positions 0..n with `view`, `version`, `total?`, `ready` |
| `view_rows` | `view`, `from`, `count` (≤ 500) | `RowPage` |
| `view_locate` | `view`, `ids[]` | positions (null when the id left the view) and current `version` |
| `view_refresh` | `view`, `anchor_ids[]` | new `version`, `total`, anchor positions; no-op when data unchanged |
| `view_counts` | `path`, `tab` | `{ filter: count | null }`, `exact: bool` |
| `view_close` | `view` | – |
| `url_detail` | `path`, `id` | `UrlDetail` |
| `url_list` | `path`, `id`, `kind` (`inlinks`, `outlinks`, `images`, `headings`), `from`, `count` | rows and total |
| `url_html` | `path`, `id`, `from_byte`, `max_bytes` | chunk or `unavailable { reason }` |
| `copy_rows` | `view`, `ranges [[from, to]]`, `columns[]` | TSV text (≤ 10,000 rows, else error) |
| `export_start` | `view`, `columns[]`, `ranges?`, `dest` | `job` |
| `export_cancel` | `job` | – |
| `scope_test` | `config`, `start_url`, `urls[]`, `fetch_robots` | one verdict per URL (spec 003 contract) |

## Events

| Event | Payload |
|---|---|
| `export-progress` | `job`, `rows_written`, `total`, `state`, `error?`, `path?` |
| `crawl-progress` | existing payload plus `active_ms` and `version` (data version for refresh) |

`crawl_urls` from spec 003's contract is superseded by `view_open` +
`view_rows`.
