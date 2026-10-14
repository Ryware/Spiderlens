# Feature Specification: Site Login, Cookies and Proxy

**Feature Branch**: `025-site-login-network`

**Created**: 2026-10-13

**Status**: Approved 2026-10-13 (Ryware), recommended defaults for all open decisions

**Input**: User description: "Crawling behind a login and network options: HTTP Basic, Digest and NTLM authentication; forms-based login through a login window in the app that captures the session cookies; custom cookies and custom request headers per crawl; a cookie storage policy (cookies are session only, never written to the crawl file, and re-entered on resume like the secrets of spec 006); proxy settings (HTTP and SOCKS with optional sign-in, or the system proxy); and a cookie audit (the cookies each URL sets, for consent reporting). Desktop first, secrets never stored, crawl speed unchanged on the default path."

## Overview

Today SpiderLens can only crawl what an anonymous visitor sees. Staging sites, intranets, membership areas and sites behind a corporate proxy are out of reach, except by pasting an `Authorization` header by hand (spec 002, custom headers). This feature lets the user crawl those sites the way a signed-in visitor would, through four mechanisms:

1. **HTTP authentication**: the server asks for a user name and password (Basic, Digest or NTLM), typically on staging sites and intranets.
2. **Forms-based login**: the site has its own login page. The user signs in once in a login window inside SpiderLens; the app keeps the session cookies the site sets and the crawl sends them.
3. **Cookies and headers**: the user supplies cookies (typed, pasted, or imported from a cookies file) and decides whether the crawler keeps the cookies the site sets during the crawl.
4. **Proxy**: the crawl's requests go through an HTTP or SOCKS proxy, with optional sign-in, or through the system's proxy settings.

It also adds the **cookie audit**: for every crawled URL, which cookies the site sets and with which attributes, so the user can report on cookies set before consent.

The rules that hold across all of it: passwords, tokens and session cookie values are never written to disk (crawl files, profiles, logs, exports), credentials are only ever sent to the sites they were given for, and a crawl that uses none of this runs exactly as fast as it does today.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Crawl a staging site behind HTTP authentication (Priority: P1)

An SEO specialist audits a client's staging site, which answers every request with "401 Unauthorized" and a request for a user name and password. In the crawl settings they add a sign-in for the staging host: the host, the user name and the password. The crawl then fetches every page as a signed-in user. If they forget, the first 401 that asks for a password shows a notice "staging.example.com asks for a sign-in" with a button to enter it; once entered, the affected URLs are retried.

**Why this priority**: Staging sites behind HTTP authentication are the most common reason an SEO crawler needs credentials; auditing a site before launch is a core use case and cannot be done today without hand-written headers.

**Independent Test**: Run fixture hosts that require Basic, Digest and NTLM authentication. Crawl each without credentials, then with the wrong password, then with the right one, and check status codes, retries, the notice and that the credentials reach only the configured host.

**Acceptance Scenarios**:

1. **Given** a fixture host that requires Basic authentication and a configured sign-in for it, **When** the crawl runs, **Then** every internal page returns 200 and the crawl results match those of the same site without authentication.
2. **Given** fixture hosts requiring Digest and NTLM authentication, **When** each is crawled with the right sign-in, **Then** every page returns 200.
3. **Given** no configured sign-in, **When** the first URL returns 401 with an authentication challenge, **Then** the crawl keeps going, the URL is recorded as 401, and a notice names the host and the authentication method it asked for, with an action to enter a sign-in; after entering it, every URL of that host that ended in 401 is retried automatically.
4. **Given** a wrong password, **When** the crawl runs, **Then** the host's URLs end as 401 with the reason "sign-in rejected", the crawl does not retry them in a loop, and the notice offers to correct the sign-in.
5. **Given** a sign-in for `staging.example.com`, **When** a page links to or redirects to another host, **Then** the credentials are never sent to that other host.
6. **Given** a sign-in using Basic authentication on an `http://` (not `https://`) address, **When** the user saves it, **Then** the app warns that the password would travel unencrypted and requires the user to confirm.

---

### User Story 2 - Crawl the members area of a site with its own login page (Priority: P1)

