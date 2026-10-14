# Data Model: Sitemaps and Orphan Pages

All new tables and columns come in one additive migration step in `crates/engine/src/crawl/migrate.rs` (number taken at merge: main's `LATEST + 1`). All new columns are nullable. The base `sitemaps` table is still written (older readers).

## Settings (`crates/config`)

`ScopeConfig.sitemaps: SitemapSettings`

| Field | Type | Default | Spec |
|---|---|---|---|
| `read` | bool | true | FR-001, FR-002 |
| `crawl_sitemap_only` | bool | true | FR-009 |
| `urls` | Vec<String> | [] | FR-002 (validated as absolute http/https URLs) |
| `files` | Vec<LocalSitemap { path: String, base_url: String }> | [] | FR-002, US6 |

Reading a config with the old `scope.use_sitemaps` sets `read` and `crawl_sitemap_only` to its value.

`ListInput` gains `Sitemap { location: String }` (a URL or a local path with base URL), FR-005.

`GenerateOptions` (stored per profile under `sitemap_generate`, FR-025):

| Field | Type | Default |
|---|---|---|
| `include_noindex` | bool | false |
| `include_canonicalised` | bool | false |
| `include` / `exclude` | Vec<String> (spec 002 patterns) | [] |
| `lastmod` | `None` / `LastModifiedHeader` / `CrawlDate` / `Fixed(date)` | `LastModifiedHeader` |
| `changefreq` | Option<Changefreq> | None |
| `priority` | `Off` / `Fixed(f32)` / `ByDepth` | `Off` |
| `images` | `Off` / `Inline` / `Separate` | `Off` |
| `image_hosts` | Vec<String> | [] (internal only) |
| `hreflang` | bool | false |
| `max_urls_per_file` | u32 (1..=50,000) | 50,000 |
| `gzip` | bool | false |
| `base_url` | String | the crawl's start URL folder |
| `file_prefix` | String | `sitemap` |

`ByDepth` priority: depth 0 = 1.0, 1 = 0.8, 2 = 0.6, 3 = 0.4, deeper 0.2.

## Crawl file

### sitemap_files

| Column | Type | Notes |
|---|---|---|
| id | INTEGER PK | |
| location | TEXT UNIQUE NOT NULL | URL, or `file:` + path for local files |
| source | TEXT NOT NULL | `robots`, `default`, `user`, `local`, `list` |
| parent_id | INTEGER | index that listed it |
| level | INTEGER NOT NULL | 0 for roots |
| base_url | TEXT | local files only |
| kind | TEXT | `urlset`, `index`, NULL when unreadable |
| http_status | INTEGER | |
| final_url | TEXT | after redirects |
| redirected | INTEGER | 0/1 |
| compressed | INTEGER | 0/1 |
| bytes | INTEGER | uncompressed |
| entry_count | INTEGER NOT NULL DEFAULT 0 | entries read (up to the limit) |
| over_limit | INTEGER | 0/1, more than 50,000 entries or 50 MB |
| error_class | TEXT | spec 004 class, or `invalid_xml`, `not_a_sitemap`, `too_deep`, `loop` |
| error | TEXT | plain message |
| error_line | INTEGER | for XML errors |
| read_at | INTEGER | unix ms |

### sitemap_entries

| Column | Type | Notes |
|---|---|---|
| id | INTEGER PK | |
| file_id | INTEGER NOT NULL | |
| pos | INTEGER NOT NULL | order in the file |
| kind | TEXT NOT NULL | `url` or `sitemap` |
| loc | TEXT NOT NULL | raw text after entity decoding and trimming |
| url_id | INTEGER | matched `urls.id` (`url` entries) |
| invalid | TEXT | why `loc` is not a usable URL |
| lastmod_raw | TEXT | |
| lastmod | INTEGER | UTC seconds when valid |
| changefreq | TEXT | as written |
| priority | TEXT | as written |

Indexes: `(url_id)`, `(file_id, pos)`.

### sitemap_refs

| Column | Type | Notes |
|---|---|---|
| entry_id | INTEGER NOT NULL | |
| kind | TEXT NOT NULL | `image` or `hreflang` |
| raw | TEXT NOT NULL | |
| url_id | INTEGER | matched `urls.id` when discovered |
| lang | TEXT | hreflang only |

Index: `(entry_id)`.

### sitemap_problems (owned by the `sitemap_audit` pass)

| Column | Type | Notes |
|---|---|---|
| rule | TEXT NOT NULL | `sitemap.*` rule id |
| file_id | INTEGER | |
| entry_id | INTEGER | |
| url_id | INTEGER | |
| detail | TEXT | evidence text |

Indexes: `(rule)`, `(url_id)`.

### orphans (owned by the `orphans` pass)

| Column | Type | Notes |
|---|---|---|
| url_id | INTEGER PK | |
| class | TEXT NOT NULL | `orphan` or `nofollow_only` |
| sources | TEXT NOT NULL | comma list: `sitemap` now; `search_console`, `analytics` from spec 024 |

### url_sources (shape agreed for spec 024; created by this step, empty until 024)

| Column | Type | Notes |
|---|---|---|
| url_id | INTEGER NOT NULL | |
| source | TEXT NOT NULL | non-link source id |
| PRIMARY KEY | (url_id, source) | |

The orphans pass reads `urls.in_sitemap = 1` plus `url_sources`.

### Changes to existing data

- `urls.reason` gains `sitemap_only` (TEXT, no schema change).
- `meta.data_features` gains `sitemap_entries`.
- `meta.analysis` gains `sitemap_audit` and `orphans` (spec 009 D12).

## Runtime types

- `sitemap::Entry`, `sitemap::FileFacts`, `sitemap::SitemapJob` (engine).
- `SitemapFileRow` (results, ts-rs exported): the `sitemap_files` row plus `problems: Vec<{ rule, count }>` and `children: u32`.
- `UrlSitemapEntry` (results): `{ file_id, location, lastmod_raw, lastmod }` for the inspector.
- `GenerateReport`: `{ files: Vec<{ name, urls, images, bytes }>, index: Option<String>, pages, skipped_by_pattern, options_cli: String }`.
