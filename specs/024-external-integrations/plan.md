# Implementation Plan: External Data Integrations

**Branch**: `024-external-integrations` | **Date**: 2026-10-14 | **Spec**: [spec.md](./spec.md)

## Summary

Spec 024 joins what the crawler found with what Google and link-data
providers know about the same URLs. Everything happens after the crawl, from
the URLs stored in the crawl file, with the user's own credentials.

The plan puts all of it in one new library crate and keeps the crawl engine
untouched:

1. **`crates/integrations`** (new, `spiderlens-integrations`): credentials in
   the OS keychain, Google sign-in (loopback and PKCE), one HTTP client with
   per-provider limits, the response cache, resumable runs stored in the
   crawl file, URL matching, and one module per provider.
2. **Engine**: one additive migration step (tables only, D8). No change to
   fetching, the frontier or the writer.
3. **Results** (`crates/results`): new `Need` variants, columns and tabs read
   from the new tables, so the app and `spiderlens export` agree.
4. **App** (`app/src-tauri/src/integrations.rs`, new): commands, run
   progress events, opening the browser.
5. **UI**: Settings › Integrations, an Integrations area in the sidebar with a
   run panel per provider, provider tabs and columns, a Search Console section
   in the inspector, "Export to Google Sheets". **CLI**: `spiderlens
   integrations …`.

