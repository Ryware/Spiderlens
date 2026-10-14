//! robots.txt parsing and matching (RFC 9309; spec 003 research R4).
//!
//! Our own matcher, because the crawler must show the exact rule that blocked
//! a URL (spec 003 FR-026) and explain decisions before a crawl (FR-016).
//! Wildcards are matched with a small backtracking loop, not regular
//! expressions, so a hostile file cannot slow the crawl down.

use std::time::Duration;

use reqwest::Client;
use url::Url;

use crate::fetch::{BodyPolicy, OnLimit, get_following};
use crate::{CheckError, ErrorClass, RequestHeader};

/// Google reads at most 500 KiB of robots.txt; so do we.
pub(crate) const MAX_ROBOTS_BYTES: u64 = 500 * 1024;
pub(crate) const MAX_ROBOTS_REDIRECTS: usize = 5;

/// One Allow or Disallow line.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Rule {
    allow: bool,
    /// Pattern with percent-encoding in the same form as crawl URLs.
    pattern: String,
    /// The line as written, for example `Disallow: /private/`.
    line: String,
}

#[derive(Debug, Default)]
struct Group {
    agents: Vec<String>,
    rules: Vec<Rule>,
    crawl_delay: Option<f64>,
}

/// A parsed robots.txt file, before choosing a user agent.
#[derive(Debug, Default)]
pub struct RobotsTxt {
    groups: Vec<Group>,
    sitemaps: Vec<String>,
}

/// The rules that apply to one user agent on one host.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RobotsPolicy {
    rules: Vec<Rule>,
    /// Applies to every URL: robots.txt was unavailable (5xx, 429).
    disallow_all: bool,
    pub crawl_delay: Option<Duration>,
    pub sitemaps: Vec<String>,
    /// The `User-agent` value of the group that matched, if any.
    pub matched_agent: Option<String>,
}

/// What robots.txt says about one URL.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RobotsVerdict {
    Allowed,
    /// The deciding rule line, or a description for disallow-all.
    Disallowed(String),
}

impl RobotsTxt {
    pub fn parse(bytes: &[u8]) -> Self {
        let text = String::from_utf8_lossy(bytes);
        let text = text.strip_prefix('\u{feff}').unwrap_or(&text);
        let mut out = Self::default();
        let mut current: Option<Group> = None;
        // True once the current group has a rule, so the next User-agent
        // line starts a new group (RFC 9309 section 2.1).
        let mut in_rules = false;

        for raw in text.lines() {
            let line = raw.split('#').next().unwrap_or("").trim();
            let Some((key, value)) = line.split_once(':') else {
                continue;
            };
            let key = key.trim().to_ascii_lowercase();
            let value = value.trim();
            match key.as_str() {
                "user-agent" => {
                    if in_rules || current.is_none() {
                        if let Some(g) = current.take() {
                            out.groups.push(g);
                        }
                        current = Some(Group::default());
                        in_rules = false;
                    }
                    if let Some(g) = current.as_mut() {
                        g.agents.push(product_token(value));
                    }
                }
                "allow" | "disallow" => {
                    in_rules = true;
                    // Rules before any User-agent line belong to no group.
                    let Some(g) = current.as_mut() else { continue };
                    // An empty value is "no rule" (an empty Disallow allows all).
                    if value.is_empty() {
                        continue;
                    }
                    let allow = key == "allow";
                    g.rules.push(Rule {
                        allow,
                        pattern: encode_pattern(value),
                        line: format!("{}: {value}", if allow { "Allow" } else { "Disallow" }),
                    });
                }
                "crawl-delay" => {
                    in_rules = true;
                    if let Some(g) = current.as_mut()
                        && g.crawl_delay.is_none()
                        && let Ok(secs) = value.parse::<f64>()
                        && secs.is_finite()
                        && secs >= 0.0
                    {
                        g.crawl_delay = Some(secs);
                    }
                }
                "sitemap" if !value.is_empty() => out.sitemaps.push(value.to_string()),
                _ => {}
            }
        }
        if let Some(g) = current {
            out.groups.push(g);
        }
        out
    }

