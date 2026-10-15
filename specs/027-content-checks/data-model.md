# Data Model: Content Checks (spec 027)

All crawl-file changes are additive migration steps (spec 006): new tables or nullable columns, no `ALTER` by hand. Step numbers are taken at merge time (latest on main plus one). Each part's tables arrive with that part's PR only.

## Part A: SERP preview

No change to `urls` or `pages`: widths are computed by `px_width()` from `urls.title` and `urls.meta_description`.

```sql
-- Part A step
CREATE TABLE serp_proposals (
    url_id INTEGER PRIMARY KEY,
    title TEXT,            -- NULL = no proposal for the title
    description TEXT,      -- NULL = no proposal for the description
    updated_at INTEGER NOT NULL
);
```

Rust (`crates/results/src/serp`):

| Type | Fields |
|---|---|
| `SerpStyle` | `Title`, `Description` |
| `SerpDevice` | `Desktop`, `Mobile` |
| `SnippetMeasure` | `chars`, `px`, `truncated_desktop`, `truncated_mobile`, `display_desktop`, `display_mobile` |

Rule thresholds (009 settings): `serp.title_over_desktop` 600, `serp.title_over_mobile` 580, `serp.title_below` 200, `serp.desc_over_desktop` 990, `serp.desc_over_mobile` 680 (unit `Px`, new in the `Unit` enum).

## Part B: Segments

No crawl-file change. Segments are part of `CrawlConfig`, so they are stored with the crawl (`StoredCrawlConfig`) and in profiles.

```rust
pub struct Segment {
    pub id: u32,            // stable within the profile
    pub name: String,       // 1..=60 chars, unique case-insensitively
    pub color: SegmentColor, // one of the design-system segment tokens
    pub enabled: bool,
    pub mode: Match,        // All | Any
    pub clauses: Vec<Clause>, // 1..=10
}
pub struct Clause { pub field: Field, pub op: Op, pub value: String }
pub enum Field { Url, Host, Path, Query, StatusClass, ContentType, Indexability, Language, Depth, WordCount }
pub enum Op { Contains, NotContains, StartsWith, EndsWith, Equals, Matches, Lt, Le, Gt, Ge }
```

Validation: text ops only on text fields, numeric ops only on `Depth`, `WordCount`, `StatusClass`; `Matches` compiles with the `config::patterns` limits; at most 100 segments.

Reader connection (not stored):

```sql
CREATE TEMP TABLE url_segment (url_id INTEGER PRIMARY KEY, seg INTEGER NOT NULL);
```

URLs in no segment have no row ("No segment").

## Part C: PDF

```sql
-- Part C step
CREATE TABLE pdfs (
    url_id INTEGER PRIMARY KEY,
    status TEXT NOT NULL,      -- read | partly_read | over_size | failed
    status_reason TEXT,
    title TEXT, author TEXT, subject TEXT, keywords TEXT,
    creator TEXT, producer TEXT,
    created_at TEXT, modified_at TEXT,  -- as written in the PDF, ISO 8601 when parseable
    page_count INTEGER,
    word_count INTEGER,
    lang TEXT,
    encrypted INTEGER,
    no_text INTEGER,
    tagged INTEGER,
    link_count INTEGER
);
```

- PDF links: existing `links` table, `source = 'pdf'`, `pos = page number`, `text` = link text when present, `followed` per the setting.
- PDF text: existing `bodies` table (zstd), same retention flag as raw HTML; `pages.html_stored` semantics unchanged for HTML.

Config (`CrawlConfig`): `crawl_pdfs: bool` (false), `follow_pdf_links: bool` (true), `pdf_max_bytes: u64` (20 MiB, range 1 to 200 MiB), `pdf_time_limit_ms: u32` (5,000, range 500 to 60,000).

## Part D: Spelling and grammar

```sql
-- Part D step
CREATE TABLE spell_pages (
    url_id INTEGER PRIMARY KEY,
    lang TEXT,                 -- dictionary language used, NULL = not checked
    status TEXT NOT NULL,      -- checked | no_dictionary | no_text | lang_mismatch
    spelling INTEGER NOT NULL DEFAULT 0,
    grammar INTEGER NOT NULL DEFAULT 0,
    checked_at INTEGER NOT NULL,
    dict_version TEXT
);
CREATE TABLE spell_findings (
    url_id INTEGER NOT NULL,
    kind TEXT NOT NULL,        -- spelling | grammar
    word TEXT NOT NULL,
    source TEXT NOT NULL,      -- title | description | heading | body | pdf
    pos INTEGER NOT NULL,      -- char offset in that source
    context TEXT NOT NULL,     -- up to 120 chars around the word
    suggestions TEXT,          -- JSON array, up to 5
    block_hash INTEGER,        -- set when the text block repeats site-wide
    message TEXT               -- grammar message
);
CREATE INDEX spell_findings_url ON spell_findings(url_id);
CREATE INDEX spell_findings_word ON spell_findings(word);
```

Config: `check_spelling: bool` (false), `check_grammar: bool` (false), `default_language: String` ("en"), `ignore_words: Vec<String>`, `check_all_caps: bool` (false).

Outside the crawl file (app data folder `dictionaries/`): `<lang>.aff`, `<lang>.dic`, `installed.json` with `{lang, script, version, licence, source: shipped|downloaded|file, sha256}`.

## Part E: AMP

```sql
-- Part E step
CREATE TABLE amp_checks (
    url_id INTEGER NOT NULL,
    check_id TEXT NOT NULL,    -- marker | runtime | boilerplate | viewport | canonical | custom_script | style_size | disallowed
    passed INTEGER NOT NULL,
    pos INTEGER,               -- element position when failed
    detail TEXT
);
CREATE INDEX amp_checks_url ON amp_checks(url_id);
```

Config: `check_amp: bool` (false).

## Rules added to the 009 catalogue

| Part | Category | Rule ids |
|---|---|---|
| A | `Serp` | `serp.title_over_desktop`, `serp.title_over_mobile`, `serp.title_below`, `serp.desc_over_desktop`, `serp.desc_over_mobile` |
| C | `Pdf` | `pdf.missing_title`, `pdf.no_text`, `pdf.over_size`, `pdf.unreadable`, `pdf.linked_noindex` |
| D | `Spelling` | `spell.errors`, `spell.grammar`, `spell.not_checked`, `spell.lang_mismatch` |
| E | `Amp` | `amp.marker`, `amp.runtime`, `amp.boilerplate`, `amp.viewport`, `amp.canonical`, `amp.canonical_pair`, `amp.custom_script`, `amp.style_size`, `amp.disallowed` |
