# Desktop Commands: Crawl Comparison

Comparison commands run on a comparison thread with its own read-only
connections; the crawl thread is not involved. Types from ts-rs.

| Command | Arguments | Returns |
|---|---|---|
| `compare_suggest` | `current` | suggested previous path (same start host, most recent) or null |
| `compare_setup` | `previous`, `current` | `{ previous: CrawlInfo, current: CrawlInfo, config_diffs, suggested_mapping? }` or error (same file, unreadable, too new) |
| `compare_test_mapping` | `rules`, `samples[]` or `from_previous: n` | `{ url, mapped, exists_in_current, state? }[]` or rule errors |
| `compare_start` | `previous`, `current`, `settings` | `comparison` id |
| `compare_cancel` | `comparison` | – |
| `compare_summary` | `comparison` | `Summary` |
| `compare_filters` | `comparison`, `tab?` | `FilterDelta[]` |
| `compare_elements` | `comparison` | `ElementDelta[]` |
| `compare_set_elements` | `comparison`, `ElementSetting[]` | `ElementDelta[]` (no rescan) |
| `compare_issues` | `comparison`, `filter: all\|regressions\|improvements` | `IssueDelta[]` |
| `compare_open_group` | `comparison`, `group`, `search`, `sort?` | `RowPage` with `view` (as 007 `view_open`) |
| `compare_rows` | `view`, `from`, `count` | `RowPage` |
| `compare_detail` | `comparison`, `prev_id?`, `cur_id?` | `{ previous?: UrlDetail, current?: UrlDetail }` |
| `compare_export` | `view`, `dest` | `job` (007 export events) |
| `compare_report` | `comparison` | `ComparisonReport` |
| `compare_close` | `comparison` | – (deletes the workspace) |
| `compare_settings_load` / `compare_settings_save` | `path` / `path`, `settings` | `CompareSettings` / – |

## Events

| Event | Payload |
|---|---|
| `compare-progress` | `comparison`, `phase` (`open`, `previous`, `current`, `match`, `counts`, `issues`, `done`, `failed`, `cancelled`), `done`, `total`, `error?` |