    /// Merges every group naming `token` (case-insensitive), or every `*`
    /// group when none does (RFC 9309 section 2.2.1).
    pub fn for_agent(&self, token: &str) -> RobotsPolicy {
        let token = token.to_ascii_lowercase();
        let specific: Vec<&Group> = self.groups.iter().filter(|g| g.agents.contains(&token)).collect();
        let (groups, matched) = if specific.is_empty() {
            let star: Vec<&Group> = self
                .groups
                .iter()
                .filter(|g| g.agents.iter().any(|a| a == "*"))
                .collect();
            let matched = (!star.is_empty()).then(|| "*".to_string());
            (star, matched)
        } else {
            (specific, Some(token))
        };
        RobotsPolicy {
            rules: groups.iter().flat_map(|g| g.rules.iter().cloned()).collect(),
            disallow_all: false,
            crawl_delay: groups
                .iter()
                .find_map(|g| g.crawl_delay)
                .map(Duration::from_secs_f64),
            sitemaps: self.sitemaps.clone(),
            matched_agent: matched,
        }
    }
}

impl RobotsPolicy {
    /// No robots.txt (4xx): everything is allowed.
    pub fn allow_all() -> Self {
        Self::default()
    }

    /// robots.txt unavailable (5xx, 429, unreachable): nothing is allowed.
    pub fn disallow_all() -> Self {
        Self {
            disallow_all: true,
            ..Self::default()
        }
    }

    pub fn is_disallow_all(&self) -> bool {
        self.disallow_all
    }

    pub fn check(&self, url: &Url) -> RobotsVerdict {
        if self.disallow_all {
            return RobotsVerdict::Disallowed("robots.txt unavailable".into());
        }
        let mut target = url.path().to_string();
        if target == "/robots.txt" {
            return RobotsVerdict::Allowed;
        }
        if let Some(q) = url.query() {
            target.push('?');
            target.push_str(q);
        }
        // Longest pattern wins; on a tie, Allow wins (RFC 9309 section 2.2.2).
        let best = self
            .rules
            .iter()
            .filter(|r| matches(&r.pattern, &target))
            .max_by_key(|r| (r.pattern.len(), r.allow));
        match best {
            Some(rule) if !rule.allow => RobotsVerdict::Disallowed(rule.line.clone()),
            _ => RobotsVerdict::Allowed,
        }
    }
}

/// `Googlebot/2.1` and `googlebot` both name the product token `googlebot`.
fn product_token(value: &str) -> String {
    value
        .split(|c: char| c == '/' || c.is_whitespace())
        .next()
        .unwrap_or("")
        .to_ascii_lowercase()
}

/// Percent-encodes non-ASCII bytes, then writes escapes in the same form as
/// crawl URLs (unreserved characters decoded, upper-case hex).
fn encode_pattern(value: &str) -> String {
    let mut ascii = String::with_capacity(value.len());
    for b in value.bytes() {
        if b.is_ascii() && !b.is_ascii_control() && b != b' ' {
            ascii.push(b as char);
        } else {
            ascii.push_str(&format!("%{b:02X}"));
        }
    }
    spiderlens_config::tidy_percent(&ascii)
}

/// Does `pattern` match a prefix of `path`? `*` matches any run of
/// characters and a final `$` anchors the end.
fn matches(pattern: &str, path: &str) -> bool {
    let (pattern, anchored) = match pattern.strip_suffix('$') {
        Some(p) => (p, true),
        None => (pattern, false),
    };
    let p = pattern.as_bytes();
    let s = path.as_bytes();
    let (mut pi, mut si) = (0, 0);
    let mut star: Option<(usize, usize)> = None;
    loop {
        if pi == p.len() {
            if !anchored || si == s.len() {
                return true;
            }
        } else if p[pi] == b'*' {
            star = Some((pi, si));
            pi += 1;
            continue;
        } else if si < s.len() && p[pi] == s[si] {
            pi += 1;
            si += 1;
            continue;
        }
        // Mismatch: let the last `*` absorb one more character.
        match star {
            Some((sp, ss)) if ss < s.len() => {
                star = Some((sp, ss + 1));
                pi = sp + 1;
                si = ss + 1;
            }
            _ => return false,
        }
    }
}

/// How fetching a host's robots.txt ended.
#[derive(Debug, Clone)]
pub enum RobotsFetch {
    /// 2xx: the file, plus a warning when it does not look like robots.txt.
    Found {
        status: u16,
        text: String,
        warning: Option<String>,
    },
    /// 4xx other than 429: no rules.
    NotFound { status: u16 },
    /// 5xx or 429 after retries.
    ServerError { status: u16 },
    /// DNS, connection, TLS or timeout after retries.
    Unreachable(CheckError),
}

