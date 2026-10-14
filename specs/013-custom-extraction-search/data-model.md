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
  rule_id INTEGER NOT NULL,
  url_id  INTEGER NOT NULL,
  count   INTEGER NOT NULL,           -- true number of matches
  flags   INTEGER NOT NULL,           -- bit set below
  first   TEXT,                       -- first value (extractors)
  PRIMARY KEY (rule_id, url_id)
) WITHOUT ROWID;
CREATE INDEX custom_results_url ON custom_results(url_id, rule_id);

CREATE TABLE custom_values (
  rule_id INTEGER NOT NULL,
  url_id  INTEGER NOT NULL,
  seq     INTEGER NOT NULL,           -- document order
  value   TEXT NOT NULL,              -- value, or match with context for searches
  PRIMARY KEY (rule_id, url_id, seq)
) WITHOUT ROWID;
```

`flags` bits: 1 `meets` (search condition true), 2 `no_match`,
4 `more_than_stored`, 8 `truncated` (a value was cut at 32,000 chars),
16 `timed_out`, 32 `error`, 64 `element_not_found`, 128 `not_available`
(no stored HTML), 256 `bad_json` (a JSON-LD block could not be parsed).

An error message, when `error` is set, is stored as `custom_values` row
`seq = -1`.

Rows exist only for eligible pages (spec 005 extraction set). A file
without these tables (older format) reads as "no rules run".

`meta.data_features` gains `custom` when the tables exist, so the results
crate can explain unavailability like other features.

## In-memory (`spiderlens-engine::custom`)

```text
CompiledRules { extractors: Vec<CExtractor>, searches: Vec<CSearch>, needs_xpath: bool, needs_text: bool }
RuleOutcome   { rule: u16, count: u32, flags: u16, values: Vec<String> }
PageData.custom: Option<Vec<RuleOutcome>>   (None when no rules)
```
