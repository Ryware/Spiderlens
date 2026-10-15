//! How a crawl reaches sites (spec 025 plan D1): the HTTP client and the
//! cookie jar, and later the sign-ins and the proxy. The crawl runner,
//! robots.txt and sitemap fetching, external link checks and the spec 014
//! render stage all take their network from one [`NetSession`].

pub mod jar;

use std::sync::Arc;

use reqwest::Client;
use spiderlens_config::{AccessSettings, InternalHosts};
use url::Url;

use crate::fetch::{client_builder, finish};
use crate::{CheckError, CheckOptions};

pub use jar::CrawlJar;

pub struct NetSession {
    client: Client,
    jar: Option<Arc<CrawlJar>>,
}

impl NetSession {
    /// With default access settings this builds the same client as before
    /// spec 025: no cookie provider, direct connections.
    pub fn new(
        opts: &CheckOptions,
        access: &AccessSettings,
        internal: InternalHosts,
        resolve: &[(String, std::net::SocketAddr)],
    ) -> Result<Self, CheckError> {
        let mut builder = client_builder(opts, resolve);
        let jar = CrawlJar::needed(access.cookie_policy, &access.cookies).then(|| {
            let mut jar = CrawlJar::new(access.cookie_policy, internal);
            jar.supply(&access.cookies);
            Arc::new(jar)
        });
        if let Some(jar) = &jar {
            builder = builder.cookie_provider(Arc::clone(jar));
        }
        Ok(Self {
            client: finish(builder)?,
            jar,
        })
    }

    pub fn client(&self) -> &Client {
        &self.client
    }

    pub fn jar(&self) -> Option<&CrawlJar> {
        self.jar.as_deref()
    }

    /// The start URL redirected to another host of the same site, which
    /// becomes internal (spec 002 FR-003).
    pub fn adopt_host(&self, url: &Url) {
        if let (Some(jar), Some(host)) = (&self.jar, url.host_str()) {
            jar.adopt_host(host);
        }
    }
}
