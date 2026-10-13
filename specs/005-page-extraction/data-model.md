# Data Model: Page Data Extraction

Crawl file format version 2. `meta.extraction_version = '1'` is written when
the crawl file is created with extraction (every new crawl). A reader
(spec 007) treats a file without that key as "not collected yet".

Text values are stored decoded, trimmed, with internal whitespace collapsed
to one space (FR-009). URLs are absolute. Booleans are 0/1. Positions
(`pos`) count from 0 in document order within one page.

## `urls`: new columns (every fetched resource, HTML or not)

| Column | Type | Meaning |
|---|---|---|
| `response_ms` | INTEGER | Request sent to last body byte read, before extraction (FR-001) |
| `size_bytes` | INTEGER | Body size after transfer decoding; for bodies not downloaded, the declared `Content-Length`; NULL if unknown |
| `transfer_bytes` | INTEGER | Size on the wire when known (uncompressed responses with `Content-Length`); NULL otherwise (plan, owner point 1) |
| `resource_type` | TEXT | `html` or `other`; NULL when not fetched (FR-005) |
| `redirect_kind` | TEXT | `http` or `meta_refresh` when `redirect_to` is set (FR-002) |
| `noindex` | INTEGER | Any noindex/none directive for all crawlers or the crawl's robots token, from meta or header |
| `nofollow` | INTEGER | Same for nofollow/none (page-level) |
| `canonical` | TEXT | First canonical declaration (HTML before header) |
| `canonical_count` | INTEGER | Number of canonical declarations |
| `canonical_conflict` | INTEGER | Declarations point to more than one distinct URL (FR-018) |

Existing `title` stays the primary (first) title. Redirect hops remain the
chain of `urls` rows linked by `redirect_to`; with `redirect_kind` that
covers FR-002 without a separate hops table.

## `pages` (one row per HTML page that went through extraction)

| Column | Type | Meaning |
|---|---|---|
| `url_id` | INTEGER PRIMARY KEY | = `urls.id` |
| `title_count` | INTEGER | FR-011 |
| `meta_description` | TEXT | First one; NULL = missing, `''` = present but empty (FR-012) |
| `meta_description_count` | INTEGER | |
| `h1_1`, `h1_2` | TEXT | First and second H1 |
| `h1_count` | INTEGER | |
| `h2_1`, `h2_2` | TEXT | First and second H2 |
| `h2_count` | INTEGER | |
| `heading_counts` | TEXT | Counts for H1 to H6 as `"1,3,0,1,0,0"` (FR-013) |
| `word_count` | INTEGER | FR-014 |
| `lang` | TEXT | Root `lang` attribute (FR-015) |
| `text_hash` | INTEGER | xxh3-64 of the visible text (FR-015) |
| `title_hash` | INTEGER | xxh3-64 of the primary title; indexed |
| `meta_description_hash` | INTEGER | xxh3-64 of the primary description; indexed |
| `meta_refresh_delay` | REAL | Seconds, when a meta refresh exists |
| `meta_refresh_url` | TEXT | Its absolute target |
| `partial` | INTEGER | Body was cut off (size limit or dropped connection) (FR-008) |
| `encoding_errors` | INTEGER | Invalid bytes were replaced (FR-008) |
| `encoding` | TEXT | Encoding used to decode, e.g. `utf-8`, `windows-1252` |
| `extract_error` | TEXT | Extraction failed; message (FR-033) |
| `no_content` | INTEGER | Labelled HTML but empty or binary (edge case) |
| `html_stored` | INTEGER | Raw HTML kept in `bodies` (FR-031) |

Indexes: `pages_title_hash ON pages(title_hash)`,
`pages_md_hash ON pages(meta_description_hash)`.

## `page_text` (every title, meta description and heading)

| Column | Type | Meaning |
|---|---|---|
| `url_id` | INTEGER | Page |
| `kind` | TEXT | `title`, `meta_description`, `heading` |
| `level` | INTEGER | 1 to 6 for headings, NULL otherwise |
| `pos` | INTEGER | Order within the page, per kind |
| `text` | TEXT | |
| `in_head` | INTEGER | Found inside `<head>` (FR-010); NULL for headings |

Index: `page_text_url ON page_text(url_id, kind, pos)`.

