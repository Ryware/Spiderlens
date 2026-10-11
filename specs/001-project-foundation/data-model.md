# Data Model: Project Foundation

All types live in `spiderlens-engine` and derive `serde::Serialize`. Field names
are serialized in `snake_case`; enums are serialized as lowercase `snake_case`
strings.

## BuildInfo

| Field | Type | Notes |
| --- | --- | --- |
| product | string | Always `SpiderLens` |
| product_version | string | Workspace version, e.g. `0.1.0` |
| engine_version | string | `spiderlens-engine` crate version |
| build_id | string | `<short sha>` locally, `<short sha>.<run number>` in CI, `dev` if unknown |
| commit | string or null | Full git sha when known; links the build to its change |

## CheckOptions (input)

| Field | Type | Default |
| --- | --- | --- |
| connect_timeout | duration | 10 s |
| read_timeout | duration | 30 s |
| max_body_bytes | integer | 52 428 800 (50 MB) |
| max_redirects | integer | 10 |
| user_agent | string | `SpiderLens/<version> (+https://github.com/Ryware/spiderlens)` |

## PageCheckResult

| Field | Type | Notes |
| --- | --- | --- |
| requested_url | string | As normalised from input (scheme added if missing) |
| state | FinalState | Exactly one |
| redirects | RedirectHop[] | Empty if no redirect |
| final_url | string or null | Null when blocked or failed before a response |
| status | integer or null | HTTP status of the final response |
| content_type | string or null | Media type of the final response, without parameters |
| response_time_ms | integer or null | Time from first request to end of final body read |
| title | Title or null | Null when not fetched |
| error | CheckError or null | Present only when `state = failed` |
| attempts | integer | Requests made for the final page; 1 in this feature |

## FinalState (enum)

`fetched` | `blocked_by_robots` | `failed`

Any HTTP status counts as `fetched` (spec scenario 3).

## RedirectHop

| Field | Type | Notes |
| --- | --- | --- |
| url | string | The URL that answered with a redirect |
| status | integer | 301, 302, 303, 307 or 308 |
| location | string | Absolute target after resolving against `url` |

## Title (tagged union)

| kind | value | Meaning |
| --- | --- | --- |
| `text` | string | Decoded, whitespace-collapsed, trimmed title |
| `missing` | none | HTML with no title or an empty one ("no title") |
| `not_applicable` | none | Response is not HTML |

Serialized as `{"kind":"text","value":"Home | Example"}` or `{"kind":"missing"}`.

## CheckError

| Field | Type | Notes |
| --- | --- | --- |
| class | ErrorClass | Exactly one |
| message | string | Plain language, safe to show to users |
| attempts | integer | Number of attempts made |

## ErrorClass (enum)

| Value | When |
| --- | --- |
| `dns` | Host does not exist (NXDOMAIN) |
| `dns_temporary` | Resolver timeout or SERVFAIL |
| `connection` | Refused, reset, unreachable |
| `timeout` | Connect or read time limit exceeded |
| `tls_invalid` | Certificate or handshake rejected |
| `redirect_loop` | Revisited URL or more than 10 hops |
| `content_problem` | Body over size limit, undecodable or truncated |
| `malformed_url` | Input is not a valid http(s) URL; no request is made |

## State rules

- `malformed_url` is detected before any network activity; the CLI reports it as
  a usage error (exit 2) but the JSON still has `state = failed` with that class.
- robots.txt is evaluated before the page request. A block yields
  `state = blocked_by_robots`, `error = null`, `attempts = 0`.
- Redirect targets are each checked against their own site's robots.txt in later
  specs; in this feature only the requested URL is checked (spec FR-015).
