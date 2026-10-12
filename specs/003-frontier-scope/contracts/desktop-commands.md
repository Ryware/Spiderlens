# Desktop Commands: frontier and scope

Specified for parity (FR-042). Not implemented in this feature: UI work waits
for the design direction. Each command calls the same engine function as the CLI.

| Command | Arguments | Returns |
| --- | --- | --- |
| `crawl_start` | `config`, `input`, `out_path` | crawl id; progress arrives as `crawl-progress` events |
| `crawl_pause` / `crawl_stop` | crawl id | nothing; final state arrives as `crawl-finished` |
| `crawl_resume` | `path` | crawl id |
| `crawl_summary` | `path` | CrawlSummary |
| `crawl_urls` | `path`, `filter { state?, reasons? }`, `offset`, `limit` | rows for the results table |
| `scope_test` | `config`, `start_url`, `urls[]`, `fetch_robots` | one verdict per URL |
