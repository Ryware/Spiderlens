# Data Model: AEO and GEO Checks (spec 015)

All tables arrive in one additive migration step (plan D14). Columns are nullable unless marked. Times are Unix milliseconds. JSON columns hold UTF-8 JSON with sorted keys, so output is byte-identical across platforms (SC-006).

## Settings

### `CrawlConfig.ai_checks` (`crates/config/src/model.rs`, stored with the crawl)

| Field | Type | Default | Spec |
|---|---|---|---|
| `auto` | bool | true | FR-002, FR-003 |
| `site_files` | bool | true | FR-010 |

### App settings (`crates/config/src/aeo_profile.rs`, `ai_bots.rs`)

- `ScoringProfile { name, weights: { access: 30, readable: 25, answer: 20, trust: 15, structured: 10 }, thresholds: { direct_answer_words: 60, long_paragraph_words: 150, long_text_words: 800, stale_months: 12, date_agree_days: 2, js_dependence_pct: 50, blocked_cap: 20, llms_txt_points: 2, probe_pages_per_host: 10 } }`. The default profile is built in and cannot be edited; users save copies (FR-040).
- `custom_ai_bots: Vec<AiBot>` (FR-007).
- `page_type_overrides: Vec<{ pattern, page_type }>` (FR-019), saved with the crawl when used.

## `AiBot` (`crates/config/src/ai_bots.json`)

| Field | Type | Meaning |
|---|---|---|
| `index` | u8 | Stable position inside a list version; bit in `aeo_pages.blocked_bots` |
| `vendor` | string | OpenAI, Anthropic, ... |
| `name` | string | GPTBot, ... |
| `token` | string | robots.txt token, lower-case match |
| `fallback_tokens` | list | e.g. Applebot → googlebot |
| `ua_pattern` | string | substring of the user agent string |
| `ua_example` | string | full published user agent string (used by the probe) |
| `purpose` | enum | `training`, `search`, `user_fetch`, `other` |
| `honours_robots` | enum | `yes`, `no`, `may_not_apply`, `unknown` |
| `runs_js` | enum | `yes`, `no`, `unknown` |
| `token_only` | bool | No crawler of its own (Google-Extended, Applebot-Extended) |
| `docs_url`, `ip_ranges_url` | string | |

List header: `version` (integer), `date`.

## `site_files` (written during the crawl)

| Column | Type | Meaning |
|---|---|---|
| `host_key` | TEXT NOT NULL | |
| `path` | TEXT NOT NULL | `/llms.txt`, `/llms-full.txt`, `/.well-known/tdmrep.json` |
| `status` | INTEGER | Final status |
| `final_url` | TEXT | After redirects |
| `content_type` | TEXT | |
| `size` | INTEGER | Decoded size before the cap |
| `body` | BLOB | zstd, first 1 MB |
| `outcome` | TEXT NOT NULL | `ok`, `not_found`, `error`, `too_large`, `skipped` |
| `error` | TEXT | |
| `fetched_at` | INTEGER | |

Primary key `(host_key, path)`.

## `aeo_hosts`

| Column | Type | Meaning |
|---|---|---|
| `host_key` | TEXT PRIMARY KEY | |
| `robots` | TEXT NOT NULL | `ok`, `missing`, `unavailable`, `ignored_not_fetched` |
| `signals` | TEXT | JSON list `{line_no, key, value, kind}`; kind `content_signal`, `content_usage`, `license`, `unknown` |
| `llms_txt` | TEXT | JSON: outcome, problems `{line, message}`, sections, links `{line, url, url_id, status, problem}` |
| `llms_full_txt` | TEXT | JSON: outcome, size, link problems |
| `tdmrep` | TEXT | JSON: file policies and header values found |

## `aeo_bot_access`

