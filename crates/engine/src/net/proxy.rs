//! The proxy (spec 025 plan D6, research R6).
//!
//! "No proxy" keeps today's direct connections whatever the shell's
//! environment says (FR-024). "Manual" uses the settings as entered.
//! "System" is read once when the crawl starts and then used like a manual
//! proxy: environment variables first, then the operating system's
//! setting. When the system names an automatic configuration (PAC) script,
//! it is downloaded and run (see [`super::pac`]); finding the proxy by
//! network discovery (WPAD) is reported, not done.

use std::sync::Arc;
use std::time::Duration;

use reqwest::{ClientBuilder, NoProxy, Proxy, StatusCode};
use serde::Serialize;
use spiderlens_config::{ProxyKind, ProxyMode, ProxySettings};
use url::Url;

use super::pac::Pac;

/// The proxy a crawl used, stored in crawl meta as `proxy_in_effect`
/// (never the password).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProxyInEffect {
    /// `none`, `system` or `manual`.
    pub mode: &'static str,
    pub kind: Option<&'static str>,
    pub host: Option<String>,
    pub port: Option<u16>,
    pub user: Option<String>,
    /// `manual`, `env`, `os`, `pac` (the system's configuration script;
    /// `host` is then the script's address), or `none` when the system has
    /// no proxy.
    pub source: String,
    pub bypass: Vec<String>,
}

impl ProxyInEffect {
    pub fn direct() -> Self {
        Self {
            mode: "none",
            kind: None,
            host: None,
            port: None,
            user: None,
            source: "none".into(),
            bypass: Vec::new(),
        }
    }

    /// One line for the crawl information sheet and `spiderlens info`.
    pub fn describe(&self) -> String {
        if self.source == "pac"
            && let Some(script) = &self.host
        {
            return format!("automatic configuration script {script}");
        }
        match (&self.host, self.port) {
            (Some(h), Some(p)) => {
                let user = self.user.as_deref().map(|u| format!("{u}@")).unwrap_or_default();
                format!(
                    "{}://{user}{h}:{p} ({})",
                    self.kind.unwrap_or("http"),
                    if self.source == "manual" {
                        "entered"
                    } else {
                        "from the system settings"
                    }
                )
            }
            _ if self.mode == "system" => "No proxy (the system has none set)".into(),
            _ => "No proxy".into(),
        }
    }
}

/// Why the proxy cannot be used (FR-022).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "problem", content = "detail", rename_all = "snake_case")]
pub enum ProxyProblem {
    Unreachable(String),
    /// The proxy answered 407: it refused the user name or password.
    Rejected,
    PacUnreadable(String),
    /// The system finds its proxy by network discovery (WPAD), without a
    /// script address.
    PacUnsupported,
    /// The settings themselves are wrong (validation catches this first).
    Invalid(String),
}

impl ProxyProblem {
    pub fn message(&self) -> String {
        match self {
            Self::Unreachable(d) => format!("The proxy could not be reached: {d}"),
            Self::Rejected => "The proxy rejected the user name or password.".into(),
            Self::PacUnreadable(d) => format!("The automatic proxy configuration script could not be read: {d}"),
            Self::PacUnsupported => "The system finds its proxy automatically (WPAD), which SpiderLens cannot do. Enter the proxy manually.".into(),
            Self::Invalid(d) => format!("The proxy settings are not valid: {d}"),
        }
    }
}

/// A proxy resolved to one address.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Resolved {
    pub kind: ProxyKind,
    pub host: String,
    pub port: u16,
    pub user: Option<String>,
    pub password: Option<String>,
    pub bypass: Vec<String>,
    pub source: &'static str,
    /// The system's configuration script; then `host` is its address and
    /// `port` is 0, and the script picks the proxy for each request.
    pub pac: Option<Arc<Pac>>,
}

/// What the operating system's settings ask for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SystemProxy {
    Fixed(Resolved),
    /// The address of a proxy auto-configuration script.
    Pac(String),
}

