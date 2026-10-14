# Data Model: Crawl Comparison

Nothing in this feature changes the crawl file schema. All tables below
live in the **workspace**, a temporary SQLite file attached as `cmp`.

## Workspace tables

### `cmp.info`
`key TEXT PRIMARY KEY, value TEXT`: both files' paths, start URLs, dates,
app and format versions, completeness, settings JSON, filter bit layout,
phase reached.

### `cmp.side_rows`
One row per URL per crawl.

| Column | Meaning |
|---|---|
| `side` | 0 previous, 1 current |
| `id` | the URL's id in that crawl file |
| `url` | stored URL |
| `key` | match key: mapped URL (previous) or URL (current) |
| `internal`, `html_ok` | flags from 007's `INTERNAL` and `HTML_OK` |
| `reach` | 0 final state, 1 queued, 2 skipped by a limit |
| `fb0`, `fb1`, … | filter bits (plan D3) |
| `title`, `md`, `h1`, `h1s`, `h2`, `words`, `status`, `state`, `index`, `canonical`, `directives`, `redirect`, `hreflang`, `sd_types`, `sd_hash`, `text_hash`, `depth`, `inlinks`, `outlinks`, `rt` | tracked element values (plan D5), NULL when the file lacks the data |

Primary key `(side, id)`; index `(side, key)`.

### `cmp.matches`
`prev_id INTEGER NULL, cur_id INTEGER NULL, key TEXT, grp INTEGER`:
`grp` is matched, new, missing, not_reached, not_reached_incomplete.
Index on `cur_id` and on `prev_id`.

### `cmp.issue_hits` (with 009)
`side, rule TEXT, id`: the rule's stable identifier and the URL id.

### `cmp.view_<n>`
Ordered positions of an open group list (`pos, prev_id, cur_id`), as 007
views.

## Rust types (spiderlens-compare, exported with ts-rs)

- `CompareSettings { mapping: Vec<MapRule>, elements: Vec<ElementSetting>, issue_settings: IssueSettingsChoice }`
- `MapRule { find: String, replace: String, regex: bool }`
- `ElementSetting { id: String, on: bool, threshold: Option<f64> }`
- `IssueSettingsChoice = Current | Previous | App`
- `CrawlInfo { path, start_url, started_at, urls, completeness, app_version, format_version, rendering }`
- `ConfigDiff { field, label, previous, current }`
- `Summary { previous: CrawlInfo, current: CrawlInfo, by_state: Vec<StateDelta>, new, missing, not_reached, matched, collisions, config_diffs, low_overlap: bool }`
- `FilterDelta { tab, filter, previous: Option<u64>, current: Option<u64>, new, added, removed, missing, comparable: bool, reason? }`
- `ElementDelta { element, changed: u64, comparable: bool, reason? }`
- `IssueDelta { rule, priority, previous, current, newly_affected, fixed, gone, comparable, reason? }`
- `Group = Filter { tab, filter, kind: New|Added|Removed|Missing } | Element { id } | Issue { rule, kind: NewlyAffected|Fixed|Gone } | Migration { kind } | Url { kind: New|Missing|NotReached|Collision }`
- `ComparisonReport` (plan D12)
- `Progress { phase, done, total }`
