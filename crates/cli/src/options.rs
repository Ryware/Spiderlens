//! Crawl options shared by `check`, `config` and (from spec 003) `crawl`.
//! Precedence: built-in defaults, then `--profile`, then flags (FR-036).

use std::path::PathBuf;

use clap::{Args, ValueEnum};
use spiderlens_config::{
    CrawlConfig, CustomHeader, HeaderTarget, HeaderValue, Mode, QueryMode, RobotsMode, SpeedSettings,
    SubdomainMode, UaPreset, import_profile,
};

#[derive(Args, Debug, Default)]
pub struct IdentityArgs {
    /// User agent preset.
    #[arg(long, value_enum, help_heading = "Identity")]
    pub user_agent: Option<UaArg>,
    /// Custom user agent string (implies --user-agent custom).
    #[arg(long, value_name = "STRING", help_heading = "Identity")]
    pub user_agent_string: Option<String>,
    /// robots.txt token for a custom user agent.
    #[arg(long, value_name = "TOKEN", help_heading = "Identity")]
    pub robots_token: Option<String>,
    /// Request header 'Name: value'. Repeatable.
    #[arg(long = "header", value_name = "'NAME: VALUE'", help_heading = "Identity")]
    pub headers: Vec<String>,
    /// Like --header, but the value is secret: never saved or printed.
    #[arg(
        long = "secret-header",
        value_name = "'NAME: VALUE'",
        help_heading = "Identity"
    )]
    pub secret_headers: Vec<String>,
    /// Send custom headers to every host, not only internal ones.
    #[arg(long, help_heading = "Identity")]
    pub header_all_hosts: bool,
}

#[derive(Args, Debug, Default)]
pub struct NetworkArgs {
    /// Connect time limit in seconds [default: 10].
    #[arg(long, value_name = "SECONDS", help_heading = "Network")]
    pub connect_timeout: Option<f64>,
    /// Time limit for each read, in seconds [default: 30].
    #[arg(long, value_name = "SECONDS", help_heading = "Network")]
    pub read_timeout: Option<f64>,
    /// Redirect hops followed per URL [default: 10].
    #[arg(long, value_name = "N", help_heading = "Network")]
    pub max_redirects: Option<u32>,
    /// Maximum response size in megabytes [default: 50].
    #[arg(long, value_name = "MB", help_heading = "Network")]
    pub max_size: Option<u64>,
}

#[derive(Args, Debug, Default)]
pub struct CrawlArgs {
    /// Exported profile file to start from.
    #[arg(long, value_name = "FILE")]
    pub profile: Option<PathBuf>,

    #[arg(long, value_name = "N", help_heading = "Limits")]
    pub max_urls: Option<u64>,
    #[arg(long, value_name = "N", help_heading = "Limits")]
    pub max_depth: Option<u32>,
    /// List mode: clicks to follow from listed URLs [default: 0].
    #[arg(long, value_name = "N", help_heading = "Limits")]
    pub list_depth: Option<u32>,

