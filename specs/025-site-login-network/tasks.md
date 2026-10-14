---
description: "Task list for spec 025, site login, cookies and proxy"
---

# Tasks: Site Login, Cookies and Proxy

**Input**: [plan.md](./plan.md), [spec.md](./spec.md), [research.md](./research.md), [data-model.md](./data-model.md), [contracts/](./contracts/), [quickstart.md](./quickstart.md)

**Tests**: required. Every acceptance scenario maps to an automated test against the local fixtures (constitution VII), and every task that touches `ui/` includes its Vitest tests in the same task; flows marked "e2e" also get a Playwright test.

**Format**: `- [ ] T### [P] [USn] Description`. `[P]` = can run in parallel with other `[P]` tasks of the same phase. `[USn]` = user story from spec.md.

**Depends on merged work**: PR #27 (storage, migrations, secrets on resume), PR #29 (errors and retries, manual retry), PR #51 (Set-Cookie scrub). Code starts on `main` after those land.

## Phase 1: Setup and spike

- [ ] T001 Spike (research R5): in `app/src-tauri/examples/login_spike.rs`, open an incognito webview on a local page that sets an HttpOnly and a normal cookie, read them with `Webview::cookies()`, close the window, and check the data directory is gone. Run on the three CI runners in a temporary workflow job; record results in research.md as "R11. Spike results". Blocks T040.
- [ ] T002 [P] Dependencies: add `cookie_store`, `digest_auth`, `arc-swap`, `md4`, `hmac` to `[workspace.dependencies]`; enable reqwest features `cookies` and `socks`. Confirm `cargo tree -d` adds no duplicate major versions. Record binary size change of `spiderlens` and `spiderlens-desktop` in research R9.
- [ ] T003 [P] Baseline benchmark: `specs/025-site-login-network/benchmarks.md`; on the merge base, the 100,000-page bench (`bench-site --pages 100000`, Fast) 7 times alternating, median and spread of time and URLs per second. Used by T060.
- [ ] T004 [P] Fixtures: `crates/test-fixtures/src/cookie_site.rs` (echo received cookies in the title, pages setting known cookies with every attribute, a redirect loop unless its cookie is returned), `auth_site.rs` (Basic and Digest with a request log; NTLM added in T070), `login_site.rs` (form login setting an HttpOnly session cookie, members pages, `/logout` that ends the session, `?expire_after=N`, `/account` showing "Sign in" when signed out), `proxy.rs` (HTTP CONNECT and SOCKS5, optional credentials, request counter); bins `auth-site`, `login-site`, `proxy` for quickstart.md.

**Checkpoint**: cookie API proven on all platforms, baselines recorded, fixtures ready.

## Phase 2: Foundational

- [ ] T005 `config::Secret` in `crates/config/src/secret.rs`: the password type (never serialised, `{"secret": true}` on disk, redacted `Debug`, zeroised); `HeaderValue::Secret` reuses it. Existing secret tests keep passing.
- [ ] T006 `AccessSettings` and its parts in `crates/config/src/model.rs` exactly as data-model.md, `#[serde(default)]`, ts-rs export; extend `all_defaults_match_spec` and `missing_fields_take_defaults` (a config without `access` loads with defaults). Commit regenerated `ui/src/generated/*.ts`. Depends on T005.
- [ ] T007 Validation rules from data-model.md in `validate.rs`, including `Context::Stored` for grandfathered `Cookie`/`Authorization` headers (plan owner point 2). Unit tests per code. Depends on T006.
- [ ] T008 [P] `crates/config/src/cookies_file.rs`: Netscape `cookies.txt` parser (tabs, `#HttpOnly_` prefix, comments, expired and malformed lines counted) and `Cookie:` line parser. Unit tests with real exports from Chrome and Firefox extensions (fixtures in `crates/config/tests/data/`).
- [ ] T009 `missing_secrets()` keys and labels from data-model.md, profile export writes placeholders for every new secret; tests that an exported profile and a stored config contain no secret value. Depends on T006.
- [ ] T010 `net::NetSession` skeleton (contracts/net-session.md): `new` builds today's client when access settings are default (test: same builder options), `client()`, `proxy_in_effect`; `fetch_once`, `get_following`, `check_page`, robots and sitemap fetching take `&NetSession`. No behaviour change; all existing tests pass. Depends on T006.

**Checkpoint**: settings exist and validate; every request already goes through `NetSession`.

## Phase 3: User Story 3, cookies and policy (P2, foundation for US2)