A specialist needs to audit the pages of a site that are only visible after signing in through the site's own login form. In the crawl settings they choose "Sign in with a login page", enter the login page address and press "Open login window". A window inside SpiderLens shows the site's login page; they sign in as they would in a browser (including two-factor codes or a single sign-on page), then press "Done". SpiderLens shows that it captured the site's session (for example "4 cookies for example.com") and the crawl runs as the signed-in user.

**Why this priority**: Forms-based login is how most real sites protect member content; without it, membership, e-commerce account and SaaS sites cannot be audited at all. Capturing the session from a real login window works with any login form, two-factor step or single sign-on provider, where filling a form automatically would break on most of them.

**Independent Test**: Run a fixture site with a login form, a session cookie, a members-only area, a logout link and a session that expires after N requests. Sign in through the login window (driven by an automated UI test), crawl, and check that members-only pages are crawled, the logout link is not requested, and session expiry is detected.

**Acceptance Scenarios**:

1. **Given** the login window shows the fixture's login page, **When** the user signs in and presses Done, **Then** the app shows how many cookies it captured for which sites, and a crawl started afterwards returns the members-only pages with 200.
2. **Given** a captured session, **When** the crawl discovers links whose address looks like signing out (for example containing `logout`, `log-out`, `signout`, `sign-out`, `logoff`), **Then** those URLs are listed but not requested, with the reason "not crawled: could end the session"; the user can edit this list of patterns.
3. **Given** a captured session and a configured "signed-out check" (a page address that redirects to the login page, or text that appears only when signed out), **When** the session expires mid-crawl, **Then** the crawl pauses, tells the user the session ended, offers to sign in again in the login window, and after a new sign-in resumes and re-fetches the URLs fetched since the session ended.
4. **Given** the user signs in through a single sign-on provider on another domain, **When** they press Done, **Then** only the cookies of the crawl's own sites are used for crawling; cookies of the sign-on provider's domain are discarded.
5. **Given** the login window, **When** it opens, **Then** it never shows or uses the user's own browser profile, cookies or saved passwords, and when it closes everything it stored is erased.
6. **Given** a captured session, **When** the user quits the app or the crawl is closed, **Then** the session cookies are gone; reopening or resuming the crawl asks the user to sign in again.

---

### User Story 3 - Supply cookies and choose whether the crawler keeps cookies (Priority: P2)

A specialist wants to crawl a site with a consent cookie already accepted, or with an A/B test variant fixed, or with a session cookie they copied from their browser's developer tools. They add cookies in the crawl settings (name, value, site and path), paste a `Cookie:` line, or import a cookies file exported from a browser extension. Separately, they choose what the crawler does with cookies the site sets during the crawl: ignore them (default, as search engines do) or keep them for the rest of the crawl.

**Why this priority**: Supplied cookies cover cases the login window does not (consent states, feature flags, sessions shared by a developer), and keeping cookies is required by sites that redirect in a loop when the visitor refuses cookies. Both are less common than logins.

**Independent Test**: A fixture that echoes received cookies, sets cookies on some pages, and loops redirects for clients that refuse cookies. Crawl with each policy and with supplied cookies and check exactly which cookies each request carried.

**Acceptance Scenarios**:

1. **Given** a supplied cookie `consent=yes` for `example.com`, **When** the crawl runs, **Then** every request to `example.com` and its subdomains carries it, and no request to another site does.
2. **Given** the policy "Ignore cookies the site sets" (default), **When** a page sets a cookie, **Then** later requests do not carry it.
3. **Given** the policy "Keep cookies during the crawl", **When** a page sets a cookie, **Then** later requests that match its site, path and security rules carry it, and expired or deleted cookies stop being sent.
4. **Given** a fixture page that redirects in a loop unless the client returns its cookie, **When** crawled with "Ignore", **Then** the URL ends with a redirect loop error whose message suggests "Keep cookies during the crawl"; with "Keep", it returns 200.
5. **Given** a cookies file in the common browser export format, **When** the user imports it, **Then** only cookies for the crawl's internal sites are kept, the app shows how many were imported and how many were skipped, and expired cookies are skipped.
6. **Given** a cookie marked "secure only", **When** the crawl requests an `http://` address, **Then** that cookie is not sent.

