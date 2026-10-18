//! Crawl options shared by `check`, `config` and (from spec 003) `crawl`.
//! Precedence: built-in defaults, then `--profile`, then flags (FR-036).

use std::path::PathBuf;

use clap::{Args, ValueEnum};
use spiderlens_config::{
    AuthMethod, CheckCondition, CookiePolicy, CrawlConfig, CustomHeader, HeaderTarget, HeaderValue,
    LoginSettings, Mode, ProxyKind, ProxyMode, ProxySettings, QueryMode, RobotsMode, Secret, SignIn,
    SignedOutCheck, SpeedSettings, SubdomainMode, SuppliedCookie, UaPreset, import_profile,
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

/// Cookies sent to the crawl's internal hosts (spec 025). Secret values
/// come from the environment, never from the command line, where other
/// programs could read them.
#[derive(Args, Debug, Default)]
pub struct AccessArgs {
    /// A cookie whose value is secret, read from SPIDERLENS_COOKIE_<NAME>.
    /// Repeatable.
    #[arg(long = "cookie", value_name = "NAME@DOMAIN[/PATH]", help_heading = "Cookies")]
    pub cookies: Vec<String>,
    /// A cookie with a non-secret value (a consent flag, a test variant).
    /// Repeatable.
    #[arg(
        long = "cookie-plain",
        value_name = "NAME=VALUE@DOMAIN[/PATH]",
        help_heading = "Cookies"
    )]
    pub plain_cookies: Vec<String>,
    /// What to do with cookies the site sets [default: ignore].
    #[arg(long, value_enum, help_heading = "Cookies")]
    pub cookie_policy: Option<CookiePolicyArg>,
    /// A browser's exported cookies.txt, used as a login session: held in
    /// memory for this run only, never stored.
    #[arg(long, value_name = "PATH", help_heading = "Login")]
    pub cookies_file: Option<PathBuf>,
    /// A sign-out link pattern the crawl never requests. Repeatable.
    #[arg(long = "sign-out-pattern", value_name = "TEXT", help_heading = "Login")]
    pub sign_out_patterns: Vec<String>,
    /// Drop the built-in sign-out patterns (logout, sign-out, ...).
    #[arg(long, help_heading = "Login")]
    pub no_default_sign_out_patterns: bool,
    /// A URL fetched now and then to notice that the session ended.
    #[arg(long, value_name = "URL", requires = "when", help_heading = "Login")]
    pub signed_out_check: Option<String>,
    /// When the check URL means signed out: redirects-to:TEXT, status:N or
    /// contains:TEXT.
    #[arg(
        long,
        value_name = "CONDITION",
        requires = "signed_out_check",
        help_heading = "Login"
    )]
    pub when: Option<String>,
    /// A site sign-in; the password comes from SPIDERLENS_PASSWORD_<HOST>.
    /// Repeatable.
    #[arg(long = "auth", value_name = "ORIGIN=USER", help_heading = "Sign-in")]
    pub sign_ins: Vec<String>,
    /// Restricts a sign-in's method. Repeatable.
    #[arg(
        long = "auth-method",
        value_name = "ORIGIN=basic|digest|ntlm",
        help_heading = "Sign-in"
    )]
    pub auth_methods: Vec<String>,
    /// Allow a sign-in on an http:// origin, where the password is sent
    /// unencrypted.
    #[arg(long, help_heading = "Sign-in")]
    pub allow_insecure_auth: bool,
    /// The proxy: http://, https:// or socks5://host:port, user in the URL;
    /// the password comes from SPIDERLENS_PROXY_PASSWORD.
    #[arg(
        long,
        value_name = "URL",
        conflicts_with = "proxy_system",
        help_heading = "Proxy"
    )]
    pub proxy: Option<String>,
    /// Use the operating system's proxy settings.
    #[arg(long, help_heading = "Proxy")]
    pub proxy_system: bool,
    /// Hosts reached without the proxy, comma-separated.
    #[arg(long, value_name = "HOSTS", help_heading = "Proxy")]
    pub no_proxy_for: Option<String>,
}

