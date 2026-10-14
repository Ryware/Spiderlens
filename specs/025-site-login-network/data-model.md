# Data Model: Site Login, Cookies and Proxy

**Feature**: [spec.md](./spec.md) | **Plan**: [plan.md](./plan.md)

## Settings (`crates/config/src/model.rs`, stored with the crawl and in profiles)

`CrawlConfig.access: AccessSettings`, `#[serde(default)]`. Every field below is optional on input; missing fields take the default, so older crawl files and profiles load unchanged.

```text
AccessSettings
├── sign_ins: Vec<SignIn>                 default []
├── login: Option<LoginSettings>          default None
├── cookies: Vec<SuppliedCookie>          default []
├── cookie_policy: CookiePolicy           default "ignore"
└── proxy: ProxySettings                  default { mode: "none" }

SignIn
├── origin: String        "https://staging.example.com" or with port; scheme + host + port only
├── user: String          may be "DOMAIN\\user" for NTLM
├── password: Secret      never serialised; {"secret": true} on disk
└── method: AuthMethod    "any" (default) | "basic" | "digest" | "ntlm"

LoginSettings
├── login_url: String
├── sign_out_patterns: Vec<String>   default ["logout","log-out","signout","sign-out","logoff","log-off"]
└── check: Option<SignedOutCheck>

SignedOutCheck
├── url: String
└── when: CheckCondition   { "redirects_to": "<substring>" } | { "status": 401 } | { "contains": "<text>" }

SuppliedCookie
├── name: String
├── value: HeaderValue     plain or secret (same type as custom headers)
├── domain: String         "example.com" (sent to subdomains) or ".example.com"
├── path: String           default "/"
└── secure_only: bool      default false

CookiePolicy: "ignore" | "keep"

ProxySettings
├── mode: "none" | "system" | "manual"
├── kind: "http" | "https" | "socks5"     manual only, default "http"
├── host: String, port: u16               manual only
├── user: Option<String>
├── password: Secret                       only with user
└── bypass: Vec<String>                    host patterns, "*.example.com", "localhost", "10.0.0.0/8"
```

### Validation (`validate.rs`), codes in the spec 002 style

| Code | Rule |
|---|---|
| `access.sign_in.origin` | parses as http(s) URL with no path, query or fragment beyond `/` |
| `access.sign_in.duplicate` | one sign-in per origin |
| `access.sign_in.insecure` | warning: Basic or any method on `http://` (UI confirm, CLI `--allow-insecure-auth`) |
| `access.cookie.name` / `.value` | RFC 6265 token and cookie-octet rules; value at most 4,096 bytes |
| `access.cookie.domain` | not a public suffix; must match an internal host of the crawl (warning otherwise: it would never be sent) |
| `access.cookie.count` | at most 180 per domain |
| `access.login.url` | http(s) URL |
| `access.login.patterns` | non-empty strings |
| `access.proxy.host` / `.port` | required in manual mode; port 1 to 65,535 |
| `access.proxy.bypass` | each entry a host, wildcard host, IP or CIDR |
| `identity.headers.forbidden` | adds `cookie` and `authorization` (error for new configs and imports, warning for `Context::Stored`) |

### Secret keys (`CrawlConfig::missing_secrets`)

| Key | Asked as | CLI environment variable |
|---|---|---|
| `header:<name>` | existing | `SPIDERLENS_HEADER_<NAME>` (new alternative to `--header-secret`) |
| `sign_in:<origin>` | "Password for staging.example.com (user ann)" | `SPIDERLENS_PASSWORD_<HOST>` |
| `proxy` | "Proxy password for user ann at proxy.corp:8080" | `SPIDERLENS_PROXY_PASSWORD` |
| `cookie:<domain>/<name>` | "Value of cookie sid for example.com" | `SPIDERLENS_COOKIE_<NAME>` |
| `login:<login_url>` | "Sign in again to example.com" (button) | none: use `--cookies-file` |

`<HOST>` and `<NAME>` are upper-cased with every non-alphanumeric character replaced by `_` (`staging.example.com:8443` → `STAGING_EXAMPLE_COM_8443`).

## Runtime types (`crates/engine/src/net/`, memory only)

```text
LoginSession
├── login_url: String
├── captured_at: Timestamp
├── cookies: Vec<SessionCookie>    name, value (SecretString), domain, path, secure, http_only, expires
└── (patterns and check come from LoginSettings)

DigestState     realm, nonce, opaque, algorithm, qop, nc: AtomicU32
ProxyInEffect   mode, kind, host, port, user, source ("manual" | "env" | "os" | "pac:<url>")
```

`LoginSession` is created by the desktop app from the login window or by the CLI from `--cookies-file` (then `login_url` is the file name), and is passed to `Crawler::run`. It implements `Drop` by wiping values (secrecy).

## Crawl events and reasons

| Item | Where | Notes |
|---|---|---|
| `CrawlEvent::AuthRequired { origin, schemes, supported }` | runner → UI/CLI | once per origin |
| `CrawlEvent::AuthRejected { origin, method }` | runner → UI/CLI | once per origin |
| `PauseReason::SessionEnded { since }` | runner | paused by the signed-out check |
| `NotCrawledReason::SessionGuard` | `reason.rs` | stored as `session_guard`, label "not crawled: could end the session" |
| `reason_detail` `auth_required:<schemes>`, `auth_rejected:<method>` | `urls` | on 401 rows |
| events table kinds | `signed_in`, `session_ended`, `signed_in_again`, `continued_without_sign_in` | detail JSON lists keys, never values |
| meta `proxy_in_effect` | JSON of `ProxyInEffect` | crawl information sheet |

## Crawl file: the `cookies` table (one additive migration step)

```sql
CREATE TABLE cookies (
    url_id    INTEGER NOT NULL,     -- urls.id of the response that set it
    name      TEXT NOT NULL,
    domain    TEXT NOT NULL,        -- leading dot removed; host_only says whether subdomains get it
    host_only INTEGER NOT NULL,
    path      TEXT NOT NULL,
    expires   INTEGER,              -- ms since epoch; NULL = session cookie
    max_age   INTEGER,              -- seconds as sent, NULL when absent
    secure    INTEGER NOT NULL,
    http_only INTEGER NOT NULL,
    same_site TEXT,                 -- 'strict' | 'lax' | 'none' | NULL when absent
    size      INTEGER NOT NULL,     -- bytes of name=value as sent
    party     TEXT NOT NULL,        -- 'first' | 'third'
    source    TEXT NOT NULL,        -- 'response' | 'script'
    extra     TEXT                  -- other attributes, e.g. 'Partitioned; Priority=High'
);
CREATE INDEX cookies_url ON cookies(url_id);
CREATE INDEX cookies_key ON cookies(name, domain, path);
```

No value column, by design (FR-025). A file without the table shows "This crawl was made before the cookie audit" in the Cookies view (results `Need::Cookies`, detected from the table's presence like the other needs).

The `Set-Cookie` scrub of stored headers is its own step in PR #51; both steps take numbers at merge time.

## Results (`crates/results/src/cookies.rs`)

```text
CookieRow     name, domain, path, session: bool, expires, secure, http_only, same_site, party, source, size, urls: u64
CookieFilter  all | session | persistent | third_party | missing_secure | missing_http_only | same_site_none
```

`missing_secure` counts only cookies set over https. Sort by any column; default `urls` descending.
