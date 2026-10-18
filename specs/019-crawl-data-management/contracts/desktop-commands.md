# Desktop Commands: Crawl Data Management (Tauri)

All commands are async and run file work on a blocking thread. Errors are user-facing strings unless noted.

| Command | Args | Returns | Notes |
|---|---|---|---|
| `crawls_manage_list` | – | `{ crawls: ManagedCrawl[], other_files: {path,size}[], totals: {all, unsaved, leftovers}, free_bytes }` | Metadata only; `html_bytes` null (FR-004). |
| `crawls_html_size` | `path` | `{ html_bytes, html_pages }` | One file; UI calls it per row after the list. |
| `crawls_add` | `path` | `ManagedCrawl` | "Add crawl file"; adds to the recent list. |
| `crawls_delete` | `paths[]` | `{ deleted: path[], skipped: {path, reason}[], trash_unavailable: path[], freed_bytes }` | Moves each crawl and its helper files to the trash. Never deletes permanently. |
| `crawls_delete_permanently` | `paths[]` | same shape | Only after the UI's second confirmation for `trash_unavailable`. |
| `crawls_purge_html_check` | `path` | `{ html_bytes, new_size, free_bytes, can_compact }` | For the confirmation (US3.1, US3.4). |
| `crawls_purge_html` | `path, compact: bool` | `{ freed_bytes, compacted }` | Refuses a running crawl. Emits `crawls-changed`. |
| `storage_prefs_get` / `storage_prefs_set` | `StoragePrefs` | `StoragePrefs` / validation issues | Reserve range checked in Rust (US5.3). |
| `cleanup_history` | – | `CleanupRecord[]` | Newest first. |
| `cleanup_run_now` | – | `CleanupReport` | Tests and a hidden debug palette command. |

Events to the window:

- `cleanup-done` `{ removed: number, freed_bytes, announced: path[] }`: shows the notice (FR-020, FR-021).
- `crawls-changed`: the manager and start screen reload.

Scheduled run retention (plan D11): no new command. `cleanup_run_now` and the automatic pass also apply each schedule's `keep_last` (the spec 021 schedule commands carry the field). The schedule editor shows "Keep the last [10] runs / Keep all" (1 to 999) and, when the limit is lowered, how many existing run files will go to the trash at the next cleanup. The `cleanup-done` event gains `retention_removed: number`, and the notice adds "N older scheduled runs were moved to the Trash (each schedule keeps its last runs)." `cleanup_history` can return the reason `schedule_retention`.

Changed 006 command: `unsaved_delete` calls the same trash path as `crawls_delete` and returns its shape.
