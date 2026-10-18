//! How a crawl reaches sites (spec 025 plan D1): the HTTP client, the
//! cookie jar, the HTTP sign-ins, the login session and the proxy. The
//! crawl runner, robots.txt and sitemap fetching, external link checks and
//! the spec 014 render stage all take their network from one
//! [`NetSession`].

pub mod auth;
pub mod jar;
pub mod login;
pub(crate) mod ntlm;
mod pac;
pub mod proxy;

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex, PoisonError};

use reqwest::header::{AUTHORIZATION, HeaderMap};
use reqwest::{Client, RequestBuilder};
use spiderlens_config::{AccessSettings, InternalHosts, SignIn};
use url::Url;

use crate::fetch::{client_builder, finish};
use crate::{CheckError, CheckOptions, ErrorClass};

pub use auth::AuthOutcome;
pub use jar::CrawlJar;
pub use login::{ImportCounts, LoginSession, SessionCookie};
pub use proxy::{ProxyInEffect, ProxyProblem};

/// What sign-in did for one response, recorded with the URL (FR-004,
/// FR-005).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthNote {
    /// 401 and no sign-in answers it: the schemes the server offered.
    Required { schemes: Vec<String>, supported: bool },
    /// 401 although credentials were sent.
    Rejected { method: &'static str },
}

impl AuthNote {
    /// The `reason_detail` stored on the URL.
    pub fn detail(&self) -> String {
        match self {
            Self::Required { schemes, .. } => format!("auth_required:{}", schemes.join(",")),
            Self::Rejected { method } => format!("auth_rejected:{method}"),
        }
    }
}

pub struct NetSession {
    client: Client,
    jar: Option<Arc<CrawlJar>>,
    auth: auth::Auth,
    /// NTLM origins get their own connections, made on first use.
    ntlm: Mutex<HashMap<String, Arc<ntlm::NtlmPool>>>,
    proxy: Option<proxy::Resolved>,
    proxy_in_effect: ProxyInEffect,
    /// To build the NTLM connections like the main client.
    opts: CheckOptions,
    resolve: Vec<(String, SocketAddr)>,
    ntlm_connections: usize,
}

impl NetSession {
    /// With default access settings this builds the same client as before
    /// spec 025: no cookie provider, direct connections.
    pub fn new(
        opts: &CheckOptions,
        access: &AccessSettings,
        internal: InternalHosts,
        resolve: &[(String, SocketAddr)],
    ) -> Result<Self, CheckError> {
        Self::with_login(opts, access, internal, resolve, None)
    }

    /// [`Self::new`] with the login session the crawl signs in with.
    pub fn with_login(
        opts: &CheckOptions,
        access: &AccessSettings,
        internal: InternalHosts,
        resolve: &[(String, SocketAddr)],
        login: Option<&LoginSession>,
    ) -> Result<Self, CheckError> {
        let proxy = proxy::resolve(&access.proxy)
            .map_err(|p| CheckError::new(ErrorClass::Connection, p.message()))?;
        let proxy_in_effect = match &proxy {
            Some(r) => r.in_effect(access.proxy.mode),
            None if access.proxy.mode == spiderlens_config::ProxyMode::System => ProxyInEffect {
                mode: "system",
                ..ProxyInEffect::direct()
            },
            None => ProxyInEffect::direct(),
        };
        let builder = proxy::apply(client_builder(opts, resolve), proxy.as_ref())
            .map_err(|p| CheckError::new(ErrorClass::Connection, p.message()))?;
        let jar = (CrawlJar::needed(access.cookie_policy, &access.cookies)
            || login.is_some_and(|l| !l.cookies.is_empty()))
        .then(|| {
            let mut jar = CrawlJar::new(access.cookie_policy, internal);
            jar.supply(&access.cookies);
            if let Some(l) = login {
                jar.supply_session(&l.cookies);
            }
            Arc::new(jar)
        });
        let builder = match &jar {
            Some(jar) => builder.cookie_provider(Arc::clone(jar)),
            None => builder,
        };
        Ok(Self {
            client: finish(builder)?,
            jar,
            auth: auth::Auth::new(&access.sign_ins),
            ntlm: Mutex::new(HashMap::new()),
            proxy,
            proxy_in_effect,
            opts: opts.clone(),
            resolve: resolve.to_vec(),
            ntlm_connections: 4,
        })
    }

    /// NTLM connections per origin; the crawl's per-site limit (spec 008).
    pub fn set_ntlm_connections(&mut self, n: usize) {
        self.ntlm_connections = n.clamp(1, 32);
    }

    pub fn client(&self) -> &Client {
        &self.client
    }

    pub fn jar(&self) -> Option<&CrawlJar> {
        self.jar.as_deref()
    }

    /// The crawl's proxy as a browser context takes it, if it has a fixed
    /// one (spec 014 FR-011).
    pub fn browser_proxy(&self) -> Option<String> {
        self.proxy.as_ref().and_then(proxy::Resolved::browser_server)
    }