---

### User Story 4 - Crawl through a proxy (Priority: P2)

A specialist in a company network can only reach the internet through the corporate proxy; another wants to see a site as it appears from another country through a paid proxy. In the crawl settings they choose: no proxy (default), the system's proxy settings, or a manual proxy (HTTP, HTTPS or SOCKS5 address and port, optional user name and password, and a list of hosts that bypass it). A "Test" button checks the proxy before the crawl starts.

**Why this priority**: Without proxy support the app does not work at all in many corporate networks, and geo-specific audits need it. It is a single setting, so it costs little.

**Independent Test**: Run a local fixture proxy (HTTP CONNECT and SOCKS5, with and without sign-in) that counts requests. Crawl the fixture site through it and check every request went through the proxy, bypassed hosts did not, and errors are clear when the proxy is down or rejects the sign-in.

**Acceptance Scenarios**:

1. **Given** a manual HTTP proxy, **When** the crawl runs, **Then** every request of the crawl (pages, redirects, robots.txt, sitemaps, images and external link checks) goes through the proxy.
2. **Given** a SOCKS5 proxy with user name and password, **When** the crawl runs, **Then** every request goes through it, signed in.
3. **Given** a bypass list containing `localhost` and `*.intranet.example`, **When** the crawl requests a matching host, **Then** that request goes direct.
4. **Given** "Use system proxy settings", **When** the operating system has a proxy configured (including an automatic configuration script), **Then** the crawl uses it, and the crawl information shows which proxy was in effect.
5. **Given** a proxy that is unreachable or rejects the sign-in, **When** the user presses Test or starts the crawl, **Then** the app shows "proxy unreachable" or "proxy rejected the sign-in" before any page is requested, instead of failing every URL with a connection error.
6. **Given** the proxy goes down mid-crawl, **When** every request starts failing, **Then** the crawl pauses as a local network problem (spec 004) naming the proxy as the likely cause.

---

### User Story 5 - Audit which cookies the site sets (Priority: P2)

A privacy-minded SEO or a consultant checking consent compliance wants to know which cookies the site sets on a first visit, before the visitor has agreed to anything. After a crawl, a Cookies view lists every distinct cookie the site set (name, site, path, whether it is a session or persistent cookie and for how long, Secure, HttpOnly, SameSite, whether it is first or third party) with the number of URLs that set it; selecting one shows those URLs. For each URL, the detail pane lists the cookies its response set.

**Why this priority**: Consent reporting is a common audit deliverable and a gap against established crawlers, and the data is a by-product of responses the crawl already receives.

**Independent Test**: A fixture site whose pages set known cookies with known attributes. Crawl it and compare the Cookies view and per-URL lists with the expected set; open the crawl file and confirm no cookie value is stored.

**Acceptance Scenarios**:

1. **Given** a crawl of the fixture site, **When** the user opens the Cookies view, **Then** each distinct cookie appears once with its attributes and the count of URLs that set it, and selecting it lists those URLs.
2. **Given** a URL whose response set three cookies, **When** the user selects it, **Then** the detail pane lists the three cookies with their attributes.
3. **Given** any crawl, **When** the crawl file is inspected, **Then** no cookie value appears anywhere in it, including in stored response headers, where cookie values are replaced by a placeholder.
4. **Given** the cookie audit, **When** the user exports the Cookies view, **Then** the export lists cookies and attributes without values.
5. **Given** JavaScript rendering (spec 014) is on, **When** a rendered page sets cookies through scripts or third-party resources, **Then** those cookies appear in the audit marked "set by script" or "third party", next to the ones set by the server's response.

---

### User Story 6 - Resume, share and repeat crawls without leaking secrets (Priority: P1)

A specialist pauses a signed-in crawl, quits, and resumes the next day; later they send the crawl file to a colleague and save the settings as a profile. The file and profile keep which hosts need a sign-in, the user names, the cookie policy and the proxy address, but never a password, token or session cookie value. On resume, the app asks for exactly what is missing (passwords, a new login-window sign-in, supplied secret cookies) before any request is sent.