    /// Crawl only URLs matching this regex. Repeatable.
    #[arg(long, value_name = "REGEX", help_heading = "Scope")]
    pub include: Vec<String>,
    /// Never crawl URLs matching this regex. Repeatable.
    #[arg(long, value_name = "REGEX", help_heading = "Scope")]
    pub exclude: Vec<String>,
    #[arg(long, help_heading = "Scope")]
    pub all_subdomains: bool,
    #[arg(long, help_heading = "Scope")]
    pub no_external_check: bool,
    /// Follow nofollow links: internal (when no value), external or both.
    #[arg(
        long,
        value_enum,
        num_args = 0..=1,
        default_missing_value = "internal",
        value_name = "WHICH",
        help_heading = "Scope"
    )]
    pub follow_nofollow: Option<NofollowArg>,
    /// Only crawl URLs inside the start URL's folder.
    #[arg(long, help_heading = "Scope")]
    pub stay_in_folder: bool,
    /// Longer URLs are recorded as malformed [default: 2000].
    #[arg(long, value_name = "N", help_heading = "Scope")]
    pub max_url_length: Option<u32>,
    /// Query strings: keep as written, or remove all.
    #[arg(long, value_enum, help_heading = "Scope")]
    pub query_params: Option<QueryArg>,
    /// Remove this query parameter. Repeatable.
    #[arg(long = "remove-param", value_name = "NAME", help_heading = "Scope")]
    pub remove_params: Vec<String>,
    /// Sort query parameters by name.
    #[arg(long, help_heading = "Scope")]
    pub sort_params: bool,
    /// Read sitemaps and crawl the URLs only they list (the default; kept
    /// from spec 003).
    #[arg(long, help_heading = "Sitemaps")]
    pub use_sitemaps: bool,
    /// Do not read sitemaps.
    #[arg(long, conflicts_with = "use_sitemaps", help_heading = "Sitemaps")]
    pub no_sitemaps: bool,
    /// Read sitemaps but record URLs that only a sitemap lists as "Found only
    /// in sitemap" instead of crawling them.
    #[arg(long, conflicts_with = "no_sitemaps", help_heading = "Sitemaps")]
    pub no_crawl_sitemap_only: bool,
    /// Also read this sitemap. Repeatable.
    #[arg(long = "sitemap-url", value_name = "URL", help_heading = "Sitemaps")]
    pub sitemap_urls: Vec<String>,
    /// Also read this sitemap file, published at BASE-URL. Repeatable.
    #[arg(long = "sitemap-file", value_name = "PATH=BASE-URL", value_parser = parse_local_sitemap, help_heading = "Sitemaps")]
    pub sitemap_files: Vec<spiderlens_config::LocalSitemap>,

    #[command(flatten)]
    pub identity: IdentityArgs,

    #[arg(long, value_enum, help_heading = "Speed")]
    pub speed: Option<SpeedArg>,
    /// Maximum concurrent requests overall.
    #[arg(long, value_name = "N", help_heading = "Speed")]
    pub concurrency: Option<u32>,
    /// Maximum concurrent requests per site.
    #[arg(long, value_name = "N", help_heading = "Speed")]
    pub per_host: Option<u32>,
    /// Maximum URLs per second ("none" for no cap).
    #[arg(long, value_name = "N|none", help_heading = "Speed")]
    pub rate: Option<RateArg>,
    #[arg(long, help_heading = "Speed")]
    pub no_adaptive: bool,

    #[arg(long, value_enum, help_heading = "Robots")]
    pub robots: Option<RobotsArg>,
    #[arg(long, help_heading = "Robots")]
    pub ignore_crawl_delay: bool,

    #[command(flatten)]
    pub network: NetworkArgs,

    /// Total attempts per URL for temporary failures (1 = no retry).
    #[arg(long, value_name = "N", help_heading = "Retry")]
    pub attempts: Option<u32>,
    /// Maximum wait between attempts, in seconds.
    #[arg(long, value_name = "SECONDS", help_heading = "Retry")]
    pub max_wait: Option<f64>,
    #[arg(long, help_heading = "Retry")]
    pub no_retry_5xx: bool,
    #[arg(long, help_heading = "Retry")]
    pub no_retry_429: bool,

    /// Do not keep each page's raw HTML in the crawl file.
    #[arg(long, help_heading = "Extraction")]
    pub no_store_html: bool,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum UaArg {
    Spiderlens,
    GooglebotSmartphone,
    GooglebotDesktop,
    Bingbot,
    ChromeDesktop,
    ChromeMobile,
    Custom,
}

impl From<UaArg> for UaPreset {
    fn from(a: UaArg) -> Self {
        match a {
            UaArg::Spiderlens => Self::Spiderlens,
            UaArg::GooglebotSmartphone => Self::GooglebotSmartphone,
            UaArg::GooglebotDesktop => Self::GooglebotDesktop,
            UaArg::Bingbot => Self::Bingbot,
            UaArg::ChromeDesktop => Self::ChromeDesktop,
            UaArg::ChromeMobile => Self::ChromeMobile,
            UaArg::Custom => Self::Custom,
        }
    }
}

