# Data Model: Crawl Data Management and Cleanup

No crawl file schema change (plan D1). New app-level files and in-memory types only.

## Crawl file (unchanged schema)

| Item | Use in 019 |
|---|---|
| `bodies` | Stored raw HTML. Purged by `DELETE FROM bodies`. Size = `SUM(length(body))`. |
| `pages.html_stored` | Set to 0 for every page on purge. |
| `events` | New kind `html_purged`, detail `{"bytes": n, "pages": n, "compacted": bool}`. Newest `at` is a fallback "last used". |
| `meta` | Read for name, start URL, status, URL count (as 006 `entry_from_file`). |

## Helper files (006, listed by `crawl_files`)

| File | Owner | Leftover when |
|---|---|---|
| `<name>.spiderlens-wal`, `-shm` | SQLite | no `<name>.spiderlens`, older than 24 h |
| `<name>.spiderlens.lock` | 006 writer lock | no crawl file, lock not held, older than 24 h |
| `.<name>.spiderlens.partial` | 006 atomic write | older than 24 h and not locked |
| `<stem> (format N backup).spiderlens` | 006 migration | never a leftover; listed under its crawl (FR-024) |

## ManagedCrawl (engine → app → UI)

| Field | Type | Notes |
|---|---|---|
| path | path | |
| name | string | meta name or start URL host |
| start_url | string? | |
| location | `unsaved` \| `saved` \| `added` | `added` = put in the list with "Add crawl file" |
| available | bool | file exists and readable |
| created_at, last_used | timestamp | last_used per plan D5 |
| urls | u64 | |
| status | string | 006 status, plus `running` when it is the active crawl |
| size_bytes | u64 | crawl file + its helper files |
| html_bytes, html_pages | u64? | loaded lazily (null until known) |
| view_only | bool | newer format (006) |
| backups | `Backup[]` | path, format version, date, size |

## Candidate and CleanupPlan (engine, pure)

`Candidate { path, kind: Crawl | Helper, in_unsaved_dir, last_used, modified, locked, open, running, auto_removed_before }`

`CleanupRule { days: Option<u32>, leftover_age: 24 h, announced: Option<Timestamp> }`

`CleanupPlan { trash: Vec<path>, remove_leftovers: Vec<path>, announce: Vec<path> }`

Rules: a crawl is in `trash` only if `kind = Crawl`, `in_unsaved_dir`, not locked/open/running, not `auto_removed_before`, `last_used` older than `days`, and `announced` is set (else it goes to `announce`). A helper is in `remove_leftovers` only if its crawl does not exist, it is not locked, and `modified` is older than 24 h.

## StoragePrefs (`<config>/storage.json`)

| Field | Type | Default | Range |
|---|---|---|---|
| version | u32 | 1 | |
| cleanup_days | u32? | 30 | 7, 14, 30, 90 or null (off) |
| reserve_bytes | u64 | 1 GiB | 256 MiB to 50 GiB |
| first_cleanup_announced | timestamp? | null | set when the one-time notice is shown |
| last_cleanup | date? | null | at most one run per calendar day |

## CleanupRecord (`<config>/cleanup-history.jsonl`, last 500)

`{ "at": ts, "path": "...", "bytes": n, "reason": "manual_delete" | "manual_delete_permanent" | "auto_age" | "leftover" | "html_purge" }`
