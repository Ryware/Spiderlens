# Data Model: Results Interface

Nothing new is written to the crawl file except one `meta` value
(`active_ms`, D13). Everything here is a read model.

## Tab (static, `crates/results/src/tabs.rs`)

| Field | Meaning |
|---|---|
| `id` | `internal`, `external`, `response-codes`, `url`, `page-titles`, `meta-description`, `h1`, `h2`, `images`, `canonicals`, `directives` |
| `title` | label |
| `eligible` | SQL condition on `urls u` selecting the tab's rows |
| `columns` | ordered `Column`s; the first `default_visible` ones show by default |
| `search_column` | the tab's main text column (FR-016), besides Address |
| `filters` | ordered `Filter`s; the first is All |

Eligibility today:

| Tab | Rows |
|---|---|
| Internal | `internal = 1 AND state = 'crawled'` |
| External | `internal = 0` and a final state (checked, blocked, or "not checked") |
| Response codes | every URL in a final state, including blocked ones |
| URL | `internal = 1 AND state = 'crawled'` |
| Page titles, Meta description, H1, H2, Canonicals, Directives | internal, crawled, status 2xx, HTML content type (FR-010) |
| Images | image URLs (needs spec 005 image records) |

## Column

| Field | Meaning |
|---|---|
| `id`, `label` | stable id (used in preferences and CLI `--columns`), header text |
| `expr` | SQL expression producing the value |
| `kind` | `text`, `url`, `int`, `bytes`, `ms`, `status`, `state`, `bool` (drives sort, alignment and formatting) |
| `width` | default width in px |
| `needs` | `Base`, `Extraction`, `Attempts`, `Timing`, `RawHtml` |

## Filter

| Field | Meaning |
|---|---|
| `id`, `label` | label includes thresholds, e.g. "Over 60 characters" (FR-012) |
| `cond` | SQL condition added to `eligible` |
| `scope` | Response codes only: `all`, `internal`, `external` |
| `needs` | as for columns |

## Availability (per opened file)

`Features { extraction, attempts, timing, raw_html }`, from schema
introspection and `meta` flags. A column or filter whose `needs` is missing
is returned as `unavailable { reason }`.

## View (per window, on the reader thread)

| Field | Meaning |
|---|---|
| `id` | handle returned to the UI |
| `query` | tab, filter, scope, search, sort column and direction |
| `version` | data version the order was built from |
| `total` | rows in the view (exact once materialised) |
| `order` | `temp.view_<id>(pos INTEGER PRIMARY KEY, id INTEGER)` plus index on id |
| `ready` | false while only the first screen exists |

## Row page (wire)

`{ view, version, from, rows: [[id, cell, cell, …]], total, ready }`; cells
are JSON scalars or null; `null` with `unavailable` columns means "not
collected".

## URL detail (wire)

`response { url, url_readable, state, status, status_text, content_type,
depth, error_class, error_message, reason, reason_detail, robots_rule,
found_on, … unavailable fields }`, `redirects [{ url, status }]` with
`loop`/`over_limit` flags, `counts { inlinks, outlinks, images, headings }`,
and `sections_unavailable { section: reason }`.

## Export job

`{ job, view, columns, dest, rows_written, total, state: running |
done | cancelled | failed, error? }`, progress as `export-progress` events.

## Column preferences (UI storage, per tab)

`{ order: [col], hidden: [col], widths: { col: px } }`; unknown ids are
ignored, so preferences survive new columns.
