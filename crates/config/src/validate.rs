//! Validation (FR-007, SC-008, SC-009; research R1).
//!
//! Returns every issue at once instead of stopping at the first, each with
//! the setting's path and how to fix it. Errors block starting a crawl;
//! warnings mark settings that are less respectful than the defaults and
//! must be shown by every interface.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::patterns::{Decision, UrlFilter, check_pattern};
use crate::presets::{UaPreset, suggest_robots_token};
use crate::{CrawlConfig, HeaderValue, Mode, ModeInput, RobotsMode, WindowPreset, normalize_url};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum Severity {
    Error,
    Warning,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Issue {
    /// Setting path, for example `scope.include[1]` or `input.start_url`.
    pub path: String,
    pub severity: Severity,
    /// Stable identifier for tests and the UI.
    pub code: String,
    pub message: String,
}

/// Headers SpiderLens sets itself (FR-020).
const RESERVED_HEADERS: [&str; 10] = [
    "host",
    "user-agent",
    "content-length",
    "connection",
    "transfer-encoding",
    "keep-alive",
    "upgrade",
    "te",
    "trailer",
    "proxy-connection",
];

struct Issues(Vec<Issue>);

impl Issues {
    fn error(&mut self, path: impl Into<String>, code: &str, message: impl Into<String>) {
        self.push(path, Severity::Error, code, message);
    }

    fn warn(&mut self, path: impl Into<String>, code: &str, message: impl Into<String>) {
        self.push(path, Severity::Warning, code, message);
    }

    fn push(&mut self, path: impl Into<String>, severity: Severity, code: &str, message: impl Into<String>) {
        self.0.push(Issue {
            path: path.into(),
            severity,
            code: code.into(),
            message: message.into(),
        });
    }
}

/// A host name with no dot, such as `r` or `intranet`, other than
/// `localhost`. Such start URLs are almost always a half-typed address
/// (`r` while typing `ryware.dev`), so Start refuses them. IP addresses
/// are fine.
fn is_single_label(url: &url::Url) -> bool {
    matches!(url.host(), Some(url::Host::Domain(d))
        if !d.trim_end_matches('.').contains('.') && !d.eq_ignore_ascii_case("localhost"))
}

/// Validates a configuration together with what the crawl starts from.
pub fn validate(config: &CrawlConfig, input: &ModeInput) -> Vec<Issue> {
    let mut issues = Issues(validate_config(config));
    match (config.mode, input) {
        (Mode::Spider, ModeInput::Spider { start_url }) => match normalize_url(start_url) {
            Ok(url) if is_single_label(&url) => issues.error(
                "input.start_url",
                "start_url_no_dot",
                format!(
                    "\"{}\" is not a full web address. Enter the whole domain, such as example.com.",
                    url.host_str().unwrap_or_default()
                ),
            ),
            Ok(url) => {
                if let Ok(filter) = UrlFilter::new(&config.scope.include, &config.scope.exclude) {
                    match filter.decide(url.as_str()) {
                        Decision::Excluded { exclude } => issues.error(
                            "input.start_url",
                            "start_url_excluded",
                            format!(
                                "Exclude pattern {} matches the start URL, so nothing would be crawled. \
                                 Change the pattern or the start URL.",
                                config.scope.exclude[exclude as usize]
                            ),
                        ),
                        Decision::NotIncluded => issues.warn(
                            "input.start_url",
                            "start_url_not_included",
                            "The start URL does not match your include patterns. It will still be crawled, \
                             but only links matching the include patterns will be followed.",
                        ),
                        Decision::Crawl { .. } => {}
                    }
                }
            }
            Err(e) => issues.error(
                "input.start_url",
                "start_url_invalid",
                format!("The start URL is not usable: {}. Enter an address such as https://example.com/.", e.0),
            ),
        },
        (Mode::List, ModeInput::List(list)) => {
            if list.urls.is_empty() {
                issues.error(
                    "input.urls",
                    "list_empty",
                    "The list has no valid web addresses. Paste or upload at least one URL.",
                );
            }
        }
        _ => issues.error(
            "mode",
            "mode_input_mismatch",
            "The crawl mode does not match the input. Enter a start URL for Spider mode or a list for List mode.",
        ),
    }
    issues.0
}

/// Validates the settings alone, as for a profile.
pub fn validate_config(c: &CrawlConfig) -> Vec<Issue> {
    let mut i = Issues(Vec::new());

    // Limits
    if c.limits.max_urls == Some(0) {
        i.error(
            "limits.max_urls",
            "limit_zero",
            "Max URLs must be at least 1. Leave it blank for no limit.",
        );
    }
    if c.limits.max_depth == Some(0) {
        i.error(
            "limits.max_depth",
            "limit_zero",
            "Max depth must be at least 1. Leave it blank for no limit.",
        );
    }
    if c.mode == Mode::List && c.limits.max_depth.is_some() {
        i.warn(
            "limits.max_depth",
            "depth_in_list_mode",
            "Max depth does not apply in List mode and is ignored.",
        );
    }

    // Patterns
    for (group, list) in [("include", &c.scope.include), ("exclude", &c.scope.exclude)] {
        for (index, p) in list.iter().enumerate() {
            if let Err(message) = check_pattern(p) {
                i.error(format!("scope.{group}[{index}]"), "pattern_syntax", message);
            }
        }
    }

    // Scope additions (spec 003)
    if !(256..=65_536).contains(&c.scope.max_url_length) {
        i.error(
            "scope.max_url_length",
            "out_of_range",
            "Maximum URL length must be between 256 and 65,536 characters.",
        );
    }
    let q = &c.scope.query_params;
    if q.mode == crate::QueryMode::Remove && q.names.iter().all(|n| n.trim().is_empty()) {
        i.error(
            "scope.query_params.names",
            "query_names_missing",
            "List at least one parameter name to remove, or choose another option.",
        );
    }

    // Identity
    let ua = &c.identity.user_agent;
    if ua.preset == UaPreset::Custom {
        match ua.custom_string.as_deref().map(str::trim) {
            None | Some("") => i.error(
                "identity.user_agent.custom_string",
                "ua_custom_missing",
                "Enter the custom user agent string, or pick a preset.",
            ),
            Some(s) if s.chars().any(|ch| ch.is_control()) => i.error(
                "identity.user_agent.custom_string",
                "ua_invalid",
                "The user agent cannot contain line breaks or control characters.",
            ),
            Some(_) => {}
        }
        if ua.robots_token.as_deref().is_none_or(|t| t.trim().is_empty()) {
            let hint = ua
                .custom_string
                .as_deref()
                .and_then(suggest_robots_token)
                .map(|t| format!(" Suggested: {t}."))
                .unwrap_or_default();
            i.error(
                "identity.user_agent.robots_token",
                "robots_token_missing",
                format!("A custom user agent needs the token used to match robots.txt rules.{hint}"),
            );
        }
    }
    if ua.preset != UaPreset::Spiderlens {
        i.warn(
            "identity.user_agent.preset",
            "ua_not_spiderlens",
            "The crawler will no longer identify itself as SpiderLens.",
        );
    }

    // Headers
    let mut seen: Vec<String> = Vec::new();
    for (index, h) in c.identity.headers.iter().enumerate() {
        let path = format!("identity.headers[{index}]");
        let lower = h.name.trim().to_ascii_lowercase();
        if lower.is_empty() {
            i.error(
                format!("{path}.name"),
                "header_name_empty",
                "Enter a header name.",
            );
        } else if !lower.bytes().all(is_token_byte) {
            i.error(
                format!("{path}.name"),
                "header_name_invalid",
                "Header names may only contain letters, digits and ! # $ % & ' * + - . ^ _ ` | ~.",
            );
        } else if RESERVED_HEADERS.contains(&lower.as_str()) {
            let where_set = if lower == "user-agent" {
                "Set the user agent under Identity > User agent instead."
            } else {
                "SpiderLens sets this header itself."
            };
            i.error(
                format!("{path}.name"),
                "header_reserved",
                format!("{} cannot be set here. {where_set}", h.name.trim()),
            );
        } else if seen.contains(&lower) {
            i.error(
                format!("{path}.name"),
                "header_duplicate",
                format!(
                    "{} is already in the list. Each header can appear once.",
                    h.name.trim()
                ),
            );
        }
        seen.push(lower);

        match &h.value {
            HeaderValue::Secret(None) => i.error(
                format!("{path}.value"),
                "header_secret_missing",
                "This secret value is not saved in files. Enter it again to start the crawl.",
            ),
            v => {
                if v.expose()
                    .is_some_and(|s| s.chars().any(|ch| ch == '\r' || ch == '\n' || ch == '\0'))
                {
                    i.error(
                        format!("{path}.value"),
                        "header_value_invalid",
                        "Header values cannot contain line breaks.",
                    );
                }
            }
        }
    }

    // Speed
    let s = &c.speed;
    if !(1..=512).contains(&s.max_concurrent) {
        i.error(
            "speed.max_concurrent",
            "out_of_range",
            "Max concurrent requests must be between 1 and 512.",
        );
    }
    if s.max_per_host < 1 || s.max_per_host > s.max_concurrent {
        i.error(
            "speed.max_per_host",
            "out_of_range",
            "Requests per site must be at least 1 and no more than max concurrent requests.",
        );
    }
    if let Some(rate) = s.max_urls_per_sec
        && !(rate.is_finite() && rate > 0.0)
    {
        i.error(
            "speed.max_urls_per_sec",
            "out_of_range",
            "URLs per second must be above 0. Leave it blank for no cap.",
        );
    }
    if s.faster_than_polite() {
        i.warn(
            "speed",
            "faster_than_polite",
            "Faster settings are intended for sites you control.",
        );
    }

    // Robots
    match c.robots.mode {
        RobotsMode::Respect => {}
        RobotsMode::IgnoreReport => i.warn(
            "robots.mode",
            "robots_ignored",
            "Pages disallowed by robots.txt will be crawled. Use this only on sites you control.",
        ),
        RobotsMode::Ignore => i.warn(
            "robots.mode",
            "robots_ignored",
            "robots.txt will not be read at all. Use this only on sites you control.",
        ),
    }
    if !c.robots.honour_crawl_delay {
        i.warn(
            "robots.honour_crawl_delay",
            "crawl_delay_ignored",
            "The site's requested crawl-delay will be ignored.",
        );
    }

    // Network
    for (path, v) in [
        ("network.connect_timeout_secs", c.network.connect_timeout_secs),
        ("network.read_timeout_secs", c.network.read_timeout_secs),
    ] {
        if !(v.is_finite() && v > 0.0 && v <= 3600.0) {
            i.error(
                path,
                "out_of_range",
                "Timeouts must be between 0 and 3600 seconds.",
            );
        }
    }
    if c.network.max_redirects > 20 {
        i.error(
            "network.max_redirects",
            "out_of_range",
            "Redirect hops must be between 0 and 20.",
        );
    }
    if !(1..=4096).contains(&c.network.max_body_mb) {
        i.error(
            "network.max_body_mb",
            "out_of_range",
            "The size limit must be between 1 and 4096 MB.",
        );
    }

    // Retry
    if !(1..=10).contains(&c.retry.attempts) {
        i.error(
            "retry.attempts",
            "out_of_range",
            "Attempts must be between 1 and 10 (1 means no retry).",
        );
    }
    if !(c.retry.max_wait_secs.is_finite() && c.retry.max_wait_secs > 0.0 && c.retry.max_wait_secs <= 3600.0)
    {
        i.error(
            "retry.max_wait_secs",
            "out_of_range",
            "The maximum wait must be between 0 and 3600 seconds.",
        );
    }

    validate_rendering(c, &mut i);

    i.0
}

/// JavaScript rendering (spec 014, data-model.md "Settings"). Ranges are
/// checked whatever the mode, so a saved profile is never half valid.
fn validate_rendering(c: &CrawlConfig, i: &mut Issues) {
    let r = &c.rendering;
    let cap = r.max_render_secs;
    let cap_ok = cap.is_finite() && (1.0..=60.0).contains(&cap);
    if !cap_ok {
        i.error(
            "rendering.max_render_secs",
            "out_of_range",
            "The maximum render time must be between 1 and 60 seconds.",
        );
    }
    let wait = r.min_wait_secs;
    if !(wait.is_finite() && wait >= 0.0 && (!cap_ok || wait <= cap)) {
        i.error(
            "rendering.min_wait_secs",
            "out_of_range",
            "The minimum wait must be between 0 seconds and the maximum render time.",
        );
    }
    if r.window.preset == WindowPreset::Custom {
        match r.window.width {
            Some(w) if (320..=3_840).contains(&w) => {}
            _ => i.error(
                "rendering.window.width",
                "out_of_range",
                "A custom window width must be between 320 and 3,840 pixels.",
            ),
        }
        match r.window.height {
            Some(h) if (480..=4_320).contains(&h) => {}
            _ => i.error(
                "rendering.window.height",
                "out_of_range",
                "A custom window height must be between 480 and 4,320 pixels.",
            ),
        }
    }
    for (group, list) in [("include", &r.include), ("exclude", &r.exclude)] {
        for (index, p) in list.iter().enumerate() {
            if let Err(message) = check_pattern(p) {
                i.error(format!("rendering.{group}[{index}]"), "pattern_syntax", message);
            }
        }
    }
    if r.concurrency.is_some_and(|n| !(1..=16).contains(&n)) {
        i.error(
            "rendering.concurrency",
            "out_of_range",
            "Pages rendered at once must be between 1 and 16. Leave it blank for automatic.",
        );
    }
    if r.memory_budget_mb.is_some_and(|n| !(256..=65_536).contains(&n)) {
        i.error(
            "rendering.memory_budget_mb",
            "out_of_range",
            "The browser memory budget must be between 256 and 65,536 MB. Leave it blank for automatic.",
        );
    }
    if !(1..=1_000).contains(&r.js_content_min_percent) {
        i.error(
            "rendering.js_content_min_percent",
            "out_of_range",
            "The JavaScript content threshold must be between 1 and 1,000 percent.",
        );
    }
    if r.js_content_min_words > 100_000 {
        i.error(
            "rendering.js_content_min_words",
            "out_of_range",
            "The JavaScript content word threshold must be between 0 and 100,000 words.",
        );
    }
    if !(1..=100).contains(&r.hidden_content_percent) {
        i.error(
            "rendering.hidden_content_percent",
            "out_of_range",
            "The hidden content threshold must be between 1 and 100 percent.",
        );
    }
    if r.is_on() && c.robots.mode == RobotsMode::Ignore {
        i.warn(
            "rendering.mode",
            "rendering_robots_ignored",
            "robots.txt is not read, so pages will load scripts and other files that robots.txt may disallow.",
        );
    }
}

/// RFC 9110 token characters.
fn is_token_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&b)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CustomHeader, HeaderTarget, ListInput, SpeedSettings};

    fn spider(url: &str) -> ModeInput {
        ModeInput::Spider {
            start_url: url.into(),
        }
    }

    fn codes(issues: &[Issue]) -> Vec<(&str, &str, Severity)> {
        issues
            .iter()
            .map(|i| (i.path.as_str(), i.code.as_str(), i.severity))
            .collect()
    }

    #[test]
    fn defaults_are_clean() {
        assert!(validate(&CrawlConfig::default(), &spider("example.com")).is_empty());
    }

    #[test]
    fn start_url_gets_https_and_bad_start_url_is_error() {
        assert!(validate(&CrawlConfig::default(), &spider("example.com")).is_empty());
        let issues = validate(&CrawlConfig::default(), &spider("ht!tp//bad"));
        assert_eq!(
            codes(&issues),
            vec![("input.start_url", "start_url_invalid", Severity::Error)]
        );
    }

    #[test]
    fn host_without_a_dot_is_error() {
        for bad in ["r", "https://r/", "ryware", "intranet/a"] {
            assert_eq!(
                codes(&validate(&CrawlConfig::default(), &spider(bad))),
                vec![("input.start_url", "start_url_no_dot", Severity::Error)],
                "{bad}"
            );
        }
        for good in [
            "ryware.dev",
            "localhost:3000",
            "http://127.0.0.1:8080/",
            "http://[::1]/",
        ] {
            assert!(
                validate(&CrawlConfig::default(), &spider(good)).is_empty(),
                "{good}"
            );
        }
    }

    #[test]
    fn empty_list_is_error() {
        let c = CrawlConfig {
            mode: Mode::List,
            ..Default::default()
        };
        let issues = validate(&c, &ModeInput::List(ListInput::default()));
        assert_eq!(
            codes(&issues),
            vec![("input.urls", "list_empty", Severity::Error)]
        );
    }

    #[test]
    fn pattern_syntax_error_names_field() {
        let mut c = CrawlConfig::default();
        c.scope.include = vec!["/ok/".into(), "(bad".into()];
        let issues = validate_config(&c);
        assert_eq!(
            codes(&issues),
            vec![("scope.include[1]", "pattern_syntax", Severity::Error)]
        );
    }

    #[test]
    fn start_url_excluded_is_error_and_not_included_warns() {
        let mut c = CrawlConfig::default();
        c.scope.exclude = vec!["example".into()];
        let issues = validate(&c, &spider("example.com"));
        assert_eq!(
            codes(&issues),
            vec![("input.start_url", "start_url_excluded", Severity::Error)]
        );
        let mut c = CrawlConfig::default();
        c.scope.include = vec!["/blog/".into()];
        let issues = validate(&c, &spider("example.com"));
        assert_eq!(
            codes(&issues),
            vec![("input.start_url", "start_url_not_included", Severity::Warning)]
        );
    }

    #[test]
    fn scope_additions_are_validated() {
        let mut c = CrawlConfig::default();
        c.scope.max_url_length = 10;
        c.scope.query_params.mode = crate::QueryMode::Remove;
        let issues = validate_config(&c);
        assert_eq!(
            codes(&issues),
            vec![
                ("scope.max_url_length", "out_of_range", Severity::Error),
                ("scope.query_params.names", "query_names_missing", Severity::Error),
            ]
        );
    }

    #[test]
    fn custom_ua_requires_token_with_suggestion() {
        let mut c = CrawlConfig::default();
        c.identity.user_agent.preset = UaPreset::Custom;
        c.identity.user_agent.custom_string = Some("AcmeBot/1.0".into());
        let issues = validate_config(&c);
        let token = issues.iter().find(|i| i.code == "robots_token_missing").unwrap();
        assert!(token.message.contains("Suggested: AcmeBot"));
        assert!(issues.iter().any(|i| i.code == "ua_not_spiderlens"));
    }

    #[test]
    fn reserved_and_duplicate_headers() {
        let mut c = CrawlConfig::default();
        let h = |n: &str| CustomHeader {
            name: n.into(),
            value: HeaderValue::Plain("v".into()),
            send_to: HeaderTarget::Internal,
        };
        c.identity.headers = vec![h("User-Agent"), h("X-Key"), h("x-key"), h("Bad Name"), h("Host")];
        let issues = validate_config(&c);
        assert_eq!(
            codes(&issues),
            vec![
                ("identity.headers[0].name", "header_reserved", Severity::Error),
                ("identity.headers[2].name", "header_duplicate", Severity::Error),
                ("identity.headers[3].name", "header_name_invalid", Severity::Error),
                ("identity.headers[4].name", "header_reserved", Severity::Error),
            ]
        );
        assert!(issues[0].message.contains("Identity > User agent"));
    }

    #[test]
    fn missing_secret_blocks_start() {
        let mut c = CrawlConfig::default();
        c.identity.headers.push(CustomHeader {
            name: "Authorization".into(),
            value: HeaderValue::Secret(None),
            send_to: HeaderTarget::Internal,
        });
        assert_eq!(validate_config(&c)[0].code, "header_secret_missing");
    }

    #[test]
    fn fast_warns_and_polite_does_not() {
        let mut c = CrawlConfig::default();
        assert!(validate_config(&c).is_empty());
        c.speed = SpeedSettings::FAST;
        assert_eq!(
            codes(&validate_config(&c)),
            vec![("speed", "faster_than_polite", Severity::Warning)]
        );
    }

    #[test]
    fn robots_modes_warn() {
        let mut c = CrawlConfig::default();
        c.robots.mode = RobotsMode::IgnoreReport;
        c.robots.honour_crawl_delay = false;
        let got: Vec<_> = validate_config(&c).into_iter().map(|i| i.code).collect();
        assert_eq!(got, vec!["robots_ignored", "crawl_delay_ignored"]);
    }

    #[test]
    fn every_less_respectful_setting_warns() {
        // SC-009: one warning per less-respectful choice, never silent.
        let mut c = CrawlConfig::default();
        c.identity.user_agent.preset = UaPreset::GooglebotSmartphone;
        c.speed.max_per_host = 5;
        c.robots.mode = RobotsMode::Ignore;
        c.robots.honour_crawl_delay = false;
        let warnings: Vec<_> = validate_config(&c)
            .into_iter()
            .filter(|i| i.severity == Severity::Warning)
            .map(|i| i.code)
            .collect();
        assert_eq!(
            warnings,
            vec![
                "ua_not_spiderlens",
                "faster_than_polite",
                "robots_ignored",
                "crawl_delay_ignored"
            ]
        );
    }

    #[test]
    fn ranges() {
        let mut c = CrawlConfig::default();
        c.limits.max_urls = Some(0);
        c.speed.max_per_host = 9;
        c.retry.attempts = 11;
        c.network.read_timeout_secs = -1.0;
        let got: Vec<_> = validate_config(&c).into_iter().map(|i| i.path).collect();
        assert!(got.contains(&"limits.max_urls".to_string()));
        assert!(got.contains(&"speed.max_per_host".to_string()));
        assert!(got.contains(&"retry.attempts".to_string()));
        assert!(got.contains(&"network.read_timeout_secs".to_string()));
    }

    #[test]
    fn rendering_defaults_are_clean_and_ranges_are_checked() {
        let mut c = CrawlConfig::default();
        c.rendering.mode = crate::RenderMode::Javascript;
        assert!(validate_config(&c).is_empty());
        c.rendering.max_render_secs = 61.0;
        c.rendering.min_wait_secs = -1.0;
        c.rendering.concurrency = Some(17);
        c.rendering.memory_budget_mb = Some(255);
        c.rendering.js_content_min_percent = 0;
        c.rendering.js_content_min_words = 100_001;
        c.rendering.hidden_content_percent = 101;
        let got = validate_config(&c);
        assert_eq!(
            codes(&got),
            vec![
                ("rendering.max_render_secs", "out_of_range", Severity::Error),
                ("rendering.min_wait_secs", "out_of_range", Severity::Error),
                ("rendering.concurrency", "out_of_range", Severity::Error),
                ("rendering.memory_budget_mb", "out_of_range", Severity::Error),
                (
                    "rendering.js_content_min_percent",
                    "out_of_range",
                    Severity::Error
                ),
                ("rendering.js_content_min_words", "out_of_range", Severity::Error),
                (
                    "rendering.hidden_content_percent",
                    "out_of_range",
                    Severity::Error
                ),
            ]
        );
    }

    #[test]
    fn rendering_range_edges_are_allowed() {
        let mut c = CrawlConfig::default();
        let r = &mut c.rendering;
        r.max_render_secs = 60.0;
        r.min_wait_secs = 60.0;
        r.concurrency = Some(16);
        r.memory_budget_mb = Some(65_536);
        r.js_content_min_percent = 1_000;
        r.js_content_min_words = 0;
        r.hidden_content_percent = 1;
        r.window.preset = WindowPreset::Custom;
        r.window.width = Some(320);
        r.window.height = Some(4_320);
        assert!(validate_config(&c).is_empty());
        c.rendering.max_render_secs = 1.0;
        c.rendering.min_wait_secs = 1.5;
        c.rendering.concurrency = Some(1);
        assert_eq!(
            codes(&validate_config(&c)),
            vec![("rendering.min_wait_secs", "out_of_range", Severity::Error)]
        );
    }

    #[test]
    fn custom_window_needs_both_sizes_in_range() {
        let mut c = CrawlConfig::default();
        c.rendering.window.preset = WindowPreset::Custom;
        assert_eq!(
            codes(&validate_config(&c)),
            vec![
                ("rendering.window.width", "out_of_range", Severity::Error),
                ("rendering.window.height", "out_of_range", Severity::Error),
            ]
        );
        c.rendering.window.width = Some(3_841);
        c.rendering.window.height = Some(479);
        assert_eq!(validate_config(&c).len(), 2);
        // Sizes are ignored for a preset.
        c.rendering.window.preset = WindowPreset::Desktop;
        assert!(validate_config(&c).is_empty());
    }

    #[test]
    fn rendering_patterns_use_the_scope_pattern_rules() {
        let mut c = CrawlConfig::default();
        c.rendering.include = vec!["/app/".into(), "(".into()];
        c.rendering.exclude = vec!["[".into()];
        assert_eq!(
            codes(&validate_config(&c)),
            vec![
                ("rendering.include[1]", "pattern_syntax", Severity::Error),
                ("rendering.exclude[0]", "pattern_syntax", Severity::Error),
            ]
        );
    }

    #[test]
    fn rendering_with_robots_ignored_warns() {
        let mut c = CrawlConfig::default();
        c.robots.mode = RobotsMode::Ignore;
        assert!(
            !validate_config(&c)
                .iter()
                .any(|i| i.code == "rendering_robots_ignored")
        );
        c.rendering.mode = crate::RenderMode::Javascript;
        let got = validate_config(&c);
        let warning = got
            .iter()
            .find(|i| i.code == "rendering_robots_ignored")
            .expect("warning");
        assert_eq!(warning.severity, Severity::Warning);
        assert_eq!(warning.path, "rendering.mode");
        // Respecting robots.txt (or ignoring it but reporting) gives no warning.
        c.robots.mode = RobotsMode::IgnoreReport;
        assert!(
            !validate_config(&c)
                .iter()
                .any(|i| i.code == "rendering_robots_ignored")
        );
    }
}
