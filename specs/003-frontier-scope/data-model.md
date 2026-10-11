# Data Model: Crawl Frontier and Scope

Enums serialize as lowercase `snake_case` strings, like specs 001 and 002.

## Configuration additions (`spiderlens-config`)

| Group | Field | Type | Default | FR |
| --- | --- | --- | --- | --- |
| limits | `list_depth` | integer | 0 | FR-041 (see research R12) |
| scope | `stay_in_folder` | bool | false | FR-011 |
| scope | `max_url_length` | integer | 2000 | FR-006 |
| scope | `query_params` | QueryParams | keep, no sort | FR-005 |
| scope | `follow_internal_nofollow` | bool | false | FR-031 (replaces 002's `follow_nofollow`, which is still read) |
| scope | `follow_external_nofollow` | bool | false | FR-031 |
| scope | `use_sitemaps` | bool | false | FR-034 |

**QueryParams**: `{ "mode": "keep" | "remove_all" | "remove", "names": [string], "sort": bool }`.
The spec 002 form `"keep"` is still read.

## Enums (`spiderlens-engine::crawl`)

**NotCrawledReason**, in FR-038 order:

| Value | Label | Detail |
| --- | --- | --- |
| `malformed_url` | Malformed URL | raw text, or "too long (N characters)" |
| `unsupported_scheme` | Unsupported scheme | the scheme |
| `external` | External | none |
| `outside_start_folder` | Outside start folder | the folder |
| `excluded` | Excluded by rule | the pattern |
| `not_included` | Not matching include rules | none |
| `over_depth_limit` | Over depth limit | the limit |
| `nofollow` | Nofollow | "link" or "page" |
| `blocked_by_robots` | Blocked by robots.txt | the rule line, e.g. `Disallow: /private/` |
| `robots_unavailable` | Blocked: robots.txt unavailable | the robots.txt error |
| `over_url_limit` | Over URL limit | the limit |
| `crawl_stopped` | Crawl stopped | none |

**DiscoverySource**: `start`, `list`, `link`, `redirect`, `canonical`, `hreflang`,
`image`, `sitemap`.

**LinkKind**: `anchor`, `redirect`, `canonical`, `hreflang`, `image`.

**UrlState**: `queued`, `in_flight`, `crawled`, `not_crawled`.

An external URL is `not_crawled` with reason `external`; when it was
status-checked, its response fields are filled in too.

## Crawl file (SQLite, `*.spiderlens`)

Format version 1 of the frontier tables. Spec 006 owns the file and will add
result tables in a later version.

### meta

| key | value |
| --- | --- |
| `format` | `spiderlens-crawl` |
| `format_version` | `1` |
| `config` | StoredCrawlConfig JSON (spec 002) |
| `status` | `running`, `paused`, `stopped`, `completed` |
| `start_url` | normalized start URL (Spider mode) |
| `url_limit_reached` | `true` when FR-019 applied |
| `next_queue_seq` | next queue order number |

### urls (one row per distinct normalized URL)

| Column | Type | Notes |
| --- | --- | --- |
| id | integer PK | discovery order |
| url | text UNIQUE | normalized URL; for malformed links, the raw text |
| host_key | text | `scheme://host:port`, the robots.txt and politeness unit |
| internal | bool | FR-007 |
| depth | integer | FR-017 |
| state | text | UrlState |
| check_only | bool | queued for an external status check |
| queue_seq | integer | order within a depth; re-queued rows get a new, larger one |
| reason | text | NotCrawledReason, null unless `not_crawled` |
| reason_detail | text | see table above |
| source | text | DiscoverySource |
| source_id | integer | page where first found, null for seeds and sitemaps |
| in_sitemap | bool | FR-035 |
| via_nofollow | bool | reached only through a nofollow link that settings allowed (US5-3) |
| robots_rule | text | rule that would block it when robots.txt is ignored but reported (FR-028) |
| status | integer | HTTP status of the single request |
| content_type | text | media type without parameters |
| redirect_to | integer | target url id when the response was a redirect |
| title | text | page title (minimal; spec 005 owns extraction) |
| error_class | text | spec 001 ErrorClass when the request failed |
| error_message | text | plain language |
| fetched_at | integer | Unix ms |

Index `urls_queue(depth, queue_seq) WHERE state = 'queued'`.

### links (one row per occurrence)

| Column | Type | Notes |
| --- | --- | --- |
| from_id | integer | source page |
| to_id | integer | target url row |
| raw | text | href exactly as written (FR-004) |
| kind | text | LinkKind |
| nofollow | bool | link `rel` contains `nofollow` |
| page_nofollow | bool | page-level nofollow applied |
| followed | bool | whether this link let the target into the queue |

### hosts

| Column | Type | Notes |
| --- | --- | --- |
| host_key | text PK | |
| robots_status | text | `ok`, `not_found`, `unavailable`, `ignored` |
| robots_http_status | integer | |
| robots_error | text | |
| crawl_delay_ms | integer | from the matching group |
| robots_txt | text | the file as read (max 500 KiB), so a reopened crawl can explain decisions |

### sitemaps

| Column | Type | Notes |
| --- | --- | --- |
| url | text PK | |
| parent | text | index that listed it |
| url_count | integer | `<loc>` entries read |
| error | text | fetch or parse problem (FR-036) |

## Runtime types

**CrawlSummary** (CLI `--json`, see contracts/crawl-summary.schema.json):
`status`, `discovered`, `crawled`, `not_crawled` (total), `by_reason` (map of
reason to count), `external_checked`, `url_limit_reached`, `robots` (per host
outcome), `sitemaps` (with errors), `warnings`.

Invariant (SC-004): `crawled + not_crawled == discovered` when status is
`completed` or `stopped`; while paused, queued rows make up the difference.

**CrawlEvent** (progress stream): `Started`, `Progress { crawled, queued,
not_crawled }` (at most every 250 ms), `Warning(String)`, `Finished(CrawlSummary)`.
