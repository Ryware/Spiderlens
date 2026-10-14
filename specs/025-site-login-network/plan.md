# Implementation Plan: Site Login, Cookies and Proxy

**Branch**: `025-site-login-plan` | **Date**: 2026-10-14 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `specs/025-site-login-network/spec.md` (approved 2026-10-13 with the recommended defaults) and [research.md](./research.md).

## Summary

Let a crawl see a site as a signed-in visitor would, through HTTP sign-ins (Basic, Digest, NTLM), a login window that captures a site's session cookies, supplied cookies with a keep-or-ignore policy, and a proxy; and record which cookies each URL sets.

The approach:

- One new engine structure, **`net::NetSession`** (`crates/engine/src/net/`), owns everything about how a crawl reaches a site: the reqwest client, the cookie jar, the HTTP sign-ins and the proxy. The crawl runner, robots.txt and sitemap fetching, external link checks and the spec 014 render stage all get their network from it. This is the shared structure spec 014 asked for (its `crawl/render/net.rs` calls `NetSession`, and seeds Chrome from `NetSession::browser_cookies`).
- **The default path does not change.** With no sign-in, no cookie, policy "Ignore" and proxy "No proxy", `NetSession` builds exactly today's client (no cookie store, `no_proxy()`), and the per-request code takes the same branch as today. The only addition on the default path is parsing `Set-Cookie` headers for the audit, which runs only when a response has one (FR-036, SC-001).
- **Secrets live in one type.** `config::Secret` generalises today's `HeaderValue::Secret` (never serialised, redacted in `Debug`, wiped on drop). Every new secret uses it, and `missing_secrets()` lists them with stable keys, so the existing resume flow of spec 006 (`CrawlError::NeedsSecrets`, `ResumeOptions.secrets`) asks for them without a new mechanism.
- **The login window is a desktop concern.** The Tauri app opens an incognito webview, the user signs in, the app reads the cookies through Tauri's webview cookie API and passes them to the engine in memory as a `LoginSession`. The engine never knows a window existed; the CLI passes a cookies file instead.
- **The cookie audit is one table** written in the existing batched commit, one results module and one sidebar view.

Crawl speed is the first constraint. The gate is SC-001: the 100,000-page benchmark with default settings stays within 2% of the baseline, and with a supplied cookie plus "Keep" within 5%.

## Technical Context

**Language/Version**: Rust 2024 (toolchain from `rust-toolchain.toml`), TypeScript 5 with React 18 for the UI.

**Primary Dependencies**: existing: Tokio, reqwest 0.12, rusqlite 0.40, secrecy, Tauri 2.12 (wry 0.57). reqwest gains its `cookies` and `socks` features. New crates (justified in research R9): `cookie_store` (RFC 6265 jar), `cookie` (Set-Cookie parsing, already a transitive dependency), `digest_auth`, `md4` and `hmac` (NTLMv2), `hyper` client connection API (already a transitive dependency of reqwest) for NTLM connections. Platform proxy lookup through small `cfg` modules (research R6), no new crate on Linux.

**Storage**: the crawl file (SQLite). One additive migration step adds the `cookies` table (data-model.md). The step number is taken at merge time (latest on `main` + 1), per the Storage thread's rule.

**Testing**: `cargo test --workspace`; new fixtures in `crates/test-fixtures`: an auth server (Basic, Digest, NTLM), a login site, a cookie site and a counting proxy (HTTP CONNECT and SOCKS5); Vitest for every UI change; Playwright for the settings and Cookies view flows; the spec 008 benchmark for SC-001.

**Target Platform**: Windows 10+, macOS (Intel and Apple Silicon), mainstream Linux desktops; desktop app and CLI.

**Project Type**: desktop app (Tauri) plus CLI over shared Rust libraries.

**Performance Goals**: default settings within 2% of baseline throughput (SC-001); supplied cookie plus "Keep" within 5%; Digest adds no extra request per URL after the first challenge per host (nonce reuse, R3); NTLM adds one handshake per connection, not per request (R4).

**Constraints**: no secret on disk (SC-002, leak test over every artifact); credentials and cookies never sent to another host (SC-004).

## Constitution Check

*Gate before research, re-checked after design.*

