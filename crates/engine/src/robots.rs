//! robots.txt handling for the single-page check (FR-015, research R6).

use reqwest::Client;
use texting_robots::Robot;
use url::Url;

use crate::fetch::{BodyPolicy, OnLimit, get_following};
use crate::{CheckError, ErrorClass};

/// Google reads at most 500 KiB of robots.txt; so do we.
const MAX_ROBOTS_BYTES: u64 = 500 * 1024;
const MAX_ROBOTS_REDIRECTS: usize = 5;

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum RobotsDecision {
    Allowed,
    Disallowed,
}

/// Fetches the site's robots.txt and decides whether `page` may be fetched.
///
/// - 2xx: parse and apply the rules.
/// - 4xx (except 429): no rules, everything is allowed.
/// - 5xx or 429: the site is treated as fully disallowed.
/// - Network failure: returned as the check's error, because the page itself
///   cannot be reached either.
pub(crate) async fn check(client: &Client, page: &Url, token: &str) -> Result<RobotsDecision, CheckError> {
    let robots_url = page
        .join("/robots.txt")
        .expect("joining an absolute path is valid");
    let keep_all = |_: Option<&str>| true;
    let policy = BodyPolicy {
        max_bytes: MAX_ROBOTS_BYTES,
        on_limit: OnLimit::Truncate,
        keep: &keep_all,
    };
    let outcome = get_following(client, &robots_url, MAX_ROBOTS_REDIRECTS, &policy, &[]).await;
    let fetched = match outcome.result {
        Ok(f) => f,
        // Too many redirects on robots.txt counts as "no robots.txt".
        Err(e) if e.class == ErrorClass::RedirectLoop => return Ok(RobotsDecision::Allowed),
        Err(e) if e.class == ErrorClass::ContentProblem => return Ok(RobotsDecision::Disallowed),
        Err(e) => return Err(e),
    };

    let status = fetched.status.as_u16();
    Ok(match status {
        200..=299 => decide(&fetched.body.unwrap_or_default(), page, token),
        429 | 500..=599 => RobotsDecision::Disallowed,
        _ => RobotsDecision::Allowed,
    })
}

fn decide(robots_txt: &[u8], page: &Url, token: &str) -> RobotsDecision {
    match Robot::new(token, robots_txt) {
        Ok(robot) if !robot.allowed(page.as_str()) => RobotsDecision::Disallowed,
        // Unparseable files impose no rules, as with search engines.
        _ => RobotsDecision::Allowed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn url(s: &str) -> Url {
        Url::parse(s).unwrap()
    }

    #[test]
    fn specific_group_overrides_star() {
        let txt = b"User-agent: SpiderLens\nDisallow: /private/\n\nUser-agent: *\nDisallow: /\n";
        assert_eq!(
            decide(txt, &url("https://a.test/ok"), "SpiderLens"),
            RobotsDecision::Allowed
        );
        assert_eq!(
            decide(txt, &url("https://a.test/private/x"), "SpiderLens"),
            RobotsDecision::Disallowed
        );
    }

    #[test]
    fn star_group_applies_when_no_specific_group() {
        let txt = b"User-agent: *\nDisallow: /admin\n";
        assert_eq!(
            decide(txt, &url("https://a.test/admin/x"), "SpiderLens"),
            RobotsDecision::Disallowed
        );
        assert_eq!(
            decide(txt, &url("https://a.test/home"), "SpiderLens"),
            RobotsDecision::Allowed
        );
    }

    #[test]
    fn empty_file_allows_everything() {
        assert_eq!(
            decide(b"", &url("https://a.test/x"), "SpiderLens"),
            RobotsDecision::Allowed
        );
    }
}