## `directives` (robots directives, all resources)

| Column | Type | Meaning |
|---|---|---|
| `url_id` | INTEGER | |
| `pos` | INTEGER | Order |
| `directive` | TEXT | Lowercase known name (`noindex`, `max-snippet`, ...) or the raw token when unknown (FR-017) |
| `value` | TEXT | Argument, e.g. `50` for `max-snippet:50`, a date for `unavailable_after` |
| `known` | INTEGER | Recognised directive |
| `source` | TEXT | `meta` or `header` |
| `crawler` | TEXT | `all`, or the crawler name in lowercase (`googlebot`) |
| `in_head` | INTEGER | For meta; NULL for header |

Index: `directives_url ON directives(url_id)`.

## `links`: extended (all references from a page)

Existing: `from_id`, `to_id` (now nullable), `raw`, `kind`, `nofollow`,
`page_nofollow`, `followed`. New:

| Column | Type | Meaning |
|---|---|---|
| `pos` | INTEGER | Document order on the page; header entries after HTML ones |
| `target` | TEXT | Resolved absolute URL; set when `to_id` is NULL (non-crawlable) |
| `skip_reason` | TEXT | `self`, `fragment`, `empty`, `malformed`, `unsupported_scheme`; NULL for crawlable (FR-024) |
| `text` | TEXT | Anchor text (anchors, area alt) |
| `alt` | TEXT | Anchors: alt of an image inside. Images: the image's alt. NULL = missing, `''` = empty (FR-020, FR-027) |
| `rel` | TEXT | Raw rel value |
| `sponsored`, `ugc` | INTEGER | From rel (FR-022); `nofollow` already exists |
| `source` | TEXT | `html` or `header` |
| `in_head` | INTEGER | Canonical, hreflang, next/prev: found in `<head>` (FR-010, US3.4) |
| `noscript` | INTEGER | Inside `<noscript>` |
| `lang` | TEXT | hreflang code, including `x-default` (FR-019) |
| `width`, `height` | INTEGER | Declared image size, when numeric (FR-027) |
| `srcset` | INTEGER | Image URL came from `srcset` (FR-027) |
| `in_link` | INTEGER | Image is inside a link (FR-027) / anchor wraps an image (US2.4) |

`kind` values: `anchor`, `redirect`, `canonical`, `hreflang`, `image`, plus
new `meta_refresh`, `next`, `prev`. Every occurrence is a row, so duplicate
links count correctly. Existing indexes `links_from`, `links_to` are kept;
no new link index.

## `structured_data`

| Column | Type | Meaning |
|---|---|---|
| `url_id` | INTEGER | |
| `pos` | INTEGER | Order |
| `kind` | TEXT | `json_ld` or `microdata` |
| `raw` | TEXT | JSON-LD text as written; microdata as a JSON tree `{"type":[..],"id":..,"properties":{"name":[values or nested items]}}` |
| `parseable` | INTEGER | JSON-LD parses as JSON (FR-028); 1 for microdata |

Index: `structured_data_url ON structured_data(url_id)`.

## `bodies` (raw HTML)

| Column | Type | Meaning |
|---|---|---|
| `url_id` | INTEGER PRIMARY KEY | |
| `codec` | TEXT | `zstd` |
| `size` | INTEGER | Uncompressed bytes |
| `body` | BLOB | Compressed body after transfer decoding (byte-identical on read) |

## `headers` (response headers, for the detail pane)

| Column | Type | Meaning |
|---|---|---|
| `url_id` | INTEGER PRIMARY KEY | |
| `headers` | TEXT | JSON array of `[name, value]` in received order; names lowercase |

For redirects, the headers of that hop's response.

## Settings

`CrawlConfig.extraction.store_html: bool` (default `true`), stored with the
crawl configuration (FR-031). Older configs without the section read as the
default.

## Write path (per page, inside the existing 200 ms group commit)

1. `urls` update (response facts) — as today, wider.
2. One `pages` insert, about 3 to 10 `page_text` inserts, 0 to 3
   `directives`, 0 to 2 `structured_data`.
3. One `links` insert per reference (as today), with context columns.
4. One `bodies` insert, one `headers` insert.

All statements are prepared once and cached; nothing is read back.
