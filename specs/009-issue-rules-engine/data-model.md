# Data Model: Issue Rules Engine

New `meta` keys, temporary tables in the reader connection, a new section of the crawl profile, and one additive migration step for readability.

## Migration step (readability, FR-031)

`ALTER`-free additive step in `migrate.rs` (number taken at merge): `pages.sentence_count INTEGER` and `pages.syllable_count INTEGER`, both nullable, NULL for pages in another language and for files made before the step. `data_features` gains `readability`.

## Catalogue (Rust, static)

```rust
pub enum Priority { High, Medium, Low, Opportunity }          // order = sort order
pub enum RuleKind { PerUrl, SiteWide }                         // SiteWide = provisional while running
pub enum Scope { InternalUrls, ExternalChecked, AllUrls, Pages, InternalHtml,
                 InternalHttpsPages, ImagesOnPages, ImagesChecked, PagesWithHreflang,
                 PagesWithPagination }                         // each maps to one SQL eligibility clause
pub struct Threshold { key: &'static str, unit: Unit /* Chars, Ms, Bytes, Words, Hops, Links, Depth */,
                       default: f64, min: f64, max: f64 }
pub struct Rule {
    id: &'static str,             // "title.missing"; stable, never reused
    name: &'static str,           // "Page title: missing"
    category: Category,           // ResponseCodes, Links, PageTitles, ... (16 in the first set)
    priority: Priority,           // default
    scope: Scope,
    kind: RuleKind,
    cond: &'static str,           // SQL boolean over `urls u`; thresholds via thr('<key>')
    threshold: Option<Threshold>,
    need: &'static [Need],        // Base, Extraction, Timing, RawHtml, Attempts (from 007)
    derived: &'static [Derived],  // temp tables it reads
    columns: &'static [&'static str], // 007 column ids shown with the issue
    evidence: &'static str,       // SQL expression giving the evidence text for one row
    why: &'static str,
    fix: &'static str,
}
pub const RULES_VERSION: u32 = 1;
pub static CATALOGUE: &[&[Rule]] = &[rules::response::RULES, rules::links::RULES, /* ... */];
```

## Issue settings (crawl profile, `crates/config`)

```rust
#[serde(default)]
pub struct IssueSettings { pub rules: BTreeMap<String, RuleOverride> }   // only overrides
pub struct RuleOverride { pub enabled: Option<bool>, pub priority: Option<Priority>, pub threshold: Option<f64> }
```

Profile JSON: `"issues": { "rules": { "title.over": { "threshold": 70 }, "canonical.missing": { "enabled": false } } }`. Absent means defaults; old profiles load unchanged.

## Crawl file meta keys

| Key | Value | Written |
|---|---|---|
| `rules_version` | integer, the `RULES_VERSION` of the last evaluation | at run end; when settings change on a writable file |
| `issue_settings` | JSON of `IssueSettings` used | same |

## Temporary tables (reader connection)

| Table | Columns | Built |
|---|---|---|
| `temp.rule_list` | `idx INTEGER PRIMARY KEY, id TEXT, scope INTEGER` | per settings change (enabled rules only) |
| `temp.issue_hits` | `rule INTEGER, url_id INTEGER, PRIMARY KEY (rule, url_id)` WITHOUT ROWID; index on `url_id` | per data version, incremental for PerUrl rules (plan D5) |
| `temp.hreflang_pairs` | `from_id, to_id, lang` | per data version, when a hreflang rule is enabled |
| `temp.broken_targets` | `id` of URLs with 4xx, 5xx or an error class | per data version, for link and image rules |

007 derived tables reused: `occ_title`, `occ_md`, `occ_h1`, `occ_h2`, `redirects`.

## Values returned to the UI and CLI (ts-rs exported)

- `RuleInfo { id, name, category, priority, default_priority, enabled, kind, threshold: Option<{ key, unit, value, default, min, max }>, why, fix, columns }`
- `IssuesSummary { version, rules_version, final: bool, by_priority: [{ priority, total }], issues: [{ id, priority, count: Option<u64>, provisional: bool, unavailable: Option<String> }] }`
- `UrlIssue { id, name, priority, evidence: String }`
- `IssueSettingsResult { notices: Vec<String> }` for unknown ids or rejected values.
