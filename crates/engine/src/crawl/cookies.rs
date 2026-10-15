//! The cookie audit (spec 025 FR-025, plan D7): which cookies each response
//! set, with their attributes and never their values.

use cookie::{Cookie, SameSite};
use spiderlens_config::registrable_domain;
use url::Url;

/// One cookie a response set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CookieObs {
    pub name: String,
    /// Without a leading dot.
    pub domain: String,
    /// No `Domain` attribute: only this exact host gets it back.
    pub host_only: bool,
    pub path: String,
    /// Milliseconds since the epoch; `None` for a session cookie.
    pub expires: Option<i64>,
    pub max_age: Option<i64>,
    pub secure: bool,
    pub http_only: bool,
    /// `strict`, `lax` or `none`, as sent.
    pub same_site: Option<&'static str>,
    /// Bytes of `name=value` as sent.
    pub size: usize,
    pub third_party: bool,
    /// `response` or `script` (spec 014 renders).
    pub source: &'static str,
    /// Other attributes, for example `Partitioned; Priority=High`.
    pub extra: Option<String>,
}

/// The cookies set by a response's `Set-Cookie` headers (names lowercase,
/// as `fetch_once` records them). Unparseable headers are skipped.
pub fn from_headers(url: &Url, headers: &[(String, String)], now_ms: i64) -> Vec<CookieObs> {
    headers
        .iter()
        .filter(|(n, _)| n == "set-cookie")
        .filter_map(|(_, v)| parse(url, v, now_ms))
        .collect()
}

fn parse(url: &Url, raw: &str, now_ms: i64) -> Option<CookieObs> {
    let c = Cookie::parse(raw).ok()?;
    let host = url.host_str()?.to_ascii_lowercase();
    let (domain, host_only) = match c.domain() {
        Some(d) if !d.is_empty() => (d.trim_start_matches('.').to_ascii_lowercase(), false),
        _ => (host.clone(), true),
    };
    let max_age = c.max_age().map(|d| d.whole_seconds());
    // Max-Age wins over Expires (RFC 6265 5.3 step 3).
    let expires = match max_age {
        Some(secs) => Some(now_ms.saturating_add(secs.saturating_mul(1000))),
        None => c
            .expires_datetime()
            .map(|t| (t.unix_timestamp_nanos() / 1_000_000) as i64),
    };
    let size = c.name().len() + 1 + c.value().len();
    Some(CookieObs {
        name: c.name().to_string(),
        third_party: registrable_domain(&domain) != registrable_domain(&host),
        host_only,
        path: c.path().unwrap_or_else(|| default_path(url)).to_string(),
        expires,
        max_age,
        secure: c.secure().unwrap_or(false),
        http_only: c.http_only().unwrap_or(false),
        same_site: c.same_site().map(|s| match s {
            SameSite::Strict => "strict",
            SameSite::Lax => "lax",
            SameSite::None => "none",
        }),
        size,
        source: "response",
        extra: extra_attributes(raw),
        domain,
    })
}

/// RFC 6265 5.1.4: the request path up to its last slash.
fn default_path(url: &Url) -> &str {
    let path = url.path();
    match path.rfind('/') {
        Some(0) | None => "/",
        Some(i) => &path[..i],
    }
}

const KNOWN: &[&str] = &[
    "domain", "path", "expires", "max-age", "secure", "httponly", "samesite",
];

fn extra_attributes(raw: &str) -> Option<String> {
    let extra: Vec<&str> = raw
        .split(';')
        .skip(1)
        .map(str::trim)
        .filter(|a| {
            let name = a
                .split('=')
                .next()
                .unwrap_or_default()
                .trim()
                .to_ascii_lowercase();
            !name.is_empty() && !KNOWN.contains(&name.as_str())
        })
        .collect();
    (!extra.is_empty()).then(|| extra.join("; "))
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_700_000_000_000;

    fn one(url: &str, raw: &str) -> CookieObs {
        let url = Url::parse(url).unwrap();
        from_headers(&url, &[("set-cookie".into(), raw.into())], NOW).remove(0)
    }

    #[test]
    fn records_attributes_not_values() {
        let c = one(
            "https://www.example.com/a/b",
            "sid=TOKEN; Domain=.example.com; Path=/; Secure; HttpOnly; SameSite=Lax; Max-Age=60; Partitioned",
        );
        assert_eq!(c.name, "sid");
        assert_eq!(c.domain, "example.com");
        assert!(!c.host_only && c.secure && c.http_only && !c.third_party);
        assert_eq!(c.same_site, Some("lax"));
        assert_eq!(c.max_age, Some(60));
        assert_eq!(c.expires, Some(NOW + 60_000));
        assert_eq!(c.size, "sid=TOKEN".len());
        assert_eq!(c.extra.as_deref(), Some("Partitioned"));
        assert!(!format!("{c:?}").contains("TOKEN"));
    }

    #[test]
    fn session_host_only_cookie_with_default_path() {
        let c = one("http://shop.example.com/cart/view", "theme=dark");
        assert_eq!((c.domain.as_str(), c.host_only), ("shop.example.com", true));
        assert_eq!(c.path, "/cart");
        assert_eq!(c.expires, None);
        assert!(!c.secure && !c.http_only && c.same_site.is_none() && c.extra.is_none());
    }

    #[test]
    fn expires_date_and_third_party() {
        let c = one(
            "https://example.com/",
            "ad=1; Domain=ads.test; Expires=Wed, 21 Oct 2037 07:28:00 GMT",
        );
        assert!(c.third_party);
        assert_eq!(c.expires, Some(2_139_722_880_000));
    }

    #[test]
    fn other_headers_and_garbage_are_skipped() {
        let url = Url::parse("https://example.com/").unwrap();
        let h = [
            ("content-type".to_string(), "text/html".to_string()),
            ("set-cookie".to_string(), "=".to_string()),
        ];
        assert!(from_headers(&url, &h, NOW).is_empty());
    }
}