/// `ORIGIN=VALUE` with the origin reduced to scheme, host and port.
fn origin_pair<'a>(raw: &'a str, flag: &str, what: &str) -> Result<(String, &'a str), String> {
    let (origin, value) = raw
        .rsplit_once('=')
        .ok_or_else(|| format!("{flag} takes ORIGIN={what}, got '{raw}'"))?;
    let url = reqwest::Url::parse(origin.trim())
        .map_err(|_| format!("{flag}: '{origin}' is not a full address such as https://example.com"))?;
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
        return Err(format!("{flag}: '{origin}' must start with http:// or https://"));
    }
    Ok((url.origin().ascii_serialization(), value.trim()))
}

/// `redirects-to:TEXT`, `status:N` or `contains:TEXT`.
fn check_condition(raw: &str) -> Result<CheckCondition, String> {
    let bad = || format!("--when takes redirects-to:TEXT, status:N or contains:TEXT, got '{raw}'");
    let (kind, value) = raw.split_once(':').ok_or_else(bad)?;
    match kind.trim().to_ascii_lowercase().as_str() {
        "redirects-to" if !value.is_empty() => Ok(CheckCondition::RedirectsTo(value.to_string())),
        "contains" if !value.is_empty() => Ok(CheckCondition::Contains(value.to_string())),
        "status" => value
            .trim()
            .parse()
            .map(CheckCondition::Status)
            .map_err(|_| bad()),
        _ => Err(bad()),
    }
}

/// `--proxy URL` into manual proxy settings. A password in the URL is
/// refused: secrets are never plain arguments.
fn proxy_from_url(raw: &str) -> Result<ProxySettings, String> {
    let with_scheme = if raw.contains("://") {
        raw.trim().to_string()
    } else {
        format!("http://{}", raw.trim())
    };
    let url =
        reqwest::Url::parse(&with_scheme).map_err(|_| format!("--proxy: '{raw}' is not a proxy address"))?;
    let kind = match url.scheme() {
        "http" => ProxyKind::Http,
        "https" => ProxyKind::Https,
        "socks5" | "socks5h" => ProxyKind::Socks5,
        other => {
            return Err(format!(
                "--proxy: {other}:// is not supported; use http, https or socks5"
            ));
        }
    };
    if url.password().is_some() {
        return Err("--proxy: put the password in SPIDERLENS_PROXY_PASSWORD, not in the URL".into());
    }
    let host = url
        .host_str()
        .ok_or_else(|| format!("--proxy: '{raw}' has no host"))?
        .trim_start_matches('[')
        .trim_end_matches(']')
        .to_string();
    let port = url.port().unwrap_or(match kind {
        ProxyKind::Http => 80,
        ProxyKind::Https => 443,
        ProxyKind::Socks5 => 1080,
    });
    let user = (!url.username().is_empty()).then(|| url.username().to_string());
    let password = match &user {
        Some(_) => Secret::new(
            std::env::var(CrawlConfig::secret_env_var("proxy"))
                .map_err(|_| "set SPIDERLENS_PROXY_PASSWORD to the proxy's password".to_string())?,
        ),
        None => Secret::default(),
    };
    Ok(ProxySettings {
        mode: ProxyMode::Manual,
        kind,
        host,
        port,
        user,
        password,
        bypass: Vec::new(),
    })
}

#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum CookiePolicyArg {
    /// Do not send them back, as search engines do.
    Ignore,
    /// Keep them in memory for the rest of the crawl.
    Keep,
}

/// `SPIDERLENS_COOKIE_SID` for a cookie named `sid`: upper case, and any
/// character other than a letter or digit becomes `_`.
pub fn cookie_env(name: &str) -> String {
    let tail: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_uppercase()
            } else {
                '_'
            }
        })
        .collect();
    format!("SPIDERLENS_COOKIE_{tail}")
}

