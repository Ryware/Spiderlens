//! The crawl's cookie jar (spec 025 plan D2).
//!
//! Wraps an RFC 6265 store with two rules of our own: cookies are only sent
//! to and accepted from the crawl's internal hosts, and the cookie policy
//! decides whether cookies the site sets are kept. Nothing in the jar is
//! ever written to disk.

use std::collections::HashSet;
use std::sync::{Mutex, PoisonError, RwLock};

use cookie::Cookie as RawCookie;
use cookie_store::CookieStore;
use reqwest::header::HeaderValue;
use spiderlens_config::{CookiePolicy, InternalHosts, SuppliedCookie};
use url::Url;

/// Browsers keep at most this many cookies per domain and this many bytes
/// per cookie; the crawler sends nothing a browser would not.
pub const MAX_PER_DOMAIN: usize = 180;
pub const MAX_COOKIE_BYTES: usize = 4096;

pub struct CrawlJar {
    store: Mutex<CookieStore>,
    policy: CookiePolicy,
    internal: RwLock<InternalHosts>,
    /// Names of supplied cookies: under "Ignore", the site may still
    /// replace these (session rotation).
    pinned: HashSet<String>,
}

impl CrawlJar {
    pub fn new(policy: CookiePolicy, internal: InternalHosts) -> Self {
        Self {
            store: Mutex::new(CookieStore::new()),
            policy,
            internal: RwLock::new(internal),
            pinned: HashSet::new(),
        }
    }

    /// Whether a crawl needs a jar at all. Without one the HTTP client has
    /// no cookie provider, exactly as before spec 025.
    pub fn needed(policy: CookiePolicy, supplied: &[SuppliedCookie]) -> bool {
        policy == CookiePolicy::Keep || !supplied.is_empty()
    }

    /// Adds the user's cookies. Secret values that are not known (a file
    /// opened without them) are skipped; the caller asks for them first.
    pub fn supply(&mut self, cookies: &[SuppliedCookie]) {
        let store = self.store.get_mut().unwrap_or_else(PoisonError::into_inner);
        for c in cookies {
            let Some(value) = c.value.expose() else { continue };
            let domain = c.domain.trim().trim_start_matches('.').to_ascii_lowercase();
            let Ok(url) = Url::parse(&format!("https://{domain}{}", c.path)) else {
                continue;
            };
            let mut raw = RawCookie::new(c.name.clone(), value.to_string());
            raw.set_domain(domain);
            raw.set_path(c.path.clone());
            raw.set_secure(c.secure_only);
            let _ = store.insert_raw(&raw, &url);
            self.pinned.insert(c.name.clone());
        }
    }

    /// The start URL redirected to another host of the same site.
    pub fn adopt_host(&self, host: &str) {
        let mut internal = self.internal.write().unwrap_or_else(PoisonError::into_inner);
        internal.add(host);
    }

    fn is_internal(&self, url: &Url) -> bool {
        let internal = self.internal.read().unwrap_or_else(PoisonError::into_inner);
        url.host_str().is_some_and(|h| internal.is_internal(h))
    }

    fn accepts(&self, cookie: &RawCookie<'_>) -> bool {
        (self.policy == CookiePolicy::Keep || self.pinned.contains(cookie.name()))
            && cookie.name().len() + cookie.value().len() <= MAX_COOKIE_BYTES
    }

    /// The `Cookie` header for a request, if any cookie matches.
    pub fn header_for(&self, url: &Url) -> Option<String> {
        if !self.is_internal(url) {
            return None;
        }
        let store = self.store.lock().unwrap_or_else(PoisonError::into_inner);
        let pairs: Vec<String> = store
            .get_request_values(url)
            .map(|(n, v)| format!("{n}={v}"))
            .collect();
        (!pairs.is_empty()).then(|| pairs.join("; "))
    }

    /// Stores cookies a response set, by the policy.
    pub fn receive<'a>(&self, set_cookie: impl Iterator<Item = &'a str>, url: &Url) {
        if !self.is_internal(url) {
            return;
        }
        let mut store = self.store.lock().unwrap_or_else(PoisonError::into_inner);
        for raw in set_cookie.filter_map(|s| RawCookie::parse(s.to_string()).ok()) {
            if !self.accepts(&raw) {
                continue;
            }
            let domain = raw
                .domain()
                .map(|d| d.trim_start_matches('.').to_ascii_lowercase())
                .or_else(|| url.host_str().map(str::to_string))
                .unwrap_or_default();
            let deleting = raw.max_age().is_some_and(|a| a.is_zero() || a.is_negative());
            let full = store
                .iter_any()
                .filter(|c| c.domain() == Some(domain.as_str()))
                .count()
                >= MAX_PER_DOMAIN;
            if full && !deleting && !store.contains_any(&domain, raw.path().unwrap_or("/"), raw.name()) {
                continue;
            }
            let _ = store.insert_raw(&raw, url);
        }
    }
}

