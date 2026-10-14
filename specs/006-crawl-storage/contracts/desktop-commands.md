# Desktop commands: crawl storage

All return `Result<T, String>` unless noted; errors are plain-language
messages.

| Command | Arguments | Returns |
|---|---|---|
| `crawl_open` | `path` | `CrawlInfo` (data-model.md) with `access`; recovers the crawl first if it was not closed cleanly; adds it to the recent list |
| `crawl_info` | `path` | `CrawlInfo` without side effects |
| `crawl_resume` | `path`, `changes?` (`{speed?, max_urls?}`), `secrets?` (`{name: value}`), `upgrade?` (`"keep_copy"` / `"no_copy"`) | crawl id; errors with code prefixes `needs_upgrade:`, `needs_secrets:`, `read_only:`, `in_use:` |
| `crawl_save_as` | `path` (current), `dest` | new path; works while running (moves the live crawl) |
| `recent_list` | | `RecentEntry[]`: `path, name, start_url, urls, status, recovered, last_used, available, unsaved` |
| `recent_remove` | `path` | nothing; the file is not touched |
| `recent_locate` | `old`, `new` | updated entry |
| `unsaved_list` | | `{ entries: RecentEntry[], total_bytes, free_bytes }` |
| `unsaved_delete` | `path` | nothing; only paths inside the unsaved folder are accepted |
| `crawl_salvage` | `path` | `CrawlInfo` of the recovered copy (a new unsaved crawl) |
| `storage_status` | | `{ unsaved_dir, free_bytes, reserve_bytes, low: bool }` (FR-031) |
| `pending_open` | | paths the OS asked us to open before the window was ready |

## Events

| Event | Payload |
|---|---|
| `crawl-finished` | unchanged, plus `local: LocalError?` when a disk or write problem paused it |
| `crawl-moved` | `id`, `from`, `to` after Save As of a running crawl |
| `app-quitting` | `{ pausing: bool }` when the user quits during a crawl |
| `open-file` | `path` from the OS (double-click, file association) |
| `crawls-recovered` | `RecentEntry[]` recovered at launch |
