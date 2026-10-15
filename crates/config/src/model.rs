//! The crawl configuration model (spec 002, data-model.md).
//!
//! Every struct uses `#[serde(default)]`, so a file written by an older
//! version (missing fields) still loads with today's defaults. Unknown fields
//! from newer versions are reported by `import_profile`, not rejected.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::list_input::ListInput;
use crate::presets::{SpeedPreset, UaPreset};
use crate::secret::HeaderValue;

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export)]
pub struct CrawlConfig {
    pub mode: Mode,
    pub limits: Limits,
    pub scope: Scope,
    pub identity: Identity,
    pub speed: SpeedSettings,
    pub robots: RobotsSettings,
    pub network: NetworkSettings,
    pub retry: RetrySettings,
    pub extraction: ExtractionSettings,
    pub access: AccessSettings,
    pub ai_checks: AiChecks,
    /// The last options used to generate XML sitemaps (spec 011 FR-025).
    pub sitemap_generate: SitemapGenerateOptions,
}

/// Options for generating XML sitemaps from a crawl (spec 011 FR-020,
/// FR-021). Defaults follow the spec's Assumptions.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export)]
pub struct SitemapGenerateOptions {
    pub include_noindex: bool,
    pub include_canonicalised: bool,
    /// Spec 002 patterns: when set, only matching pages are written.
    pub include: Vec<String>,
    pub exclude: Vec<String>,
    pub lastmod: LastmodSource,
    pub changefreq: Option<Changefreq>,
    pub priority: PrioritySource,
    pub images: ImageEntries,
    /// Hosts besides the crawl's own whose images may be listed.
    pub image_hosts: Vec<String>,
    pub hreflang: bool,
    pub max_urls_per_file: u32,
    pub gzip: bool,
    /// Where the files will be published; blank means the start host's root.
    pub base_url: String,
    pub file_prefix: String,
}

