# Data Model: External Data Integrations

## Crawl file: one additive migration step (plan D8)

The step number is "latest on main + 1" when this code merges. All tables
are new; no existing table changes.

```sql
CREATE TABLE integration_runs (
  id INTEGER PRIMARY KEY,
  provider TEXT NOT NULL,          -- search_console | url_inspection | analytics | field_data | lab_tests | ahrefs | majestic | moz
  account_label TEXT,              -- e.g. Google account email or key label; never a secret
  property TEXT,                   -- sc-domain:example.com, GA4 property/stream, or NULL
  params TEXT NOT NULL,            -- JSON: date range, search type, devices, metrics, URL set rule
  state TEXT NOT NULL,             -- queued | running | finished | paused_user | paused_quota | cancelled | failed
  created_at INTEGER NOT NULL,
  started_at INTEGER,
  finished_at INTEGER,
  resume_at INTEGER,               -- set when paused_quota
  cursor TEXT,                     -- page-based providers: next startRow / offset
  requested INTEGER NOT NULL DEFAULT 0,
  done INTEGER NOT NULL DEFAULT 0,
  failed INTEGER NOT NULL DEFAULT 0,
  matched INTEGER,                 -- page-based: rows matched to crawled URLs
  unmatched INTEGER,
  usage INTEGER,                   -- requests, units or rows consumed
  usage_cap INTEGER,               -- link metrics cap
  message TEXT,
  app_version TEXT NOT NULL
);

CREATE TABLE integration_queue (   -- URL-based providers only
  run_id INTEGER NOT NULL,
  url_id INTEGER NOT NULL,
  pos INTEGER NOT NULL,
  state TEXT NOT NULL,             -- pending | done | failed
  PRIMARY KEY (run_id, url_id)
);
CREATE INDEX integration_queue_pending ON integration_queue(run_id, state, pos);

CREATE TABLE integration_status (  -- why a URL has no value (FR-007)
  url_id INTEGER NOT NULL,
  provider TEXT NOT NULL,
  run_id INTEGER NOT NULL,
  reason TEXT NOT NULL,            -- no_data | provider_error | quota | not_requested
  error_class TEXT,
  attempts INTEGER,
  detail TEXT,
  PRIMARY KEY (url_id, provider)
);

CREATE TABLE gsc_pages (
  url_id INTEGER PRIMARY KEY,
  run_id INTEGER NOT NULL,
  clicks INTEGER NOT NULL,
  impressions INTEGER NOT NULL,
  ctr REAL NOT NULL,
  position REAL NOT NULL
);

CREATE TABLE gsc_queries (         -- on demand from the inspector
  url_id INTEGER NOT NULL,
  run_id INTEGER NOT NULL,
  query TEXT NOT NULL,
  clicks INTEGER NOT NULL,
  impressions INTEGER NOT NULL,
  ctr REAL NOT NULL,
  position REAL NOT NULL,
  PRIMARY KEY (url_id, query)
);

CREATE TABLE gsc_inspection (
  url_id INTEGER PRIMARY KEY,
  run_id INTEGER NOT NULL,
  verdict TEXT,
  coverage_state TEXT,
  indexing_state TEXT,
  robots_state TEXT,
  page_fetch_state TEXT,
  google_canonical TEXT,
  user_canonical TEXT,
  canonical_mismatch INTEGER,      -- Google canonical differs from the crawled canonical
  last_crawl INTEGER,
  crawled_as TEXT,
  inspected_at INTEGER NOT NULL,
  raw BLOB                         -- zstd JSON of the full result
);

CREATE TABLE ga4_pages (
  url_id INTEGER PRIMARY KEY,
  run_id INTEGER NOT NULL,
  sessions INTEGER NOT NULL,
  engaged_sessions INTEGER NOT NULL,
  engagement_rate REAL,
  key_events REAL,
  revenue REAL
);

CREATE TABLE cwv_field (
  url_id INTEGER NOT NULL,
  form_factor TEXT NOT NULL,       -- phone | desktop
  run_id INTEGER NOT NULL,
  level TEXT NOT NULL,             -- url | origin | none
  lcp_ms INTEGER, inp_ms INTEGER, cls REAL, fcp_ms INTEGER, ttfb_ms INTEGER,
  lcp_rating TEXT, inp_rating TEXT, cls_rating TEXT, fcp_rating TEXT, ttfb_rating TEXT,
  period_start TEXT, period_end TEXT,
  PRIMARY KEY (url_id, form_factor)
);

CREATE TABLE psi_lab (
  url_id INTEGER NOT NULL,
  strategy TEXT NOT NULL,          -- mobile | desktop
  run_id INTEGER NOT NULL,
  performance INTEGER, accessibility INTEGER, best_practices INTEGER, seo INTEGER,
  lcp_ms INTEGER, cls REAL, tbt_ms INTEGER, speed_index_ms INTEGER,
  opportunities TEXT,              -- JSON, top five: id, title, savings_ms
  audits BLOB,                     -- zstd JSON: accessibility, best-practices, seo audits (spec 026)
  tested_at INTEGER NOT NULL,
  PRIMARY KEY (url_id, strategy)
);

CREATE TABLE link_metrics (
  url_id INTEGER NOT NULL,
  provider TEXT NOT NULL,          -- ahrefs | majestic | moz
  run_id INTEGER NOT NULL,
  metrics TEXT NOT NULL,           -- JSON object, metric id → number
  fetched_at INTEGER NOT NULL,
  PRIMARY KEY (url_id, provider)
);

CREATE TABLE orphan_candidates (
  id INTEGER PRIMARY KEY,
  url TEXT NOT NULL,
  source TEXT NOT NULL,            -- search_console | analytics
  run_id INTEGER NOT NULL,
  clicks INTEGER, impressions INTEGER, sessions INTEGER,
  UNIQUE (url, source)
);
```

