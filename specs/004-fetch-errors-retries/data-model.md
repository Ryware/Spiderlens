# Data Model: Fetching Pages, Errors and Retries

All changes are additive to crawl format 2 (spec 005). New files get them
in the schema; an older format-2 file gets them on resume through
`ALTER TABLE ADD COLUMN` and `CREATE TABLE IF NOT EXISTS`. Readers detect
them by schema introspection, as spec 007 does for other data. Once written,
`meta.data_features` lists `attempts`.

## `urls` (new columns)

| Column | Type | Meaning |
|---|---|---|
| `attempts` | INTEGER | Requests made for this URL in the whole crawl, manual retries included (FR-013, FR-022). NULL in files from before this feature. Requests returned to the queue because of a host pause or a local problem do not count. |
| `round_attempts` | INTEGER NOT NULL DEFAULT 0 | Requests in the current round. A manual retry resets it so the URL gets a fresh set of automatic attempts (FR-032). |
| `error_reason` | TEXT | Sub-reason of `error_class`, see below. |
| `retry_at` | INTEGER | Epoch ms when a `waiting` URL is due again. NULL otherwise. |
| `manual_retry` | INTEGER NOT NULL DEFAULT 0 | 1 when the URL was re-queued by "Retry failed URLs" (FR-032; "recovered by manual retry" = this and no error). |

`fetched_at` (existing) is the time of the last attempt (FR-013).

### New `urls.state` value

| State | Meaning |
|---|---|
| `waiting` | Failed transiently, waiting `retry_at` before rejoining the queue. Shown as "Waiting to retry". Counted with queued URLs in progress figures. |

### `urls.error_class` values

| Class | Transient | Sub-reasons (`error_reason`) |
|---|---|---|
| `dns` | no | |
| `dns_temporary` | yes | |
| `connection` | yes | `refused`, `reset` (when the OS says which) |
| `timeout` | yes | `connect`, `read` |
| `tls_invalid` | no | `expired`, `hostname`, `untrusted`, `other` |
| `redirect_problem` | no | `invalid_target` (set by the engine), `loop`, `over_limit` (derived at read time) |
| `content_problem` | `truncated` only | `truncated`, `over_size`, `decompression` |
| `malformed_url` | no | |
| `host_unavailable` | yes | |
| `redirect_loop` | no | Read-only: written by files from before this feature. |

### Derived HTTP classes (not stored)

From `status` when `error_class IS NULL`:

| Status | Class | Transient |
|---|---|---|
| 429 | `http_429` | yes |
| 500-599 except 501, 505 | `http_5xx` | yes |
| 501, 505 | `http_5xx` | no |
| 400-499 except 429 | `http_4xx` | no |
| below 100 or 600 and above | `http_other` | no |

One Rust function (`spiderlens_engine::final_class`) and one SQL function
in the results crate implement this table; a test checks they agree.

## `urls.reason` (new not-crawled reason)

| Reason | Label | Meaning |
|---|---|---|
| `redirect_limit` | Redirect chain over limit | A redirect target that would exceed the hop limit; not requested (US3.2). Declared after `nofollow` in FR-038 order. |

## `crawl_log` (new table)

```sql
CREATE TABLE crawl_log (
    id INTEGER PRIMARY KEY,
    at INTEGER NOT NULL,          -- epoch ms
    kind TEXT NOT NULL,
    host_key TEXT,
    url_id INTEGER,
    message TEXT NOT NULL
);
```

| Kind | Written when | Fields |
|---|---|---|
| `retry` | A transient failure is scheduled for another attempt | url_id, host_key; message has attempt n of max, class, wait, whether Retry-After applied |
| `retry_after_capped` | The server asked for a wait above the maximum | url_id, host_key, both values |
| `failure` | A URL ends failed (final attempt or permanent class) | url_id, class, attempts |
| `host_paused` | Breaker opens | host_key, failures, pause length |
| `host_resumed` | Probe succeeded, or user resumed | host_key |
| `host_given_up` | 5th probe failed | host_key, URLs marked |
| `local_problem` | Network down or disk full | message |
| `manual_retry` | "Retry failed URLs" started | counts by class |

Secret header values never appear in messages (FR-003; the engine never
formats header values into messages).

## `meta` (new keys)

| Key | Value |
|---|---|
| `pause_reason` | `network` when the crawl paused itself; removed on resume |
| `manual_retries` | Number of "Retry failed URLs" runs |

## Configuration (`RetrySettings`, spec 002, extended)

| Field | Default | Range | FR |
|---|---|---|---|
| `attempts` | 3 | 1-10 | FR-016 (existing) |
| `max_wait_secs` | 60 | (0, 3600] | FR-017 (existing) |
| `retry_5xx` | true | | existing |
| `retry_429` | true | | existing |
| `base_wait_secs` | 1 | [0.1, 60] | FR-017 |
| `host_pause_after` | 5 | 1-100 | FR-024 |
| `host_pause_secs` | 60 | [1, 3600] | FR-024 |
| `host_pause_max_secs` | 600 | [pause, 86400] | FR-025 |
| `host_give_up_after` | 5 | 1-50 | FR-026 |

Old stored configurations read the new fields as defaults (`serde(default)`),
and `unavailable_settings` reports them, as for every setting (FR-042).

## Engine types

- `ErrorClass` gains `RedirectProblem`, `HostUnavailable`; `RedirectLoop`
  stays for reading old files only.
- `CheckError` gains `reason: Option<ErrorReason>` (serialized only when
  set, so `spiderlens check --json` output keeps its shape).
- `FinalClass` (derived, see table above) with `is_transient()`.
- `Progress` gains `waiting` (URLs waiting to retry), `paused_hosts`
  (host, reason, seconds left, probing) and `local_problem`.
- `CrawlSummary` gains `by_class`, `transient_errors`, `permanent_errors`,
  `retries`, `recovered`, `hosts_paused`, `manual_retries`.
- `RunCommand`: `ResumeHost(String)`, `Apply(Change)`.
