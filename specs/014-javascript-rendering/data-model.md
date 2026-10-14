# Data Model: JavaScript Rendering

All tables below arrive in one additive migration step in `crates/engine/src/crawl/migrate.rs` (spec 006 mechanism). **It is step 6**, after 006's step 3, 004's step 4 and step 5 (Set-Cookie values removed, PR #51), main at 3ccf82c. `store.rs` `SCHEMA` (the format-2 base) is not edited. Nothing in the existing tables changes, so a reader that does not know about rendering sees the raw crawl exactly as before (FR-020).

Conventions are those of spec 005's data model: text decoded, trimmed and whitespace-collapsed; URLs absolute; booleans 0/1; `pos` counts from 0 in document order. `url_id` always equals `urls.id`.

## Settings: `CrawlConfig.rendering` (`crates/config/src/model.rs`)

Saved with the crawl configuration (FR-034). Older configurations without the section load with these defaults, so rendering is off.

| Field | Type | Default | Rule |
|---|---|---|---|
| `mode` | `raw` \| `javascript` | `raw` | FR-001 |
| `max_render_secs` | number | `10` | 1 to 60 (FR-010) |
| `min_wait_secs` | number | `0` | 0 to `max_render_secs` |
| `window.preset` | `auto` \| `mobile` \| `desktop` \| `custom` | `auto` | `auto` follows the user agent: mobile for `googlebot_smartphone`, `chrome_mobile` and custom strings containing `Mobile`; desktop otherwise (FR-008) |
| `window.width`, `window.height` | integer | `null` | Required for `custom`: width 320 to 3,840, height 480 to 4,320 CSS px. Presets: mobile 411 × 731, desktop 1024 × 768 |
| `block.images`, `block.media`, `block.fonts` | bool | `true` | FR-014 |
| `block.stylesheets`, `block.third_party` | bool | `false` | FR-014. "Load all resources" in the UI sets the three image/media/font flags to false |
| `include`, `exclude` | list of regex | `[]` | Same pattern rules and validation as `scope.include`/`exclude` (FR-005) |
| `render_error_pages` | bool | `false` | Also render internal 4xx/5xx HTML pages (FR-004) |
| `concurrency` | integer or `null` | `null` (auto) | 1 to 16; auto = total memory GB / 4, clamped (FR-037) |
| `memory_budget_mb` | integer or `null` | `null` (auto) | 256 to 65,536; auto = total memory / 8 (FR-038) |
| `screenshots` | bool | `false` | FR-033 |
| `js_content_min_percent` | integer | `20` | 1 to 1,000 (FR-027) |
| `js_content_min_words` | integer | `50` | 0 to 100,000 (FR-027) |
| `hidden_content_percent` | integer | `50` | 1 to 100 (FR-030) |

Validation codes follow spec 002 (`rendering.max_render_secs.range`, `rendering.include.syntax`, and so on). The browser path is a machine preference (desktop app setting, `--browser` flag), never stored in a crawl file, because the file is meant to be shared.

`StoredCrawlConfig.changes` gains `Change::Rendering(RenderSettings)` for rendering an existing crawl later (plan owner point 2).

## Meta keys

| Key | Value |
|---|---|
| `renderer` | JSON `{"name": "Chrome Headless Shell" \| "Google Chrome" \| "Microsoft Edge" \| "Chromium", "version": "155.0.8059.39", "source": "downloaded" \| "installed" \| "user_path"}` (FR-003). No file path: the file may be shared |
| `render_version` | `"1"`: written when a crawl renders |
| `data_features` | gains `rendering`; `rendered_html` when "Store page HTML" is on; `screenshots` when screenshots are on |
| `fetch_completed_at` | Unix ms when the fetch stage finished while rendering continued |

## `renders` (one row per page eligible for rendering; also the render queue)

| Column | Type | Meaning |
|---|---|---|
| `url_id` | INTEGER PRIMARY KEY | |
| `state` | TEXT NOT NULL | `queued`, `rendering`, `rendered`, `timed_out`, `failed`, `not_rendered` |
| `reason` | TEXT | `failed`: `crash`, `renderer_error`, `page_too_large`, `navigation_error`. `not_rendered`: `outside_rules`, `crawl_stopped`, `browser_unavailable`, `html_not_stored`. NULL otherwise |
| `reason_detail` | TEXT | Message from the browser or the engine |
| `queue_seq` | INTEGER | Queue order; NULL once final |
| `attempts` | INTEGER NOT NULL DEFAULT 0 | At most 2 (FR-042) |
| `render_ms` | INTEGER | Navigation start to output, last attempt |
| `window_w`, `window_h` | INTEGER | Window used, CSS px, before the height extension |
| `js_redirect` | TEXT | Target URL of a script navigation (FR-013) |
| `js_redirect_id` | INTEGER | Its `urls.id` once discovered |
| `console_errors`, `console_warnings` | INTEGER | Totals, including entries over the stored limit (FR-043) |
| `resources_failed`, `resources_blocked` | INTEGER | Totals (FR-018) |
| `bot_challenge` | INTEGER | FR-044 |
| `rendered_at` | INTEGER | Unix ms |

Indexes: `renders_queue ON renders(queue_seq) WHERE state = 'queued'` (the queue), `renders_state ON renders(state, reason)` (summary and filters).

State transitions: `queued → rendering → {rendered | timed_out | failed}`; `rendering → queued` (retry after a crash, or resume after pause); `queued → not_rendered` (stop, browser unavailable); insert directly as `not_rendered` (`outside_rules`, `html_not_stored`). No other transitions.

## `render_inputs` (temporary)

Only when rendering is on and "Store page HTML" is off (research R8). Deleted when the page's `renders` row becomes final.

| Column | Type | Meaning |
|---|---|---|
| `url_id` | INTEGER PRIMARY KEY | |
| `codec` | TEXT | `zstd` |
| `size` | INTEGER | Uncompressed bytes |
| `body` | BLOB | Compressed raw body, as in `bodies` |

## `rendered_pages` (rendered counterpart of `pages`)

Same columns and meanings as `pages` (spec 005 data model) except `partial`, `encoding_errors`, `encoding` and `html_stored`, plus:

| Column | Type | Meaning |
|---|---|---|
| `title` | TEXT | Primary rendered title (raw is `urls.title`) |
| `noindex`, `nofollow` | INTEGER | Rendered meta directives combined with the header directives (headers are the same for both versions, FR-019) |
| `canonical`, `canonical_count`, `canonical_conflict` | TEXT, INTEGER, INTEGER | As in `urls`, from the rendered page plus the Link header |
| `rendered_html_stored` | INTEGER | Row in `rendered_bodies` |

Indexes: `rendered_pages_title_hash`, `rendered_pages_md_hash`, built and dropped with the spec 005 result indexes.

## `rendered_text`, `rendered_directives`, `rendered_structured_data`

Same columns as `page_text`, `directives` (meta directives only; header directives stay in `directives`) and `structured_data`, plus:

| Column | Type | Meaning |
|---|---|---|
| `origin` | TEXT NOT NULL | `both`, `rendered` (only after rendering) or `raw` (in the raw HTML, gone after rendering) (FR-023) |

Rows with origin `both` or `rendered` are the rendered page in rendered order (`pos`). Rows with origin `raw` carry the raw `pos` and exist so the detail pane can list what JavaScript removed. Indexes as for the raw tables.

## `rendered_links` and `raw_only_links`

Links are the largest table (spec 005 benchmarks: 144 MB for 100,000 pages), so links found in both versions are not copied.

`rendered_links`: same columns as `links`, for references found **only** in the rendered page (origin `rendered`), with `pos` counting on from the page's last raw position and an extra `rpos` (position in the rendered document). Indexes: `rendered_links_from(from_id)`, `rendered_links_to(to_id)`.

`raw_only_links (from_id INTEGER, pos INTEGER, PRIMARY KEY (from_id, pos)) WITHOUT ROWID`: raw `links` rows whose reference is gone after rendering.

So for a rendered page: rendered view = raw links not in `raw_only_links` plus `rendered_links`; origin of a raw link = `raw` if listed, else `both`.

## `render_comparisons` (FR-024, FR-029)

| Column | Type | Meaning |
|---|---|---|
| `url_id` | INTEGER PRIMARY KEY | |
| `words_raw`, `words_rendered`, `words_change` | INTEGER | |
| `links_raw`, `links_rendered` | INTEGER | Anchor links |
| `links_rendered_only`, `links_raw_only` | INTEGER | |
| `title_diff`, `description_diff`, `h1_diff`, `canonical_diff`, `noindex_diff`, `nofollow_diff`, `hreflang_diff` | TEXT | `same`, `added`, `removed`, `changed` |
| `structured_rendered_only` | INTEGER | Blocks only after rendering |
| `dep_text`, `dep_headings`, `dep_internal_links`, `dep_structured`, `dep_images` | REAL | 0 to 1: share only in the rendered page; NULL when the rendered page has none of that type |
| `raw_has_title`, `raw_has_description`, `raw_has_canonical`, `raw_has_directives` | INTEGER | |
| `dependence` | REAL | Overall percentage, 0 to 100 = `dep_text` × 100 (FR-029) |

Both values of a changed element are read from `urls`/`pages` and `rendered_pages`, not copied here.

## `render_resources` (FR-018)

| Column | Type | Meaning |
|---|---|---|
| `url_id` | INTEGER | Page |
| `pos` | INTEGER | Request order |
| `url` | TEXT | |
| `type` | TEXT | CDP resource type, lowercase: `script`, `stylesheet`, `image`, `media`, `font`, `xhr`, `fetch`, `document`, `other` |
| `host` | TEXT | Host key |
| `third_party` | INTEGER | Host is not internal |
| `outcome` | TEXT | `loaded`, `cached`, `failed`, `blocked_setting`, `blocked_robots` |
| `status` | INTEGER | HTTP status when loaded |
| `error` | TEXT | Error class or failure text |
| `robots_rule` | TEXT | The rule that blocked it |
| `bytes` | INTEGER | Body size |

Index: `render_resources_url ON render_resources(url_id)`. At most 500 rows per page; the totals in `renders` count all.

## `console_entries` (FR-043)

| Column | Type | Meaning |
|---|---|---|
| `url_id` | INTEGER | |
| `pos` | INTEGER | Order |
| `level` | TEXT | `error`, `warning`, `exception` |
| `message` | TEXT | Up to 2,000 characters |
| `source_url`, `line`, `col` | TEXT, INTEGER, INTEGER | Source location when known |

Index: `console_entries_url ON console_entries(url_id)`. At most 50 rows per page.

## `rendered_bodies` (FR-032)

Same columns as `bodies` (`url_id` PRIMARY KEY, `codec`, `size`, `body`). Written only when "Store page HTML" is on. Read through `CrawlStore::rendered_html(url_id)`; the bytes are exactly the serialised DOM.

## `screenshots` (FR-033)

| Column | Type | Meaning |
|---|---|---|
| `url_id` | INTEGER PRIMARY KEY | |
| `format` | TEXT | `webp` |
| `width`, `height` | INTEGER | Pixels |
| `image` | BLOB | |

## Engine types (in memory, not stored)

- `RenderProgress` (in `Progress.render`): `rendered`, `queued`, `rendering`, `pages_per_sec`, `avg_pages_per_sec`, `limit`, `limiting: Option<Limiting>` (`concurrency`, `memory_guard`, `slow_pages`, `site_limits`, `queue_full`).
- `RenderSummary` (in `CrawlSummary.render`): `by_state`, `by_reason`, `avg_render_ms`, `max_render_ms`, `js_errors`, `failed_resources`, `blocked_resources`, `renderer`.

## Write path per rendered page (inside the existing 200 ms group commit)

1. `renders` update to the final state.
2. One `rendered_pages` and one `render_comparisons` insert; a few `rendered_text`, `rendered_directives`, `rendered_structured_data` inserts.
3. `rendered_links` inserts for rendered-only references (through `discover()`), `raw_only_links` inserts.
4. Up to 500 `render_resources` and 50 `console_entries` inserts.
5. `rendered_bodies` and `screenshots` when on; `render_inputs` delete.
