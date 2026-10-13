# Desktop Commands: frontier and scope

Specified for parity (FR-042). Each command calls the same engine function as
the CLI. `crawl_start`, `crawl_pause`, `crawl_stop`, `crawl_resume` and
`crawl_summary` are implemented in `app/src-tauri/src/crawl.rs` (tasks T020);
`scope_test` is in `app/src-tauri/src/settings.rs` (spec 007 T041).
`crawl_urls` was superseded by spec 007's view commands (`view_open`,
`view_rows` and the rest, specs/007-results-ui/contracts/desktop-commands.md).

| Command | Arguments | Returns |
| --- | --- | --- |
| `crawl_start` | `config`, `start_url` (List mode uses the list parsed in the window) | crawl id and file path; the file goes to the app data folder under `crawls/`; progress arrives as `crawl-progress` events, warnings as `crawl-warning` |
| `crawl_pause` / `crawl_stop` | crawl id | nothing; final state arrives as `crawl-finished` |
| `crawl_resume` | `path` | crawl id |
| `crawl_summary` | `path` | CrawlSummary with reason labels (same payload as `crawl-finished`) |
| `crawl_urls` | superseded by spec 007 `view_open` / `view_rows` | |
| `scope_test` | `config`, `start_url`, `urls[]`, `fetch_robots` | one verdict per URL: `input`, `url`, `would_crawl`, `reason`, `label`, `detail`, `robots_checked` |

## Events

| Event | Payload |
| --- | --- |
| `crawl-progress` | `id`, `discovered`, `crawled`, `in_flight`, `queued`, `not_crawled`, `errors` (about four a second) |
| `crawl-warning` | `id`, `message` |
| `crawl-finished` | `id`, `path`, and either `report` (`summary`, `reasons` with labels) or `error` |