    pub fn proxy_in_effect(&self) -> &ProxyInEffect {
        &self.proxy_in_effect
    }

    /// The start URL redirected to another host of the same site, which
    /// becomes internal (spec 002 FR-003).
    pub fn adopt_host(&self, url: &Url) {
        if let (Some(jar), Some(host)) = (&self.jar, url.host_str()) {
            jar.adopt_host(host);
        }
    }

    /// Adds a sign-in while the crawl runs (FR-004). An origin that already
    /// has one gets the new one.
    pub fn add_sign_in(&self, sign_in: &SignIn) {
        self.auth.add(sign_in);
        if let Some(origin) = sign_in.origin_key() {
            self.ntlm
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .remove(&origin);
        }
    }

    /// Credentials for a request: the `Authorization` header when the
    /// origin settled on Basic or Digest. Internal URLs only.
    pub(crate) fn authorize(
        &self,
        request: RequestBuilder,
        url: &Url,
        internal: bool,
    ) -> (RequestBuilder, bool) {
        if !internal {
            return (request, false);
        }
        match self.auth.authorize(url) {
            Some(v) => (request.header(AUTHORIZATION, v), true),
            None => (request, false),
        }
    }

    /// Reads a 401 (see [`auth::Auth::on_unauthorized`]).
    pub(crate) fn on_unauthorized(&self, url: &Url, headers: &HeaderMap, sent: bool) -> AuthOutcome {
        self.auth.on_unauthorized(url, headers, sent)
    }

    /// The NTLM connections of `url`'s origin, when it settled on NTLM.
    pub(crate) fn ntlm_for(&self, url: &Url, internal: bool) -> Option<Arc<ntlm::NtlmPool>> {
        if !internal || !self.auth.is_ntlm(url) {
            return None;
        }
        let origin = auth::origin_of(url);
        let mut pools = self.ntlm.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(p) = pools.get(&origin) {
            return Some(Arc::clone(p));
        }
        let (user, password) = self.auth.ntlm_credentials(url)?;
        let clients: Vec<Client> = (0..self.ntlm_connections)
            .filter_map(|_| self.ntlm_client().ok())
            .collect();
        if clients.is_empty() {
            return None;
        }
        let pool = Arc::new(ntlm::NtlmPool::new(clients, user, password));
        pools.insert(origin, Arc::clone(&pool));
        Some(pool)
    }

    /// A client with one idle HTTP/1.1 connection and the same settings as
    /// the main one.
    fn ntlm_client(&self) -> Result<Client, CheckError> {
        let builder = proxy::apply(client_builder(&self.opts, &self.resolve), self.proxy.as_ref())
            .map_err(|p| CheckError::new(ErrorClass::Connection, p.message()))?
            .http1_only()
            .pool_max_idle_per_host(1);
        let builder = match &self.jar {
            Some(jar) => builder.cookie_provider(Arc::clone(jar)),
            None => builder,
        };
        finish(builder)
    }

    /// A client for requests that must carry no cookies, such as the agent
    /// checks after the last page (spec 016): the main client when it has
    /// no cookie jar, else one built the same way without it. Credentials
    /// are only ever added per request by [`Self::authorize`], never here.
    pub fn plain_client(&self) -> Result<Client, CheckError> {
        if self.jar.is_none() {
            return Ok(self.client.clone());
        }
        let builder = proxy::apply(client_builder(&self.opts, &self.resolve), self.proxy.as_ref())
            .map_err(|p| CheckError::new(ErrorClass::Connection, p.message()))?;
        finish(builder)
    }

    /// Checks the proxy by requesting `probe` (the start URL's robots.txt)
    /// through it (FR-022). Without a proxy there is nothing to check.
    pub async fn check_proxy(&self, probe: &Url) -> Result<(), ProxyProblem> {
        if self.proxy.is_none() {
            return Ok(());
        }
        proxy::check(&self.client, probe).await
    }
}

/// Checks proxy settings before a crawl, for the Test button and the CLI's
/// `--proxy-test` (FR-022): resolves them and requests `probe` through
/// the proxy. Returns the proxy in effect.
pub async fn test_proxy(
    access: &AccessSettings,
    opts: &CheckOptions,
    probe: &Url,
) -> Result<ProxyInEffect, ProxyProblem> {
    let resolved = proxy::resolve(&access.proxy)?;
    let Some(r) = resolved else {
        return Ok(if access.proxy.mode == spiderlens_config::ProxyMode::System {
            ProxyInEffect {
                mode: "system",
                ..ProxyInEffect::direct()
            }
        } else {
            ProxyInEffect::direct()
        });
    };
    let client = proxy::apply(client_builder(opts, &[]), Some(&r))
        .and_then(|b| finish(b).map_err(|e| ProxyProblem::Invalid(e.message)))?;
    proxy::check(&client, probe).await?;
    let mut shown = r.in_effect(access.proxy.mode);
    if let (Some(note), Some(script)) = (r.pac_note(probe), shown.host.clone()) {
        shown.host = Some(format!("{script} ({note})"));
    }
    Ok(shown)
}