`meta.data_features` gains `search_console`, `url_inspection`,
`analytics`, `field_data`, `lab_tests`, `link_metrics`. An `events` row of
kind `integration_run` (detail: provider, run id, state, counts) is
written when a run ends, so the crawl information window lists it.

Spec 011 reads `orphan_candidates` as an extra orphan source; spec 026
reads `psi_lab.audits`.

## App-level files (outside crawl files)

| File (app folder) | Content | Secret? |
|---|---|---|
| config `integrations/connections.json` | `[{provider, label, scopes, kind: google\|key, created_at, store: keychain\|memory}]` | No |
| config `integrations/settings.json` | Per provider: enabled, limits overrides, cache periods, own-client flag | No |
| config `integrations/usage.json` | `{provider: {credential label: {property: {day, used, allowance}}}}` | No |
| cache `integrations-cache.sqlite` | Response cache (plan D6) | No |
| OS keychain, service `com.ryware.spiderlens` | Google refresh token and client, provider keys | Yes, only here |

## Types (Rust → UI through ts-rs)

| Type | Fields |
|---|---|
| `Provider` | enum of the eight providers plus `sheets` |
| `Connection` | provider, label, scopes, store kind, created_at |
| `ProviderStatus` | provider, enabled, connection?, limits, usage_today, allowance_today, terms_url, sends (text) |
| `RunRequest` | provider, connection label, property?, params, url_set rule, cap? |
| `RunEstimate` | requests or units, share_of_allowance, duration_estimate, days_needed |
| `RunProgress` | run id, state, done, failed, requested, usage, eta, resume_at?, message? |
| `PropertyChoice` | id, display, kind (domain, url_prefix, ga4_stream), suggested |
| `OrphanCandidate` | url, source, clicks?, impressions?, sessions? |
| `SheetExport` | spreadsheet url, worksheet ids, rows, truncated |