- [ ] T011 [US3] `net/jar.rs` `CrawlJar` (plan D2): internal-host gate, policy Ignore/Keep, login names exception, bounds, implements `reqwest::cookie::CookieStore`; built only when needed. Unit tests for matching, Secure over http, expiry deletion, external hosts.
- [ ] T012 [US3] Engine tests on `cookie_site`: acceptance scenarios 1–4 and 6 (supplied cookie reaches internal hosts only, Ignore vs Keep, redirect loop message suggests Keep, Secure cookie not sent over http). Depends on T011, T004.
- [ ] T013 [P] [US3] Redirect-loop message (FR-018): `ErrorClass::RedirectLoop` message gains the suggestion when cookies were set along the loop and the policy is Ignore.
- [ ] T014 [US3] CLI flags `--cookie`, `--cookie-plain`, `--cookies-file`, `--cookie-policy` with environment secrets and prompts (contracts/cli.md); CLI tests including scenario 5 (import counts). Depends on T008, T011.
- [ ] T015 [US3] UI `ui/src/settings/CookiesEditor.tsx` in a new "Sign-in and network" category (`categories.tsx`, id `access`): rows, paste a `Cookie:` line, Import file (`cookies_import` command), policy control. Vitest with `fakeTransport`; e2e: import a file and see the counts. Tauri command `cookies_import` in `app/src-tauri/src/crawl.rs`.

## Phase 4: User Story 5, cookie audit (P2)

- [ ] T016 [US5] Migration step for the `cookies` table (data-model.md) in `crates/engine/src/crawl/migrate.rs`; number = latest on `main` + 1 at merge time. Test: a file from the previous format upgrades and gains the table.
- [ ] T017 [US5] `finish_page` parses `Set-Cookie` into `CookieObs` (party by registrable domain) and `store.record_cookies` writes them in the page's batch; only when the header is present. Engine test on `cookie_site`: every expected cookie and attribute stored, no value anywhere (byte search of the file). Depends on T016.
- [ ] T018 [P] [US5] `crates/results/src/cookies.rs`: `distinct`, `urls_for`, filters, `Need::Cookies`; detail section in `detail.rs`; CSV in `export.rs`. Unit tests on a synthetic crawl (`synth.rs`).
- [ ] T019 [US5] CLI `spiderlens cookies` and `export --tab cookies`, `info` lines; CLI tests. Depends on T018.
- [ ] T020 [US5] UI `ui/src/results/CookiesView.tsx` (two panes, filter strip, sorting) with sidebar entry, and the Cookies section in `Inspector.tsx`; Tauri commands `cookies_list`, `cookie_urls`. Vitest; e2e: filter Missing HttpOnly, select a cookie, see its URLs. Depends on T018.

## Phase 5: User Story 1, HTTP sign-ins Basic and Digest (P1)

- [ ] T021 [US1] `net/auth.rs`: challenge parsing (several schemes in one or several headers), Basic, Digest with nonce reuse and `stale`, `authorize` and `on_unauthorized`; per-hop origin check in `get_following`. Unit tests with RFC 7616 examples.
- [ ] T022 [US1] Runner: 401 handling, `reason_detail`, `AuthRequired` and `AuthRejected` events once per origin, no retry of 401 (FR-005). Engine tests on `auth_site`: acceptance scenarios 1, 3, 4, 5 for Basic and Digest; request log proves Digest does not double requests. Depends on T021.
- [ ] T023 [US1] `Control::AddSignIn` and re-queue of the origin's 401 URLs through the 004 manual retry; test: crawl without sign-in, add it, the 401 URLs end 200. Depends on T022.
- [ ] T024 [US1] CLI `--auth`, `--auth-method`, `--allow-insecure-auth`, prompt and environment; `AuthRequired` line in CLI progress; CLI tests including scenario 6.
- [ ] T025 [US1] UI `SignInsEditor.tsx` (insecure confirm dialog) and `ui/src/crawl/AuthRequiredNotice.tsx` with the credential sheet calling `crawl_add_sign_in`. Vitest for both; e2e: add a sign-in on http and confirm the warning.

## Phase 6: User Story 4, proxy (P2)

- [ ] T026 [US4] `net/proxy.rs` manual mode (HTTP, HTTPS, SOCKS5 with `socks5h`, credentials, bypass) and `check_proxy`; proxy connect errors classified as `LocalNetwork` with the proxy named (FR-023). Engine tests on `proxy.rs` fixture: scenarios 1, 2, 3, 5, 6 (counts prove every request incl. robots, sitemaps and external checks went through).
- [ ] T027 [US4] System mode: environment variables (all platforms), Windows WinHTTP and PAC, macOS CFNetwork and PAC, Linux GNOME and KDE fixed proxies, `PacUnsupported` on Linux (owner point 1). Unit tests per platform behind `cfg`, environment-variable test on all CI runners; `proxy_in_effect` written to meta and shown by `info`.
- [ ] T028 [US4] CLI `--proxy`, `--proxy-system`, `--no-proxy-for`, `--proxy-test`; CLI tests.
- [ ] T029 [US4] UI `ProxyPanel.tsx` with Test (`proxy_test` command) and result line; crawl information sheet shows the proxy in effect. Vitest; e2e: Test against a closed port shows "proxy unreachable".

