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

impl Issue {
    pub(crate) fn error(path: impl Into<String>, code: &str, message: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            severity: Severity::Error,
            code: code.into(),
            message: message.into(),
        }
    }
}

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

    // Custom rules (spec 013)
    for p in crate::custom::check_rules(&c.custom) {
        if p.warning {
            i.warn(p.path, p.code, p.message);
        } else {
            i.error(p.path, p.code, p.message);
        }
    }

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

    // Sitemaps (spec 011)
    let http_url = |s: &str| {
        url::Url::parse(s.trim())
            .ok()
            .filter(|u| matches!(u.scheme(), "http" | "https") && u.host().is_some())
    };
    for (index, u) in c.scope.sitemaps.urls.iter().enumerate() {
        if http_url(u).is_none() {
            i.error(
                format!("scope.sitemaps.urls[{index}]"),
                "sitemap_url_invalid",
                "Enter the full sitemap address, starting with http:// or https://.",
            );
        }
    }
    for (index, f) in c.scope.sitemaps.files.iter().enumerate() {
        if f.path.trim().is_empty() {
            i.error(
                format!("scope.sitemaps.files[{index}].path"),
                "sitemap_file_missing",
                "Choose the sitemap file.",
            );
        }
        if http_url(&f.base_url).is_none() {
            i.error(
                format!("scope.sitemaps.files[{index}].base_url"),
                "sitemap_base_invalid",
                "Enter the address this file will be published at, starting with http:// or https://.",
            );
        }
    }
    if !c.scope.sitemaps.read && (!c.scope.sitemaps.urls.is_empty() || !c.scope.sitemaps.files.is_empty()) {
        i.warn(
            "scope.sitemaps.read",
            "sitemaps_off",
            "Sitemaps you added are not read while \"Read sitemaps\" is off.",
        );
    }

    // Sitemap generation (spec 011 FR-021)
    let g = &c.sitemap_generate;
    if !(1..=50_000).contains(&g.max_urls_per_file) {
        i.error(
            "sitemap_generate.max_urls_per_file",
            "out_of_range",
            "URLs per sitemap file must be between 1 and 50,000.",
        );
    }
    if let crate::PrioritySource::Fixed(p) = g.priority
        && !(0.0..=1.0).contains(&p)
    {
        i.error(
            "sitemap_generate.priority",
            "out_of_range",
            "Priority must be between 0.0 and 1.0.",
        );
    }
    if let crate::LastmodSource::Fixed(d) = &g.lastmod
        && jiff::civil::Date::strptime("%Y-%m-%d", d.trim()).is_err()
    {
        i.error(
            "sitemap_generate.lastmod",
            "lastmod_invalid",
            "Enter the date as YYYY-MM-DD.",
        );
    }
    if !g.base_url.trim().is_empty() && http_url(&g.base_url).is_none() {
        i.error(
            "sitemap_generate.base_url",
            "sitemap_base_invalid",
            "Enter the address the sitemap files will be published at, starting with http:// or https://.",
        );
    }
    for (group, list) in [("include", &g.include), ("exclude", &g.exclude)] {
        for (index, p) in list.iter().enumerate() {
            if let Err(message) = check_pattern(p) {
                i.error(
                    format!("sitemap_generate.{group}[{index}]"),
                    "pattern_syntax",
                    message,
                );
            }
        }
    }
    let prefix_ok = !g.file_prefix.is_empty()
        && g.file_prefix
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.'));
    if !prefix_ok {
        i.error(
            "sitemap_generate.file_prefix",
            "file_prefix_invalid",
            "Use letters, digits, dots, hyphens and underscores for the file name.",
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

    // Cookies (spec 025 data-model.md)
    let token = |s: &str| {
        !s.is_empty()
            && s.bytes()
                .all(|b| b.is_ascii_graphic() && !b"()<>@,;:\\\"/[]?={}".contains(&b))
    };
    let mut per_domain: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for (index, ck) in c.access.cookies.iter().enumerate() {
        let path = format!("access.cookies[{index}]");
        if !token(ck.name.trim()) {
            i.error(
                format!("{path}.name"),
                "cookie_name",
                "A cookie name is letters, digits and simple symbols, without spaces, = or ;.",
            );
        }
        match ck.value.expose() {
            None => i.error(
                format!("{path}.value"),
                "cookie_secret_missing",
                "This secret value is not saved in files. Enter it again to start the crawl.",
            ),
            Some(v)
                if v.bytes()
                    .any(|b| b.is_ascii_control() || b == b';' || b == b',' || b == b' ') =>
            {
                i.error(
                    format!("{path}.value"),
                    "cookie_value",
                    "A cookie value cannot contain spaces, commas, semicolons or line breaks.",
                )
            }
            Some(v) if ck.name.len() + v.len() > 4096 => i.error(
                format!("{path}.value"),
                "cookie_too_large",
                "Browsers refuse cookies over 4 KB, so sites never receive them.",
            ),
            Some(_) => {}
        }
        let domain = ck.domain.trim().trim_start_matches('.').to_ascii_lowercase();
        if domain.is_empty() || domain.contains('/') || domain.contains(' ') {
            i.error(
                format!("{path}.domain"),
                "cookie_domain",
                "Enter the site the cookie belongs to, for example example.com.",
            );
        }
        if !ck.path.starts_with('/') {
            i.error(
                format!("{path}.path"),
                "cookie_path",
                "A cookie path starts with /.",
            );
        }
        let n = per_domain.entry(domain).or_default();
        *n += 1;
        if *n == 181 {
            i.error(
                format!("{path}.domain"),
                "cookie_count",
                "Browsers keep at most 180 cookies per site.",
            );
        }
    }

    validate_sign_ins(c, &mut i);
    validate_login(c, &mut i);
    validate_proxy(c, &mut i);

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

    // Spelling
    let lang = c.spelling.default_lang.trim();
    if lang.is_empty()
        || lang.len() > 35
        || !lang
            .split('-')
            .all(|p| !p.is_empty() && p.chars().all(|ch| ch.is_ascii_alphanumeric()))
    {
        i.error(
            "spelling.default_lang",
            "invalid_language",
            "Use a language code such as en, en-GB or he.",
        );
    }
    // PDFs (spec 027 part C)
    if !(1..=500).contains(&c.extraction.pdf.max_mb) {
        i.error(
            "extraction.pdf.max_mb",
            "out_of_range",
            "The PDF size limit must be between 1 and 500 MB.",
        );
    }
    if !(1..=120).contains(&c.extraction.pdf.time_limit_secs) {
        i.error(
            "extraction.pdf.time_limit_secs",
            "out_of_range",
            "The PDF time limit must be between 1 and 120 seconds.",
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
    let r = &c.retry;
    if !(r.base_wait_secs.is_finite() && (0.1..=60.0).contains(&r.base_wait_secs)) {
        i.error(
            "retry.base_wait_secs",
            "out_of_range",
            "The first wait must be between 0.1 and 60 seconds.",
        );
    }
    if !(1..=100).contains(&r.host_pause_after) {
        i.error(
            "retry.host_pause_after",
            "out_of_range",
            "Failures before a site is paused must be between 1 and 100.",
        );
    }
    let pause_ok = r.host_pause_secs.is_finite() && (1.0..=3600.0).contains(&r.host_pause_secs);
    if !pause_ok {
        i.error(
            "retry.host_pause_secs",
            "out_of_range",
            "The pause must be between 1 and 3600 seconds.",
        );
    }
    if !(r.host_pause_max_secs.is_finite() && r.host_pause_max_secs <= 86_400.0)
        || (pause_ok && r.host_pause_max_secs < r.host_pause_secs)
    {
        i.error(
            "retry.host_pause_max_secs",
            "out_of_range",
            "The longest pause must be at least the first pause and at most 86400 seconds.",
        );
    }
    if !(1..=50).contains(&r.host_give_up_after) {
        i.error(
            "retry.host_give_up_after",
            "out_of_range",
            "Failed checks before a site is given up must be between 1 and 50.",
        );
    }

    // Near duplicate content (spec 020)
    let ct = &c.content;
    if !(50..=100).contains(&ct.threshold) {
        i.error(
            "content.threshold",
            "out_of_range",
            "The near duplicate threshold must be between 50% and 100%.",
        );
    }
    if ct.report_floor < 50 || ct.report_floor > ct.threshold.max(50) {
        i.error(
            "content.report_floor",
            "out_of_range",
            "The closest match floor must be between 50% and the near duplicate threshold.",
        );
    }
    if !(1..=10_000).contains(&ct.min_words) {
        i.error(
            "content.min_words",
            "out_of_range",
            "The minimum words to compare must be between 1 and 10,000.",
        );
    }
    if let Some(sel) = ct.content_selector.as_deref().filter(|s| !s.trim().is_empty())
        && scraper::Selector::parse(sel.trim()).is_err()
    {
        i.error(
            "content.content_selector",
            "selector_syntax",
            format!("\"{sel}\" is not a valid CSS selector."),
        );
    }
    if ct.exclude_selectors.len() > 50 {
        i.error(
            "content.exclude_selectors",
            "too_many",
            "List at most 50 elements to leave out.",
        );
    }
    for (index, sel) in ct.exclude_selectors.iter().enumerate() {
        if !sel.trim().is_empty() && scraper::Selector::parse(sel.trim()).is_err() {
            i.error(
                format!("content.exclude_selectors[{index}]"),
                "selector_syntax",
                format!("\"{sel}\" is not a valid CSS selector."),
            );
        }
    }
    validate_rendering(c, &mut i);
    // Link analysis (spec 023)
    for (index, rule) in c.links.position_rules.iter().enumerate() {
        if let Err(message) = crate::check_selector(&rule.selector) {
            i.error(
                format!("links.position_rules[{index}].selector"),
                "selector_syntax",
                message,
            );
        }
    }
    if let Some(w) = &c.links.weighting {
        for (name, value) in w.fields() {
            if !(value.is_finite() && (0.0..=1.0).contains(&value)) {
                i.error(
                    format!("links.weighting.{name}"),
                    "out_of_range",
                    "Weights must be between 0 and 1.",
                );
            }
        }
    }
    // Accessibility (spec 026)
    let a = &c.accessibility;
    for (name, value, lo, hi, text) in [
        ("budget_secs", a.budget_secs, 1, 30, "The check time per page must be between 1 and 30 seconds."),
        ("small_text_px", a.small_text_px, 6, 24, "The small text size must be between 6 and 24 pixels."),
        ("small_text_share", a.small_text_share, 1, 100, "The small text share must be between 1 and 100 percent."),
        ("tap_target_px", a.tap_target_px, 16, 64, "The tap target size must be between 16 and 64 pixels."),
        ("overflow_px", a.overflow_px, 0, 100, "The allowed overflow must be between 0 and 100 pixels."),
    ] {
        if !(lo..=hi).contains(&value) {
            i.error(format!("accessibility.{name}"), "out_of_range", text);
        }
    }
    if a.level == crate::A11yLevel::Full && c.rendering.mode != crate::RenderMode::Javascript {
        i.warn(
            "accessibility.level",
            "full_needs_rendering",
            "Full accessibility checks need JavaScript rendering. Without it only the static checks run.",
        );
    }
    // Segments (spec 027 part B)
    i.0.extend(crate::segments::validate_segments(&c.segments));

    i.0
}

/// JavaScript rendering (spec 014, data-model.md "Settings"). Ranges are
/// checked whatever the mode, so a saved profile is never half valid.
/// HTTP sign-ins (spec 025 data-model.md).
fn validate_sign_ins(c: &CrawlConfig, i: &mut Issues) {
    let mut seen = std::collections::HashSet::new();
    for (index, s) in c.access.sign_ins.iter().enumerate() {
        let path = format!("access.sign_ins[{index}]");
        let url = url::Url::parse(s.origin.trim()).ok();
        let origin_ok = url.as_ref().is_some_and(|u| {
            matches!(u.scheme(), "http" | "https")
                && u.host_str().is_some()
                && u.path() == "/"
                && u.query().is_none()
                && u.fragment().is_none()
                && u.username().is_empty()
        });
        if !origin_ok {
            i.error(
                format!("{path}.origin"),
                "sign_in_origin",
                "Enter the site's address only, for example https://staging.example.com, without a path.",
            );
        } else if let Some(key) = s.origin_key()
            && !seen.insert(key)
        {
            i.error(
                format!("{path}.origin"),
                "sign_in_duplicate",
                "This site already has a sign-in. Keep one per site.",
            );
        }
        if s.user.trim().is_empty() {
            i.error(format!("{path}.user"), "sign_in_user", "Enter the user name.");
        }
        if !s.password.is_set() {
            i.error(
                format!("{path}.password"),
                "sign_in_password_missing",
                "Passwords are not saved in files. Enter it again to start the crawl.",
            );
        }
        if url.as_ref().is_some_and(|u| u.scheme() == "http") {
            i.warn(
                format!("{path}.origin"),
                "sign_in_insecure",
                "This site uses http://, so the password can be read by anyone on the network. Use it only for test sites.",
            );
        }
    }
}

/// The login page settings (spec 025 data-model.md).
fn validate_login(c: &CrawlConfig, i: &mut Issues) {
    let Some(login) = &c.access.login else { return };
    let http = |s: &str| url::Url::parse(s.trim()).is_ok_and(|u| matches!(u.scheme(), "http" | "https"));
    if !http(&login.login_url) {
        i.error(
            "access.login.login_url",
            "login_url",
            "Enter the login page's full address, starting with https://.",
        );
    }
    for (index, p) in login.sign_out_patterns.iter().enumerate() {
        if p.trim().is_empty() {
            i.error(
                format!("access.login.sign_out_patterns[{index}]"),
                "login_pattern_empty",
                "Remove the empty pattern or enter text such as logout.",
            );
        }
    }
    if let Some(check) = &login.check {
        if !http(&check.url) {
            i.error(
                "access.login.check.url",
                "login_check_url",
                "Enter the full address of a page that only signed-in visitors can see.",
            );
        }
        let empty = match &check.when {
            crate::CheckCondition::RedirectsTo(t) | crate::CheckCondition::Contains(t) => t.trim().is_empty(),
            crate::CheckCondition::Status(n) => !(100..=599).contains(n),
        };
        if empty {
            i.error(
                "access.login.check.when",
                "login_check_condition",
                "Enter the text or status code that shows the session ended.",
            );
        }
    }
}

/// A bypass entry: a host, `*.host`, an IP address or a CIDR range.
fn bypass_entry_ok(entry: &str) -> bool {
    let e = entry.trim();
    if e.is_empty() || e.contains(' ') {
        return false;
    }
    if let Some((ip, bits)) = e.split_once('/') {
        let Ok(ip) = ip.parse::<std::net::IpAddr>() else {
            return false;
        };
        let max = if ip.is_ipv4() { 32 } else { 128 };
        return bits.parse::<u8>().is_ok_and(|b| b <= max);
    }
    if e.parse::<std::net::IpAddr>().is_ok() {
        return true;
    }
    let host = e.strip_prefix("*.").or_else(|| e.strip_prefix('.')).unwrap_or(e);
    !host.is_empty()
        && host
            .split('.')
            .all(|l| !l.is_empty() && l.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-'))
}

/// The proxy (spec 025 data-model.md).
fn validate_proxy(c: &CrawlConfig, i: &mut Issues) {
    let p = &c.access.proxy;
    if p.mode == crate::ProxyMode::Manual {
        let host = p.host.trim();
        if host.is_empty() || host.contains("://") || host.contains('/') || host.contains(' ') {
            i.error(
                "access.proxy.host",
                "proxy_host",
                "Enter the proxy's host name or IP address, without http://.",
            );
        }
        if p.port == 0 {
            i.error(
                "access.proxy.port",
                "proxy_port",
                "Enter the proxy's port, a number from 1 to 65535.",
            );
        }
        if p.needs_password() {
            i.error(
                "access.proxy.password",
                "proxy_password_missing",
                "Passwords are not saved in files. Enter the proxy password again to start the crawl.",
            );
        }
    }
    if p.mode != crate::ProxyMode::None {
        for (index, b) in p.bypass.iter().enumerate() {
            if !bypass_entry_ok(b) {
                i.error(
                    format!("access.proxy.bypass[{index}]"),
                    "proxy_bypass",
                    "Enter a host (example.com), a wildcard (*.example.com), an IP address or a range (10.0.0.0/8).",
                );
            }
        }
    }
}

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
    fn sitemap_sources_are_checked() {
        let mut c = CrawlConfig::default();
        c.scope.sitemaps.urls = vec!["https://a.test/s.xml".into(), "/sitemap.xml".into()];
        c.scope.sitemaps.files = vec![crate::LocalSitemap {
            path: String::new(),
            base_url: "ftp://a.test/".into(),
        }];
        assert_eq!(
            codes(&validate_config(&c)),
            vec![
                ("scope.sitemaps.urls[1]", "sitemap_url_invalid", Severity::Error),
                (
                    "scope.sitemaps.files[0].path",
                    "sitemap_file_missing",
                    Severity::Error
                ),
                (
                    "scope.sitemaps.files[0].base_url",
                    "sitemap_base_invalid",
                    Severity::Error
                ),
            ]
        );
        c.scope.sitemaps.urls.pop();
        c.scope.sitemaps.files.clear();
        c.scope.sitemaps.read = false;
        assert_eq!(
            codes(&validate_config(&c)),
            vec![("scope.sitemaps.read", "sitemaps_off", Severity::Warning)]
        );
    }

    #[test]
    fn link_position_rules_and_weights_are_checked() {
        let mut c = CrawlConfig::default();
        c.links.position_rules = vec![
            crate::PositionRule {
                selector: ".mega-menu".into(),
                position: crate::Position::Navigation,
            },
            crate::PositionRule {
                selector: "div[".into(),
                position: crate::Position::Footer,
            },
        ];
        c.links.weighting = Some(crate::PositionWeights {
            footer: 1.5,
            ..Default::default()
        });
        assert_eq!(
            codes(&validate_config(&c)),
            vec![
                (
                    "links.position_rules[1].selector",
                    "selector_syntax",
                    Severity::Error
                ),
                ("links.weighting.footer", "out_of_range", Severity::Error),
            ]
        );
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
    fn content_settings_are_checked() {
        let mut c = CrawlConfig::default();
        c.content.threshold = 40;
        c.content.report_floor = 95;
        c.content.min_words = 0;
        c.content.content_selector = Some("div[[".into());
        c.content.exclude_selectors = vec![".ok".into(), "a >".into()];
        let got: Vec<_> = validate_config(&c).into_iter().map(|i| i.path).collect();
        assert_eq!(
            got,
            vec![
                "content.threshold",
                "content.report_floor",
                "content.min_words",
                "content.content_selector",
                "content.exclude_selectors[1]"
            ]
        );
        let mut ok = CrawlConfig::default();
        ok.content.report_floor = 90;
        ok.content.content_selector = Some("#content".into());
        assert!(validate_config(&ok).is_empty());
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
    fn cookies() {
        use crate::{HeaderValue, SuppliedCookie};
        let ck = |name: &str, value: HeaderValue, domain: &str, path: &str| SuppliedCookie {
            name: name.into(),
            value,
            domain: domain.into(),
            path: path.into(),
            secure_only: false,
        };
        let mut c = CrawlConfig::default();
        c.access.cookies = vec![
            ck("ok", HeaderValue::Plain("yes".into()), "example.com", "/"),
            ck("bad name", HeaderValue::Plain("x".into()), "example.com", "/"),
            ck("v", HeaderValue::Plain("a;b".into()), "example.com", "/"),
            ck("s", HeaderValue::Secret(None), "", "shop"),
        ];
        let got: Vec<_> = validate_config(&c)
            .into_iter()
            .map(|i| (i.path, i.code))
            .collect();
        assert_eq!(
            got,
            [
                ("access.cookies[1].name".to_string(), "cookie_name".to_string()),
                ("access.cookies[2].value".to_string(), "cookie_value".to_string()),
                (
                    "access.cookies[3].value".to_string(),
                    "cookie_secret_missing".to_string()
                ),
                (
                    "access.cookies[3].domain".to_string(),
                    "cookie_domain".to_string()
                ),
                ("access.cookies[3].path".to_string(), "cookie_path".to_string()),
            ]
        );
    }

    #[test]
    fn sign_ins_login_and_proxy() {
        use crate::{AuthMethod, CheckCondition, LoginSettings, ProxyMode, Secret, SignIn, SignedOutCheck};
        let si = |origin: &str, user: &str, pw: Option<&str>| SignIn {
            origin: origin.into(),
            user: user.into(),
            password: pw.map(Secret::new).unwrap_or_default(),
            method: AuthMethod::Any,
        };
        let mut c = CrawlConfig::default();
        c.access.sign_ins = vec![
            si("https://staging.example.com", "ann", Some("pw")),
            si("https://staging.example.com/", "bob", Some("pw")),
            si("https://example.com/admin", "", None),
            si("http://test.example.com:8080", "eve", Some("pw")),
        ];
        let mut login = LoginSettings::new("example.com/login");
        login.sign_out_patterns.push(" ".into());
        login.check = Some(SignedOutCheck {
            url: "https://example.com/account".into(),
            when: CheckCondition::Contains(String::new()),
        });
        c.access.login = Some(login);
        c.access.proxy.mode = ProxyMode::Manual;
        c.access.proxy.host = "http://proxy".into();
        c.access.proxy.user = Some("ann".into());
        c.access.proxy.bypass = vec![
            "localhost".into(),
            "*.corp.example".into(),
            "10.0.0.0/8".into(),
            "bad host".into(),
        ];
        let got: Vec<_> = validate_config(&c)
            .into_iter()
            .map(|i| (i.path, i.code))
            .collect();
        let want = [
            ("access.sign_ins[1].origin", "sign_in_duplicate"),
            ("access.sign_ins[2].origin", "sign_in_origin"),
            ("access.sign_ins[2].user", "sign_in_user"),
            ("access.sign_ins[2].password", "sign_in_password_missing"),
            ("access.sign_ins[3].origin", "sign_in_insecure"),
            ("access.login.login_url", "login_url"),
            ("access.login.sign_out_patterns[6]", "login_pattern_empty"),
            ("access.login.check.when", "login_check_condition"),
            ("access.proxy.host", "proxy_host"),
            ("access.proxy.port", "proxy_port"),
            ("access.proxy.password", "proxy_password_missing"),
            ("access.proxy.bypass[3]", "proxy_bypass"),
        ];
        assert_eq!(got, want.map(|(p, c)| (p.to_string(), c.to_string())));
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

    #[test]
    fn retry_and_host_pause_ranges() {
        let paths = |f: &dyn Fn(&mut CrawlConfig)| {
            let mut c = CrawlConfig::default();
            f(&mut c);
            validate_config(&c)
                .into_iter()
                .map(|i| i.path)
                .collect::<Vec<_>>()
        };
        assert!(paths(&|_| {}).is_empty());
        assert_eq!(
            paths(&|c| c.retry.base_wait_secs = 0.05),
            ["retry.base_wait_secs"]
        );
        assert_eq!(
            paths(&|c| c.retry.base_wait_secs = f64::NAN),
            ["retry.base_wait_secs"]
        );
        assert_eq!(
            paths(&|c| c.retry.host_pause_after = 0),
            ["retry.host_pause_after"]
        );
        assert_eq!(
            paths(&|c| c.retry.host_pause_after = 101),
            ["retry.host_pause_after"]
        );
        assert_eq!(
            paths(&|c| c.retry.host_pause_secs = 0.5),
            ["retry.host_pause_secs"]
        );
        assert_eq!(
            paths(&|c| c.retry.host_give_up_after = 0),
            ["retry.host_give_up_after"]
        );
        assert_eq!(
            paths(&|c| c.retry.host_give_up_after = 51),
            ["retry.host_give_up_after"]
        );
        // The longest pause can't be shorter than the first one.
        assert_eq!(
            paths(&|c| {
                c.retry.host_pause_secs = 120.0;
                c.retry.host_pause_max_secs = 60.0;
            }),
            ["retry.host_pause_max_secs"]
        );
        assert_eq!(
            paths(&|c| c.retry.host_pause_max_secs = 90_000.0),
            ["retry.host_pause_max_secs"]
        );
        assert!(
            paths(&|c| {
                c.retry.host_pause_secs = 60.0;
                c.retry.host_pause_max_secs = 60.0;
            })
            .is_empty()
        );
    }
}
