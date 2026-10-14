# Data Model: Link Score and Internal Link Analysis

## Tables (crawl file; stage 1 in the reader's `temp` schema, stage 2 in `main`)

### link_metrics (one row per internal URL)

| Column | Type | Meaning |
|--------|------|---------|
| url_id | INTEGER PK | `urls.id` |
| score | REAL | Link Score 0-100 (FR-004), unrounded |
| rank | INTEGER | 1 = highest; ties share a rank (competition ranking on the one-decimal value) |
| unique_inlinks | INTEGER | distinct internal source pages with an anchor to this URL (any rel) |
| followed_inlinks | INTEGER | distinct internal source pages with a followed anchor (graph in-degree) |
| all_inlinks | INTEGER | every anchor occurrence pointing here |
| unique_outlinks | INTEGER | distinct internal targets of this page's anchors |
| external_outlinks | INTEGER | distinct external targets of this page's anchors |
| pct_linking | REAL | unique_inlinks / internal HTML pages x 100 |

### link_positions (one row per body anchor or area link from an internal HTML page)

| Column | Type | Meaning |
|--------|------|---------|
| from_id | INTEGER | `links.from_id` |
| pos | INTEGER | `links.pos` (document order) |
| position | INTEGER | 0 Unknown, 1 Navigation, 2 Header, 3 Content, 4 Sidebar, 5 Footer |
| rule | INTEGER NULL | index of the custom rule that matched; NULL = built-in |

PRIMARY KEY (from_id, pos), WITHOUT ROWID.

### meta

Stage 2: the `analysis` key defined by spec 009 holds `{ "link_metrics": { version, data_version, ms }, "link_positions": {…} }`. Run details (FR-014) are stored as `link_analysis_run`: `{ ran_at, app_version, crawl_complete, urls, links, rounds, weighting: null | {weights}, rules: [...] }`. Stage 1 keeps the same object in memory for the session.

## Settings (profile and stored crawl config)

```rust
pub struct LinkAnalysisSettings {
    pub auto_analyse: bool,                  // true
    pub position_rules: Vec<PositionRule>,   // []
    pub weighting: Option<PositionWeights>,  // None
}
pub struct PositionRule { pub selector: String, pub position: Position }
pub enum Position { Navigation, Header, Content, Sidebar, Footer }
pub struct PositionWeights { pub navigation: f32, pub header: f32, pub content: f32, pub sidebar: f32, pub footer: f32, pub unknown: f32 }
// defaults 0.3, 0.3, 1.0, 0.5, 0.2, 1.0; each 0.0..=1.0
```

## Results types (TypeScript generated with ts-rs)

- `LinkMetrics { score, rank, of, unique_inlinks, followed_inlinks, all_inlinks, unique_outlinks, external_outlinks, pct_linking }` on `UrlDetail`.
- `LinkRow.position: Position | "unknown" | null` (null = not calculated).
- `AnalysisState = { kind: "running" } | { kind: "not_calculated" } | { kind: "stale", ran_at } | { kind: "ready", run }`.
- `AnalysisProgress { pass, label, done, total }` (event shape from spec 009).