impl Resolved {
    /// A proxy that a configuration script chooses per request.
    pub(crate) fn with_pac(location: &str, pac: Pac) -> Self {
        Self {
            kind: ProxyKind::Http,
            host: location.to_string(),
            port: 0,
            user: None,
            password: None,
            bypass: Vec::new(),
            source: "pac",
            pac: Some(Arc::new(pac)),
        }
    }

    /// For the proxy test: which proxy the script picks for `probe`.
    pub(crate) fn pac_note(&self, probe: &Url) -> Option<String> {
        let pac = self.pac.as_ref()?;
        let host = probe.host_str().unwrap_or_default();
        Some(match pac.lookup(probe) {
            Some(proxy) => format!("{host} goes through {proxy}"),
            None => format!("{host} goes direct"),
        })
    }

    fn url(&self) -> String {
        let host = if self.host.contains(':') && !self.host.starts_with('[') {
            format!("[{}]", self.host)
        } else {
            self.host.clone()
        };
        format!("{}://{host}:{}", self.kind.scheme(), self.port)
    }

    pub fn in_effect(&self, mode: ProxyMode) -> ProxyInEffect {
        if self.pac.is_some() {
            return ProxyInEffect {
                mode: mode_name(mode),
                kind: None,
                host: Some(self.host.clone()),
                port: None,
                user: None,
                source: self.source.into(),
                bypass: self.bypass.clone(),
            };
        }
        ProxyInEffect {
            mode: mode_name(mode),
            kind: Some(match self.kind {
                ProxyKind::Http => "http",
                ProxyKind::Https => "https",
                ProxyKind::Socks5 => "socks5",
            }),
            host: Some(self.host.clone()),
            port: Some(self.port),
            user: self.user.clone(),
            source: self.source.into(),
            bypass: self.bypass.clone(),
        }
    }

    /// The reqwest proxy for every request, with credentials and bypass.
    pub fn proxy(&self) -> Result<Proxy, ProxyProblem> {
        let bypass: Vec<&str> = self
            .bypass
            .iter()
            .map(|b| b.trim())
            .filter(|b| !b.is_empty())
            .collect();
        if let Some(pac) = &self.pac {
            let pac = Arc::clone(pac);
            let proxy = Proxy::custom(move |url: &Url| -> Option<String> { pac.lookup(url) });
            return Ok(proxy.no_proxy(NoProxy::from_string(&bypass.join(","))));
        }
        let mut url = Url::parse(&self.url()).map_err(|e| ProxyProblem::Invalid(e.to_string()))?;
        // SOCKS5 takes its credentials in the URL; HTTP proxies as a
        // Proxy-Authorization header.
        if self.kind == ProxyKind::Socks5
            && let Some(user) = &self.user
        {
            let _ = url.set_username(user);
            let _ = url.set_password(self.password.as_deref());
        }
        let mut proxy = Proxy::all(url.as_str()).map_err(|e| ProxyProblem::Invalid(e.to_string()))?;
        if self.kind != ProxyKind::Socks5
            && let Some(user) = &self.user
        {
            proxy = proxy.basic_auth(user, self.password.as_deref().unwrap_or_default());
        }
        Ok(proxy.no_proxy(NoProxy::from_string(&bypass.join(","))))
    }
}

fn mode_name(mode: ProxyMode) -> &'static str {
    match mode {
        ProxyMode::None => "none",
        ProxyMode::System => "system",
        ProxyMode::Manual => "manual",
    }
}