/// `NAME@DOMAIN[/PATH]` into name, domain and path.
fn cookie_target(raw: &str) -> Result<(String, String, String), String> {
    let (name, rest) = raw
        .rsplit_once('@')
        .ok_or_else(|| format!("--cookie takes NAME@DOMAIN[/PATH], got '{raw}'"))?;
    let (domain, path) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, "/"),
    };
    if name.trim().is_empty() || domain.trim().is_empty() {
        return Err(format!("--cookie takes NAME@DOMAIN[/PATH], got '{raw}'"));
    }
    Ok((
        name.trim().to_string(),
        domain.trim().to_string(),
        path.to_string(),
    ))
}

impl AccessArgs {
    pub fn apply(&self, c: &mut CrawlConfig) -> Result<(), String> {
        for raw in &self.cookies {
            let (name, domain, path) = cookie_target(raw)?;
            let var = cookie_env(&name);
            let value =
                std::env::var(&var).map_err(|_| format!("set {var} to the value of cookie {name}"))?;
            c.access.cookies.push(SuppliedCookie {
                name,
                value: HeaderValue::secret(value),
                domain,
                path,
                secure_only: false,
            });
        }
        for raw in &self.plain_cookies {
            let (pair, target) = raw
                .rsplit_once('@')
                .ok_or_else(|| format!("--cookie-plain takes NAME=VALUE@DOMAIN[/PATH], got '{raw}'"))?;
            let (name, value) = pair
                .split_once('=')
                .ok_or_else(|| format!("--cookie-plain takes NAME=VALUE@DOMAIN[/PATH], got '{raw}'"))?;
            let (name, domain, path) = cookie_target(&format!("{name}@{target}"))?;
            c.access.cookies.push(SuppliedCookie {
                name,
                value: HeaderValue::Plain(value.to_string()),
                domain,
                path,
                secure_only: false,
            });
        }
        if let Some(p) = self.cookie_policy {
            c.access.cookie_policy = match p {
                CookiePolicyArg::Ignore => CookiePolicy::Ignore,
                CookiePolicyArg::Keep => CookiePolicy::Keep,
            };
        }
        self.apply_sign_ins(c)?;
        self.apply_login(c)?;
        self.apply_proxy(c)
    }

    fn apply_sign_ins(&self, c: &mut CrawlConfig) -> Result<(), String> {
        for raw in &self.sign_ins {
            let (origin, user) = origin_pair(raw, "--auth", "USER")?;
            if user.is_empty() {
                return Err(format!("--auth takes ORIGIN=USER, got '{raw}'"));
            }
            if origin.starts_with("http://") && !self.allow_insecure_auth {
                return Err(format!(
                    "{origin} is not encrypted, so the password would be readable on the way; \
                     add --allow-insecure-auth to sign in anyway"
                ));
            }
            let var = CrawlConfig::secret_env_var(&format!("sign_in:{origin}"));
            let password =
                std::env::var(&var).map_err(|_| format!("set {var} to the password for {origin}"))?;
            c.access
                .sign_ins
                .retain(|s| s.origin_key().as_deref() != Some(origin.as_str()));
            c.access.sign_ins.push(SignIn {
                origin,
                user: user.to_string(),
                password: Secret::new(password),
                method: AuthMethod::Any,
            });
        }
        for raw in &self.auth_methods {
            let (origin, m) = origin_pair(raw, "--auth-method", "basic|digest|ntlm")?;
            let method = AuthMethod::parse(m)
                .ok_or_else(|| format!("--auth-method: '{m}' is not basic, digest or ntlm"))?;
            let s = c
                .access
                .sign_ins
                .iter_mut()
                .find(|s| s.origin_key().as_deref() == Some(origin.as_str()))
                .ok_or_else(|| format!("--auth-method: add --auth {origin}=USER first"))?;
            s.method = method;
        }
        Ok(())
    }

