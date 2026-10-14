# Data Model: Crawl Report and Exports

No change to the crawl file: no migration step, no meta key. Everything below is computed per request.

## Report (Rust, ts-rs exported)

```rust
pub struct Report {
    pub meta: ReportMeta,
    pub provisional: bool,            // crawl can still change (FR-006)
    pub incomplete: Option<Incomplete>, // FR-007
    pub score: Score,
    pub sections: Vec<Section>,
}
pub struct ReportMeta {
    pub start_url: String, pub started_at: Option<String>, pub ended_at: Option<String>,
    pub active_ms: u64, pub profile: Option<String>, pub app_version: String,
    pub rules_version: u32, pub score_version: u32, pub custom_issue_settings: bool,
    pub generated_at: String,
}
pub struct Incomplete { pub not_crawled: u64, pub reasons: Vec<(String, u64)> } // stopped, limit, failed, paused
pub struct Score {
    pub value: Option<u8>,            // None when no pages (FR-012)
    pub unavailable: Option<String>,
    pub partial: bool,                // some enabled rules could not run
    pub pages: u64,
    pub by_priority: Vec<PriorityLoss>, // points lost per priority; sums to 100 - value
    pub issues: Vec<IssueCost>,       // ordered by gain
    pub not_run: Vec<String>,         // rule ids unavailable on this file
}
pub struct PriorityLoss { pub priority: Priority, pub points: f64, pub pages: u64 }
pub struct IssueCost { pub id: String, pub name: String, pub priority: Priority, pub pages: u64, pub share: f64, pub gain: f64 }
pub struct Section { pub id: String, pub title: String, pub state: SectionState, pub blocks: Vec<Block> }
pub enum SectionState { Available, Provisional, Unavailable { reason: String } }
pub enum Block {
    Figures(Vec<Figure>),
    Table { columns: Vec<String>, rows: Vec<Vec<Cell>> },
    Bars(Vec<Bar>),
    Text(String),
}
pub struct Figure { pub label: String, pub value: String, pub target: Option<Target> }
pub struct Cell { pub text: String, pub target: Option<Target> }
pub struct Bar { pub label: String, pub value: u64, pub share: f64, pub target: Option<Target> }
pub enum Target { Tab { tab: String, filter: String, scope: UrlScope }, Issue { id: String } }
```

Score weights: High 1.0, Medium 0.5, Low 0.2, Opportunity 0. `SCORE_VERSION = 1`.

## Report file options

```rust
pub struct FileOptions { pub title: String, pub note: String, pub urls_per_issue: u32 } // 0..=10_000, default 100
```

Remembered by the app in its UI settings (not in the crawl profile, not in the crawl file).

## Bulk export

```rust
pub enum BulkFormat { CsvFolder, Xlsx }
pub struct BulkOptions { pub tables: Vec<String>, pub format: BulkFormat, pub out: PathBuf, pub overwrite: bool }
pub struct BulkProgress { pub table: String, pub tables_done: u32, pub tables_total: u32, pub rows: u64 }
pub struct BulkResult { pub files: Vec<PathBuf>, pub tables: Vec<(String, u64)>, pub truncated_cells: u64, pub split_sheets: u32 }
```

Table ids: `tabs`, `issues`, `links`, `images`, `redirect_chains`, `headers`, `summary`.

| Table | One row per | Columns |
|---|---|---|
| tab `<id>` | URL in the tab, filter All | every column of the tab |
| issue `<rule>` | affected URL | Address, the rule's columns, Evidence |
| links | stored link | Source, Target, Type, Anchor text, Alt text, Rel, Nofollow, Followed, Internal, Target status, Target state |
| images | image link on a page | Page, Image, Alt text, Alt missing, Alt empty, Status, Size |
| redirect_chains | chain start | Start, Hops, Hop count, Final URL, Final status, Final state, Loop |
| headers | header of a URL | Address, Header, Value |
| summary | report figure | Section, Label, Value |

CSV file names: `tab-<id>.csv`, `issue-<rule id>.csv`, `links.csv`, `images.csv`, `redirect-chains.csv`, `headers.csv`, `summary.csv`, plus `issues.csv` (the overview) when issues are exported. Excel: `Overview` sheet, then one sheet per table in the same order.