## Phase 7: User Story 2, login window and session guard (P1)

- [ ] T040 [US2] `app/src-tauri/src/login.rs`: incognito login window with the address strip and Done/Cancel in the app window, capture through `cookies()`, keep internal-host cookies only, delete the data directory, `LoginSession` held in app state; commands `login_open`, `login_done`, `login_cancel`, `login_clear`; no capability for the login webview. Rust tests for the filtering and summary; manual check in quickstart step 2. Depends on T001.
- [ ] T041 [US2] Engine: `LoginSession` in run options, inserted into the jar; sign-out guard rule in `scope.rs` with `NotCrawledReason::SessionGuard`; engine tests on `login_site` with a session built from the fixture's cookie (scenarios 1, 2, 4).
- [ ] T042 [US2] Signed-out check (`net/session_check.rs`, plan D5), `PauseReason::SessionEnded`, `store.requeue_fetched_since`, events `session_ended` and `signed_in_again`; engine test for SC-006 with `?expire_after=40` (pauses within 50 requests, after a new session the results equal an uninterrupted crawl).
- [ ] T043 [US2] CLI `--cookies-file` as a login session, `--sign-out-pattern`, `--no-default-sign-out-patterns`, `--signed-out-check`/`--when`; on session end without a terminal the run ends paused (exit code `PAUSED`) with the reason. CLI tests.
- [ ] T044 [US2] UI `LoginPanel.tsx` (Open login window, captured summary with names only, patterns, check) and `SessionEndedBanner.tsx`; first signed-in crawl warning (FR-014) remembered in app preferences. Vitest for all; e2e with the login commands faked.

## Phase 8: User Story 6, secrets, resume and sharing (P1)

- [ ] T050 [US6] Resume: the 006 secrets dialog (`ui/src/files/`) lists every new key with the right input (password field, Sign in button for `login:`), plus "Resume without them" (`skip_secrets`), recorded as `continued_without_sign_in`. Engine and UI tests (scenarios 2, 3).
- [ ] T051 [US6] CLI: environment variables for every key, prompt with a terminal, `SECRETS_MISSING` exit code without one (number agreed with specs 009 and 014 at merge), `--resume-without-secrets`. CLI tests (scenario 4).
- [ ] T052 [US6] Leak test `crates/engine/tests/secrets_leak.rs`: one crawl using every secret kind with distinctive values; byte-search the crawl file, an exported profile, the crawl log, every export format and the CLI output for each value (SC-002).

## Phase 9: NTLM (US1 continued)

- [ ] T070 [US1] NTLMv2 messages in `net/ntlm.rs` with the MS-NLMP 4.2.4 test vectors; NTLM server side in `auth_site.rs` using the same functions.
- [ ] T071 [US1] `NtlmPool` over hyper's connection API (TCP, rustls, proxy CONNECT), wired into `fetch_once` through `ntlm_for`; engine tests: scenario 2 for NTLM, handshakes counted per connection not per request, `DOMAIN\user` form.

## Phase 10: Spec 014 hand-off, performance and polish

- [ ] T058 [P] `browser_cookies`, `browser_proxy`, `record_script_cookies` with unit tests, and a note in `specs/014-javascript-rendering/plan.md` D4 pointing to contracts/net-session.md (014 thread owns its code changes; send through the coordinator).
- [ ] T059 [P] Crawl information sheet and `spiderlens info`: sign-ins (origin and user), login used, cookie policy, proxy in effect. Vitest.
- [ ] T060 Benchmark against T003: default settings within 2% (median of 7), supplied cookie plus Keep within 5%; results in benchmarks.md. Blocks the merge if out of bounds.
- [ ] T061 [P] Docs: `docs/site-login.md` (sign-ins, login window, cookies file export from browsers, proxy, test accounts warning), linked from README.
- [ ] T062 `/speckit-converge` until Converged; quickstart.md steps run on the desktop app.

## Dependencies and order

Phase 1 → Phase 2 → Phases 3–6 in any order (3 before 7, because the login session lives in the jar) → Phase 7 → Phase 8 → Phase 9 → Phase 10. Code PRs follow the phases: first PR Phases 2–4 (settings, `NetSession`, cookies, audit), then Phases 5–6, then 7–8, then 9–10, each a draft PR with its UI tests and green CI on all three platforms.