#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum NofollowArg {
    Internal,
    External,
    Both,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum QueryArg {
    Keep,
    RemoveAll,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum SpeedArg {
    Polite,
    Fast,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum RobotsArg {
    Respect,
    IgnoreReport,
    Ignore,
}

#[derive(Clone, Copy, Debug)]
pub struct RateArg(pub Option<f64>);

impl std::str::FromStr for RateArg {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, String> {
        if s.eq_ignore_ascii_case("none") {
            return Ok(Self(None));
        }
        s.parse::<f64>()
            .map(|v| Self(Some(v)))
            .map_err(|_| "expected a number or \"none\"".into())
    }
}

/// Result of applying options: the configuration and anything worth telling
/// the user (for example settings a newer profile had that we ignored).
pub struct Resolved {
    pub config: CrawlConfig,
    pub notices: Vec<String>,
}

fn parse_header(raw: &str) -> Result<(String, String), String> {
    let (name, value) = raw
        .split_once(':')
        .ok_or_else(|| format!("header \"{raw}\" must look like 'Name: value'"))?;
    Ok((name.trim().to_string(), value.trim().to_string()))
}

impl IdentityArgs {
    pub fn apply(&self, c: &mut CrawlConfig) -> Result<(), String> {
        if let Some(p) = self.user_agent {
            c.identity.user_agent.preset = p.into();
        }
        if let Some(s) = &self.user_agent_string {
            c.identity.user_agent.preset = UaPreset::Custom;
            c.identity.user_agent.custom_string = Some(s.clone());
        }
        if let Some(t) = &self.robots_token {
            c.identity.user_agent.robots_token = Some(t.clone());
        }
        let send_to = if self.header_all_hosts {
            HeaderTarget::All
        } else {
            HeaderTarget::Internal
        };
        for (raw, secret) in self
            .headers
            .iter()
            .map(|h| (h, false))
            .chain(self.secret_headers.iter().map(|h| (h, true)))
        {
            let (name, value) = parse_header(raw)?;
            c.identity.headers.push(CustomHeader {
                name,
                value: if secret {
                    HeaderValue::secret(value)
                } else {
                    HeaderValue::Plain(value)
                },
                send_to,
            });
        }
        if self.header_all_hosts {
            for h in &mut c.identity.headers {
                h.send_to = HeaderTarget::All;
            }
        }
        Ok(())
    }
}

impl NetworkArgs {
    pub fn apply(&self, c: &mut CrawlConfig) {
        if let Some(v) = self.connect_timeout {
            c.network.connect_timeout_secs = v;
        }
        if let Some(v) = self.read_timeout {
            c.network.read_timeout_secs = v;
        }
        if let Some(v) = self.max_redirects {
            c.network.max_redirects = v;
        }
        if let Some(v) = self.max_size {
            c.network.max_body_mb = v;
        }
    }
}

impl CrawlArgs {
    /// The speed options given on the command line applied to `speed`, or
    /// None when none was given (`crawl --resume`, spec 006 FR-011).
    pub fn speed_change(&self, speed: SpeedSettings) -> Option<SpeedSettings> {
        let given = self.speed.is_some()
            || self.concurrency.is_some()
            || self.per_host.is_some()
            || self.rate.is_some()
            || self.no_adaptive;
        if !given {
            return None;
        }
        let mut s = match self.speed {
            Some(SpeedArg::Polite) => SpeedSettings::POLITE,
            Some(SpeedArg::Fast) => SpeedSettings::FAST,
            None => speed,
        };
        if let Some(v) = self.concurrency {
            s.max_concurrent = v;
        }
        if let Some(v) = self.per_host {
            s.max_per_host = v;
        }
        if let Some(RateArg(v)) = self.rate {
            s.max_urls_per_sec = v;
        }
        if self.no_adaptive {
            s.adaptive = false;
        }
        Some(s)
    }

    pub fn resolve(&self, mode: Mode) -> Result<Resolved, String> {
        let mut notices = Vec::new();
        let mut c = match &self.profile {
            Some(path) => {
                let text = std::fs::read_to_string(path)
                    .map_err(|e| format!("cannot read profile {}: {e}", path.display()))?;
                let p = import_profile(&text).map_err(|e| format!("{}: {e}", path.display()))?;
                if !p.ignored.is_empty() {
                    notices.push(format!(
                        "profile {} has settings this version does not know; ignored: {}",
                        path.display(),
                        p.ignored.join(", ")
                    ));
                }
                p.config
            }
            None => CrawlConfig::default(),
        };
        c.mode = mode;

        if let Some(v) = self.max_urls {
            c.limits.max_urls = Some(v);
        }
        if let Some(v) = self.max_depth {
            c.limits.max_depth = Some(v);
        }
        c.scope.include.extend(self.include.iter().cloned());
        c.scope.exclude.extend(self.exclude.iter().cloned());
        if self.all_subdomains {
            c.scope.subdomains = SubdomainMode::AllSubdomains;
        }
        if self.no_external_check {
            c.scope.check_external = false;
        }
        if let Some(v) = self.list_depth {
            c.limits.list_depth = v;
        }
        match self.follow_nofollow {
            Some(NofollowArg::Internal) => c.scope.follow_internal_nofollow = true,
            Some(NofollowArg::External) => c.scope.follow_external_nofollow = true,
            Some(NofollowArg::Both) => {
                c.scope.follow_internal_nofollow = true;
                c.scope.follow_external_nofollow = true;
            }
            None => {}
        }
        if self.stay_in_folder {
            c.scope.stay_in_folder = true;
        }
        if let Some(v) = self.max_url_length {
            c.scope.max_url_length = v;
        }
        match self.query_params {
            Some(QueryArg::Keep) => c.scope.query_params.mode = QueryMode::Keep,
            Some(QueryArg::RemoveAll) => c.scope.query_params.mode = QueryMode::RemoveAll,
            None => {}
        }
        if !self.remove_params.is_empty() {
            c.scope.query_params.mode = QueryMode::Remove;
            c.scope
                .query_params
                .names
                .extend(self.remove_params.iter().cloned());
        }
        if self.sort_params {
            c.scope.query_params.sort = true;
        }
        let sm = &mut c.scope.sitemaps;
        if self.use_sitemaps {
            sm.read = true;
            sm.crawl_sitemap_only = true;
        }
        if self.no_sitemaps {
            sm.read = false;
            sm.crawl_sitemap_only = false;
        }
        if self.no_crawl_sitemap_only {
            sm.read = true;
            sm.crawl_sitemap_only = false;
        }
        sm.urls.extend(self.sitemap_urls.iter().cloned());
        sm.files.extend(self.sitemap_files.iter().cloned());

        self.identity.apply(&mut c)?;

        if let Some(s) = self.speed {
            c.speed = match s {
                SpeedArg::Polite => SpeedSettings::POLITE,
                SpeedArg::Fast => SpeedSettings::FAST,
            };
        }
        if let Some(v) = self.concurrency {
            c.speed.max_concurrent = v;
        }
        if let Some(v) = self.per_host {
            c.speed.max_per_host = v;
        }
        if let Some(RateArg(v)) = self.rate {
            c.speed.max_urls_per_sec = v;
        }
        if self.no_adaptive {
            c.speed.adaptive = false;
        }

        if let Some(r) = self.robots {
            c.robots.mode = match r {
                RobotsArg::Respect => RobotsMode::Respect,
                RobotsArg::IgnoreReport => RobotsMode::IgnoreReport,
                RobotsArg::Ignore => RobotsMode::Ignore,
            };
        }
        if self.ignore_crawl_delay {
            c.robots.honour_crawl_delay = false;
        }

        self.network.apply(&mut c);

        if let Some(v) = self.attempts {
            c.retry.attempts = v;
        }
        if let Some(v) = self.max_wait {
            c.retry.max_wait_secs = v;
        }
        if self.no_retry_5xx {
            c.retry.retry_5xx = false;
        }
        if self.no_retry_429 {
            c.retry.retry_429 = false;
        }
        if self.no_store_html {
            c.extraction.store_html = false;
        }
        Ok(Resolved { config: c, notices })
    }
}

/// `PATH=BASE-URL` for `--sitemap-file`. The split is at the last `=` that
/// is followed by `http`, so paths may contain `=`.
fn parse_local_sitemap(s: &str) -> Result<spiderlens_config::LocalSitemap, String> {
    let at = s
        .rmatch_indices('=')
        .map(|(i, _)| i)
        .find(|&i| s[i + 1..].starts_with("http"))
        .ok_or_else(|| {
            "expected PATH=BASE-URL, for example sitemap.xml=https://example.com/sitemap.xml".to_string()
        })?;
    Ok(spiderlens_config::LocalSitemap {
        path: s[..at].to_string(),
        base_url: s[at + 1..].to_string(),
    })
}