    fn apply_login(&self, c: &mut CrawlConfig) -> Result<(), String> {
        // A cookies file alone needs no stored login settings: the crawl
        // skips the default sign-out links for any login session.
        let wants_login = !self.sign_out_patterns.is_empty()
            || self.no_default_sign_out_patterns
            || self.signed_out_check.is_some();
        if !wants_login {
            return Ok(());
        }
        if self.cookies_file.is_none() && c.access.login.is_none() {
            return Err("--sign-out-pattern and --signed-out-check need --cookies-file".into());
        }
        // The stored login page must be a web address, never the cookies
        // file's path (spec 025 FR-015): the profile's, else the check URL.
        let source = c
            .access
            .login
            .as_ref()
            .map(|l| l.login_url.clone())
            .or_else(|| self.signed_out_check.as_ref().map(|u| u.trim().to_string()))
            .ok_or("--sign-out-pattern with --cookies-file needs --signed-out-check or a profile with a login page")?;
        let login = c.access.login.get_or_insert_with(|| LoginSettings::new(source));
        if self.no_default_sign_out_patterns {
            login.sign_out_patterns.clear();
        }
        for p in &self.sign_out_patterns {
            if !p.trim().is_empty()
                && !login
                    .sign_out_patterns
                    .iter()
                    .any(|q| q.eq_ignore_ascii_case(p.trim()))
            {
                login.sign_out_patterns.push(p.trim().to_string());
            }
        }
        if let (Some(url), Some(when)) = (&self.signed_out_check, &self.when) {
            login.check = Some(SignedOutCheck {
                url: url.trim().to_string(),
                when: check_condition(when)?,
            });
        }
        Ok(())
    }

    fn apply_proxy(&self, c: &mut CrawlConfig) -> Result<(), String> {
        if let Some(raw) = &self.proxy {
            c.access.proxy = proxy_from_url(raw)?;
        } else if self.proxy_system {
            c.access.proxy = ProxySettings {
                mode: ProxyMode::System,
                ..ProxySettings::default()
            };
        }
        if let Some(list) = &self.no_proxy_for {
            c.access.proxy.bypass = list
                .split(',')
                .map(str::trim)
                .filter(|h| !h.is_empty())
                .map(str::to_string)
                .collect();
        }
        Ok(())
    }
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

    #[command(flatten)]
    pub access: AccessArgs,

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
    /// Wait before the first retry, in seconds; doubles for each next one.
    #[arg(long, value_name = "SECONDS", help_heading = "Retry")]
    pub retry_base_wait: Option<f64>,
    /// Temporary failures in a row that pause a host.
    #[arg(long, value_name = "N", help_heading = "Retry")]
    pub host_pause_after: Option<u32>,
    /// First pause of a failing host, in seconds.
    #[arg(long, value_name = "SECONDS", help_heading = "Retry")]
    pub host_pause: Option<f64>,
    /// Longest pause of a failing host, in seconds.
    #[arg(long, value_name = "SECONDS", help_heading = "Retry")]
    pub host_pause_max: Option<f64>,
    /// Failed checks in a row after which a host is given up.
    #[arg(long, value_name = "N", help_heading = "Retry")]
    pub host_give_up_after: Option<u32>,

    /// Do not keep each page's raw HTML in the crawl file.
    #[arg(long, help_heading = "Extraction")]
    pub no_store_html: bool,
    /// Follow amphtml links and check AMP pages (core subset of AMP's rules).
    #[arg(long, help_heading = "Extraction")]
    pub check_amp: bool,

    /// Download and read PDFs: properties, text and links.
    #[arg(long, help_heading = "PDF")]
    pub crawl_pdfs: bool,
    /// Store links found in PDFs without crawling them.
    #[arg(long, help_heading = "PDF")]
    pub no_follow_pdf_links: bool,
    /// PDF size limit in MB (default 20).
    #[arg(long, value_name = "MB", help_heading = "PDF")]
    pub pdf_max_size: Option<u32>,
    /// Time limit for reading one PDF, in seconds (default 5).
    #[arg(long, value_name = "SECONDS", help_heading = "PDF")]
    pub pdf_time_limit: Option<u32>,
    /// Custom extraction and search rules (a rule file). Rules with the
    /// profile's names replace them.
    #[arg(long, value_name = "FILE", help_heading = "Custom rules")]
    pub rules: Option<PathBuf>,
    /// Do not run the AI search checks when the crawl finishes.
    #[arg(long, help_heading = "AI search")]
    pub no_ai_checks: bool,
    /// Do not fetch llms.txt, llms-full.txt and tdmrep.json per site.
    #[arg(long, help_heading = "AI search")]
    pub no_site_files: bool,