impl reqwest::cookie::CookieStore for CrawlJar {
    fn set_cookies(&self, headers: &mut dyn Iterator<Item = &HeaderValue>, url: &Url) {
        self.receive(headers.filter_map(|h| h.to_str().ok()), url);
    }

    fn cookies(&self, url: &Url) -> Option<HeaderValue> {
        self.header_for(url).and_then(|h| HeaderValue::from_str(&h).ok())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use spiderlens_config::{HeaderValue as ConfigValue, SubdomainMode};

    fn url(s: &str) -> Url {
        Url::parse(s).unwrap()
    }

    fn jar(policy: CookiePolicy) -> CrawlJar {
        CrawlJar::new(
            policy,
            InternalHosts::new(SubdomainMode::AllSubdomains, "example.com"),
        )
    }

    fn supplied(name: &str, value: &str, secure_only: bool) -> SuppliedCookie {
        SuppliedCookie {
            name: name.into(),
            value: ConfigValue::Plain(value.into()),
            domain: "example.com".into(),
            path: "/".into(),
            secure_only,
        }
    }

    #[test]
    fn supplied_cookies_reach_the_site_and_its_subdomains_only() {
        let mut j = jar(CookiePolicy::Ignore);
        j.supply(&[supplied("consent", "yes", false)]);
        assert_eq!(
            j.header_for(&url("https://example.com/a")).as_deref(),
            Some("consent=yes")
        );
        assert_eq!(
            j.header_for(&url("http://shop.example.com/")).as_deref(),
            Some("consent=yes")
        );
        assert_eq!(j.header_for(&url("https://other.test/")), None);
    }

    #[test]
    fn secure_only_cookies_skip_plain_http() {
        let mut j = jar(CookiePolicy::Ignore);
        j.supply(&[supplied("sid", "1", true)]);
        assert_eq!(j.header_for(&url("http://example.com/")), None);
        assert_eq!(
            j.header_for(&url("https://example.com/")).as_deref(),
            Some("sid=1")
        );
    }

    #[test]
    fn ignore_drops_site_cookies_but_keeps_rotating_supplied_ones() {
        let mut j = jar(CookiePolicy::Ignore);
        j.supply(&[supplied("sid", "old", false)]);
        let u = url("https://example.com/");
        j.receive(["sid=new; Path=/", "tracker=1"].into_iter(), &u);
        assert_eq!(j.header_for(&u).as_deref(), Some("sid=new"));
    }

    #[test]
    fn keep_stores_and_deletes_by_the_cookie_rules() {
        let j = jar(CookiePolicy::Keep);
        let u = url("https://example.com/shop/item");
        j.receive(["a=1; Path=/", "b=2; Path=/shop"].into_iter(), &u);
        assert_eq!(j.header_for(&url("https://example.com/")).as_deref(), Some("a=1"));
        let mut both: Vec<String> = j
            .header_for(&url("https://example.com/shop/x"))
            .unwrap()
            .split("; ")
            .map(str::to_string)
            .collect();
        both.sort();
        assert_eq!(both, ["a=1", "b=2"]);
        j.receive(["a=; Max-Age=0; Path=/"].into_iter(), &u);
        assert_eq!(j.header_for(&url("https://example.com/")), None);
    }

    #[test]
    fn external_hosts_neither_set_nor_get_cookies() {
        let j = jar(CookiePolicy::Keep);
        j.receive(["x=1"].into_iter(), &url("https://other.test/"));
        assert_eq!(j.header_for(&url("https://other.test/")), None);
    }

    #[test]
    fn oversized_cookies_are_dropped() {
        let j = jar(CookiePolicy::Keep);
        let big = format!("big={}", "x".repeat(MAX_COOKIE_BYTES));
        j.receive([big.as_str()].into_iter(), &url("https://example.com/"));
        assert_eq!(j.header_for(&url("https://example.com/")), None);
    }
}
