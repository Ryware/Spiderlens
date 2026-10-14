# Contract: `spiderlens_engine::net::NetSession`

Shared by the crawl runner, single-page check, robots.txt and sitemap fetching, external link checks, and the spec 014 render stage. No Tauri dependency.

```rust
pub struct NetSession { /* see plan D1 */ }

impl NetSession {
    /// Builds the session for one crawl. `secrets` are the values the user
    /// entered for `CrawlConfig::missing_secrets()` keys. With default
    /// settings this builds the same client as before spec 025.
    pub fn new(
        config: &CrawlConfig,
        secrets: &Secrets,
        login: Option<LoginSession>,
        internal: Arc<dyn Fn(&Url) -> bool + Send + Sync>,
        resolve: &[(String, SocketAddr)],
    ) -> Result<Self, NetError>;

    /// The client every request goes through (proxy and cookies included).
    pub fn client(&self) -> &reqwest::Client;

    /// Adds credentials for an origin's request: Basic or Digest header,
    /// or `None`. Cheap when there are no sign-ins.
    pub(crate) fn authorize(&self, req: RequestBuilder, url: &Url) -> RequestBuilder;

    /// Reads a 401 for the challenge, updates Digest state; says whether to
    /// retry once with credentials.
    pub(crate) fn on_unauthorized(&self, url: &Url, headers: &HeaderMap) -> AuthOutcome;

    /// NTLM origins are served through owned connections.
    pub(crate) fn ntlm_for(&self, url: &Url) -> Option<&NtlmPool>;

    /// Adds a sign-in while the crawl runs (Control::AddSignIn).
    pub fn add_sign_in(&self, sign_in: SignIn);

    /// Checks the proxy by fetching `probe` (the start URL's robots.txt).
    pub async fn check_proxy(&self, probe: &Url) -> Result<(), ProxyProblem>;

    pub fn proxy_in_effect(&self) -> &ProxyInEffect;

    // ---- for spec 014 rendering ----

    /// Cookies Chrome should hold before rendering `url`: the matching
    /// supplied and login cookies plus those kept under policy Keep.
    /// Values are exposed only through this call.
    pub fn browser_cookies(&self, url: &Url) -> Vec<BrowserCookie>;

    /// Proxy for a browser that makes its own connections (normally none:
    /// spec 014 answers every request through `client()`).
    pub fn browser_proxy(&self) -> Option<String>;

    /// Cookies a render left in the browser for `url`'s site that the
    /// response did not set; stored with source = 'script'.
    pub fn record_script_cookies(&self, url: &Url, cookies: &[BrowserCookie]);
}

pub struct BrowserCookie {
    pub name: String,
    pub value: SecretString,
    pub domain: String,
    pub path: String,
    pub secure: bool,
    pub http_only: bool,
    pub same_site: Option<SameSite>,
    pub expires: Option<Timestamp>,
}

pub enum ProxyProblem { Unreachable(String), Rejected, PacUnreadable(String), PacUnsupported }
pub enum AuthOutcome { NotAuth, RetryWithCredentials, Rejected { method: AuthMethod }, Required { schemes: Vec<String>, supported: bool } }
```

Rules both sides rely on:

- Nothing returned by `NetSession` is ever written to a crawl file except through `record_script_cookies` (attributes only, no values).
- A request for a non-internal URL never carries a sign-in or a cookie, whatever the caller does.
- Spec 014 renders with a clean browser state per page and calls `browser_cookies` to seed it (014 FR-011 as amended by 025 FR-035).