impl RobotsFetch {
    /// The policy for `token`, following the cross-spec decision: 4xx allows
    /// all, 5xx/429/unreachable disallows all.
    pub fn policy(&self, token: &str) -> RobotsPolicy {
        match self {
            Self::Found { text, warning, .. } => {
                if warning.is_some() {
                    RobotsPolicy::allow_all()
                } else {
                    RobotsTxt::parse(text.as_bytes()).for_agent(token)
                }
            }
            Self::NotFound { .. } => RobotsPolicy::allow_all(),
            Self::ServerError { .. } | Self::Unreachable(_) => RobotsPolicy::disallow_all(),
        }
    }

    /// Worth another attempt under the retry policy.
    pub fn is_transient(&self) -> bool {
        matches!(self, Self::ServerError { .. } | Self::Unreachable(_))
    }
}

/// Fetches `/robots.txt` for the origin of `url` once, with the configured
/// custom headers (spec 004 FR-001). `headers` is already filtered to the
/// ones this host should get.
pub(crate) async fn fetch(client: &Client, url: &Url, headers: &[RequestHeader]) -> RobotsFetch {
    let robots_url = url
        .join("/robots.txt")
        .expect("joining an absolute path is valid");
    let keep_all = |_: Option<&str>| true;
    let policy = BodyPolicy {
        max_bytes: MAX_ROBOTS_BYTES,
        on_limit: OnLimit::Truncate,
        keep: &keep_all,
    };
    let outcome = get_following(client, &robots_url, MAX_ROBOTS_REDIRECTS, &policy, headers).await;
    let fetched = match outcome.result {
        Ok(f) => f,
        // Too many redirects on robots.txt counts as "no robots.txt".
        Err(e) if e.class == ErrorClass::RedirectProblem => return RobotsFetch::NotFound { status: 0 },
        Err(e) => return RobotsFetch::Unreachable(e),
    };
    let status = fetched.status.as_u16();
    match status {
        200..=299 => {
            let body = fetched.body.unwrap_or_default();
            let text = String::from_utf8_lossy(&body).into_owned();
            let looks_html =
                fetched.content_type.as_deref() == Some("text/html") || text.trim_start().starts_with('<');
            let warning = looks_html.then(|| {
                "robots.txt was served as HTML, not as rules; it was treated as allowing everything."
                    .to_string()
            });
            RobotsFetch::Found {
                status,
                text,
                warning,
            }
        }
        429 | 500..=599 => RobotsFetch::ServerError { status },
        _ => RobotsFetch::NotFound { status },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn url(s: &str) -> Url {
        Url::parse(s).unwrap()
    }

    fn verdict(txt: &str, token: &str, u: &str) -> RobotsVerdict {
        RobotsTxt::parse(txt.as_bytes()).for_agent(token).check(&url(u))
    }

    fn blocked(rule: &str) -> RobotsVerdict {
        RobotsVerdict::Disallowed(rule.into())
    }

    #[test]
    fn specific_group_overrides_star_and_names_the_rule() {
        let txt = "User-agent: SpiderLens\nDisallow: /private/\n\nUser-agent: *\nDisallow: /\n";
        assert_eq!(
            verdict(txt, "SpiderLens", "https://a.test/ok"),
            RobotsVerdict::Allowed
        );
        assert_eq!(
            verdict(txt, "spiderlens", "https://a.test/private/x"),
            blocked("Disallow: /private/")
        );
        assert_eq!(
            verdict(txt, "Googlebot", "https://a.test/ok"),
            blocked("Disallow: /")
        );
    }

    #[test]
    fn star_group_applies_when_no_specific_group() {
        let txt = "User-agent: *\nDisallow: /tmp/\n";
        assert_eq!(
            verdict(txt, "SpiderLens", "https://a.test/tmp/x"),
            blocked("Disallow: /tmp/")
        );
        assert_eq!(
            verdict(txt, "SpiderLens", "https://a.test/home"),
            RobotsVerdict::Allowed
        );
    }

    #[test]
    fn longest_match_wins_and_allow_wins_ties() {
        let txt = "User-agent: *\nDisallow: /a\nAllow: /a/b\nDisallow: /a/b/c\nAllow: /x\nDisallow: /x\n";
        assert_eq!(verdict(txt, "bot", "https://a.test/a/z"), blocked("Disallow: /a"));
        assert_eq!(
            verdict(txt, "bot", "https://a.test/a/b/z"),
            RobotsVerdict::Allowed
        );
        assert_eq!(
            verdict(txt, "bot", "https://a.test/a/b/c"),
            blocked("Disallow: /a/b/c")
        );
        assert_eq!(verdict(txt, "bot", "https://a.test/x"), RobotsVerdict::Allowed);
    }

    #[test]
    fn wildcards_and_end_anchor() {
        let txt = "User-agent: *\nDisallow: /*.pdf$\nDisallow: /*?sort=\nDisallow: /p*q/\n";
        assert_eq!(
            verdict(txt, "bot", "https://a.test/f/a.pdf"),
            blocked("Disallow: /*.pdf$")
        );
        assert_eq!(
            verdict(txt, "bot", "https://a.test/f/a.pdf?x=1"),
            RobotsVerdict::Allowed
        );
        assert_eq!(
            verdict(txt, "bot", "https://a.test/list?sort=asc"),
            blocked("Disallow: /*?sort=")
        );
        assert_eq!(
            verdict(txt, "bot", "https://a.test/pxxq/1"),
            blocked("Disallow: /p*q/")
        );
        assert_eq!(verdict(txt, "bot", "https://a.test/pxxq"), RobotsVerdict::Allowed);
    }

    #[test]
    fn groups_with_several_agents_and_merged_groups() {
        let txt = "User-agent: a\nUser-agent: SpiderLens/1.0\nDisallow: /one\n\nUser-agent: spiderlens\nDisallow: /two\n";
        assert_eq!(
            verdict(txt, "SpiderLens", "https://a.test/one"),
            blocked("Disallow: /one")
        );
        assert_eq!(
            verdict(txt, "SpiderLens", "https://a.test/two"),
            blocked("Disallow: /two")
        );
    }

    #[test]
    fn crawl_delay_sitemaps_comments_and_empty_rules() {
        let txt = "# hi\nSitemap: https://a.test/s.xml\nUser-agent: *\nCrawl-delay: 2 # polite\nDisallow:\n";
        let p = RobotsTxt::parse(txt.as_bytes()).for_agent("SpiderLens");
        assert_eq!(p.crawl_delay, Some(Duration::from_secs(2)));
        assert_eq!(p.sitemaps, vec!["https://a.test/s.xml"]);
        assert_eq!(p.matched_agent.as_deref(), Some("*"));
        assert_eq!(p.check(&url("https://a.test/anything")), RobotsVerdict::Allowed);
    }

    #[test]
    fn percent_encoding_and_non_ascii_patterns() {
        let txt = "User-agent: *\nDisallow: /%7Euser\nDisallow: /café\n";
        assert_eq!(
            verdict(txt, "bot", "https://a.test/~user/x"),
            blocked("Disallow: /%7Euser")
        );
        assert_eq!(
            verdict(txt, "bot", "https://a.test/caf%C3%A9"),
            blocked("Disallow: /café")
        );
    }

    #[test]
    fn robots_txt_itself_and_empty_file() {
        assert_eq!(
            verdict("User-agent: *\nDisallow: /\n", "bot", "https://a.test/robots.txt"),
            RobotsVerdict::Allowed
        );
        assert_eq!(verdict("", "bot", "https://a.test/x"), RobotsVerdict::Allowed);
    }

    #[test]
    fn rules_outside_groups_are_ignored() {
        assert_eq!(
            verdict("Disallow: /x\n", "bot", "https://a.test/x"),
            RobotsVerdict::Allowed
        );
    }

    #[test]
    fn hostile_patterns_are_fast() {
        let pattern = format!("User-agent: *\nDisallow: /{}b\n", "*a".repeat(200));
        let path = format!("https://a.test/{}", "a".repeat(5000));
        let start = std::time::Instant::now();
        assert_eq!(verdict(&pattern, "bot", &path), RobotsVerdict::Allowed);
        assert!(start.elapsed() < Duration::from_secs(1));
    }
}
