# Data Model: Near Duplicate Content

One additive migration step (spec 006 rule: number taken at merge as main's latest plus one; nullable columns, new tables only). Older readers can still view files with this step.

## Written during the crawl

### `pages`: new columns

| Column | Type | Meaning |
|---|---|---|
| `main_words` | INTEGER | Words in the main content (005 word rules) |
| `main_hash` | INTEGER | xxh3-64 of the main text, normalised as in 005 FR-009; equal values = exact content duplicates (FR-014) |
| `main_source` | TEXT | `main`, `article`, `fallback`, `selector`, `selector_missing` (the user's content selector did not match; fallback rules used, FR-003) |

### `content_sig`

| Column | Type | Meaning |
|---|---|---|
| `url_id` | INTEGER PRIMARY KEY | = `urls.id` |
| `sig` | BLOB NOT NULL | 960 bytes, one byte per bin: low 8 bits of each one-permutation MinHash bin after densification (plan D2) |

`meta.data_features` gains `near_dup` (`Need::ContentSig`).

## Written by the analysis pass `near_duplicates`

Replaced in one transaction per run. In read-only files the same tables exist in the reader's `temp` schema.

### `near_dup_pages` (one row per internal HTML 200 page)

| Column | Type | Meaning |
|---|---|---|
| `url_id` | INTEGER PRIMARY KEY | |
| `status` | TEXT | `compared`, `too_short`, `not_indexable`, `no_signature` |
| `closest_id` | INTEGER | Most similar other page, when its similarity ≥ the floor |
| `closest_sim` | INTEGER | 0 to 100 |
| `near_count` | INTEGER | Other pages at or above the threshold |
| `near_count_is_min` | INTEGER | 1 when counted from a sample (group over 50,000) |
| `cluster` | INTEGER | Cluster number, NULL when not in a cluster |

Index: `near_dup_pages_cluster ON near_dup_pages(cluster) WHERE cluster IS NOT NULL`.

### `near_dup_pairs` (top 50 per page, both directions)

| Column | Type | Meaning |
|---|---|---|
| `url_id` | INTEGER | |
| `other_id` | INTEGER | |
| `sim` | INTEGER | 0 to 100 |
| `exact` | INTEGER | Same `main_hash` |

Primary key `(url_id, other_id)`, `WITHOUT ROWID`. Pairs at or above the floor are stored, so the inspector can show "close but under the threshold" pages too.

### `near_dup_clusters`

| Column | Type | Meaning |
|---|---|---|
| `id` | INTEGER PRIMARY KEY | Number: by size descending, then smallest member id |
| `size` | INTEGER | |
| `min_sim`, `max_sim` | INTEGER | Range over the pairs found inside the cluster |
| `representative_id` | INTEGER | Most inlinks, then shortest URL |
| `exact` | INTEGER | All members share one `main_hash` |

### `meta.analysis` entry

`{"near_duplicates": {"version": 1, "data_version": N, "ms": T, "settings": {"threshold": 90, "report_floor": 80, "min_words": 50, "indexable_only": true, "content_selector": null, "exclude_selectors": []}}}`. A settings mismatch with the active crawl settings marks the pass stale.

## Configuration

`CrawlConfig.content` (profile and stored config, spec 002):

| Field | Type | Default | Range |
|---|---|---|---|
| `threshold` | u8 | 90 | 50 to 100 |
| `report_floor` | u8 | 80 | 50 to `threshold` |
| `min_words` | u32 | 50 | 1 to 10,000 |
| `indexable_only` | bool | true | |
| `content_selector` | Option<String> | none | a valid CSS selector |
| `exclude_selectors` | Vec<String> | empty | each a valid CSS selector, at most 50 |

## Size

Per compared page: 960 bytes of signature plus about 20 bytes of `pages` columns plus about 30 bytes in `near_dup_pages`, within SC-009 before pairs. Pairs: at most 50 per page, about 20 bytes each.
