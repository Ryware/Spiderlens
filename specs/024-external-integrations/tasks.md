# Tasks: External Data Integrations

**Input**: [spec.md](./spec.md), [plan.md](./plan.md), [data-model.md](./data-model.md), [contracts/](./contracts/), [quickstart.md](./quickstart.md)

Tests come first in each phase (constitution VII). `[P]` = can run in parallel with the other `[P]` tasks of its phase. Code starts from main after spec 006 (#27) merges. Phases map to the delivery slices of plan D14; each slice is its own draft PR.

## Slice 1: Foundation and Search Console

### Phase 1: Setup

- [ ] T001 Create `crates/integrations` (`spiderlens-integrations`), add to the workspace and default members; CI check that `spiderlens-engine` does not depend on it (`cargo tree -p spiderlens-engine -i spiderlens-integrations` fails).
- [ ] T002 [P] `config::dirs`: config, data and cache folders for `com.ryware.spiderlens` matching Tauri's; app-crate test asserts equality with `app.path()` values.
- [ ] T003 [P] Fake provider framework in `crates/test-fixtures/src/providers/`: axum router per provider, request log, injectable clock, rate and daily limits, error injection (429 + Retry-After, 5xx, 401) (plan D12).
- [ ] T004 [P] Fake Google OAuth: authorize (redirects to the loopback with code and state), token (code + PKCE verifier check, refresh), revoke.

### Phase 2: Foundation

- [ ] T005 Tests then code: `credentials` with `CredentialStore`, `KeyringStore`, `MemoryStore`, fallback detection, `connections.json` (atomic write); redacted `Debug` test (plan D2, FR-013, FR-014).
- [ ] T006 [P] CI: Linux job step running keyring tests under `dbus-run-session` + `gnome-keyring-daemon --unlock`; macOS and Windows run them against the real stores with a test service name and clean up.
- [ ] T007 Tests then code: `oauth` loopback flow with PKCE S256 and state check against the fake (success, wrong state, denied, timeout, cancel); refresh on 401; scope union re-consent (plan D3, FR-015).
- [ ] T008 [P] Tests then code: `limits` token bucket, semaphore, daily ledger in `usage.json` with Pacific-midnight reset (fixed clock tests), 429 handling with `Retry-After` and the 004 backoff rules (FR-004, FR-006, FR-007).
- [ ] T009 [P] Tests then code: `cache` SQLite with xxh3 keys, expiry per provider, purge on open, clear, size (FR-008, SC-005).
- [ ] T010 Migration step (number = latest on main + 1) with the tables of data-model.md; migration test from the v2 reference files and from the latest format; read-compat stays (additive).
- [ ] T011 Tests then code: `run` state machine, queue and cursor resume, 200 ms batched writes under 006's writer lock, refusal while a crawl holds the lock, kill test mid-run (no half rows) (plan D7, FR-005).
- [ ] T012 Tests then code: `matching` key table with secondary keys and ambiguity, GA4 host join, unmatched rows to `orphan_candidates`; 100,000-URL memory test (plan D9, FR-012).
- [ ] T013 Tests then code: `store::remove(provider|all)` in one transaction and `data_features` update (FR-011).
- [ ] T014 Byte-search test helper: given secrets, scan crawl file, profile export, cache, settings and captured logs (SC-004).

### Phase 3: US1 Search Console page data (P1)

- [ ] T015 Fake Search Console: `sites.list`, `searchAnalytics.query` with paging over 60,000 rows, byPage, dataState.
- [ ] T016 Tests then code: `gsc::analytics` property suggestion (domain property covers host; longest URL-prefix), fetch with paging, zero vs not fetched, matched and unmatched counts (US1.2 to US1.5).
- [ ] T017 Tests then code: top queries on demand with cache (US1.6).
- [ ] T018 `results`: `Need::SearchConsole`, Search Console tab (clicks, impressions, CTR, position) and filters "Indexable, no impressions", "Has clicks, not indexable"; columns available in Internal, hidden by default; export parity test (FR-010).
- [ ] T019 Performance test: 100,000 pages in under 2 minutes against the fake, memory flat (SC-002).

### Phase 4: US7 Opt-in, visible, safe (P1)

- [ ] T020 App `integrations.rs` commands of contracts/desktop-commands.md needed for slice 1; progress event; browser opening via `tauri-plugin-opener`.
- [ ] T021 [P] Vitest: Settings › Integrations rows (off by default, what is sent, status, store kind, limits within bounds, Forget, own client), memory-only notice (US7.1, US7.5, FR-019).
- [ ] T022 [P] Vitest: Integrations area card, confirmation sheet with property, date range and estimate (FR-003), run progress, pause, resume, cancel, quota pause message.
- [ ] T023 [P] Vitest: Search Console tab, inspector section with top queries, provider and period labels (US7.6), orphan candidates panel with "Crawl these as a list".
- [ ] T024 Playwright: connect (fake), fetch, see columns, Forget; no request before opt-in (US7.1 to US7.4).
- [ ] T025 Test: received crawl file with integration data shows it with labels; refresh asks for own connection (US7.4).
- [ ] T026 Test: Forget revokes at the fake and deletes the keychain entry; offline revoke still deletes locally with a notice (US7.3, FR-017).

### Phase 5: US8 CLI for slice 1 (P3)

- [ ] T027 `spiderlens integrations list|connect|forget|properties|fetch --provider search-console|resume|queries|remove|cache clear`; key from stdin only; quota exit code; CLI tests including parity with the app's rows (US8.1, US8.2, FR-032).

## Slice 2: US2 URL Inspection (P2)

- [ ] T028 Fake URL Inspection with daily allowance and clock.
- [ ] T029 Tests then code: `gsc::inspect` queue (default set and order), fields, canonical mismatch, quota pause and next-day resume, multi-day warning (US2.1 to US2.4).
- [ ] T030 `results` Index status tab and filters (Not indexed, Canonical mismatch, Blocked by robots per Google); Vitest + Playwright for the run panel with allowance.
- [ ] T031 CLI `--provider url-inspection`.

## Slice 3: US3 Field data and lab tests (P2)

- [ ] T032 Fakes for CrUX (404 for low-traffic URLs) and PageSpeed (slow responses, categories).
- [ ] T033 Tests then code: `crux` per URL and form factor, origin fallback cached per origin, ratings, period (US3.1, US3.2).
- [ ] T034 Tests then code: `psi` strategy choice, stored score, metrics, opportunities, audits for 026 (US3.3).
- [ ] T035 Rate log test: no run exceeds limits; 1,000 URLs × 2 devices estimate under 15 min (US3.4, SC-003).
- [ ] T036 `results` Page experience tab and filters (Poor LCP, Poor INP, Poor CLS, origin data only); Vitest + Playwright; CLI providers `field-data`, `lab-tests`.

## Slice 4: US4 Analytics (P2)

- [ ] T037 Fake GA4 Admin and Data with `propertyQuota`.
- [ ] T038 Tests then code: `ga4` property and stream pick, landing page fetch with offset paging, query-string combine option, quota pause (US4.1 to US4.3).
- [ ] T039 `results` Analytics tab; orphan candidates from Analytics; Vitest + Playwright; CLI provider `analytics`.

## Slice 5: US5 Link metrics (P3)

- [x] T040 Fakes for Ahrefs, Majestic and Moz with metered units.
- [x] T041 Tests then code: `links::{ahrefs,majestic,moz}` batching, estimate, re-estimate after the first batch, cap stop, cache reuse (US5.1 to US5.3, FR-029, SC-006).
- [x] T042 `results` Link metrics tab with per-provider columns; Vitest + Playwright for the cost confirmation; CLI with `--cap` and `--yes`.

## Slice 6: US6 Sheets and Looker Studio (P3)

- [x] T043 Fake Sheets API with cell capacity.
- [x] T044 Tests then code: `sheets` export of a tab view (visible columns, filter, sort), batches, capacity warning, truncate and split (US6.1, US6.2, FR-030).
- [x] T045 Tests then code: `looker` link (US6.3, FR-031).
- [x] T046 Vitest + Playwright: Export to Google Sheets, Open in Looker Studio; CLI `export-sheets --looker`.

## Final phase: Polish

- [ ] T047 Run the quickstart scenarios on all three platforms; SC-004 byte search over every slice's outputs.
- [ ] T048 Crawl benchmark (008) unchanged with every integration connected (SC-001).
- [ ] T049 Moderated first-run check of connect-to-columns time (SC-007), done by the owner on their machine when the built-in client exists.
- [ ] T050 `/speckit-converge` reports Converged.