Crawl speed cannot change: the engine does not depend on the new crate, no
integration request is allowed while a crawl is fetching (D7), and the
CI crawl benchmark (spec 008, PR #55) runs unchanged (SC-001).

## Technical Context

- Builds on 006 (PR #27): `crawl/file.rs` (writer lock, atomic writes),
  `crawl/migrate.rs` (additive steps), `events`. Code starts from main after
  #27 merges; the migration step takes "latest on main + 1" at merge time.
- Retry policy: reuses spec 004's backoff rules (three attempts, exponential
  backoff with jitter, `Retry-After` honoured). If 004 (#29) has not merged
  when this code starts, the crate carries a 40-line copy with a task to
  switch to the shared type once it lands.
- New dependencies, all justified in D2, D3 and D6:
  - `keyring` 3 (features `apple-native`, `windows-native`,
    `sync-secret-service`, `crypto-rust`) in `integrations` only.
  - `sha2` and `base64` for PKCE (small, already transitive through rustls
    and reqwest).
  - `dirs` 6 in `config`, so the CLI finds the same app folders as Tauri.
- Already in the workspace and reused: `reqwest`, `tokio`, `rusqlite`,
  `zstd`, `xxhash-rust`, `serde_json`, `secrecy`, `jiff`, `axum` (fake
  providers in tests), `ts-rs`.
- Rust 2024; Tauri 2; React, Vitest, Playwright with the fake transport.

## Constitution Check

| Principle | How this plan complies |
|---|---|
| I. Spec before code | Spec 024 approved 2026-10-13 (keychain decision the same day); plan and tasks are this separate docs PR. |
| II. Local-first | No SpiderLens server. Google sign-in uses the loopback flow; Looker Studio is a link opened in the user's browser; keys are the user's own. With no integration enabled the crate is never called (FR-001). |
| III. Engine independent of shell | All logic is in `crates/integrations`; the app and the CLI are thin. The CLI shares the app's keychain entries, cache and usage ledger (D2, D6). |
| IV. Respectful crawling | Provider limits are enforced client-side below the published limits, and `Retry-After` and quota resets are honoured (D5). |
| V. Correct and complete data | Every requested URL ends with a value or a recorded reason (`integration_status`, D8). Data carries provider, account label, property, parameters, fetch time and period. |
| VI. Scales by design | Search Console and Analytics rows are streamed page by page into SQLite and matched through an indexed key table; nothing holds a provider's full answer in memory (D9). |
| VII. Test-first | Fake provider servers in `crates/test-fixtures` (D12); no test calls a live provider. Every acceptance scenario is mapped in tasks.md. |
| VIII. Simplicity | One crate, one migration step, one cache file, three small dependencies. Per-provider tables instead of a generic metric store, because each tab is a plain SQL query (D8). |

No violations. One recorded exception to an earlier spec: integration
credentials persist in the OS keychain, while crawl secrets (002 FR-038,
006 FR-034) are still re-entered. The owner chose this on 2026-10-13.

## Design decisions

### D1. A separate crate, never linked into the engine

`spiderlens-integrations` depends on `spiderlens-config` (URL
normalisation, app folders) and `spiderlens-engine` (opening a crawl file
for writing through 006's lock and migration). The engine does not depend
on it. `spiderlens-results` does not depend on it either: it only knows the
table names. The app and the CLI depend on all three.

```
config ──► engine ──► integrations ──► app, cli
   └──────► results ─────────────────► app, cli
```

Analogy for an architect from .NET: a separate assembly referenced only by
the hosts, so the crawler's hot path cannot accidentally call it.

### D2. Credentials: the OS keychain behind a small trait

```rust
pub trait CredentialStore: Send + Sync {
    fn get(&self, id: &CredentialId) -> Result<Option<SecretString>>;
    fn set(&self, id: &CredentialId, secret: &SecretString) -> Result<()>;
    fn delete(&self, id: &CredentialId) -> Result<()>;
    fn kind(&self) -> StoreKind; // Keychain | MemoryOnly
}
```

- `KeyringStore` uses the `keyring` crate: macOS Keychain, Windows
  Credential Manager, Linux Secret Service. Service name
  `com.ryware.spiderlens` (the Tauri identifier); account
  `<provider>/<label>`, for example `google/ryware@example.com` or
  `ahrefs/Agency key`.
- `MemoryStore` is used when the keychain answers "no storage access" or
  "platform failure" (FR-014). The UI shows "kept until you quit".
- Values are `secrecy::SecretString` everywhere (redacted `Debug`, wiped on
  drop), like 002's secret headers. A Google credential is a JSON blob
  `{client_id, client_secret?, refresh_token, scopes}`; a key credential is
  the key, or `access_id:secret` for Moz.
- The non-secret list of connections (labels, provider, scopes, created) is
  in `integrations/connections.json` in the app config folder, so the UI can
  list connections without touching the keychain (and without the macOS
  keychain prompt on every launch).
- **Forget** deletes the keychain entry and the connection row; for Google
  it first calls Google's revoke endpoint, and still deletes locally if
  revocation fails (offline), saying so.
- Tests use `MemoryStore`. One CI job on Linux runs the keyring tests under
  `dbus-run-session` with `gnome-keyring-daemon --unlock`; macOS and Windows
  CI run them against the real stores with a unique test service name and
  clean up.

### D3. Google sign-in: system browser, loopback, PKCE

- The crate opens a `tokio` listener on `127.0.0.1:0`, builds the
  authorization URL (`code_challenge_method=S256`, random `state`,
  `access_type=offline`, `prompt=consent` on first connect), and asks the
  host to open it: the app through `tauri-plugin-opener`, the CLI through
  `open`/`xdg-open`/`start`, printing the URL when that fails (US8.2).
- The listener accepts one request, checks `state`, answers with a small
  "You can close this tab" page, exchanges the code and closes. Timeout 5
  minutes; Cancel in the UI drops it.
- Scopes per feature (FR-015): `webmasters.readonly`, `analytics.readonly`,
  `drive.file`. Google does not support incremental authorization for
  installed apps, so adding a feature re-runs consent with the union of
  the stored and new scopes and replaces the stored refresh token.
- Access tokens live in memory only and are refreshed on 401 or expiry.
- **Client**: the built-in client ID and secret are compiled in from
  `SPIDERLENS_GOOGLE_CLIENT_ID`/`_SECRET` at build time (empty by default,
  so builds without them show "use your own client"). Settings ›
  Integrations › Google › Advanced accepts the user's own client, stored
  in the keychain (FR-016). Google treats desktop client secrets as not
  confidential, so compiling one in is allowed.

### D4. One HTTP client per run, separate from the crawler's

A `reqwest::Client` built by the crate (rustls, HTTP/2, gzip; connect 10 s,
read 30 s, PageSpeed 120 s), with the SpiderLens user agent. Base URLs come
from an `Endpoints` struct so tests point every provider at the fake
servers. No shared pool or semaphore with the engine.

### D5. Limits: token bucket, concurrency, daily ledger

```rust
pub struct Limits {
    pub per_window: u32, pub window: Duration,   // token bucket
    pub concurrency: u32,                        // semaphore
    pub daily: Option<Daily>,                    // allowance + reset time zone
}
```

| Provider | Default rate | Concurrency | Daily | Published (research.md) |
|---|---|---|---|---|
| Search Console analytics | 600/min | 4 | none | 1,200/min per site and user |
| URL Inspection | 300/min | 4 | 2,000 per site, reset midnight Pacific | 600/min, 2,000/day per site |
| GA4 Data | 60/min | 4 | tokens from `propertyQuota` | 10 concurrent, token buckets |
| CrUX | 140/min | 8 | none | 150/min per project |
| PageSpeed | 200 per 100 s | 6 | 25,000, reset midnight Pacific | 400 per 100 s, 25,000/day |
| Ahrefs | 50/min | 2 | user unit cap | 60/min |
| Majestic | 30/min | 2 | user unit cap | per plan |
| Moz | 30/min | 2 | user row cap | per plan |
| Sheets | 50/min | 1 | none | 60/min per user |

The user can lower any value, or raise it up to the published limit
(FR-004). The usage ledger (`integrations/usage.json`, per provider,
credential and property, per provider day) is shared by the app and CLI and
is what "N left today" shows. Providers that report usage (GA4
`propertyQuota`, link-metric unit headers or subscription endpoints) update
it from the answer; the rest count requests. When the provider answers 429
with a quota reason, or the ledger reaches the allowance, the run goes to
`paused_quota` with `resume_at` = the provider's reset (FR-006).

### D6. Cache: one SQLite file outside crawl files

`integrations-cache.sqlite` in the app cache folder:

```sql
CREATE TABLE cache (key BLOB PRIMARY KEY, provider TEXT NOT NULL,
  expires_at INTEGER NOT NULL, body BLOB NOT NULL);  -- zstd JSON
CREATE INDEX cache_expiry ON cache(expires_at);
```

The key is xxh3-128 of a canonical string (provider, credential id,
property, parameters, URL or page number). Expiry per FR-008; for CrUX the
next 04:00 UTC update. Expired rows are deleted on open; Settings has
"Clear cache" with its size. The cache never holds credentials (an id is a
label, not a token). A second identical fetch inside the period sends zero
requests (SC-005).

### D7. Runs are resumable jobs inside the crawl file

An integration run opens the crawl file through 006's writer lock (the same
lock a crawl or resume takes), so a run and a crawl can never write at once,
and the app disables "Fetch" while the crawl is running (FR-002). The
profile option "Run after crawl" queues chosen runs when the crawl
finishes; off by default.

State machine: `queued → running → finished | paused_user | paused_quota |
cancelled | failed`. URL-based providers keep a queue table (D8); a
resumed run takes only `pending` rows. Page-based providers (Search
Console, GA4) store their next page token or offset. Results are written in
batched transactions every 200 ms, like 008's crawl writer, so a kill loses
at most the last batch and never leaves half a row.

### D8. Storage: one additive migration step

All tables are new and keyed by `url_id` where they hold per-URL data, so a
results column is one primary-key lookup per visible row (as spec 007 D3).

- `integration_runs`, `integration_queue`, `integration_status`
- `gsc_pages`, `gsc_inspection`, `ga4_pages`, `cwv_field`, `psi_lab`,
  `link_metrics`, `orphan_candidates`

Full columns in [data-model.md](./data-model.md). `meta.data_features` gains
`search_console`, `url_inspection`, `analytics`, `field_data`, `lab_tests`,
`link_metrics` when a run of that provider finishes, which is how
`results::Features` reports "fetched / not fetched". Removing a provider's
data (FR-011) deletes its rows, its runs and its feature id in one
transaction, then compacts like 019 does when the user asks.

### D9. URL matching

Before writing rows, the run builds a temporary table `match_key(key TEXT
PRIMARY KEY, url_id INTEGER, ambiguous INTEGER)` from internal crawled
URLs, keyed by `config::url_norm` output without fragment. Secondary keys
(scheme swapped, trailing slash toggled) are added only where they map to
exactly one URL; collisions are marked ambiguous and never matched (spec
edge case). GA4 landing pages are joined to the stream's default URI host
first. Each provider page is matched with indexed lookups in SQLite, so
memory stays flat (SC-002). Unmatched rows go to `orphan_candidates`.

### D10. Providers

| Module | Calls | Notes |
|---|---|---|
| `gsc::analytics` | `sites.list`, `searchAnalytics.query` (`dimensions=[page]`, `aggregationType=byPage`, `rowLimit=25000`, `startRow` paging) | Property pick: `sc-domain:` covering the host, else the longest URL-prefix. Top queries on demand: `dimensions=[query]`, page filter, cached. |
| `gsc::inspect` | `urlInspection.index:inspect` | Queue default: indexable HTML ordered by clicks, then depth. Canonical mismatch compares with the crawl's canonical through D9 keys. |
| `ga4` | Admin `accountSummaries.list`, `dataStreams.list`; Data `runReport` (`landingPagePlusQueryString` or `landingPage`; sessions, engagedSessions, engagementRate, keyEvents, totalRevenue; `limit` 100,000 with `offset`; `returnPropertyQuota`) | Query-string variants combined by default. |
| `crux` | `records:queryRecord` per URL and form factor | 404 on a URL falls back to the origin, cached per origin, stored with `level = origin`. |
| `psi` | `runPagespeed` with all four categories and the chosen strategy | Stores the score, lab metrics, top five opportunities, category scores and the accessibility, best-practices and SEO audits (no screenshots), zstd JSON, for spec 026. |
| `links::ahrefs` | Batch Analysis, up to 100 targets | Estimate from the documented minimum per request and the field count; re-estimated after the first batch from the units reported, and stopped when the cap would be passed. |
| `links::majestic` | `GetIndexItemInfo`, batched | Units = items. |
| `links::moz` | `url_metrics`, batched | Rows = targets. |
| `sheets` | `spreadsheets.create`, `values.append` in 5,000-row batches | Capacity check before sending (FR-030); split option creates one worksheet per 1,000,000 rows. |
| `looker` | none | Builds `…/reporting/create?ds.ds0.connector=googleSheets&ds.ds0.spreadsheetId=…&ds.ds0.worksheetId=…` and asks the host to open it. |

Batch sizes and field names are confirmed against each provider's
reference when its module is written, and kept in one constant per module.

### D11. UI

- **Settings › Integrations** (sheet section): one row per provider with
  on/off, status, account or key label, "what is sent" text, limits (edit
  within published bounds), today's usage, Forget, terms link; Google
  advanced: own client ID. Cache size and Clear.
- **Integrations area** (sidebar, under Issues): a card per enabled
  provider with "Fetch" → a confirmation sheet (FR-003: provider, account,
  property picker, date range, URL set, request or unit estimate, share of
  today's allowance), then live progress with pause, resume, cancel and the
  quota pause message with local reset time.
- **Tabs and columns**: tabs Search Console, Index status, Analytics, Page
  experience, Link metrics, defined in `results::tabs` like every other
  tab; their columns can also be added to Internal (hidden by default).
  Orphan candidates is a list in the Search Console and Analytics tabs'
  side panel with "Crawl these as a list".
- **Inspector**: a Search Console section with the page's numbers and its
  top queries on demand.
- **Export**: "Export to Google Sheets" next to CSV export, then "Open in
  Looker Studio".
- Labels: every integration value shows its provider and period in the
  column header tooltip and in the inspector (US7.6).
- All colours from `tokens.css`; Vitest and Playwright tests with the fake
  transport for every screen (UI tests are mandatory).

### D12. Fake providers for tests

`crates/test-fixtures/src/providers/` (axum): fake Google OAuth (authorize
redirect, token, revoke), Search Console, URL Inspection, GA4 Admin and
Data, CrUX, PageSpeed, Ahrefs, Majestic, Moz and Sheets. Each fake has
configurable rows, a request log, rate and daily limits with an injectable
clock, and error injection (429 with `Retry-After`, 5xx, 401 expired
token). The Playwright suite uses the same scenarios through
`fakeTransport.ts`.

### D13. CLI

`spiderlens integrations` with `list`, `connect google|<provider>`,
`forget`, `properties`, `fetch FILE --provider …`, `queries FILE URL`,
`export-sheets FILE --tab …`, `cache clear`, `remove FILE --provider …`.
See [contracts/cli.md](./contracts/cli.md). Exit code for "paused for
quota" is the next free code in the CLI contract at merge time (proposed:
10); other failures use the existing codes.

### D14. Delivery in slices

The code ships as consecutive draft PRs, each green on all three
platforms and usable on its own:

1. Foundation and Search Console (US1, US7, US8 for Search Console):
   crate, keychain, sign-in, limits, cache, runs, migration step,
   matching, fakes, Settings › Integrations, Search Console tab, inspector
   section, CLI.
2. URL Inspection (US2).
3. Field data and lab tests (US3).
4. Analytics (US4).
5. Link metrics (US5).
6. Sheets and Looker Studio (US6).

## Performance

- Crawl: unchanged by construction (D1, D7); the 008 CI benchmark is the
  guard for SC-001.
- Search Console on 100,000 pages: 4 to 5 pages of 25,000 rows, each
  matched by indexed lookups in one transaction; target under 2 minutes
  against the fake with realistic payloads (SC-002), memory sampled in the
  test.
- CrUX 1,000 URLs on two devices: 2,000 calls at 140/min, about 14
  minutes, under SC-003's 15. The run panel shows the estimate.
- Results columns: one primary-key probe per visible row per column, as
  existing extraction columns.

## Project structure

```
crates/integrations/
  Cargo.toml
  src/lib.rs            run API used by app and CLI
  src/credentials.rs    D2
  src/oauth.rs          D3
  src/http.rs           D4
  src/limits.rs         D5, usage ledger
  src/cache.rs          D6
  src/run.rs            D7 state machine, batching
  src/store.rs          D8 writes, removal
  src/matching.rs       D9
  src/gsc/{analytics,inspect}.rs
  src/ga4.rs  src/crux.rs  src/psi.rs
  src/links/{ahrefs,majestic,moz}.rs
  src/sheets.rs  src/looker.rs
  tests/
crates/engine/src/crawl/migrate.rs   + one step
crates/results/src/{tabs,features,detail}.rs
crates/config/src/dirs.rs            app folders shared with Tauri
crates/test-fixtures/src/providers/  D12
crates/cli/src/integrations_cmd.rs
app/src-tauri/src/integrations.rs
ui/src/integrations/                 settings, area, run panel, sheets
```

## Risks

| Risk | Mitigation |
|---|---|
| Google verification takes time or asks for changes | Own-client path works from day one; the built-in client is a build-time setting. |
| Linux machines without a Secret Service | Memory-only mode with a clear notice (FR-014); tested in CI. |
| macOS keychain prompts on unsigned builds | Connections list read from `connections.json`; the keychain is read only when a run starts. |
| Provider APIs change field names or limits | One module and one limits row per provider; fakes mirror the documented shapes; limits are data. |
| Migration step number race with other specs | Take latest + 1 at merge, rebase and bump if the race is lost (006 rule). |
| Link-metric costs underestimated | Re-estimate after the first batch; hard stop at the user's cap (SC-006). |