**Why this priority**: This is the constitution's rule that secrets are never stored, applied to the new kinds of secret. A leak through a shared crawl file would be the most damaging failure of this feature.

**Independent Test**: For every kind of secret this spec adds, save a crawl file, a profile and an export, then search them byte by byte for the secret values. Resume the crawl and check the prompt asks for each missing secret.

**Acceptance Scenarios**:

1. **Given** a crawl using an HTTP sign-in, a login-window session, supplied cookies marked secret and a proxy with a password, **When** the crawl file, an exported profile, the crawl log and every export are searched for those values, **Then** none of them is found.
2. **Given** that crawl paused and reopened, **When** the user presses Resume, **Then** a single dialog lists each missing secret (for example "Password for staging.example.com", "Sign in again to example.com", "Proxy password") and nothing is requested until the user provides them or chooses to resume without them.
3. **Given** the user resumes without a sign-in, **When** the crawl continues, **Then** it is recorded in the crawl's history that it ran signed out from that point.
4. **Given** the command-line crawler, **When** a configuration needs a secret, **Then** it reads it from an environment variable named in the documentation or asks for it on the terminal, and never accepts it as a plain command-line argument (which other programs on the computer can see).

---

### Edge Cases

- A site uses both HTTP authentication and a login form: both are configured and both apply.
- Several hosts need different sign-ins (for example `staging.example.com` and `cdn-staging.example.com`): each sign-in is set per host.
- A host asks for an authentication method SpiderLens does not support (for example Negotiate/Kerberos only): the URL is recorded as 401 and the notice names the method as unsupported.
- NTLM with a domain: the user name can be given as `DOMAIN\user`.
- A signed-in crawl follows links that change data with a plain link (for example "delete", "unsubscribe", "cancel order"). SpiderLens only ever sends read requests, but some sites still change data on them: the app shows a one-time warning when a signed-in crawl starts, recommending a test account, and the sign-out patterns can be extended with any such patterns.
- The site sets a new session cookie on every response (session rotation) while signed in: in a login-window session, cookies the site sets for the crawl's sites update the captured session even under the "Ignore" policy, so the session does not break.
- The login window is closed without signing in: nothing is captured and the setting shows "not signed in".
- The login page is on a host outside the crawl's scope (for example `login.example.com` while crawling `www.example.com`): the window allows it; only cookies that the crawl's internal hosts would receive are kept.
- robots.txt disallows the members area: signing in does not change robots handling; the robots settings of spec 002 still apply.
- A supplied cookie or header is very large, or there are hundreds of cookies: the app refuses values the site would reject by common limits (cookie over 4 KB, more than 180 cookies for one site) with a message.
- A cookie file contains malformed lines: those lines are skipped and counted, the rest is imported.
- System proxy uses an automatic configuration script that cannot be fetched: the crawl does not start, and the message names the script address.
- A proxy changes the TLS certificate (a corporate inspection proxy): the certificate is accepted only if the operating system trusts it; otherwise the URL fails with a TLS error naming the proxy as a likely cause.
- External links are checked (spec 002): they never receive supplied cookies, sign-ins or login-window cookies, but they do go through the proxy.
- List mode with URLs from several sites: sign-ins and cookies apply per host as configured; the login window can be used for each site that needs it.
- Scheduled or unattended crawls (spec 021): there is nobody to sign in or type a password. Such a crawl gets secrets only from the environment it is started in (see FR-034); if one is missing it does not start and records why.

## Requirements *(mandatory)*

### Functional Requirements

**HTTP authentication**