    /// Render pages in a headless Chromium browser and store the rendered
    /// version next to the raw HTML (spec 014).
    #[arg(long, help_heading = "Rendering")]
    pub render: bool,
    #[command(flatten)]
    pub rendering: RenderingArgs,

    /// Accessibility checks: off, static (from the HTML) or full (also in the
    /// rendered page; needs --render). Default static.
    #[arg(long, value_enum, value_name = "LEVEL", help_heading = "Accessibility")]
    pub accessibility: Option<A11yArg>,
    /// WCAG version and level to count against, such as 2.2-aa (the default).
    #[arg(
        long,
        value_name = "TARGET",
        value_parser = crate::accessibility_cmd::parse_wcag,
        help_heading = "Accessibility"
    )]
    pub wcag: Option<spiderlens_config::WcagTarget>,
    /// Leave best practice checks out of the accessibility results.
    #[arg(long, help_heading = "Accessibility")]
    pub no_a11y_best_practice: bool,
    /// Do not measure mobile usability.
    #[arg(long, help_heading = "Accessibility")]
    pub no_a11y_mobile: bool,
    /// Time limit for the full checks on one page, in seconds (1 to 30,
    /// default 5).
    #[arg(long, value_name = "SECONDS", help_heading = "Accessibility")]
    pub a11y_budget: Option<u32>,
}

/// The rendering settings flags other than `--render`: shared by `crawl`
/// (where they need `--render`) and `render FILE` (where rendering is the
/// point).
#[derive(Args, Debug, Default)]
pub struct RenderingArgs {
    /// Longest time one page may render, in seconds (1 to 60, default 10).
    #[arg(long, value_name = "SECONDS", help_heading = "Rendering")]
    pub render_timeout: Option<f64>,
    /// Seconds to wait before the page may count as settled (default 0).
    #[arg(long, value_name = "SECONDS", help_heading = "Rendering")]
    pub render_min_wait: Option<f64>,
    /// Window size: auto (the default), mobile, desktop or WIDTHxHEIGHT.
    #[arg(long, value_name = "SIZE", help_heading = "Rendering")]
    pub render_window: Option<WindowArg>,
    /// Resource types that are not loaded: a comma list of images, media,
    /// fonts, stylesheets and third-party, or none (default
    /// images,media,fonts).
    #[arg(
        long,
        value_enum,
        value_delimiter = ',',
        value_name = "TYPES",
        help_heading = "Rendering"
    )]
    pub render_block: Vec<BlockArg>,
    /// Render only pages whose address matches this regular expression.
    /// Repeatable.
    #[arg(long, value_name = "REGEX", help_heading = "Rendering")]
    pub render_include: Vec<String>,
    /// Do not render pages whose address matches this regular expression.
    /// Repeatable.
    #[arg(long, value_name = "REGEX", help_heading = "Rendering")]
    pub render_exclude: Vec<String>,
    /// Also render internal 4xx and 5xx HTML pages.
    #[arg(long, help_heading = "Rendering")]
    pub render_error_pages: bool,
    /// Pages rendered at once, 1 to 16, or auto (total memory in GB / 4).
    #[arg(long, value_name = "N|auto", help_heading = "Rendering")]
    pub render_tabs: Option<AutoNumber>,
    /// Browser memory budget in MB, or auto (total memory / 8).
    #[arg(long, value_name = "MB|auto", help_heading = "Rendering")]
    pub render_memory: Option<AutoNumber>,
    /// Store a screenshot of every rendered page.
    #[arg(long, help_heading = "Rendering")]
    pub screenshots: bool,
    /// When a page "contains JavaScript content": share of new text in
    /// percent, then words added (default 20,50).
    #[arg(long, value_name = "PERCENT,WORDS", help_heading = "Rendering")]
    pub js_content_threshold: Option<ContentThreshold>,
    /// Share of text hidden from crawlers that do not render, in percent
    /// (default 50).
    #[arg(long, value_name = "PERCENT", help_heading = "Rendering")]
    pub js_hidden_threshold: Option<u32>,
}