/// The proxy the settings mean, or `None` for direct connections.
pub(crate) fn resolve(settings: &ProxySettings) -> Result<Option<Resolved>, ProxyProblem> {
    match settings.mode {
        ProxyMode::None => Ok(None),
        ProxyMode::Manual => Ok(Some(Resolved {
            kind: settings.kind,
            host: settings.host.trim().to_string(),
            port: settings.port,
            user: settings.user.clone().filter(|u| !u.trim().is_empty()),
            password: settings.password.expose().map(str::to_string),
            bypass: settings.bypass.clone(),
            source: "manual",
            pac: None,
        })),
        ProxyMode::System => {
            let mut found = from_env(&|k: &str| std::env::var(k).ok());
            if found.is_none() {
                found = match os::system_proxy()? {
                    None => None,
                    Some(SystemProxy::Fixed(r)) => Some(r),
                    Some(SystemProxy::Pac(location)) => {
                        let pac = Pac::load(&location)?;
                        Some(Resolved::with_pac(&location, pac))
                    }
                };
            }
            Ok(found.map(|mut r| {
                r.bypass.extend(settings.bypass.iter().cloned());
                r
            }))
        }
    }
}

/// Builder settings for the proxy in effect. Without one, connections go
/// direct, ignoring the environment.
pub(crate) fn apply(
    builder: ClientBuilder,
    resolved: Option<&Resolved>,
) -> Result<ClientBuilder, ProxyProblem> {
    match resolved {
        None => Ok(builder.no_proxy()),
        Some(r) => Ok(builder.proxy(r.proxy()?)),
    }
}