- **FR-001**: Users MUST be able to add sign-ins to the crawl configuration, each made of a host (scheme, host and optional port), a user name and a password, optionally restricted to one authentication method.
- **FR-002**: The system MUST support the Basic, Digest and NTLM authentication methods, answering the method the server asks for.
- **FR-003**: The system MUST send a sign-in's credentials only to the host it was configured for, never to another host, including across redirects.
- **FR-004**: When a URL returns 401 with an authentication challenge and no sign-in is configured for its host, the system MUST record the URL as 401, keep crawling, and show a notice naming the host and the method asked for, with an action to add a sign-in. After a sign-in is added, the system MUST automatically retry every URL of that host that ended in 401.
- **FR-005**: When the server rejects configured credentials, the system MUST record the URL as 401 with the reason "sign-in rejected", MUST NOT retry it beyond the normal retry rules of spec 004, and MUST offer to correct the sign-in.
- **FR-006**: Saving a sign-in that would send a password unencrypted (Basic over `http://`) MUST show a warning and require confirmation.
- **FR-007**: A 407 (proxy sign-in required) MUST be reported as a proxy problem, not a site problem.

**Forms-based login (login window)**

- **FR-008**: Users MUST be able to open a login window inside the app at a login page address they choose, sign in there manually, and finish with "Done", after which the system keeps the cookies that the crawl's internal hosts would receive and discards all others.
- **FR-009**: The login window MUST use a private, empty browsing state: never the user's own browser profile, cookies, saved passwords or signed-in accounts. Everything it stored MUST be erased when it closes.
- **FR-010**: After capture, the system MUST show the number of captured cookies per site and their names (not their values), and let the user sign in again or clear the session.
- **FR-011**: When a login-window session is in use, the system MUST NOT request URLs matching the sign-out patterns, recording them as "not crawled: could end the session". The default patterns cover common sign-out addresses (`logout`, `log-out`, `signout`, `sign-out`, `logoff`, `log-off`) and are editable.
- **FR-012**: Users MAY configure a signed-out check: a URL to test periodically, the result that means "signed out" (a redirect to a given address, a given status code, or a given text in the page). When the check fails mid-crawl, the system MUST pause the crawl, tell the user, offer to sign in again, and on resume re-fetch the URLs fetched since the last successful check.
- **FR-013**: Cookies that the crawl's internal hosts set during a login-window session MUST update the captured session (so rotating session cookies keep working), whatever the cookie policy.
- **FR-014**: When a signed-in crawl (HTTP sign-in or login window) starts for the first time, the system MUST show a one-time warning that a signed-in crawl can change data on sites that do so with plain links, and recommend using a test account.

**Cookies and headers**

- **FR-015**: Users MUST be able to add cookies to the crawl configuration as name, value, domain and path, mark a value as secret, paste a `Cookie:` header line to add several at once, and import a cookies file in the common browser export format (Netscape `cookies.txt`), keeping only unexpired cookies for the crawl's internal sites and reporting how many were imported and skipped.
- **FR-016**: Supplied and captured cookies MUST be sent only to hosts and paths they match under the standard cookie rules, never to external hosts, and cookies marked secure only MUST NOT be sent over `http://`.
- **FR-017**: Users MUST be able to choose the cookie policy for cookies the site sets during the crawl: "Ignore cookies the site sets" (default) or "Keep cookies during the crawl". With "Keep", the system MUST apply the standard cookie rules (domain, path, expiry, Secure, deletion) and keep the cookies in memory only.
- **FR-018**: When a URL ends in a redirect loop and the site set cookies along the loop while the policy is "Ignore", the error message MUST suggest "Keep cookies during the crawl".
- **FR-019**: Custom request headers remain as defined in spec 002 (FR-019, FR-020, FR-038). The `Cookie` and `Authorization` headers MUST be rejected as custom headers with a message pointing to the cookie and sign-in settings, so each secret has one place and one masking rule. `Proxy-Authorization` stays rejected.

**Proxy**

- **FR-020**: Users MUST be able to choose one proxy mode: "No proxy" (default), "Use system proxy settings" (including automatic configuration scripts), or "Manual" with type (HTTP, HTTPS or SOCKS5), address, port, optional user name and password, and a bypass list of host patterns.
- **FR-021**: Every network request of the crawl (pages, redirects, robots.txt, sitemaps, resources, external link checks and, when spec 014 is on, rendering) MUST use the configured proxy, except hosts on the bypass list.
- **FR-022**: Users MUST be able to test the proxy before crawling, and the system MUST check it when a crawl starts, refusing to start with a clear message ("proxy unreachable", "proxy rejected the sign-in", "automatic configuration script could not be read") instead of failing each URL.
- **FR-023**: A proxy failure in the middle of a crawl MUST be handled as a local network problem (spec 004), naming the proxy as the likely cause.
- **FR-024**: The crawl information MUST show the proxy in effect (mode, type, address and port, never the password), and stored results MUST NOT depend on proxy environment variables of the process unless the user chose "Use system proxy settings".

