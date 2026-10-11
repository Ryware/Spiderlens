use std::time::Duration;

use spiderlens_config::{CrawlConfig, HeaderTarget};

/// A request header with where it may be sent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestHeader {
    pub name: String,
    pub value: String,
    /// False means the header goes to the requested page's host only.
    pub all_hosts: bool,
}

/// Request options for a single-page check, derived from the crawl
/// configuration (spec 002) so defaults live in one place.
#[derive(Debug, Clone)]
pub struct CheckOptions {
    pub connect_timeout: Duration,
    pub read_timeout: Duration,
    pub max_body_bytes: u64,
    pub max_redirects: usize,
    pub user_agent: String,
    /// Token matched against robots.txt `User-agent:` groups.
    pub robots_token: String,
    pub headers: Vec<RequestHeader>,
}

impl Default for CheckOptions {
    fn default() -> Self {
        Self::from(&CrawlConfig::default())
    }
}

impl From<&CrawlConfig> for CheckOptions {
    /// Assumes a validated configuration; incomplete settings fall back to
    /// the SpiderLens identity rather than sending an empty user agent.
    fn from(c: &CrawlConfig) -> Self {
        let ua = &c.identity.user_agent;
        Self {
            connect_timeout: Duration::from_secs_f64(c.network.connect_timeout_secs),
            read_timeout: Duration::from_secs_f64(c.network.read_timeout_secs),
            max_body_bytes: c.network.max_body_mb * 1024 * 1024,
            max_redirects: c.network.max_redirects as usize,
            user_agent: ua
                .string()
                .unwrap_or_else(spiderlens_config::spiderlens_user_agent),
            robots_token: ua.token().unwrap_or_else(|| "SpiderLens".into()),
            headers: c
                .identity
                .headers
                .iter()
                .filter_map(|h| {
                    Some(RequestHeader {
                        name: h.name.trim().to_string(),
                        value: h.value.expose()?.to_string(),
                        all_hosts: h.send_to == HeaderTarget::All,
                    })
                })
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use spiderlens_config::{CustomHeader, HeaderValue, UaPreset};

    #[test]
    fn defaults_match_spec_001() {
        let o = CheckOptions::default();
        assert_eq!(o.connect_timeout, Duration::from_secs(10));
        assert_eq!(o.read_timeout, Duration::from_secs(30));
        assert_eq!(o.max_body_bytes, 50 * 1024 * 1024);
        assert_eq!(o.max_redirects, 10);
        assert!(o.user_agent.starts_with("SpiderLens/"));
        assert_eq!(o.robots_token, "SpiderLens");
    }

    #[test]
    fn identity_comes_from_config() {
        let mut c = CrawlConfig::default();
        c.identity.user_agent.preset = UaPreset::Bingbot;
        c.identity.headers.push(CustomHeader {
            name: "X-Key".into(),
            value: HeaderValue::secret("abc"),
            send_to: HeaderTarget::Internal,
        });
        let o = CheckOptions::from(&c);
        assert!(o.user_agent.contains("bingbot"));
        assert_eq!(o.robots_token, "bingbot");
        assert_eq!(o.headers[0].value, "abc");
        assert!(!o.headers[0].all_hosts);
    }
}
