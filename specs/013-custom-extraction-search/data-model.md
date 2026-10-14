# Data Model: Custom Extraction and Custom Search

## Configuration (`spiderlens-config`)

```text
CrawlConfig.custom: CustomRules            (serde default: no rules)
  extractors: Vec<Extractor>               (max 100, names unique)
  searches:   Vec<Search>                  (max 100, names unique)

Extractor
  name: String
  kind: css | xpath | regex | json_ld
  expr: String                             (selector, XPath, pattern, path or JSONPath)
  mode: text | inner_html | outer_html | attribute | value
        (css/xpath: text default; attribute needs `attr`;
         value = XPath expression result; regex/json_ld: value)
  attr: Option<String>
  json_type: Option<String>                (json_ld item path: @type filter)

Search
  name: String
  term: String
  regex: bool
  condition: contains | not_contains
  target: html | text
  ignore_case: bool
  within: Option<ElementLimit { kind: css | xpath, expr }>
```

Rule file: `{ "format": "spiderlens-rules", "version": 1, "extractors": [...], "searches": [...] }`.

## Crawl file (one additive `migrate.rs` step)

```sql
CREATE TABLE custom_rules (
  id          INTEGER PRIMARY KEY,
  slot        INTEGER NOT NULL UNIQUE,
  kind        TEXT NOT NULL,          -- 'extractor' | 'search'
  name        TEXT NOT NULL,
  def_json    TEXT NOT NULL,          -- the rule as configured
  pos         INTEGER NOT NULL,       -- display order
  last_run_at TEXT,                   -- RFC 3339, NULL = never run
  run_pages   INTEGER,                -- pages processed in the last run
  run_state   TEXT,                   -- 'crawl' | 'complete' | 'partial' | 'stale'
  UNIQUE (kind, name)
);

CREATE TABLE custom_results (
  url_id  INTEGER PRIMARY KEY,        -- one row per eligible page
  slots   BLOB NOT NULL,              -- 8 bytes per rule slot: u32 count, u16 flags, u16 0 (little endian)
  firsts  TEXT NOT NULL               -- JSON array, by slot: first value or null
);

CREATE TABLE custom_values (
  url_id  INTEGER PRIMARY KEY,
  data    BLOB NOT NULL               -- zstd JSON: { slot: [values...] } for extractors with
                                      -- more than one value, { slot: [contexts...] } for searches,
                                      -- { slot: "error message" } for errors
);
```

`custom_rules.slot` (INTEGER NOT NULL, UNIQUE) is the rule's position in
`slots` and `firsts`. Slots are assigned when a rule is first stored and
never reused within a crawl; a deleted rule's slot is left empty. Rows
exist only for eligible pages (spec 005 extraction set); `custom_values`
only when a page has something beyond the first values.

Why packed rows (plan D7): the crawl's single SQLite writer is the
bottleneck (spec 005 benchmarks), and a 30-rule set as one row per page
and rule would add 30 rows per page, as many as the page's links. Packed,
it is one or two rows per page whatever the rule count. The results crate
reads slots with SQL functions `custom_count(slots, i)`,
`custom_flags(slots, i)` and `json_extract(firsts, '$[i]')`; views
materialise their order once (spec 007 plan D4), so these run once per
row per view, not per scroll.

`flags` bits: 1 `meets` (search condition true), 2 `no_match`,
4 `more_than_stored`, 8 `truncated` (a value was cut at 32,000 chars),
16 `timed_out`, 32 `error`, 64 `element_not_found`, 128 `not_available`
(no stored HTML), 256 `bad_json` (a JSON-LD block could not be parsed),
512 `run` (the slot was evaluated for this page; unset = not processed in
a partial run).

A file without these tables (older format) reads as "no rules run".

`meta.data_features` gains `custom` when the tables exist, so the results
crate can explain unavailability like other features.

## In-memory (`spiderlens-engine::custom`)

```text
CompiledRules { extractors: Vec<CExtractor>, searches: Vec<CSearch>, needs_xpath: bool, needs_text: bool }
RuleOutcome   { rule: u16, count: u32, flags: u16, values: Vec<String> }
PageData.custom: Option<Vec<RuleOutcome>>   (None when no rules)
```