/// `http://user:pw@host:port`, `socks5://host:1080` or `host:port`.
pub(crate) fn parse_proxy_url(value: &str, source: &'static str) -> Option<Resolved> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }
    let with_scheme = if value.contains("://") {
        value.to_string()
    } else {
        format!("http://{value}")
    };
    let url = Url::parse(&with_scheme).ok()?;
    let kind = match url.scheme() {
        "http" => ProxyKind::Http,
        "https" => ProxyKind::Https,
        "socks5" | "socks5h" => ProxyKind::Socks5,
        _ => return None,
    };
    let default_port = match kind {
        ProxyKind::Http => 80,
        ProxyKind::Https => 443,
        ProxyKind::Socks5 => 1080,
    };
    let host = url
        .host_str()?
        .trim_start_matches('[')
        .trim_end_matches(']')
        .to_string();
    Some(Resolved {
        kind,
        host,
        port: url.port().unwrap_or(default_port),
        user: (!url.username().is_empty()).then(|| percent_decode(url.username())),
        password: url.password().map(percent_decode),
        bypass: Vec::new(),
        source,
        pac: None,
    })
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && let Some(h) = s.get(i + 1..i + 3)
            && let Ok(b) = u8::from_str_radix(h, 16)
        {
            out.push(b);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// `HTTPS_PROXY`, `HTTP_PROXY`, `ALL_PROXY` (either case) and `NO_PROXY`.
pub(crate) fn from_env(get: &dyn Fn(&str) -> Option<String>) -> Option<Resolved> {
    let var = |name: &str| {
        get(name)
            .or_else(|| get(&name.to_ascii_lowercase()))
            .filter(|v| !v.trim().is_empty())
    };
    let value = var("HTTPS_PROXY")
        .or_else(|| var("HTTP_PROXY"))
        .or_else(|| var("ALL_PROXY"))?;
    let mut r = parse_proxy_url(&value, "env")?;
    if let Some(no) = var("NO_PROXY") {
        r.bypass = no
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
    }
    Some(r)
}

/// Checks the proxy by requesting `probe` (the start URL's robots.txt)
/// through `client`, with a 10 s limit (FR-022).
pub(crate) async fn check(client: &reqwest::Client, probe: &Url) -> Result<(), ProxyProblem> {
    let sent = client
        .get(probe.clone())
        .timeout(Duration::from_secs(10))
        .send()
        .await;
    match sent {
        Ok(r) if r.status() == StatusCode::PROXY_AUTHENTICATION_REQUIRED => Err(ProxyProblem::Rejected),
        Ok(_) => Ok(()),
        Err(e) => {
            let mut detail = e.to_string();
            let mut source = std::error::Error::source(&e);
            while let Some(s) = source {
                detail = s.to_string();
                source = s.source();
            }
            Err(ProxyProblem::Unreachable(detail))
        }
    }
}

/// The operating system's proxy setting (research R6). Each platform reads
/// it with a tool every installation has, so no new crate is needed.
mod os {
    use super::{ProxyProblem, SystemProxy};

    #[cfg(target_os = "macos")]
    pub(super) fn system_proxy() -> Result<Option<SystemProxy>, ProxyProblem> {
        let out = std::process::Command::new("scutil").arg("--proxy").output();
        let Ok(out) = out else { return Ok(None) };
        super::parse_scutil(&String::from_utf8_lossy(&out.stdout))
    }

    #[cfg(target_os = "windows")]
    pub(super) fn system_proxy() -> Result<Option<SystemProxy>, ProxyProblem> {
        let out = std::process::Command::new("reg")
            .args([
                "query",
                r"HKCU\Software\Microsoft\Windows\CurrentVersion\Internet Settings",
            ])
            .output();
        let Ok(out) = out else { return Ok(None) };
        super::parse_windows_registry(&String::from_utf8_lossy(&out.stdout))
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    pub(super) fn system_proxy() -> Result<Option<SystemProxy>, ProxyProblem> {
        let get = |key: &str, schema: &str| {
            std::process::Command::new("gsettings")
                .args(["get", schema, key])
                .output()
                .ok()
                .map(|o| {
                    String::from_utf8_lossy(&o.stdout)
                        .trim()
                        .trim_matches('\'')
                        .to_string()
                })
        };
        match get("mode", "org.gnome.system.proxy").as_deref() {
            Some("manual") => {}
            Some("auto") => {
                return match get("autoconfig-url", "org.gnome.system.proxy") {
                    Some(url) if !url.is_empty() => Ok(Some(SystemProxy::Pac(url))),
                    _ => Err(ProxyProblem::PacUnsupported),
                };
            }
            _ => return Ok(None),
        }
        for (schema, kind) in [
            ("org.gnome.system.proxy.https", "http"),
            ("org.gnome.system.proxy.http", "http"),
            ("org.gnome.system.proxy.socks", "socks5"),
        ] {
            let host = get("host", schema).unwrap_or_default();
            let port = get("port", schema)
                .and_then(|p| p.parse::<u16>().ok())
                .unwrap_or(0);
            if !host.is_empty() && port > 0 {
                let mut r = super::parse_proxy_url(&format!("{kind}://{host}:{port}"), "os");
                if let Some(r) = &mut r {
                    r.bypass = get("ignore-hosts", "org.gnome.system.proxy")
                        .map(|v| super::parse_gsettings_list(&v))
                        .unwrap_or_default();
                }
                return Ok(r.map(SystemProxy::Fixed));
            }
        }
        Ok(None)
    }
}

/// `scutil --proxy` output on macOS.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) fn parse_scutil(text: &str) -> Result<Option<SystemProxy>, ProxyProblem> {
    let mut values = std::collections::HashMap::new();
    let mut exceptions = Vec::new();
    let mut in_exceptions = false;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with("ExceptionsList") {
            in_exceptions = true;
            continue;
        }
        if in_exceptions {
            if line == "}" {
                in_exceptions = false;
            } else if let Some((_, v)) = line.split_once(" : ") {
                exceptions.push(v.trim().to_string());
            }
            continue;
        }
        if let Some((k, v)) = line.split_once(" : ") {
            values.insert(k.trim().to_string(), v.trim().to_string());
        }
    }
    let on = |k: &str| values.get(k).is_some_and(|v| v == "1");
    if on("ProxyAutoConfigEnable") {
        return match values.get("ProxyAutoConfigURLString") {
            Some(url) if !url.is_empty() => Ok(Some(SystemProxy::Pac(url.clone()))),
            _ => Err(ProxyProblem::PacUnsupported),
        };
    }
    for (enable, host, port, kind) in [
        ("HTTPSEnable", "HTTPSProxy", "HTTPSPort", "http"),
        ("HTTPEnable", "HTTPProxy", "HTTPPort", "http"),
        ("SOCKSEnable", "SOCKSProxy", "SOCKSPort", "socks5"),
    ] {
        if on(enable)
            && let (Some(h), Some(p)) = (values.get(host), values.get(port))
        {
            let mut r = parse_proxy_url(&format!("{kind}://{h}:{p}"), "os");
            if let Some(r) = &mut r {
                r.bypass.clone_from(&exceptions);
            }
            return Ok(r.map(SystemProxy::Fixed));
        }
    }
    Ok(None)
}

/// `reg query` of the Internet Settings key on Windows.
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
pub(crate) fn parse_windows_registry(text: &str) -> Result<Option<SystemProxy>, ProxyProblem> {
    let value = |name: &str| {
        text.lines().find_map(|l| {
            let mut parts = l.split_whitespace();
            (parts.next()? == name).then(|| {
                let _kind = parts.next();
                parts.collect::<Vec<_>>().join(" ")
            })
        })
    };
    if let Some(url) = value("AutoConfigURL")
        && !url.is_empty()
    {
        return Ok(Some(SystemProxy::Pac(url)));
    }
    if value("ProxyEnable").as_deref() != Some("0x1") {
        return Ok(None);
    }
    let Some(server) = value("ProxyServer") else {
        return Ok(None);
    };
    // Either one proxy for everything or "http=h:p;https=h:p;socks=h:p".
    let pick = if server.contains('=') {
        let parts: Vec<(&str, &str)> = server.split(';').filter_map(|p| p.split_once('=')).collect();
        let find = |k: &str| {
            parts
                .iter()
                .find(|(n, _)| n.eq_ignore_ascii_case(k))
                .map(|(_, v)| *v)
        };
        find("https")
            .map(|v| format!("http://{v}"))
            .or_else(|| find("http").map(|v| format!("http://{v}")))
            .or_else(|| find("socks").map(|v| format!("socks5://{v}")))
    } else {
        Some(server.clone())
    };
    let mut r = pick.and_then(|p| parse_proxy_url(&p, "os"));
    if let (Some(r), Some(over)) = (&mut r, value("ProxyOverride")) {
        r.bypass = over
            .split(';')
            .map(str::trim)
            .filter(|s| !s.is_empty() && *s != "<local>")
            .map(str::to_string)
            .collect();
    }
    Ok(r.map(SystemProxy::Fixed))
}

/// `['localhost', '127.0.0.0/8']` from gsettings.
#[cfg_attr(any(target_os = "macos", target_os = "windows"), allow(dead_code))]
pub(crate) fn parse_gsettings_list(v: &str) -> Vec<String> {
    v.trim()
        .trim_start_matches('[')
        .trim_end_matches(']')
        .split(',')
        .map(|s| s.trim().trim_matches('\'').to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixed(found: Option<SystemProxy>) -> Resolved {
        match found {
            Some(SystemProxy::Fixed(r)) => r,
            other => panic!("expected a fixed proxy, got {other:?}"),
        }
    }

    #[test]
    fn proxy_urls_from_settings_and_environment() {
        let r = parse_proxy_url("http://ann:p%40ss@proxy.corp:8080", "env").unwrap();
        assert_eq!(
            (r.kind, r.host.as_str(), r.port),
            (ProxyKind::Http, "proxy.corp", 8080)
        );
        assert_eq!(r.user.as_deref(), Some("ann"));
        assert_eq!(r.password.as_deref(), Some("p@ss"));
        let s = parse_proxy_url("socks5://10.0.0.2", "env").unwrap();
        assert_eq!((s.kind, s.port), (ProxyKind::Socks5, 1080));
        assert_eq!(s.url(), "socks5h://10.0.0.2:1080");
        assert_eq!(parse_proxy_url("proxy:3128", "env").unwrap().port, 3128);
        assert!(parse_proxy_url("ftp://x", "env").is_none());

        let env = |k: &str| match k {
            "https_proxy" => Some("http://p:3128".to_string()),
            "NO_PROXY" => Some("localhost, .corp".to_string()),
            _ => None,
        };
        let r = from_env(&env).unwrap();
        assert_eq!(r.host, "p");
        assert_eq!(r.bypass, ["localhost", ".corp"]);
        assert!(from_env(&|_| None).is_none());
    }

    #[test]
    fn manual_and_none_modes() {
        assert_eq!(resolve(&ProxySettings::default()).unwrap(), None);
        let s = ProxySettings {
            mode: ProxyMode::Manual,
            kind: ProxyKind::Socks5,
            host: "proxy".into(),
            port: 1080,
            user: Some("ann".into()),
            password: spiderlens_config::Secret::new("pw"),
            bypass: vec!["localhost".into()],
        };
        let r = resolve(&s).unwrap().unwrap();
        assert_eq!(r.password.as_deref(), Some("pw"));
        let shown = r.in_effect(ProxyMode::Manual);
        assert_eq!(shown.describe(), "socks5://ann@proxy:1080 (entered)");
        assert!(!serde_json::to_string(&shown).unwrap().contains("pw"));
        assert!(r.proxy().is_ok());
    }

    #[test]
    fn system_settings_of_each_platform() {
        let mac = "<dictionary> {\n  ExceptionsList : <array> {\n    0 : *.local\n    1 : 169.254/16\n  }\n  HTTPEnable : 1\n  HTTPPort : 8080\n  HTTPProxy : proxy.corp\n  HTTPSEnable : 0\n}";
        let r = fixed(parse_scutil(mac).unwrap());
        assert_eq!((r.host.as_str(), r.port), ("proxy.corp", 8080));
        assert_eq!(r.bypass, ["*.local", "169.254/16"]);
        assert_eq!(
            parse_scutil("ProxyAutoConfigEnable : 1"),
            Err(ProxyProblem::PacUnsupported)
        );

        let win = "    ProxyEnable    REG_DWORD    0x1\n    ProxyServer    REG_SZ    http=p1:80;https=p2:443\n    ProxyOverride    REG_SZ    *.corp;<local>\n";
        let r = fixed(parse_windows_registry(win).unwrap());
        assert_eq!((r.host.as_str(), r.port), ("p2", 443));
        assert_eq!(r.bypass, ["*.corp"]);
        assert_eq!(
            parse_windows_registry("    ProxyEnable    REG_DWORD    0x0\n").unwrap(),
            None
        );

        assert_eq!(
            parse_scutil(
                "ProxyAutoConfigEnable : 1\nProxyAutoConfigURLString : http://wpad.corp/proxy.pac\n"
            ),
            Ok(Some(SystemProxy::Pac("http://wpad.corp/proxy.pac".into())))
        );
        assert_eq!(
            parse_scutil("ProxyAutoConfigEnable : 0\nProxyAutoConfigURLString : http://x/p.pac\n"),
            Ok(None)
        );
        assert_eq!(
            parse_windows_registry("    AutoConfigURL    REG_SZ    http://wpad.corp/proxy.pac\n"),
            Ok(Some(SystemProxy::Pac("http://wpad.corp/proxy.pac".into())))
        );

        assert_eq!(
            parse_gsettings_list("['localhost', '127.0.0.0/8']"),
            ["localhost", "127.0.0.0/8"]
        );
    }

    #[test]
    fn a_configuration_script_is_shown_and_used() {
        let pac = Pac::from_script("function FindProxyForURL(u, h) { return 'PROXY gw:3128'; }").unwrap();
        let r = Resolved::with_pac("http://wpad.corp/proxy.pac", pac);
        let shown = r.in_effect(ProxyMode::System);
        assert_eq!(shown.source, "pac");
        assert_eq!(
            shown.describe(),
            "automatic configuration script http://wpad.corp/proxy.pac"
        );
        assert!(r.proxy().is_ok());
        let probe = Url::parse("http://example.com/robots.txt").unwrap();
        assert_eq!(
            r.pac_note(&probe).as_deref(),
            Some("example.com goes through http://gw:3128")
        );
        assert!(ProxyProblem::PacUnsupported.message().contains("WPAD"));
    }
}
