# Research: Site Login, Cookies and Proxy

**Feature**: [spec.md](./spec.md) | **Plan**: [plan.md](./plan.md) | **Date**: 2026-10-14

Each entry: the question, the decision, why, and what else was considered. Facts about the current code were checked against `main` at 5347999 and the open storage and errors branches (PR #27 at b9bb34f, PR #29 at 3ff6b51).

## R1. Where the network lives today

- One `reqwest::Client` per crawl, built by `fetch::build_client_with` (`crates/engine/src/fetch.rs:16`) with rustls, gzip and brotli, manual redirects, the crawl's timeouts and `.no_proxy()`. No cookie store.
- `fetch_once` (crawler) and `get_following` (single page check) add custom headers per request; internal-only headers are filtered per hop.
- Every response header is kept in `Once::headers` and stored as JSON pairs (`store.rs` `record_headers`). That stored `Set-Cookie` values; PR #51 fixes it with `crawl::redact`.
- Secret header values use `config::HeaderValue` (`crates/config/src/secret.rs`): never serialised, `Secret(None)` after loading, and `missing_secrets` drives the 006 resume prompt (`CrawlError::NeedsSecrets`, `ResumeOptions.secrets` in PR #27). The CLI passes them with `--header-secret NAME=VALUE`.

**Decision**: put the client, cookies, sign-ins and proxy behind one `NetSession` (plan D1) and reuse the 006 secrets flow with new keys. **Rejected**: a second client for signed-in requests (two connection pools, two places to get proxy right, and spec 014 would have to pick one).

## R2. Cookie jar

**Decision**: `cookie_store` 0.21 inside our own `CrawlJar` implementing `reqwest::cookie::CookieStore`.

- reqwest's built-in `Jar` cannot list, remove or filter cookies, which the policy (D2), the login session summary and spec 014's browser seeding all need.
- `cookie_store` implements RFC 6265 matching (domain, path, Secure, expiry, public suffix rejection when given the list) and is what `reqwest_cookie_store` wraps; we wrap it ourselves to add the internal-host gate and the policy, so the extra crate is not needed.
- Bounds of 180 cookies per domain and 4 KB per cookie follow the limits browsers enforce, so the crawler never sends what a browser would not.

**Rejected**: a hand-written jar (RFC 6265 matching has many edge cases: leading dots, host-only cookies, path prefixes, expiry in the past to delete).

## R3. Basic and Digest

- **Basic** is a header; sending it pre-emptively after the first challenge avoids a 401 round trip per URL.
- **Digest** (RFC 7616): `digest_auth` 0.3 parses the challenge and computes MD5, MD5-sess and SHA-256 responses with `qop=auth`. Reusing the server nonce with an increasing `nc` lets every later request carry credentials up front; `stale=true` means "new nonce, same password" and must not count as a rejection. `auth-int` is rare and not supported.

**Rejected**: answering every request after a fresh 401 (doubles requests per URL against a Digest site, violating the speed goal and principle IV).

## R4. NTLM

- NTLM (MS-NLMP) is a three-message handshake bound to one TCP connection. reqwest and hyper's pooled client choose connections internally, so the Type 3 message can land on another connection and fail. Owning the connections is the only reliable way.
- **Decision**: `NtlmPool` with hyper's low-level `client::conn::http1` API (already a dependency through reqwest), NTLMv2 implemented in about 300 lines with `md4` and `hmac` (MD5 from the `md-5` crate already in the lock file through other crates; checked by T001). Verified against the NTLMv2 test vectors in MS-NLMP section 4.2.4.
- Channel binding (EPA) and NTLMv1 are out of scope; servers requiring them get a clear message.
- **Rejected**: `sspi` (Devolutions) covers NTLM and Kerberos but adds a large dependency tree for one method we use; the OS SSPI on Windows only (no macOS and Linux parity); Negotiate/Kerberos (out of scope per spec).

## R5. Login window cookies

- Tauri 2.12 exposes `Webview::cookies()` and `cookies_for_url()` (wry 0.57): WebView2 `ICoreWebView2CookieManager::GetCookies`, WKWebView `WKHTTPCookieStore.getAllCookies`, WebKitGTK `webkit_cookie_manager_get_all_cookies`. All three include HttpOnly cookies, which the page's own JavaScript cannot see, and which are usually the session cookies. Verified by spike T001 on the three CI runners before anything builds on it.
- `incognito(true)` gives an ephemeral data store on all three (WebView2 InPrivate, `WKWebsiteDataStore.nonPersistent`, WebKitGTK ephemeral network session), so the user's other app data, saved passwords and the system browser profile are never involved (FR-009). A per-window temporary `data_directory` is deleted when the window closes as a second guarantee on Windows.
- The login webview gets no Tauri capability, so a page in it cannot call the app's commands; Done and Cancel live in the app's own window.

**Rejected**: a headless browser driven by recorded form steps (breaks on two-factor, captcha and single sign-on, and needs spec 014's browser); reading cookies from the user's installed browser (privacy, and platform-specific encrypted stores).

## R6. Proxy

- reqwest 0.12 supports HTTP, HTTPS and (with feature `socks`) SOCKS5 proxies with credentials, plus `NoProxy` lists. `socks5h` resolves names through the proxy, which geo-specific audits need.
- **System settings**: reqwest only reads environment variables. OS settings need per-platform calls: WinHTTP on Windows (fixed proxy and PAC via `WinHttpGetProxyForUrl`), CFNetwork on macOS (fixed and PAC), and on Linux GNOME `gsettings` and KDE `kioslaverc` for fixed proxies. Evaluating a PAC script on Linux needs a JavaScript engine (for example `boa_engine`, about 3 MB in release builds) because there is no system service; this is owner point 1 in the plan.
- Resolving once per crawl (and per host for PAC, cached) keeps the request path free of proxy lookups.

## R7. Cookie audit data

- `Set-Cookie` attributes: Domain, Path, Expires, Max-Age, Secure, HttpOnly, SameSite, plus the newer Partitioned and Priority, which are stored in a free `extra` text field rather than columns.
- First or third party is decided by registrable domain (public suffix list, already used by `crates/config` for subdomain scope), the same rule browsers use for "same site".
- Values are never stored, only their size, so the file stays safe to share (FR-025). The Cookies view groups by (name, domain, path), which is how browsers identify a cookie.

## R8. Signed-out detection

- A fixed check every 25 internal fetches or 60 s, whichever comes first, finds expiry within the SC-006 budget of 50 requests even with up to 25 requests in flight at Fast speed per host (spec 008 Fast preset allows 32 per host; the check counts completions, so the in-flight window is covered by the remaining 25).
- Three conditions cover the common sites: a redirect to the login URL, a status code (401/403), or a text present only when signed out ("Sign in"). Regular expressions are not offered to keep the setting understandable.

## R9. New dependencies

| Crate | Why | Size note |
|---|---|---|
| `cookie_store` | RFC 6265 jar (R2) | small, pure Rust |
| `cookie` | Set-Cookie parsing for the audit | already in the lock file |
| `digest_auth` | Digest responses (R3) | small |
| `md4`, `hmac` | NTLMv2 (R4) | RustCrypto, small |
| reqwest features `cookies`, `socks` | cookie provider hook, SOCKS5 | feature flags |
| `arc-swap` | lock-free sign-in map on the request path | small, already used widely |

No new dependency for the default path's code: the jar crate is linked but not used when no jar is built.

## R10. Owner decisions already taken

- 2026-10-13: tokens of third-party integrations (spec 024) may be kept in the OS keychain; site passwords, proxy passwords and login cookies are never stored anywhere. Scheduled crawls (spec 021) get site credentials only from their environment (FR-034).
- 2026-10-13: spec approved with the recommended defaults (Ignore, No proxy, cookie audit on, manual login in a private window).