| Column | Type | Meaning |
|---|---|---|
| `host_key` | TEXT NOT NULL | |
| `bot` | TEXT NOT NULL | token |
| `home_verdict` | TEXT NOT NULL | `allowed`, `blocked`, `unavailable` |
| `matched_group` | TEXT | `User-agent` of the group used, or `*` |
| `urls_total` | INTEGER NOT NULL | Discovered internal URLs on the host |
| `urls_blocked` | INTEGER NOT NULL | |
| `rules` | TEXT | JSON list `{line_no, rule, urls}` sorted by `urls` desc |

Primary key `(host_key, bot)`.

## `aeo_pages`

| Column | Type | Meaning |
|---|---|---|
| `url_id` | INTEGER PRIMARY KEY | |
| `page_type` | TEXT NOT NULL | `article`, `listing`, `utility`, `other` |
| `page_type_reason` | TEXT | |
| `score` | INTEGER | 0 to 100; NULL for non-HTML resources (access only) |
| `access`, `readable`, `answer`, `trust`, `structured` | INTEGER | Part scores scaled to the part maximum |
| `checks_used`, `checks_total` | INTEGER NOT NULL | |
| `provisional` | INTEGER NOT NULL | |
| `blocked_bots` | INTEGER NOT NULL | Bitmask by `AiBot.index` |
| `flags` | INTEGER NOT NULL | Bitmask of failed checks with a tab filter (contracts/aeo-crate.md lists the bits) |
| `version_used` | TEXT NOT NULL | `raw`, `rendered`, `mixed` |
| `js_dependence` | REAL | From spec 014 when rendered |
| `published`, `modified` | TEXT | ISO dates chosen |
| `dates_agree` | INTEGER | |
| `author` | TEXT | |
| `author_url_id` | INTEGER | Crawled author page |
| `question_headings`, `direct_answers` | INTEGER | |
| `figures`, `quotations`, `citations` | INTEGER | |
| `lists`, `tables`, `longest_paragraph` | INTEGER | |
| `snippet_limits` | TEXT | JSON: `nosnippet`, `max_snippet`, `noarchive`, `nocache`, `data_nosnippet`, `data_snippet`, `noai`, `noimageai`, by crawler |

## `aeo_findings`

| Column | Type | Meaning |
|---|---|---|
| `url_id` | INTEGER | NULL for site findings |
| `rule_id` | TEXT NOT NULL | e.g. `ai.access.search_blocked` |
| `outcome` | TEXT NOT NULL | `fail`, `not_available` |
| `points_lost` | INTEGER NOT NULL | Hundredths |
| `evidence` | TEXT | JSON, rule-specific |

Index `(rule_id, url_id)`; index `(url_id)`.

## `aeo_site`

One row, `id = 1`: `score`, part scores, `distribution` (JSON bands 0-19, 20-49, 50-79, 80-100), `entity` (JSON), `pages_found` (JSON about, contact, authors), `coverage` (JSON), `top_fixes` (JSON `{rule_id, pages, gain}`), `profile` (JSON), `versions` (JSON: app, bots list, vocabulary, rules), `ran_at`, `pages_analyzed`, `duration_ms`.

## `aeo_probe`

| Column | Type | Meaning |
|---|---|---|
| `host_key`, `bot`, `url_id` | | Primary key |
| `status`, `size`, `content_type` | | Probe response |
| `outcome` | TEXT NOT NULL | `same`, `blocked`, `challenge`, `payment_required`, `different`, `error` |
| `detail` | TEXT | e.g. `crawler-price` header |
| `probed_at` | INTEGER | |

## Engine types (in memory)

- `PageFacts`: everything a page check reads: URL facts, page row, headings, directives, headers subset, structured items (parsed), DOM-derived `ContentFacts` (D7) when HTML is stored, `RenderFacts` when rendered, the host's robots policies.
- `Outcome { Pass, Fail(Evidence), NotApplicable, NotAvailable(&'static str) }`.
- `PageRow`: what the writer inserts for a page (one `aeo_pages` row plus its findings).
