# Contract: `crates/aeo` (spec 015)

```rust
pub struct AnalyzeOptions { pub profile: ScoringProfile, pub custom_bots: Vec<AiBot>,
    pub page_type_overrides: Vec<PageTypeOverride>, pub workers: Option<usize>, pub cancel: CancelToken }
pub enum Progress { Hosts { done: u32, total: u32 }, Pages { done: u64, total: u64 }, Site }
pub struct Summary { pub score: u8, pub parts: Parts, pub distribution: [u64; 4], pub top_fixes: Vec<Fix>,
    pub hosts: Vec<HostSummary>, pub entity: Entity, pub versions: Versions, pub provisional_pages: u64 }

pub fn analyze(file: &Path, opts: &AnalyzeOptions, progress: &dyn Fn(Progress)) -> Result<Summary>;
pub fn analyze_open(conn: &mut Connection, opts: &AnalyzeOptions, progress: &dyn Fn(Progress)) -> Result<Summary>; // used by the crawl's finish step
pub fn summary(file: &Path) -> Result<Option<Summary>>;
pub fn page_result(file: &Path, url_id: i64) -> Result<Option<PageResult>>;
pub fn generate_llms_txt(file: &Path, opts: &LlmsTxtOptions) -> Result<String>;
pub async fn probe(file: &Path, opts: &ProbeOptions, progress: &(dyn Fn(ProbeProgress) + Sync)) -> Result<ProbeSummary>;
pub fn rules() -> &'static [AeoRule];
```

## `aeo_pages.flags` bits (filter ids in the AI search tab)

| Bit | Filter | Rule id |
|---|---|---|
| 0 | Blocked for AI search crawlers | `ai.access.search_blocked` |
| 1 | Blocked for AI training only | `ai.access.training_only_blocked` |
| 2 | Blocked for user-requested fetches | `ai.access.user_fetch_blocked` |
| 3 | Snippet restricted | `ai.access.snippet_restricted` |
| 4 | Main content missing without JavaScript | `ai.readable.js_content` |
| 5 | Possibly built with JavaScript | `ai.readable.js_suspected` |
| 6 | Question without a direct answer | `ai.answer.question_no_answer` |
| 7 | Long text without structure | `ai.answer.unstructured` |
| 8 | No facts with sources | `ai.answer.no_facts` |
| 9 | Structured data content not visible on the page | `ai.structured.not_visible` |
| 10 | No date | `ai.trust.no_date` |
| 11 | Dates disagree | `ai.trust.dates_disagree` |
| 12 | Not updated in N months | `ai.trust.stale` |
| 13 | No author | `ai.trust.no_author` |
| 14 | Structured data errors | `ai.structured.errors` |
| 15 | Structured data warnings | `ai.structured.warnings` |

"Score below 50" filters on `score`. Bits are append-only.

## Determinism

`analyze` sorts every list it writes, writes JSON with sorted keys, uses integer arithmetic for scores and never reads the clock except for `ran_at` and `duration_ms`, which SC-006 comparisons exclude.
