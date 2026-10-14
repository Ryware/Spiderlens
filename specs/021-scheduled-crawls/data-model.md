# Data Model: Scheduled Crawls

## Schedule (`schedules.json`, one array)

| Field | Type | Notes |
|---|---|---|
| `id` | string | 8 lowercase base32 chars, stable; used in OS registration names |
| `name` | string | 1 to 80 chars, unique (case-insensitive) |
| `enabled` | bool | |
| `config` | `CrawlConfig` | own copy (FR-003); secret headers refused |
| `source_profile` | string? | profile it was copied from |
| `source_hash` | string? | xxh3 of the profile JSON at copy time |
| `input` | `{ spider: { start_url } }` or `{ list: { path } }` | List file re-read every run |
| `frequency` | `{ kind, cron }` | kind: `hours`, `daily`, `weekly`, `monthly`, `advanced`; cron always set |
| `output_dir` | path | |
| `catch_up` | bool | default true |
| `notify` | `always` / `problems` / `never` | default `problems` |
| `after` | `AfterStep[]` | |
| `created_at`, `updated_at` | timestamp | |

`AfterStep`: `{ kind: "export", tabs: [{ tab, filter? }] }`,
`{ kind: "report", formats: [...] }` (010), `{ kind: "compare" }` (012).

Registration state is not stored; it is read from the OS (`inspect`):
`registered { program }`, `suspended`, `not_registered { reason }`.

## Run (`history/<id>.jsonl`, one JSON object per line)

| Field | Type | Notes |
|---|---|---|
| `run` | int | 1, 2, 3 per schedule |
| `slot` | timestamp? | scheduled time; null for Run now |
| `trigger` | `scheduled` / `caught_up` / `run_now` | |
| `status` | `running`, `finished`, `stopped_at_limit`, `stopped_by_user`, `failed`, `paused_disk`, `interrupted`, `skipped`, `missed` | |
| `reason` | string? | plain language, required unless finished |
| `started_at`, `ended_at` | timestamp? | |
| `counts` | `{ discovered, crawled, not_crawled, failed }`? | |
| `file` | path? | |
| `steps` | `[{ kind, status: done/skipped/failed, outputs: [path], reason? }]` | |

## Crawl file `meta.schedule`

`{"id":"k3f9a2qd","name":"Client A weekly","slot":"2026-10-19T07:00:00+03:00[Asia/Jerusalem]","trigger":"scheduled","run":12}`