| Principle | How this plan complies |
|---|---|
| I. Spec before code | Spec approved 2026-10-13 (Ryware). This plan and tasks go to the owner as a docs-only PR before code. Two spec points that need the owner's nod are listed below. |
| II. Local-first, no servers | Everything runs on the user's machine. The login window talks only to the site the user opens. No SpiderLens server. |
| III. Engine independent of the shell | `NetSession`, sign-ins, cookies, proxy and the audit live in `crates/engine` and `crates/config` with no Tauri dependency. **CLI parity**: every setting is a `spiderlens crawl` flag with the same default; the login window's CLI equivalent is `--cookies-file`; the Cookies view has `spiderlens cookies` (contracts/cli.md). |
| IV. Respectful crawling | Signing in changes nothing about robots.txt, crawl-delay or rate limits. Sign-out URLs are not requested while a login session is in use (FR-011). Digest and Basic do not double the request count (R3). |
| V. Correct and complete data | Every 401 records its challenge scheme; a crawl that resumes without its sign-in records that in its history (FR-031); the proxy in effect is stored with the crawl (FR-024). |
| VI. Scales by design | Cookie observations go to disk in the existing batch; the jar holds only cookies for internal hosts and is bounded (R2); the Cookies view is a SQL aggregate, never a full load. Performance impact below. |
| VII. Test-first | Every acceptance scenario maps to a fixture test (tasks.md). **UI tests are mandatory**: every task that touches `ui/` includes Vitest tests in the same task, and the settings and Cookies view flows get Playwright tests. A leak test searches every artifact for every secret kind (SC-002). |
| VIII. Simplicity | No new crate in the workspace: `net/` is a module of the engine. The secret type is generalised, not duplicated. Resume reuses the 006 secrets flow; "retry 401s after adding a sign-in" reuses the 004 manual retry. NTLM is the one costly piece and is isolated in `net/ntlm.rs` and its own task phase (research R4). |

**Speed first, explicitly**: on the default path `NetSession::new` returns the same reqwest client as `fetch::build_client_with` does today, and `fetch_once` gains one `Option` check (`session.auth_for(host)` returns `None` without a lock when there are no sign-ins: the sign-in list is an `ArcSwap` of an empty map). `Set-Cookie` parsing runs only when the header is present; on the bench site it is absent, so the benchmark measures the true default. Task T060 runs the benchmark (median of 7 alternating runs, as in spec 005) and blocks the merge if throughput drops by more than 2%.

Result: no violations.

## Design decisions

### D1. `net::NetSession`: one structure for how a crawl reaches sites

`crates/engine/src/net/mod.rs`, API in [contracts/net-session.md](./contracts/net-session.md):

```text
NetSession
├── client: reqwest::Client          built once per crawl (proxy, timeouts, cookie provider)
├── jar: Option<Arc<CrawlJar>>       None on the default path
├── sign_ins: ArcSwap<HashMap<Origin, SignIn>>   can grow mid-crawl (FR-004)
├── digest: DashMap<Origin, DigestState>         nonce + counter per origin
├── ntlm: Option<NtlmPool>                       only when an NTLM sign-in exists
├── proxy: ProxyInEffect                         for crawl info and Chrome
└── internal: Arc<dyn Fn(&Url) -> bool>          scope's internal-host test
```

- Built by `NetSession::new(&CrawlConfig, secrets, login: Option<LoginSession>, resolve)` in the runner where `build_client_with` is called today (`crates/engine/src/crawl/runner.rs:526`), and by `check::check_page` for the single-page command.
- `fetch_once` and `get_following` take `&NetSession` instead of `&Client`. Custom headers stay as they are (spec 002).
- Spec 014's render stage receives `Arc<NetSession>`: every resource request it answers goes through `session.client`, so proxy, sign-ins and cookies apply automatically; `session.browser_cookies(url)` gives the cookies to seed into Chrome before a page renders (CDP `Network.setCookies`), and `session.record_script_cookies(url, cookies)` takes the cookies Chrome reports after the render for the audit (FR-028, FR-035).

### D2. The cookie jar and the policy

