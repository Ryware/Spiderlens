//! Which hosts count as internal (FR-003, FR-014; research R4).

use std::net::IpAddr;

use crate::SubdomainMode;

/// Registrable domain from the compiled-in Public Suffix List, so
/// `blog.example.co.uk` gives `example.co.uk` while `a.github.io` stays
/// `a.github.io`. IP addresses and single-label hosts are their own site.
pub fn registrable_domain(host: &str) -> String {
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    if host.parse::<IpAddr>().is_ok() || host.starts_with('[') || !host.contains('.') {
        return host;
    }
    psl::domain_str(&host).map(str::to_string).unwrap_or(host)
}

/// The internal hosts of a crawl: the start host plus any host the start URL
/// redirected to within the same site.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InternalHosts {
    mode: SubdomainMode,
    hosts: Vec<String>,
    sites: Vec<String>,
}

impl InternalHosts {
    pub fn new(mode: SubdomainMode, start_host: &str) -> Self {
        let host = start_host.to_ascii_lowercase();
        Self {
            mode,
            sites: vec![registrable_domain(&host)],
            hosts: vec![host],
        }
    }

    /// Adopts the host a start URL redirected to, if it is the same site
    /// (for example `example.com` to `www.example.com`). Returns false for an
    /// off-site redirect, which the crawl reports instead of following.
    pub fn adopt_redirect(&mut self, target_host: &str) -> bool {
        let host = target_host.to_ascii_lowercase();
        if !self.sites.contains(&registrable_domain(&host)) {
            return false;
        }
        if !self.hosts.contains(&host) {
            self.hosts.push(host);
        }
        true
    }

    /// The bare domain and its `www.` variant count as one host (spec 003
    /// FR-007); scheme and port do not matter.
    /// Makes another host internal, as List mode does for every listed host
    /// (spec 003 edge case "List mode").
    pub fn add(&mut self, host: &str) {
        let host = host.to_ascii_lowercase();
        let site = registrable_domain(&host);
        if !self.sites.contains(&site) {
            self.sites.push(site);
        }
        if !self.hosts.contains(&host) {
            self.hosts.push(host);
        }
    }

    pub fn is_internal(&self, host: &str) -> bool {
        let host = host.to_ascii_lowercase();
        match self.mode {
            SubdomainMode::ThisHost => self.hosts.iter().any(|h| without_www(h) == without_www(&host)),
            SubdomainMode::AllSubdomains => self.sites.contains(&registrable_domain(&host)),
        }
    }

    pub fn hosts(&self) -> &[String] {
        &self.hosts
    }
}

fn without_www(host: &str) -> &str {
    host.strip_prefix("www.").unwrap_or(host)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registrable_domains() {
        assert_eq!(registrable_domain("blog.example.co.uk"), "example.co.uk");
        assert_eq!(registrable_domain("a.github.io"), "a.github.io");
        assert_eq!(registrable_domain("WWW.Example.com"), "example.com");
        assert_eq!(registrable_domain("127.0.0.1"), "127.0.0.1");
        assert_eq!(registrable_domain("localhost"), "localhost");
    }

    #[test]
    fn subdomain_modes() {
        let this = InternalHosts::new(SubdomainMode::ThisHost, "www.example.com");
        assert!(this.is_internal("www.example.com"));
        assert!(!this.is_internal("blog.example.com"));
        assert!(
            this.is_internal("example.com"),
            "www and bare domain are one host"
        );
        assert!(InternalHosts::new(SubdomainMode::ThisHost, "example.com").is_internal("WWW.example.com"));
        let all = InternalHosts::new(SubdomainMode::AllSubdomains, "www.example.com");
        assert!(all.is_internal("blog.example.com"));
        assert!(!all.is_internal("example.org"));
    }

    #[test]
    fn start_redirect_within_site_is_adopted() {
        let mut h = InternalHosts::new(SubdomainMode::ThisHost, "example.com");
        assert!(h.adopt_redirect("www.example.com"));
        assert!(h.is_internal("www.example.com"));
        assert!(!h.adopt_redirect("new-brand.com"));
        assert!(!h.is_internal("new-brand.com"));
    }

    #[test]
    fn github_pages_are_separate_sites() {
        let all = InternalHosts::new(SubdomainMode::AllSubdomains, "a.github.io");
        assert!(!all.is_internal("b.github.io"));
    }
}