impl Default for SitemapGenerateOptions {
    fn default() -> Self {
        Self {
            include_noindex: false,
            include_canonicalised: false,
            include: Vec::new(),
            exclude: Vec::new(),
            lastmod: LastmodSource::LastModifiedHeader,
            changefreq: None,
            priority: PrioritySource::Off,
            images: ImageEntries::Off,
            image_hosts: Vec::new(),
            hreflang: false,
            max_urls_per_file: 50_000,
            gzip: false,
            base_url: String::new(),
            file_prefix: "sitemap".into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case", tag = "kind", content = "date")]
#[ts(export)]
pub enum LastmodSource {
    None,
    /// The page's `Last-Modified` response header; entries without one get
    /// no `lastmod`.
    LastModifiedHeader,
    /// The time the page was fetched.
    CrawlDate,
    /// One date for every entry, `YYYY-MM-DD`.
    Fixed(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum Changefreq {
    Always,
    Hourly,
    Daily,
    Weekly,
    Monthly,
    Yearly,
    Never,
}

impl Changefreq {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Always => "always",
            Self::Hourly => "hourly",
            Self::Daily => "daily",
            Self::Weekly => "weekly",
            Self::Monthly => "monthly",
            Self::Yearly => "yearly",
            Self::Never => "never",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case", tag = "kind", content = "value")]
#[ts(export)]
pub enum PrioritySource {
    Off,
    Fixed(f32),
    /// 1.0 for the start page, then 0.8, 0.6, 0.4 and 0.2 by click depth.
    ByDepth,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ImageEntries {
    Off,
    /// In the page sitemaps, with the image extension.
    Inline,
    /// In their own image sitemap files.
    Separate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum Mode {
    #[default]
    Spider,
    List,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export)]
pub struct Limits {
    /// Blank (null) means no limit.
    #[ts(type = "number | null")]
    pub max_urls: Option<u64>,
    /// Spider mode only. The start URL is depth 0.
    pub max_depth: Option<u32>,
    /// List mode only: how many clicks to follow from listed URLs. 0 means
    /// links are recorded but not followed (spec 003 FR-041).
    pub list_depth: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct Scope {
    pub include: Vec<String>,
    pub exclude: Vec<String>,
    pub subdomains: SubdomainMode,
    /// Internal URLs outside the start URL's folder are not crawled.
    pub stay_in_folder: bool,
    pub check_external: bool,
    /// Spec 002 called this `follow_nofollow`. No crawl or profile files
    /// were released with that name, so it is not kept as an alias.
    pub follow_internal_nofollow: bool,
    pub follow_external_nofollow: bool,
    pub query_params: QueryParams,
    /// Longer URLs are recorded as malformed and not crawled.
    pub max_url_length: u32,
    /// Where sitemaps come from and what is done with their URLs (spec 011).
    pub sitemaps: SitemapSettings,
}

/// Spec 011 FR-001, FR-002, FR-009.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export)]
pub struct SitemapSettings {
    /// Read sitemaps (robots.txt, the default locations, the ones below) and
    /// keep every entry for the sitemap audit.
    pub read: bool,
    /// Queue internal URLs that only a sitemap lists; when off they are
    /// recorded as "Found only in sitemap" and not requested.
    pub crawl_sitemap_only: bool,
    /// Sitemaps the user added by hand.
    pub urls: Vec<String>,
    /// Sitemap files on this computer, each with the URL it will live at.
    pub files: Vec<LocalSitemap>,
}

impl Default for SitemapSettings {
    fn default() -> Self {
        Self {
            read: true,
            crawl_sitemap_only: true,
            urls: Vec::new(),
            files: Vec::new(),
        }
    }
}

impl SitemapSettings {
    /// Both switches on or both off: the meaning of spec 003's single
    /// "Use sitemaps from robots.txt" setting.
    pub fn all(on: bool) -> Self {
        Self {
            read: on,
            crawl_sitemap_only: on,
            ..Self::default()
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct LocalSitemap {
    pub path: String,
    /// Where the file will be published; the location rule is checked
    /// against it (spec 011 US6 scenario 2).
    pub base_url: String,
}

/// What a stored `scope` may look like: the current shape, or the spec 003
/// one with `use_sitemaps`, which maps to both sitemap switches (spec 011
/// FR-001, research R10). Only the current shape is ever written.
#[derive(Deserialize)]
#[serde(default)]
struct ScopeWire {
    include: Vec<String>,
    exclude: Vec<String>,
    subdomains: SubdomainMode,
    stay_in_folder: bool,
    check_external: bool,
    follow_internal_nofollow: bool,
    follow_external_nofollow: bool,
    query_params: QueryParams,
    max_url_length: u32,
    sitemaps: Option<SitemapSettings>,
    use_sitemaps: Option<bool>,
}

impl Default for ScopeWire {
    fn default() -> Self {
        let d = Scope::default();
        Self {
            include: d.include,
            exclude: d.exclude,
            subdomains: d.subdomains,
            stay_in_folder: d.stay_in_folder,
            check_external: d.check_external,
            follow_internal_nofollow: d.follow_internal_nofollow,
            follow_external_nofollow: d.follow_external_nofollow,
            query_params: d.query_params,
            max_url_length: d.max_url_length,
            sitemaps: None,
            use_sitemaps: None,
        }
    }
}

impl<'de> Deserialize<'de> for Scope {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        ScopeWire::deserialize(d).map(Into::into)
    }
}

impl From<ScopeWire> for Scope {
    fn from(f: ScopeWire) -> Self {
        let sitemaps = match (f.sitemaps, f.use_sitemaps) {
            (Some(s), _) => s,
            (None, Some(on)) => SitemapSettings::all(on),
            (None, None) => SitemapSettings::default(),
        };
        Self {
            include: f.include,
            exclude: f.exclude,
            subdomains: f.subdomains,
            stay_in_folder: f.stay_in_folder,
            check_external: f.check_external,
            follow_internal_nofollow: f.follow_internal_nofollow,
            follow_external_nofollow: f.follow_external_nofollow,
            query_params: f.query_params,
            max_url_length: f.max_url_length,
            sitemaps,
        }
    }
}

impl Default for Scope {
    fn default() -> Self {
        Self {
            include: Vec::new(),
            exclude: Vec::new(),
            subdomains: SubdomainMode::ThisHost,
            stay_in_folder: false,
            check_external: true,
            follow_internal_nofollow: false,
            follow_external_nofollow: false,
            query_params: QueryParams::default(),
            max_url_length: 2000,
            sitemaps: SitemapSettings::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum SubdomainMode {
    /// The start URL's host, with and without `www.`, is internal.
    #[default]
    ThisHost,
    /// Every host of the same registrable domain is internal.
    AllSubdomains,
}

/// Query parameter handling (spec 003 FR-005).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, TS)]
#[ts(export)]
pub struct QueryParams {
    pub mode: QueryMode,
    /// Parameter names removed when `mode` is `remove`. Case-sensitive.
    pub names: Vec<String>,
    /// Sort parameters by name so that order differences de-duplicate.
    pub sort: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum QueryMode {
    #[default]
    Keep,
    RemoveAll,
    Remove,
}

impl<'de> Deserialize<'de> for QueryParams {
    /// Accepts the spec 002 form `"keep"` as well as the object form.
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(default)]
        struct Full {
            mode: QueryMode,
            names: Vec<String>,
            sort: bool,
        }
        impl Default for Full {
            fn default() -> Self {
                Self {
                    mode: QueryMode::Keep,
                    names: Vec::new(),
                    sort: false,
                }
            }
        }
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Wire {
            Legacy(QueryMode),
            Full(Full),
        }
        Ok(match Wire::deserialize(d)? {
            Wire::Legacy(mode) => Self {
                mode,
                ..Self::default()
            },
            Wire::Full(f) => Self {
                mode: f.mode,
                names: f.names,
                sort: f.sort,
            },
        })
    }
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export)]
pub struct Identity {
    pub user_agent: UserAgent,
    pub headers: Vec<CustomHeader>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export)]
pub struct UserAgent {
    pub preset: UaPreset,
    /// Required when `preset` is `custom`.
    pub custom_string: Option<String>,
    /// Required when `preset` is `custom`; presets supply their own.
    pub robots_token: Option<String>,
}

impl Default for UserAgent {
    fn default() -> Self {
        Self {
            preset: UaPreset::Spiderlens,
            custom_string: None,
            robots_token: None,
        }
    }
}

impl UserAgent {
    /// The string sent in `User-Agent`, if the setting is complete.
    pub fn string(&self) -> Option<String> {
        match self.preset {
            UaPreset::Custom => self.custom_string.clone().filter(|s| !s.trim().is_empty()),
            p => p.user_agent(),
        }
    }

    /// The token matched against robots.txt groups.
    pub fn token(&self) -> Option<String> {
        match self.preset {
            UaPreset::Custom => self.robots_token.clone().filter(|s| !s.trim().is_empty()),
            p => p.robots_token().map(str::to_string),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct CustomHeader {
    pub name: String,
    #[ts(type = "{ plain: string } | { secret: true, value?: string }")]
    pub value: HeaderValue,
    #[serde(default)]
    pub send_to: HeaderTarget,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum HeaderTarget {
    #[default]
    Internal,
    All,
}

/// Speed values. Serialized with the derived preset name for readability;
/// the name is ignored when loading (research R10).
#[derive(Debug, Clone, Copy, PartialEq, TS)]
#[ts(as = "SpeedWire")]
pub struct SpeedSettings {
    pub max_concurrent: u32,
    pub max_per_host: u32,
    /// Null means no rate cap.
    pub max_urls_per_sec: Option<f64>,
    /// Lower per-host concurrency when a host slows down or returns 429/503.
    pub adaptive: bool,
}

impl Default for SpeedSettings {
    fn default() -> Self {
        Self::POLITE
    }
}

/// Speed values. Serialized with the derived preset name for readability;
/// the name is ignored when loading (research R10).
// Same doc as `SpeedSettings`: both export to SpeedSettings.ts, and parallel
// test threads must write identical files or the generated-types check flakes.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export, rename = "SpeedSettings")]
pub struct SpeedWire {
    pub max_concurrent: u32,
    pub max_per_host: u32,
    pub max_urls_per_sec: Option<f64>,
    pub adaptive: bool,
    /// Output only.
    #[ts(optional)]
    pub preset: Option<SpeedPreset>,
}

impl Default for SpeedWire {
    fn default() -> Self {
        SpeedSettings::POLITE.into()
    }
}

impl From<SpeedSettings> for SpeedWire {
    fn from(s: SpeedSettings) -> Self {
        Self {
            max_concurrent: s.max_concurrent,
            max_per_host: s.max_per_host,
            max_urls_per_sec: s.max_urls_per_sec,
            adaptive: s.adaptive,
            preset: Some(s.preset()),
        }
    }
}

impl Serialize for SpeedSettings {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        SpeedWire::from(*self).serialize(s)
    }
}

impl<'de> Deserialize<'de> for SpeedSettings {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        SpeedWire::deserialize(d).map(Self::from)
    }
}

impl From<SpeedWire> for SpeedSettings {
    fn from(w: SpeedWire) -> Self {
        Self {
            max_concurrent: w.max_concurrent,
            max_per_host: w.max_per_host,
            max_urls_per_sec: w.max_urls_per_sec,
            adaptive: w.adaptive,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export)]
pub struct RobotsSettings {
    pub mode: RobotsMode,
    pub honour_crawl_delay: bool,
}

impl Default for RobotsSettings {
    fn default() -> Self {
        Self {
            mode: RobotsMode::Respect,
            honour_crawl_delay: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum RobotsMode {
    /// Disallowed URLs are not requested.
    #[default]
    Respect,
    /// Disallowed URLs are crawled and marked as disallowed.
    IgnoreReport,
    /// robots.txt is not fetched.
    Ignore,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export)]
pub struct NetworkSettings {
    pub connect_timeout_secs: f64,
    pub read_timeout_secs: f64,
    pub max_redirects: u32,
    #[ts(type = "number")]
    pub max_body_mb: u64,
}

impl Default for NetworkSettings {
    fn default() -> Self {
        Self {
            connect_timeout_secs: 10.0,
            read_timeout_secs: 30.0,
            max_redirects: 10,
            max_body_mb: 50,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export)]
pub struct RetrySettings {
    /// Total attempts, including the first (1 means no retry).
    pub attempts: u32,
    pub max_wait_secs: f64,
    pub retry_5xx: bool,
    pub retry_429: bool,
    /// Wait before the second attempt; doubles for each one after (spec 004
    /// FR-017).
    pub base_wait_secs: f64,
    /// Transient failures in a row that pause a host (FR-024).
    pub host_pause_after: u32,
    /// First pause of a failing host (FR-024).
    pub host_pause_secs: f64,
    /// Longest pause; each failed probe doubles the pause up to this
    /// (FR-025).
    pub host_pause_max_secs: f64,
    /// Failed probes in a row after which the host is given up (FR-026).
    pub host_give_up_after: u32,
}

impl Default for RetrySettings {
    fn default() -> Self {
        Self {
            attempts: 3,
            max_wait_secs: 60.0,
            retry_5xx: true,
            retry_429: true,
            base_wait_secs: 1.0,
            host_pause_after: 5,
            host_pause_secs: 60.0,
            host_pause_max_secs: 600.0,
            host_give_up_after: 5,
        }
    }
}

/// Page data extraction (spec 005).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export)]
pub struct ExtractionSettings {
    /// Keep each page's raw HTML, compressed, in the crawl file (FR-030,
    /// FR-031).
    pub store_html: bool,
}

impl Default for ExtractionSettings {
    fn default() -> Self {
        Self { store_html: true }
    }
}

/// Cookies and, later, sign-ins and proxy: how the crawl reaches a site
/// (spec 025, data-model.md).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export)]
pub struct AccessSettings {
    /// Sent to the crawl's internal hosts only (FR-015, FR-016).
    pub cookies: Vec<SuppliedCookie>,
    pub cookie_policy: CookiePolicy,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SuppliedCookie {
    pub name: String,
    #[ts(type = "{ plain: string } | { secret: true, value?: string }")]
    pub value: HeaderValue,
    /// `example.com` also reaches its subdomains, as a `Domain` attribute
    /// would.
    pub domain: String,
    #[serde(default = "root_path")]
    pub path: String,
    #[serde(default)]
    pub secure_only: bool,
}

fn root_path() -> String {
    "/".into()
}

/// What happens to cookies the site sets during the crawl (FR-017).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum CookiePolicy {
    /// As search engines do, and as SpiderLens did before spec 025.
    #[default]
    Ignore,
    /// Kept in memory for the rest of the crawl, never written.
    Keep,
}

impl CrawlConfig {
    /// Secrets the configuration needs but does not hold (loaded from a
    /// file, where they are never written). Header names are reported as
    /// they are; other secrets carry a `kind:` prefix (spec 025
    /// data-model.md), which no header name can contain.
    pub fn missing_secrets(&self) -> Vec<String> {
        let headers = self
            .identity
            .headers
            .iter()
            .filter(|h| h.value.is_secret() && h.value.expose().is_none())
            .map(|h| h.name.clone());
        let cookies = self
            .access
            .cookies
            .iter()
            .filter(|c| c.value.is_secret() && c.value.expose().is_none())
            .map(|c| format!("cookie:{}/{}", c.domain, c.name));
        headers.chain(cookies).collect()
    }

    /// Puts secret values back from `(key, value)` pairs keyed as in
    /// [`Self::missing_secrets`] (header names case-insensitively). Returns
    /// the keys still missing.
    pub fn fill_secrets(&mut self, secrets: &[(String, String)]) -> Vec<String> {
        let find = |key: &str, case_insensitive: bool| {
            secrets
                .iter()
                .find(|(k, _)| {
                    if case_insensitive {
                        k.eq_ignore_ascii_case(key)
                    } else {
                        k == key
                    }
                })
                .map(|(_, v)| v.clone())
        };
        for h in &mut self.identity.headers {
            if h.value.is_secret()
                && h.value.expose().is_none()
                && let Some(v) = find(&h.name, true)
            {
                h.value = HeaderValue::secret(v);
            }
        }
        for c in &mut self.access.cookies {
            if c.value.is_secret()
                && c.value.expose().is_none()
                && let Some(v) = find(&format!("cookie:{}/{}", c.domain, c.name), false)
            {
                c.value = HeaderValue::secret(v);
            }
        }
        self.missing_secrets()
    }
}

/// AI search checks (spec 015 FR-002, FR-003, FR-010).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export)]
pub struct AiChecks {
    /// Run the AI checks when the crawl finishes.
    pub auto: bool,
    /// Fetch `/llms.txt`, `/llms-full.txt` and `/.well-known/tdmrep.json`
    /// once per internal host at the end of the crawl.
    pub site_files: bool,
}

impl Default for AiChecks {
    fn default() -> Self {
        Self {
            auto: true,
            site_files: true,
        }
    }
}

/// What the crawl starts from: one URL or a list (FR-001).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(export)]
pub enum ModeInput {
    Spider { start_url: String },
    List(ListInput),
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn spec_003_sitemap_setting_maps_to_both_switches() {
        for on in [true, false] {
            let s: Scope =
                serde_json::from_value(json!({ "use_sitemaps": on, "max_url_length": 500 })).unwrap();
            assert_eq!(s.sitemaps, SitemapSettings::all(on));
            assert_eq!(s.max_url_length, 500);
            let written = serde_json::to_value(&s).unwrap();
            assert!(
                written.get("use_sitemaps").is_none(),
                "only the new shape is written"
            );
            let back: Scope = serde_json::from_value(written).unwrap();
            assert_eq!(back, s);
        }
        let s: Scope = serde_json::from_value(json!({})).unwrap();
        assert_eq!(s, Scope::default());
        let s: Scope = serde_json::from_value(json!({
            "use_sitemaps": true,
            "sitemaps": { "read": false, "urls": ["https://a.test/s.xml"] }
        }))
        .unwrap();
        assert!(
            !s.sitemaps.read && s.sitemaps.crawl_sitemap_only,
            "the new key wins"
        );
        assert_eq!(s.sitemaps.urls, vec!["https://a.test/s.xml"]);
    }

    #[test]
    fn all_defaults_match_spec() {
        let c = CrawlConfig::default();
        assert_eq!(c.mode, Mode::Spider);
        assert_eq!(
            c.limits,
            Limits {
                max_urls: None,
                max_depth: None,
                list_depth: 0
            }
        );
        assert!(c.scope.include.is_empty() && c.scope.exclude.is_empty());
        assert_eq!(c.scope.subdomains, SubdomainMode::ThisHost);
        assert!(c.scope.check_external);
        assert!(!c.scope.follow_internal_nofollow && !c.scope.follow_external_nofollow);
        assert!(!c.scope.stay_in_folder);
        assert!(c.scope.sitemaps.read && c.scope.sitemaps.crawl_sitemap_only);
        assert_eq!(c.scope.max_url_length, 2000);
        assert_eq!(c.scope.query_params.mode, QueryMode::Keep);
        assert_eq!(c.limits.list_depth, 0);
        assert_eq!(c.identity.user_agent.preset, UaPreset::Spiderlens);
        assert!(c.identity.headers.is_empty());
        assert_eq!(c.speed, SpeedSettings::POLITE);
        assert_eq!(c.speed.max_concurrent, 8);
        assert_eq!(c.speed.max_per_host, 4);
        assert_eq!(c.speed.max_urls_per_sec, Some(5.0));
        assert!(c.speed.adaptive);
        assert_eq!(c.robots.mode, RobotsMode::Respect);
        assert!(c.robots.honour_crawl_delay);
        assert_eq!(c.network.connect_timeout_secs, 10.0);
        assert_eq!(c.network.read_timeout_secs, 30.0);
        assert_eq!(c.network.max_redirects, 10);
        assert_eq!(c.network.max_body_mb, 50);
        assert_eq!(c.retry.attempts, 3);
        assert_eq!(c.retry.max_wait_secs, 60.0);
        assert!(c.extraction.store_html);
        assert!(c.access.cookies.is_empty());
        assert_eq!(c.access.cookie_policy, CookiePolicy::Ignore);
    }

    #[test]
    fn serializes_every_field_including_untouched_defaults() {
        let v = serde_json::to_value(CrawlConfig::default()).unwrap();
        for group in [
            "mode",
            "limits",
            "scope",
            "identity",
            "speed",
            "robots",
            "network",
            "retry",
            "extraction",
            "access",
        ] {
            assert!(v.get(group).is_some(), "missing {group}");
        }
        assert_eq!(v["limits"]["max_urls"], json!(null));
        assert_eq!(v["speed"]["preset"], "polite");
    }

    #[test]
    fn secret_cookies_are_listed_and_filled() {
        let mut c: CrawlConfig = serde_json::from_value(json!({
            "identity": {"headers": [{"name": "X-Key", "value": {"secret": true}}]},
            "access": {"cookies": [
                {"name": "sid", "value": {"secret": true}, "domain": "example.com"},
                {"name": "consent", "value": {"plain": "yes"}, "domain": "example.com"}
            ]}
        }))
        .unwrap();
        assert_eq!(c.access.cookies[0].path, "/");
        assert_eq!(c.missing_secrets(), ["X-Key", "cookie:example.com/sid"]);
        let left = c.fill_secrets(&[
            ("x-key".into(), "k".into()),
            ("cookie:example.com/sid".into(), "v".into()),
        ]);
        assert!(left.is_empty());
        assert_eq!(c.access.cookies[0].value.expose(), Some("v"));
        assert!(!serde_json::to_string(&c).unwrap().contains("\"v\""));
    }

    #[test]
    fn missing_fields_take_defaults() {
        let c: CrawlConfig = serde_json::from_value(json!({"speed": {"max_concurrent": 16}})).unwrap();
        assert_eq!(c.speed.max_concurrent, 16);
        assert_eq!(c.speed.max_per_host, 4);
        assert_eq!(c.retry, RetrySettings::default());
        // Configs from before spec 005 keep raw HTML (the default).
        assert!(c.extraction.store_html);
    }

    #[test]
    fn stored_preset_name_is_ignored() {
        let c: CrawlConfig =
            serde_json::from_value(json!({"speed": {"max_concurrent": 64, "preset": "polite"}})).unwrap();
        assert_eq!(c.speed.preset(), SpeedPreset::Custom);
    }

    #[test]
    fn spec_002_scope_fields_still_load() {
        let c: CrawlConfig = serde_json::from_value(json!({"scope": {"query_params": "keep"}})).unwrap();
        assert_eq!(c.scope.query_params, QueryParams::default());
        let c: CrawlConfig = serde_json::from_value(
            json!({"scope": {"query_params": {"mode": "remove", "names": ["utm_source"], "sort": true}}}),
        )
        .unwrap();
        assert_eq!(c.scope.query_params.mode, QueryMode::Remove);
        assert!(c.scope.query_params.sort);
    }

    #[test]
    fn round_trip() {
        let mut c = CrawlConfig::default();
        c.scope.include.push("/blog/".into());
        c.identity.headers.push(CustomHeader {
            name: "X-Key".into(),
            value: HeaderValue::Plain("abc".into()),
            send_to: HeaderTarget::All,
        });
        c.speed = SpeedSettings::FAST;
        let back: CrawlConfig = serde_json::from_str(&serde_json::to_string(&c).unwrap()).unwrap();
        assert_eq!(back, c);
    }
}
