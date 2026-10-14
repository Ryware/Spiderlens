//! Testing rules on one page before a crawl (FR-018, US4).

use url::Url;

use super::{CompiledRules, PageRules};
use crate::check::parse_input_url;
use crate::extract::{PageInput, extract_page_with};
use crate::fetch::{BodyPolicy, OnLimit, build_client, get_following};
use crate::robots::{self, RobotsFetch, RobotsVerdict};
use crate::title::is_html;
use crate::{CheckError, CheckOptions, ErrorClass};

/// What a test on a typed URL fetched.
pub struct Tested {
    pub final_url: Url,
    pub status: u16,
    pub rules: PageRules,
}

/// Fetches one URL with the crawl's identity and headers, following
/// redirects, honouring robots.txt when `respect_robots`, and applies the
/// rules to it. One request for the page (plus robots.txt).
pub async fn test_url(
    input: &str,
    opts: &CheckOptions,
    respect_robots: bool,
    rules: &CompiledRules,
) -> Result<Tested, CheckError> {
    let url = parse_input_url(input)?;
    let client = build_client(opts)?;
    if respect_robots {
        let fetch = robots::fetch(&client, &url, &opts.headers).await;
        if let RobotsFetch::Unreachable(e) = &fetch
            && e.class != ErrorClass::ContentProblem
        {
            return Err(e.clone());
        }
        if let RobotsVerdict::Disallowed(rule) = fetch.policy(&opts.robots_token).check(&url) {
            return Err(CheckError {
                class: ErrorClass::ContentProblem,
                reason: None,
                message: format!("robots.txt does not allow this page ({rule})."),
                attempts: 0,
            });
        }
    }
    let keep = |_: Option<&str>| true;
    let policy = BodyPolicy {
        max_bytes: opts.max_body_bytes,
        on_limit: OnLimit::Fail,
        keep: &keep,
    };
    let fetched = get_following(&client, &url, opts.max_redirects, &policy, &opts.headers)
        .await
        .result?;
    let body = fetched.body.unwrap_or_default();
    if !is_html(fetched.content_type.as_deref(), &body) {
        return Err(CheckError {
            class: ErrorClass::ContentProblem,
            reason: None,
            message: format!(
                "The page is not HTML ({}); rules run on HTML pages.",
                fetched.content_type.as_deref().unwrap_or("no content type")
            ),
            attempts: 1,
        });
    }
    let d = extract_page_with(
        &PageInput {
            url: &fetched.final_url,
            charset: fetched.charset.as_deref(),
            body: &body,
            truncated: false,
            robots_token: &opts.robots_token,
        },
        Some(rules),
    );
    Ok(Tested {
        final_url: fetched.final_url,
        status: fetched.status.as_u16(),
        rules: d.custom.unwrap_or_else(|| rules.on_empty_page()),
    })
}
