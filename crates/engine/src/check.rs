//! The single-page check: robots.txt, then fetch, then title (FR-012..FR-021).

use std::time::Instant;

use url::Url;

use crate::fetch::{BodyPolicy, OnLimit, build_client, get_following};
use crate::robots::{self, RobotsFetch, RobotsVerdict};
use crate::title::{extract_title, is_html};
use crate::{CheckError, CheckOptions, ErrorClass, FinalState, PageCheckResult, Title};

/// Parses user input into an http(s) URL. A missing scheme means `https://`.
/// No network activity happens here (spec scenario US2-7).
pub fn parse_input_url(input: &str) -> Result<Url, CheckError> {
    spiderlens_config::normalize_url(input).map_err(|e| CheckError {
        class: ErrorClass::MalformedUrl,
        reason: None,
        message: format!("\"{}\" is not a valid web address: {}.", input.trim(), e.0),
        attempts: 0,
    })
}

/// Runs one single-page check. It never returns a Rust error: every outcome,
/// including failures, is a `PageCheckResult` with exactly one final state.
pub async fn check_page(input: &str, opts: &CheckOptions) -> PageCheckResult {
    let url = match parse_input_url(input) {
        Ok(url) => url,
        Err(e) => return PageCheckResult::failed(input.trim(), e),
    };
    let requested = url.to_string();

    let client = match build_client(opts) {
        Ok(c) => c,
        Err(e) => return PageCheckResult::failed(requested, e),
    };

    // A network failure on robots.txt is the page's failure too: the host
    // cannot be reached (spec 001 research R6).
    let fetch = robots::fetch(&client, &url, &opts.headers).await;
    match &fetch {
        RobotsFetch::Unreachable(e) if e.class != ErrorClass::ContentProblem => {
            return PageCheckResult::failed(requested, e.clone());
        }
        _ => {}
    }
    if let RobotsVerdict::Disallowed(_) = fetch.policy(&opts.robots_token).check(&url) {
        return PageCheckResult::empty(requested, FinalState::BlockedByRobots);
    }

    let keep_html = |ct: Option<&str>| ct.is_none() || is_html(ct, b"");
    let policy = BodyPolicy {
        max_bytes: opts.max_body_bytes,
        on_limit: OnLimit::Fail,
        keep: &keep_html,
    };
    let started = Instant::now();
    let outcome = get_following(&client, &url, opts.max_redirects, &policy, &opts.headers).await;
    let elapsed = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);

    let mut result = PageCheckResult::empty(requested, FinalState::Fetched);
    result.redirects = outcome.hops;
    result.attempts = 1;
    match outcome.result {
        Ok(fetched) => {
            let body = fetched.body.unwrap_or_default();
            result.title = Some(if is_html(fetched.content_type.as_deref(), &body) {
                extract_title(fetched.charset.as_deref(), &body)
            } else {
                Title::NotApplicable
            });
            result.final_url = Some(fetched.final_url.to_string());
            result.status = Some(fetched.status.as_u16());
            result.content_type = fetched.content_type;
            result.response_time_ms = Some(elapsed);
        }
        Err(e) => {
            result.state = FinalState::Failed;
            result.error = Some(e);
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_scheme_defaults_to_https() {
        assert_eq!(
            parse_input_url("example.com").unwrap().as_str(),
            "https://example.com/"
        );
    }

    #[test]
    fn malformed_inputs_are_rejected() {
        for bad in [
            "",
            "   ",
            "not a url",
            "ftp://example.com",
            "http://",
            "https://exa mple.com",
        ] {
            let err = parse_input_url(bad).unwrap_err();
            assert_eq!(err.class, ErrorClass::MalformedUrl, "input {bad:?}");
            assert_eq!(err.attempts, 0);
        }
    }
}