impl RenderingArgs {
    /// The names of the flags that were given.
    pub fn given(&self) -> Vec<&'static str> {
        [
            ("--render-timeout", self.render_timeout.is_some()),
            ("--render-min-wait", self.render_min_wait.is_some()),
            ("--render-window", self.render_window.is_some()),
            ("--render-block", !self.render_block.is_empty()),
            ("--render-include", !self.render_include.is_empty()),
            ("--render-exclude", !self.render_exclude.is_empty()),
            ("--render-error-pages", self.render_error_pages),
            ("--render-tabs", self.render_tabs.is_some()),
            ("--render-memory", self.render_memory.is_some()),
            ("--screenshots", self.screenshots),
            ("--js-content-threshold", self.js_content_threshold.is_some()),
            ("--js-hidden-threshold", self.js_hidden_threshold.is_some()),
        ]
        .iter()
        .filter(|(_, on)| *on)
        .map(|(n, _)| *n)
        .collect()
    }

    /// Lays the flags over rendering settings (spec 014 contracts/cli.md).
    pub fn apply(&self, r: &mut spiderlens_config::RenderSettings) -> Result<(), String> {
        if let Some(v) = self.render_timeout {
            r.max_render_secs = v;
        }
        if let Some(v) = self.render_min_wait {
            r.min_wait_secs = v;
        }
        if let Some(w) = self.render_window {
            r.window = w.0;
        }
        if !self.render_block.is_empty() {
            let has = |b: BlockArg| self.render_block.contains(&b);
            if has(BlockArg::None) && self.render_block.len() > 1 {
                return Err("--render-block none cannot be combined with other types".into());
            }
            r.block = spiderlens_config::RenderBlock {
                images: has(BlockArg::Images),
                media: has(BlockArg::Media),
                fonts: has(BlockArg::Fonts),
                stylesheets: has(BlockArg::Stylesheets),
                third_party: has(BlockArg::ThirdParty),
            };
        }
        r.include.extend(self.render_include.iter().cloned());
        r.exclude.extend(self.render_exclude.iter().cloned());
        if self.render_error_pages {
            r.render_error_pages = true;
        }
        if let Some(v) = self.render_tabs {
            r.concurrency = v.0;
        }
        if let Some(v) = self.render_memory {
            r.memory_budget_mb = v.0;
        }
        if self.screenshots {
            r.screenshots = true;
        }
        if let Some(t) = self.js_content_threshold {
            r.js_content_min_percent = t.percent;
            r.js_content_min_words = t.words;
        }
        if let Some(v) = self.js_hidden_threshold {
            r.hidden_content_percent = v;
        }
        Ok(())
    }
}

/// `--render-window`: `auto`, `mobile`, `desktop` or `WIDTHxHEIGHT`.
#[derive(Clone, Copy, Debug)]
pub struct WindowArg(pub spiderlens_config::RenderWindow);

impl std::str::FromStr for WindowArg {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, String> {
        use spiderlens_config::{RenderWindow, WindowPreset};
        let preset = |preset| {
            Ok(Self(RenderWindow {
                preset,
                width: None,
                height: None,
            }))
        };
        match s.trim().to_ascii_lowercase().as_str() {
            "auto" => preset(WindowPreset::Auto),
            "mobile" => preset(WindowPreset::Mobile),
            "desktop" => preset(WindowPreset::Desktop),
            other => {
                let bad = || format!("{s:?} is not a window; use auto, mobile, desktop or WIDTHxHEIGHT");
                let (w, h) = other.split_once('x').ok_or_else(bad)?;
                Ok(Self(RenderWindow {
                    preset: WindowPreset::Custom,
                    width: Some(w.trim().parse().map_err(|_| bad())?),
                    height: Some(h.trim().parse().map_err(|_| bad())?),
                }))
            }
        }
    }
}

/// `--render-block` items.
#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum BlockArg {
    Images,
    Media,
    Fonts,
    Stylesheets,
    ThirdParty,
    None,
}

/// `--render-tabs` and `--render-memory`: a number or `auto` (stored as
/// no value).
#[derive(Clone, Copy, Debug)]
pub struct AutoNumber(pub Option<u32>);

