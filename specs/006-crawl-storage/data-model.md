# Data Model: Crawl Storage

## Crawl file (format 3)

The format-2 schema from spec 005 ([data-model](../005-page-extraction/data-model.md))
is the base. Format 3 adds one table through migration step 3.

### `events` (new, step 3, additive)

| Column | Type | Notes |
|---|---|---|
| `id` | INTEGER PRIMARY KEY | order of events |
| `at` | INTEGER NOT NULL | Unix milliseconds |
| `kind` | TEXT NOT NULL | `created`, `resumed`, `paused`, `stopped`, `completed`, `recovered`, `saved`, `upgraded`, `config_changed`, `local_error` |
| `app_version` | TEXT NOT NULL | e.g. `0.1.0` |
| `os` | TEXT NOT NULL | `macos`, `windows`, `linux` plus arch, e.g. `macos-aarch64` |
| `detail` | TEXT | JSON: `{"from": 2, "to": 3}` for upgraded, `{"path": ...}` for saved, `LocalError` for local_error, the change for config_changed |

Version records (FR-032) are derived: the `created`, `resumed` and
`upgraded` rows, each with its `app_version` and `os`.

### `meta` keys (new or changed)

| Key | Values | Meaning |
|---|---|---|
| `format_version` | `3` | last migration step applied (was fixed at 2) |
| `read_compat` | `2` | oldest format whose reader can read this file; raised only by non-additive steps |
| `status` | `running`, `paused`, `stopped`, `completed` | unchanged; `running` with no lock holder = crashed (plan D3) |
| `recovered` | `1` or absent | set by recovery, cleared when the crawl completes |
| `pause_reason` | `user`, `quit`, `disk_low`, `disk_full`, `drive_gone`, `permission`, `other` | why it is paused; absent when not paused |
| `name` | text | optional display name (defaults to the file name) |

All other keys (`config`, `start_url`, `active_ms`, `warnings`,
`url_limit_reached`, `sitemaps_read`, `extraction_version`) are unchanged.

### Migration steps (`crates/engine/src/crawl/migrate.rs`)

| Version | Additive | Change | Owner |
|---|---|---|---|
| 2 | base | spec 005 schema (`store.rs` `SCHEMA`) | 005 |
| 3 | yes | `events` table | 006 |
| 4+ | | next spec (004 attempts/retries columns expected) | 004 |

## Lock file `<file>.lock`

Not part of the crawl. JSON note `{app_version, pid, host, since}` under an
OS advisory lock held while a process may write the crawl. Deleted on clean
close; a stale file without a live lock is ignored.

## Recent crawls (`<app config>/recent.json`)

```json
{ "version": 1, "entries": [
  { "path": "/Users/x/Crawls/shop.spiderlens", "last_used": "2026-10-13T14:02:11Z",
    "name": "shop", "start_url": "https://shop.example/", "urls": 48211,
    "status": "paused", "recovered": false } ] }
```

At most 20 entries, most recent first. Cached fields are refreshed from the
file when listed. `available` and `unsaved` are computed, not stored.

## Engine types

```rust
pub struct CrawlInfo {           // crawl_open / spiderlens info
    pub path: PathBuf, pub name: String, pub size_bytes: u64,
    pub format_version: i64, pub read_compat: i64,
    pub status: String, pub pause_reason: Option<String>, pub recovered: bool,
    pub mode: Mode, pub start_url: Option<String>,
    pub config: Option<StoredCrawlConfig>,           // None if unreadable
    pub started_at: Option<Timestamp>, pub ended_at: Option<Timestamp>,
    pub counts: InfoCounts,                          // discovered, crawled, failed, not_crawled, queued
    pub versions: Vec<VersionRecord>, pub events: Vec<EventRow>,
    pub access: Access,                              // see below
    pub missing_secrets: Vec<String>,                // header names to ask for on resume
}
pub enum Access { Writable, InUse { by: LockNote }, ReadOnlyMedia, NewerFormat { needs: String },
                  NeedsUpgrade { from: i64 }, ViewOnly { reason: String } }
pub struct LocalError { pub kind: LocalErrorKind, pub message: String,
                        pub path: PathBuf, pub free_bytes: Option<u64> }
```