**Cookie audit**

- **FR-025**: For every fetched URL, the system MUST record each cookie its response set: name, domain, path, session or persistent with expiry, Secure, HttpOnly, SameSite, size in bytes, and whether it is first or third party relative to the URL. Cookie values MUST NOT be stored.
- **FR-026**: Stored response headers MUST keep `Set-Cookie` headers with their values replaced by a placeholder, so no cookie value is written to the crawl file. This also applies to crawl files written before this feature when they are upgraded (spec 006 migrations).
- **FR-027**: The results MUST offer a Cookies view listing each distinct cookie (by name, domain and path) with its attributes and the number of URLs that set it, with filters at least for session, persistent, third party, missing Secure, missing HttpOnly and SameSite=None; selecting a cookie MUST list the URLs that set it, and the URL detail pane MUST list the cookies each URL set.
- **FR-028**: When JavaScript rendering (spec 014) is on, cookies set while rendering a page (by scripts or third-party resources) MUST be recorded too, marked as "set by script" and first or third party.
- **FR-029**: The cookie audit MUST be on by default, record only what responses already contain, and be exportable (spec 010) without values.

**Secrets, resume and parity**

- **FR-030**: Passwords, proxy passwords, cookie values marked secret, all login-window cookie values and all cookie values the site set MUST NOT be written to crawl files, exported profiles, logs, exports or crash data, and MUST be masked wherever displayed. Crawl files and profiles keep only what is needed to ask for them again: hosts, user names, cookie names and domains, proxy address and user name.
- **FR-031**: On resume or reopen of a crawl, and when a profile is used, the system MUST ask in one dialog for every missing secret and every login-window session before sending any request, and MUST allow resuming without them, recording in the crawl's history that it ran without them from that point.
- **FR-032**: Login-window sessions and cookies kept under "Keep cookies during the crawl" MUST exist only in memory and MUST be erased when the crawl is closed or the app quits.
- **FR-033**: Every setting in this spec MUST be available in the command-line crawler, with secrets read from documented environment variables or typed at a terminal prompt, never as plain arguments. The command line MUST accept a cookies file (FR-015) in place of the login window, which is desktop only.
- **FR-034**: Unattended crawls (spec 021 scheduling, or the command line without a terminal) MUST get secrets only from their environment as in FR-033; when a required secret is missing they MUST NOT start, and MUST record the reason. Storing secrets in the operating system's secure credential store for unattended runs is out of scope for this spec (see Assumptions).
- **FR-035**: When JavaScript rendering (spec 014) is on, rendering MUST use the same proxy, HTTP sign-ins, supplied cookies and login-window session as the raw crawl for internal pages, while keeping the clean state of spec 014 FR-011 for everything else (no cookies or storage carried from page to page beyond these). This amends spec 014 FR-011 and its out-of-scope note on logins.
- **FR-036**: A crawl that uses none of the settings in this spec MUST behave and perform exactly as before: no cookies sent or kept, direct connections, and no additional work per request beyond recording `Set-Cookie` attributes (FR-025).

### Key Entities *(include if feature involves data)*

