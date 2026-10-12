# Desktop Commands: frontier and scope

Specified for parity (FR-042). Each command calls the same engine function as
the CLI. `crawl_start`, `crawl_pause`, `crawl_stop`, `crawl_resume` and
`crawl_summary` are implemented in `app/src-tauri/src/crawl.rs` (tasks T020);
`crawl_urls` and `scope_test` arrive with spec 007's results table.

| Command | Arguments | Returns |
| --- | --- | --- |
| `crawl_start` | `config`, `start_url` (List mode uses the list parsed in the window) | crawl id and file path; the file goes to the app data folder under `crawls/`; progress arrives as `crawl-progress` events, warnings as `crawl-warning` |
| `crawl_pause` / `crawl_stop` | crawl id | nothing; final state arrives as `crawl-finished` |
| `crawl_resume` | `path` | crawl id |
| `crawl_summary` | `path` | CrawlSummary with reason labels (same payload as `crawl-finished`) |
| `crawl_urls` | `path`, `filter { state?, reasons? }`, `offset`, `limit` | rows for the results table |
| `scope_test` | `config`, `start_url`, `urls[]`, `fetch_robots` | one verdict per URL |

## Events

| Event | Payload |
| --- | --- |
| `crawl-progress` | `id`, `discovered`, `crawled`, `in_flight`, `queued`, `not_crawled`, `errors` (about four a second) |
| `crawl-warning` | `id`, `message` |
| `crawl-finished` | `id`, `path`, and either `report` (`summary`, `reasons` with labels) or `error` |