`net/jar.rs`: `CrawlJar` wraps `cookie_store::CookieStore` behind a `parking_lot::Mutex` and implements `reqwest::cookie::CookieStore` (the trait reqwest's `cookie_provider` takes), so reqwest attaches and stores cookies itself on every request, including redirect hops we follow by hand.

- **Sending** (`cookies(url)`): returns nothing for a URL whose host is not internal (FR-016); otherwise the RFC 6265 match (domain, path, Secure only on https, expiry).
- **Receiving** (`set_cookies(headers, url)`): ignored for external hosts. For internal hosts: policy "Keep" stores every cookie; policy "Ignore" stores only cookies whose name is in the login session (session rotation, FR-013) or that replace a supplied cookie with the same name and domain.
- **Supplied cookies** and **login cookies** are inserted at session start. Expired cookies in an import are skipped (FR-015).
- **Bounds**: at most 180 cookies per domain and 4,096 bytes per cookie, as browsers do (spec edge cases); excess is dropped and counted in the crawl log.
- The jar exists only when the crawl has a supplied cookie, a login session, or policy "Keep". Otherwise `client` is built without a cookie provider, exactly as today.
- Nothing in the jar is ever written: it is not part of the crawl state that spec 006 stores for resume. On resume, supplied secret cookies and the login session are asked for again (FR-031).

### D3. HTTP sign-ins: Basic, Digest, NTLM (research R3, R4)

`net/auth.rs`. A `SignIn` is an origin (scheme, host, port), a user name, a `Secret` password and an optional method.

- **Basic**: sent pre-emptively on every request to the origin once the first 401 asked for Basic, or from the first request when the method is set to Basic. Never to another origin, including redirect targets (`get_following` already filters internal-only headers per hop; the sign-in check is per hop too).
- **Digest**: on a 401 with `WWW-Authenticate: Digest`, compute the response with `digest_auth` (MD5 and SHA-256, `qop=auth`), retry once, then keep the nonce and a nonce counter per origin and answer pre-emptively. A `stale=true` challenge refreshes the nonce without counting as a rejection.
- **NTLM**: see D4.
- **Retry rule**: a request that gets 401 after credentials were sent is recorded as 401 with reason detail `auth_rejected` (FR-005); the 004 retry policy does not retry 401. One rejection per origin raises the `AuthRejected` event once; later 401s for the same origin are recorded without new events.
- **No sign-in configured**: the 401's challenge schemes are parsed and stored with the URL (`reason_detail` `auth_required:<schemes>`), and the runner raises `CrawlEvent::AuthRequired { origin, schemes }` once per origin (FR-004). An unsupported scheme (Negotiate only) raises the same event with `supported: false`.
- **Adding a sign-in mid-crawl**: `Control::AddSignIn(SignIn)` inserts into `sign_ins` (an `ArcSwap` swap, no lock on the request path) and re-queues the origin's URLs that ended in 401 through the 004 manual retry (`Crawler::retry_failed`, PR #29) with a host filter. In the desktop app this is the notice's "Sign in" button; on the CLI it is `spiderlens retry FILE --host H` after a run with `--auth`.
- **Plain http warning** (FR-006) is a validation warning in `crates/config/src/validate.rs` that the UI turns into a confirm dialog and the CLI into a `--allow-insecure-auth` requirement.

### D4. NTLM over owned connections

NTLM authenticates a connection, not a request: the three messages must travel on one keep-alive connection, and reqwest gives no control over which pooled connection a request uses. So NTLM origins use their own small pool, `net/ntlm.rs`:

- `NtlmPool` keeps up to `max_per_host` (spec 008 allowance cap) HTTP/1.1 connections per NTLM origin, each opened with hyper's client connection API (`hyper::client::conn::http1::handshake`) over TCP or rustls, through the proxy when one is set (CONNECT tunnel).
- A new connection performs Negotiate (Type 1) → Challenge (Type 2) → Authenticate (Type 3, NTLMv2 with `md4` and `hmac`), then serves requests until it closes. The runner's per-host limiter already bounds how many connections can be in use.
- `fetch_once` asks `session.ntlm_for(origin)`; when it returns a pool, the request goes through it and the response is adapted to the same `Once` shape. Every other origin uses reqwest as before.
- NTLM is phase 7 in tasks.md, after Basic and Digest ship, because it is the piece with the most code. The fixture server implements the server side with the same NTLMv2 functions plus a test vector from the MS-NLMP specification (research R4).

### D5. Login window and session guard (desktop)

- **Window** (`app/src-tauri/src/login.rs`): `WebviewWindowBuilder` with `incognito(true)` and a per-window temporary data directory, opened at the login URL with the crawl's user agent. A toolbar strip above the page shows the current address and the buttons Done and Cancel (a second small webview label, so the site's page cannot reach the app's IPC: the login webview gets no capabilities in `capabilities/`).
- **Capture**: on Done the app calls `webview.cookies()` (Tauri 2.12, wry 0.57; includes HttpOnly cookies on all three platforms, research R5), keeps the cookies whose domain matches an internal host of the crawl (FR-008), discards the rest, closes the window and deletes its data directory. The result goes to the UI as names per site (FR-010); values stay in the Rust process in a `LoginSession` held by the app's crawl state, never sent to the webview.
- **Engine side**: `LoginSession { captured_at, cookies, sign_out_patterns, check }` is passed to `Crawler::run` in memory (new field on the run options); it is never part of `CrawlConfig` except its non-secret settings (`access.login`: login URL, patterns, check).
- **Sign-out guard** (FR-011): when a login session is active, `scope::decide` gets one more rule after the robots check: URLs matching a sign-out pattern (case-insensitive substring on path and query) end as `NotCrawledReason::SessionGuard`. Patterns are plain substrings, not regexes, to keep them obvious to edit.
- **Signed-out check** (FR-012): every 25 completed internal fetches and at least every 60 s, the runner fetches the check URL through the session (not counted as a crawled URL) and evaluates the condition (redirect to a URL containing X, status code, or body contains text). On failure it pauses the crawl with `PauseReason::SessionEnded`, records the event, and remembers the last passing time. On resume with a new session, URLs with `fetched_at` after that time are put back in the queue (`store.requeue_fetched_since`). 25 fetches is half the SC-006 budget of 50, which leaves room for requests already in flight.
- **Data-changing links warning** (FR-014): a one-time confirm before the first signed-in crawl, remembered in app preferences (`settings.rs`).

### D6. Proxy (research R6)

- `ProxySettings { mode: none | system | manual, kind: http | https | socks5, host, port, user, password: Secret, bypass: Vec<String> }`.
- **Manual**: `reqwest::Proxy::all(url)` with `basic_auth` for HTTP/HTTPS and credentials in the URL for SOCKS5 (`socks5h://`, so DNS goes through the proxy too), plus `NoProxy::from_string(bypass)`.
- **System**: resolved once when the crawl starts, then treated as manual: environment variables (`HTTPS_PROXY`, `HTTP_PROXY`, `ALL_PROXY`, `NO_PROXY`), then the OS setting: Windows `WinHttpGetIEProxyConfigForCurrentUser` and `WinHttpGetProxyForUrl` for automatic configuration scripts, macOS `CFNetworkCopySystemProxySettings` and `CFNetworkCopyProxiesForAutoConfigurationScript`, Linux GNOME (`gsettings get org.gnome.system.proxy`) and KDE (`kioslaverc`). A PAC script is evaluated per host on first use and cached for the crawl. On Linux a PAC script cannot be evaluated without a JavaScript engine: see owner point 1.
- **Check before the crawl** (FR-022): `NetSession::check_proxy` requests the start URL's `robots.txt` through the proxy with a 10 s timeout. Errors map to "proxy unreachable" (connect error to the proxy), "proxy rejected the sign-in" (407) and "automatic configuration script could not be read". The same function backs the Test button and the CLI `--proxy-test`.
- **Mid-crawl failure** (FR-023): connect errors to the proxy are classified as `ErrorClass::LocalNetwork` (spec 004), whose existing "everything fails" detector pauses the crawl; the pause message names the proxy.
- **Environment independence** (FR-024): `no_proxy()` stays unless the mode is system or manual, so results never depend on a shell's variables by accident.
- The proxy in effect (mode, kind, host, port, user, never the password) is written to crawl meta as `proxy_in_effect` for the crawl information sheet.

### D7. Cookie audit (research R7)

- In `finish_page` (`runner.rs`), the `Set-Cookie` headers already in `Once::headers` are parsed with `cookie::Cookie::parse` into `CookieObs { name, domain, path, expires, max_age, secure, http_only, same_site, size, party, source }`. `domain` defaults to the response host (host-only cookie); `party` is first when the cookie's domain is the URL's registrable domain (public suffix list already in `crates/config`), else third.
- Rows go to the new `cookies` table in the same batched transaction as the page (spec 008's 200 ms commits). No value column exists (FR-025).
- **Stored headers** keep `Set-Cookie` with the value replaced: that rule already ships in PR #51 (`crawl::redact`); this feature depends on it.
- **Results** (`crates/results/src/cookies.rs`): `distinct(filter, sort, page)` groups by (name, domain, path) with attributes from the most recent observation and `count(DISTINCT url_id)`; `urls_for(cookie, page)`; filters Session, Persistent, Third party, Missing Secure, Missing HttpOnly, SameSite None. `detail.rs` adds a Cookies section per URL. `export.rs` exports both lists as CSV without values (FR-029).
- **Script cookies** (FR-028): spec 014 calls `session.record_script_cookies` with the cookies Chrome holds after rendering minus the ones the response set; they are stored with `source = 'script'`.

### D8. Configuration, secrets and resume

- `CrawlConfig` gains `access: AccessSettings` (`#[serde(default)]`, so every existing crawl file and profile loads with the defaults): `sign_ins`, `login`, `cookies`, `cookie_policy`, `proxy`. Shapes in data-model.md; TypeScript types regenerated by ts-rs as today.
- `config::Secret` (new, `crates/config/src/secret.rs`) is the password and token type; `HeaderValue::Secret` and `SuppliedCookie.value` use it.
- `CrawlConfig::missing_secrets()` returns stable keys: `header:<name>` (exists), `sign_in:<origin>`, `proxy`, `cookie:<domain>/<name>`, plus `login:<url>` when `access.login` is set and no session was passed. The 006 resume dialog shows each with a label and the right input (password field, or a "Sign in" button that opens the login window); the CLI reads the documented environment variables (contracts/cli.md).
- **Resume without secrets** (FR-031): `ResumeOptions.skip_secrets: bool` lets the run start with the missing ones dropped; the runner records `continued_without_sign_in` in the events table with the keys.
- **Custom header rules** (FR-019): `Cookie` and `Authorization` are added to the forbidden custom header names in `validate.rs` for new configurations and profile imports. A stored crawl configuration that already has one keeps working on resume (validation of stored configs runs with `Context::Stored`, which reports a warning instead of an error), so no existing crawl file stops resuming.
- **Unattended runs** (FR-034): the CLI without a terminal never prompts; a missing secret ends the command with a new exit code `SECRETS_MISSING` (the next free number when this merges; 6 is taken by spec 006 and specs 009 and 014 also add codes, so the number is agreed with those threads) and a line naming each key and its environment variable. Scheduling (spec 021) uses the same rule; per the owner's 2026-10-13 decision, only integration tokens of spec 024 may come from the OS keychain, never site credentials.

### D9. Desktop UI

- **Settings**: a new category "Sign-in and network" (`ui/src/settings/categories.tsx`, id `access`) with four groups: Site sign-ins (`SignInsEditor.tsx`, rows of origin, user, password, method), Login page (`LoginPanel.tsx`: login URL, Open login window, captured summary, sign-out patterns, signed-out check), Cookies (`CookiesEditor.tsx`: rows, paste a `Cookie:` line, Import cookies file, policy segmented control) and Proxy (`ProxyPanel.tsx`: mode, fields, bypass list, Test with result line). Same `Group`/`Row` field components as the other categories; colours only from tokens.
- **Run notices**: `AuthRequiredNotice` (host, method, Sign in button opening a small credential sheet) and the `SessionEnded` pause banner (Sign in again, Resume without sign-in) in the crawl run view, next to the 004 host-pause notices.
- **Resume dialog**: the 006 secrets dialog lists the new keys (D8).
- **Cookies view**: a "Cookies" entry in the sidebar's results group; two panes (distinct cookies with filter strip; URLs setting the selected cookie), reusing `Grid` and `FilterStrip`. The URL inspector gains a Cookies section.
- **Crawl information**: sign-ins (origins and users), login used (yes/no, when), cookie policy, proxy in effect.
- **Tests**: Vitest for every component with `fakeTransport` (new handlers for the commands in contracts/desktop-commands.md); Playwright e2e for: add a sign-in and see validation; import a cookies file; proxy Test failure message; Cookies view filter and selection.

## Performance impact (constitution principle VI)

| Path | Added work | Expected effect |
|---|---|---|
| Default settings | one `Option`/empty-map check per request; `Set-Cookie` parse only when present | within noise; gated at 2% by T060 |
| Supplied cookie or Keep | jar lookup per request (mutex, small map), store on `Set-Cookie` | small; gated at 5% by T060 |
| Basic | one header per request | none measurable |
| Digest | one extra request per origin (first challenge), then a hash per request | none measurable |
| NTLM | 2 extra round trips per new connection | proportional to connections, not requests |
| Proxy | the proxy's own latency | user's choice |
| Cookie audit | one insert per cookie set, in the existing batch | proportional to cookies set |

## Spec points that need the owner's nod

1. **Automatic proxy configuration scripts on Linux** (FR-020): Windows and macOS evaluate a PAC script through the OS. Linux has no system service for it, and evaluating it ourselves means bundling a JavaScript engine (about 3 MB more in the binary, research R6). Proposed: on Linux, "Use system proxy settings" supports fixed proxies and environment variables; a PAC script gets the message "automatic proxy scripts are not supported on Linux, enter the proxy manually". Default if no answer: this proposal.
2. **Custom headers named `Cookie` or `Authorization` in existing files** (FR-019): rejected for new settings, but a crawl file or profile that already has one keeps working with a warning instead of failing to resume. Default if no answer: this proposal.

## Open risks

1. **Webview cookie API coverage**: the plan relies on Tauri's `Webview::cookies()` returning HttpOnly cookies on WebView2, WKWebView and WebKitGTK. The first task (T001) is a spike that proves it on all three CI runners; the fallback is platform calls (`ICoreWebView2CookieManager`, `WKHTTPCookieStore`, `webkit_cookie_manager_get_cookies`) in `login.rs`.
2. **NTLM servers in the wild** vary (NTLMv1 only, channel binding on HTTPS). v1 supports NTLMv2 without channel binding; a server that requires either fails with "sign-in rejected (NTLM variant not supported)". Documented.
3. **Sites that bind sessions to the browser** (IP, TLS fingerprint, user agent): the crawl sends the same user agent as the login window, but a site that ties the session to a TLS fingerprint will reject the crawler. The signed-out check makes this visible on the first check rather than silently crawling the login page.
4. **PR #51 and the migration step**: the audit's table and the Set-Cookie scrub are separate steps; whichever merges later takes the next number.

## Project Structure

### Documentation (this feature)

```text
specs/025-site-login-network/
├── spec.md
├── plan.md              # this file
├── research.md          # R1–R10
├── data-model.md        # settings, runtime types, cookies table
├── quickstart.md        # manual checks with the fixtures
├── contracts/
│   ├── net-session.md   # engine API, shared with spec 014
│   ├── cli.md           # flags, environment variables, commands
│   └── desktop-commands.md
├── checklists/requirements.md
└── tasks.md
```

### Source Code (repository root)

```text
crates/config/src/
├── secret.rs              # Secret (generalised from HeaderValue)
├── model.rs               # AccessSettings and its parts
├── validate.rs            # new rules, Context::Stored, forbidden headers
├── cookies_file.rs        # NEW: Netscape cookies.txt and Cookie: line parsing
└── stored.rs              # missing_secrets keys
crates/engine/src/
├── net/                   # NEW module
│   ├── mod.rs             # NetSession
│   ├── jar.rs             # CrawlJar, policy
│   ├── auth.rs            # Basic, Digest, challenges
│   ├── ntlm.rs            # NTLMv2 messages and NtlmPool
│   ├── proxy.rs           # manual, system (per-OS cfg modules), check
│   └── session_check.rs   # signed-out check
├── fetch.rs               # takes &NetSession
├── check.rs               # single page uses NetSession
└── crawl/
    ├── runner.rs          # session wiring, events, guard, audit rows
    ├── scope.rs           # sign-out rule
    ├── reason.rs          # SessionGuard, PauseReason::SessionEnded
    ├── store.rs           # record_cookies, requeue_fetched_since
    └── migrate.rs         # cookies table step
crates/results/src/
├── cookies.rs             # NEW: distinct, urls_for, filters
├── detail.rs              # Cookies section
└── export.rs              # cookies CSV
crates/cli/src/            # flags, env secrets, `cookies` command, `SECRETS_MISSING` exit code
crates/test-fixtures/src/
├── auth_site.rs           # NEW: Basic, Digest, NTLM server
├── login_site.rs          # NEW: form login, session cookie, logout, expiry
├── cookie_site.rs         # NEW: echo, set, loop-unless-cookie
└── proxy.rs               # NEW: counting HTTP CONNECT and SOCKS5 proxy
app/src-tauri/src/
├── login.rs               # NEW: login window, capture
├── crawl.rs               # session state, new commands, events
└── capabilities/          # login webview gets no IPC capability
ui/src/
├── settings/SignInsEditor.tsx, LoginPanel.tsx, CookiesEditor.tsx, ProxyPanel.tsx (+ tests)
├── crawl/AuthRequiredNotice.tsx, SessionEndedBanner.tsx (+ tests)
├── files/ResumeSecretsDialog.tsx (extended, + tests)
└── results/CookiesView.tsx (+ tests), Inspector.tsx (Cookies section)
```

**Structure Decision**: no new crate. `net/` is a module of `crates/engine` because the runner, robots, sitemaps and the render stage all need it and it has no UI dependency.