- **Site Sign-in**: Host (scheme, host, port), user name, password (secret, never stored), allowed method (any, Basic, Digest or NTLM). Part of the crawl configuration without the password.
- **Login Session**: The login page address, the time of sign-in, the sites and cookie names captured, the sign-out patterns and the optional signed-out check. Cookie values live in memory only.
- **Supplied Cookie**: Name, value (plain or secret), domain, path, secure only. Plain values are stored with the configuration; secret ones are not.
- **Cookie Policy**: Ignore (default) or Keep during the crawl.
- **Proxy Setting**: Mode (none, system, manual), type, address, port, user name, password (secret, never stored), bypass list; plus the proxy actually in effect for a crawl.
- **Cookie Observation**: For one URL and one cookie it set: name, domain, path, session or expiry, Secure, HttpOnly, SameSite, size, first or third party, source (response header or script). No value.
- **Crawl History Entry** (spec 006): adds "signed in", "session ended", "signed in again" and "continued without sign-in" events.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: On the benchmark fixture site, a crawl with default settings (none of this feature's settings used) runs within 2% of the pages per second measured on main before this feature, and a crawl with a supplied cookie and "Keep cookies" on runs within 5%.
- **SC-002**: In automated leak tests covering every secret kind of this spec, 0 secret values are found in crawl files, exported profiles, logs or exports, byte by byte.
- **SC-003**: Fixture hosts requiring Basic, Digest and NTLM authentication are crawled with 100% of internal pages returning 200 when the right sign-in is configured.
- **SC-004**: In automated tests, credentials and cookies are sent to 0 hosts other than those they were configured or set for, including across redirects to other hosts and external link checks.
- **SC-005**: On the login fixture, a user can sign in through the login window and start a signed-in crawl in under 1 minute, and sign-out URLs are requested 0 times.
- **SC-006**: When the fixture session expires mid-crawl with a signed-out check configured, the crawl pauses within 50 requests of the expiry in 100% of runs, and after signing in again finishes with the same results as an uninterrupted signed-in crawl.
- **SC-007**: Through the fixture proxy (HTTP and SOCKS5, with and without sign-in), 100% of the crawl's requests to non-bypassed hosts go through the proxy, and a down proxy is reported before the first page request in 100% of runs.
- **SC-008**: The cookie audit lists 100% of the fixture's cookies with correct attributes, and 0 cookie values appear in the crawl file.
- **SC-009**: Every acceptance scenario above maps to at least one automated test, including UI tests for the sign-in, cookie, proxy and Cookies view screens, passing on Windows, macOS and Linux.

## Assumptions

- **Desktop first**: the login window is a desktop feature; the command line uses a cookies file instead. A later web version would need its own approach to logins.
- **Default cookie policy is "Ignore"**, which is what search engine crawlers do and what SpiderLens does today, so default crawls see what Google sees and stay as fast as today. "Keep" is one switch away and is suggested when a redirect loop needs it.
- **Default proxy mode is "No proxy"**, which is today's behaviour and keeps results independent of the machine they run on. Users behind a corporate proxy choose "Use system proxy settings" once and can save it in a profile.
- **The login window does not fill forms automatically.** The user signs in by hand, which works with two-factor steps, captchas and single sign-on. Recording and replaying a login form is out of scope.
- **NTLM** is supported on all three platforms with a user name, password and optional domain; Negotiate/Kerberos (signing in with the computer's own Windows account) and client certificates are out of scope.
- **Unattended crawls** (spec 021) take site secrets only from their environment. Owner decision 2026-10-13: tokens and API keys of third-party integrations (spec 024) are kept in the operating system's secure credential store, but site passwords, proxy passwords and login cookies are never stored anywhere, so a scheduled crawl can use integration credentials from that store but never site credentials.
- **Cookie audit scope**: the raw crawl sees only cookies set by the server's responses (first party). Cookies set by scripts and third-party tags are seen only with JavaScript rendering (spec 014). Checking whether a consent banner is respected (cookies before and after consent) is out of scope; the audit reports the state of a first visit, or of the supplied consent cookie.
- **Rules**: issue rules about cookies and security (for example "cookie without Secure on HTTPS", "third-party cookie before consent", "session cookie missing HttpOnly") belong to the rule catalogue of spec 009 and use the data recorded here.
- **Storage**: new data (cookie observations, sign-in and proxy configuration without secrets, history events) is added to the crawl file through the additive migration steps owned by spec 006.
- **Out of scope**: other HTTP methods than GET and HEAD (no form submission by the crawler), client TLS certificates, per-URL different sign-ins within one host, rotating proxy pools, Tor, cookie consent banner automation, and storing secrets anywhere on disk.