impl std::str::FromStr for AutoNumber {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, String> {
        if s.trim().eq_ignore_ascii_case("auto") {
            return Ok(Self(None));
        }
        s.trim()
            .parse::<u32>()
            .map(|v| Self(Some(v)))
            .map_err(|_| "expected a whole number or \"auto\"".into())
    }
}

/// `--js-content-threshold PERCENT,WORDS`.
#[derive(Clone, Copy, Debug)]
pub struct ContentThreshold {
    pub percent: u32,
    pub words: u32,
}

impl std::str::FromStr for ContentThreshold {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, String> {
        let bad = || format!("{s:?} is not PERCENT,WORDS, for example 20,50");
        let (p, w) = s.split_once(',').ok_or_else(bad)?;
        Ok(Self {
            percent: p.trim().parse().map_err(|_| bad())?,
            words: w.trim().parse().map_err(|_| bad())?,
        })
    }
}

#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum A11yArg {
    Off,
    Static,
    Full,
}

impl From<A11yArg> for spiderlens_config::A11yLevel {
    fn from(a: A11yArg) -> Self {
        match a {
            A11yArg::Off => Self::Off,
            A11yArg::Static => Self::Static,
            A11yArg::Full => Self::Full,
        }
    }
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

    /// The rendering flags other than `--render`. Given without `--render`
    /// they are an error, so a typo cannot quietly produce a raw crawl.
    fn apply_rendering(&self, c: &mut CrawlConfig) -> Result<(), String> {
        let names = self.rendering.given();
        if names.is_empty() {
            return Ok(());
        }
        // A profile that already turns rendering on counts.
        if !c.rendering.is_on() {
            return Err(format!("{} need --render", names.join(", ")));
        }
        self.rendering.apply(&mut c.rendering)
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
        self.access.apply(&mut c)?;

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
        if let Some(v) = self.retry_base_wait {
            c.retry.base_wait_secs = v;
        }
        if let Some(v) = self.host_pause_after {
            c.retry.host_pause_after = v;
        }
        if let Some(v) = self.host_pause {
            c.retry.host_pause_secs = v;
        }
        if let Some(v) = self.host_pause_max {
            c.retry.host_pause_max_secs = v;
        }
        if let Some(v) = self.host_give_up_after {
            c.retry.host_give_up_after = v;
        }
        if self.check_amp {
            c.extraction.check_amp = true;
        }
        if self.no_ai_checks {
            c.ai_checks.auto = false;
        }
        if self.no_site_files {
            c.ai_checks.site_files = false;
        }
        if self.no_store_html {
            c.extraction.store_html = false;
        }
        if self.crawl_pdfs {
            c.extraction.pdf.crawl = true;
        }
        if self.no_follow_pdf_links {
            c.extraction.pdf.follow_links = false;
        }
        if let Some(v) = self.pdf_max_size {
            c.extraction.pdf.max_mb = v;
        }
        if let Some(v) = self.pdf_time_limit {
            c.extraction.pdf.time_limit_secs = v;
        }
        if let Some(path) = &self.rules {
            let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
            let more =
                spiderlens_config::import_rules(&text).map_err(|e| format!("{}: {e}", path.display()))?;
            spiderlens_config::merge_rules(&mut c.custom, more);
        }
        if self.render {
            c.rendering.mode = spiderlens_config::RenderMode::Javascript;
        }
        self.apply_rendering(&mut c)?;
        if let Some(level) = self.accessibility {
            c.accessibility.level = level.into();
        }
        if let Some(target) = &self.wcag {
            c.accessibility.wcag = target.clone();
        }
        if self.no_a11y_best_practice {
            c.accessibility.best_practice = false;
        }
        if self.no_a11y_mobile {
            c.accessibility.mobile = false;
        }
        if let Some(secs) = self.a11y_budget {
            c.accessibility.budget_secs = secs;
        }
        if matches!(c.accessibility.level, spiderlens_config::A11yLevel::Full)
            && !matches!(c.rendering.mode, spiderlens_config::RenderMode::Javascript)
        {
            notices
                .push("full accessibility checks need JavaScript rendering; static checks will run".into());
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
