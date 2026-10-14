# Contract: desktop commands and events

Each command calls the same engine function as the CLI (constitution III).

| Command | Arguments | Returns |
|---|---|---|
| `crawl_resume_host` | crawl `id`, `host` | nothing; the host is probed at once (US4.5) |
| `crawl_apply_settings` | crawl `id`, `speed?`, `retry?` | nothing; recorded in the crawl's stored configuration as a change (D13) |
| `retry_preview` | `path` | `{ total, by_class: [{ class, label, count }], available, reason? }`; `available = false` with a reason while a crawl runs or when nothing is transient (FR-034) |
| `retry_start` | `path` | crawl id and path, like `crawl_resume`; progress and finish arrive as the usual crawl events |
| `crawl_log` | `path`, `limit` | the newest log entries `{ at, kind, host, url, message }` |

## Events (extended)

| Event | New payload fields |
|---|---|
| `crawl-progress` | `waiting` (URLs waiting to retry), `paused_hosts: [{ host, reason, seconds_left, probing }]`, `waiting_for_host` (every remaining URL's host is paused) |
| `crawl-finished` | `report.summary` gains `by_class`, `transient_errors`, `permanent_errors`, `retries`, `recovered`, `hosts_paused`, `manual_retries`, `pause_reason` |
