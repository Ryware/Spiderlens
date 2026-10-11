# Data Model: Crawl Configuration

All types are in `spiderlens-config`, derive `Serialize`, `Deserialize` and
`ts_rs::TS`, and serialize in `snake_case`. `CrawlConfig` serializes nested by
group: `{mode, limits, scope, identity, speed, robots, network, retry}`; in
JSON, robots fields are `robots.mode` and `robots.honour_crawl_delay`. Missing fields take defaults when
loading (older files); unknown fields are reported (newer files).

## CrawlConfig

| Group | Field | Type | Default |
| --- | --- | --- | --- |
| mode | `mode` | `spider` \| `list` | `spider` |
| limits | `max_urls` | integer ≥ 1 or null | null (no limit) |
| | `max_depth` | integer ≥ 0 or null (spider only) | null |
| scope | `include` | string[] (regex) | [] |
| | `exclude` | string[] (regex) | [] |
| | `subdomains` | `this_host` \| `all_subdomains` | `this_host` |
| | `check_external` | bool | true |
| | `follow_nofollow` | bool | false |
| | `query_params` | `keep` (003 adds options) | `keep` |
| identity | `user_agent` | UserAgent | preset `spiderlens` |
| | `headers` | CustomHeader[] | [] |
| speed | `speed` | SpeedSettings | Polite |
| robots | `robots_mode` | `respect` \| `ignore_report` \| `ignore` | `respect` |
| | `honour_crawl_delay` | bool | true |
| network | `connect_timeout_secs` | number > 0 | 10 |
| | `read_timeout_secs` | number > 0 | 30 |
| | `max_redirects` | integer 0–20 | 10 |
| | `max_body_mb` | integer 1–4096 | 50 |
| retry | `retry` | RetrySettings | see below |

## UserAgent

| Field | Type | Notes |
| --- | --- | --- |
| `preset` | `spiderlens` \| `googlebot_smartphone` \| `googlebot_desktop` \| `bingbot` \| `chrome_desktop` \| `chrome_mobile` \| `custom` | |
| `custom_string` | string or null | Required when `preset = custom` |
| `robots_token` | string or null | Required when `custom`; presets supply it |

Preset strings and tokens are constants in `presets.rs`; the Chrome strings
are updated with each release. Tokens: `SpiderLens`, `Googlebot`, `Googlebot`,
`bingbot`, `*`, `*`.

## CustomHeader

| Field | Type | Notes |
| --- | --- | --- |
| `name` | string | RFC 9110 token; not in the reserved list |
| `value` | `{"plain": "…"}` or `{"secret": true}` | Secret values never serialized |
| `send_to` | `internal` \| `all` | default `internal` |

Reserved names: Host, User-Agent, Content-Length, Connection,
Transfer-Encoding, Keep-Alive, Upgrade, TE, Trailer, Proxy-Connection.

## SpeedSettings

| Field | Type | Polite | Fast |
| --- | --- | --- | --- |
| `max_concurrent` | integer 1–512 | 8 | 64 |
| `max_per_host` | integer 1–`max_concurrent` | 4 | 32 |
| `max_urls_per_sec` | number > 0 or null | 5 | null |
| `adaptive` | bool | true | true |
| `preset` (derived, output only) | `polite` \| `fast` \| `custom` | | |

## RetrySettings

| Field | Type | Default |
| --- | --- | --- |
| `attempts` | integer 1–10 (total, not extra) | 3 |
| `max_wait_secs` | number > 0 | 60 |
| `retry_5xx` | bool | true |
| `retry_429` | bool | true |

## ModeInput

- `Spider { start_url: String }`
- `List { urls: Vec<String>, duplicates: u64, invalid_count: u64, invalid_sample: Vec<InvalidLine> }`

`InvalidLine { line: u64, text: String, reason: String }`.

## Issue (validation result)

| Field | Type | Notes |
| --- | --- | --- |
| `path` | string | e.g. `scope.include[1]`, `identity.headers[0].name`, `input.start_url` |
| `severity` | `error` \| `warning` | errors block start |
| `code` | string | stable id, e.g. `pattern_syntax`, `reserved_header`, `less_respectful_speed` |
| `message` | string | plain language with the fix |

Warnings always produced (SC-009): non-SpiderLens user agent, speed above
Polite, robots mode not `respect`, crawl-delay off, start URL excluded by
patterns (FR-013).

## Profile file

See [contracts/profile.schema.json](./contracts/profile.schema.json).

## StoredCrawlConfig

| Field | Type |
| --- | --- |
| `app_version` | string |
| `format_version` | integer |
| `started_at` | RFC 3339 timestamp |
| `config` | CrawlConfig (locked) |
| `input` | ModeInput |
| `resolved_hosts` | string[] (internal hosts after start redirect, FR-003) |
| `changes` | `{at, speed}` or `{at, retry}` entries, in order |

`effective()` returns `config` with the changes applied. Fields missing in a
stored config from an older version are reported as "not available in this
version" by comparing the raw JSON keys with the current model.
