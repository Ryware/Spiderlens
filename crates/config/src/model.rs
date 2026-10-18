//! The crawl configuration model (spec 002, data-model.md).
//!
//! Every struct uses `#[serde(default)]`, so a file written by an older
//! version (missing fields) still loads with today's defaults. Unknown fields
//! from newer versions are reported by `import_profile`, not rejected.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::custom::CustomRules;
use crate::links::LinkAnalysisSettings;
use crate::list_input::ListInput;
use crate::presets::{SpeedPreset, UaPreset};
use crate::secret::{HeaderValue, Secret};

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
    pub content: ContentSettings,
    /// JavaScript rendering (spec 014). Off (`raw`) by default.
    pub rendering: RenderSettings,
    /// Issue rule overrides (spec 009 FR-021, FR-022). Only what differs from
    /// the rule catalogue's defaults is stored.
    #[serde(skip_serializing_if = "IssueSettings::is_empty")]
    pub issues: IssueSettings,
    /// Custom extraction and search rules (spec 013).
    pub custom: CustomRules,
    /// Spec 023: Link Score and link positions.
    pub links: LinkAnalysisSettings,
    pub access: AccessSettings,
    pub ai_checks: AiChecks,
    /// AI agent readiness (spec 016).
    pub agents: AgentSettings,
    /// The last options used to generate XML sitemaps (spec 011 FR-025).
    pub sitemap_generate: SitemapGenerateOptions,
    /// URL groups shown across the results (spec 027 part B). Results-only:
    /// they never change what is crawled.
    pub segments: Vec<crate::segments::Segment>,
    pub spelling: SpellingSettings,
    /// Accessibility and mobile checks (spec 026).
    pub accessibility: AccessibilitySettings,
}

/// How far the accessibility checks go (spec 026 FR-001).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum A11yLevel {
    /// Nothing is checked or stored.
    Off,
    /// Checks the HTML alone can decide.
    #[default]
    Static,
    /// Static checks plus axe-core in the rendered page.
    Full,
}

/// WCAG version a run is measured against (spec 026 FR-003).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[ts(export)]
pub enum WcagVersion {
    #[serde(rename = "2.0")]
    V2_0,
    #[serde(rename = "2.1")]
    V2_1,
    #[default]
    #[serde(rename = "2.2")]
    V2_2,
}

/// WCAG conformance level (spec 026 FR-003).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum WcagLevel {
    A,
    #[default]
    Aa,
    Aaa,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export)]
pub struct WcagTarget {
    pub version: WcagVersion,
    pub level: WcagLevel,
}

/// Accessibility and mobile usability checks (spec 026, data-model.md).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export)]
pub struct AccessibilitySettings {
    pub level: A11yLevel,
    pub wcag: WcagTarget,
    /// Also report axe-core best practices.
    pub best_practice: bool,
    /// Mobile usability checks (FR-004).
    pub mobile: bool,
    /// Probe time allowed per page, 1 to 30 seconds (FR-005).
    pub budget_secs: u32,
    /// Text below this size counts as small, 6 to 24 px (FR-016).
    pub small_text_px: u32,
    /// Share of small text that is reported, 1 to 100 percent.
    pub small_text_share: u32,
    /// Smallest tap target, 16 to 64 px.
    pub tap_target_px: u32,
    /// Horizontal overflow allowed, 0 to 100 px.
    pub overflow_px: u32,
}

impl Default for AccessibilitySettings {
    fn default() -> Self {
        Self {
            level: A11yLevel::Static,
            wcag: WcagTarget::default(),
            best_practice: true,
            mobile: true,
            budget_secs: 5,
            small_text_px: 12,
            small_text_share: 40,
            tap_target_px: 24,
            overflow_px: 5,
        }
    }
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
    /// Most pages requested from one host (subdomain); blank means no limit.
    #[ts(type = "number | null")]
    pub max_urls_per_subdomain: Option<u64>,
    /// Most pages requested at one crawl depth; blank means no limit.
    #[ts(type = "number | null")]
    pub max_urls_per_depth: Option<u64>,
    /// Most folders in a URL path: `/a/b/page` has 2. Deeper URLs are not
    /// crawled. Seeds are exempt.
    pub max_folder_depth: Option<u32>,
    /// Most query parameters in a URL; 0 means no query strings. Seeds are
    /// exempt.
    pub max_query_params: Option<u32>,
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
    /// Regex replacements applied in order to every discovered link after
    /// normalization (spec 002 follow-up).
    pub rewrite: Vec<RewriteRule>,
}

/// One URL rewrite: `pattern` is a regex, `replace` may use `$1` or `${name}`.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export)]
pub struct RewriteRule {
    pub pattern: String,
    pub replace: String,
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
    rewrite: Vec<RewriteRule>,
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
            rewrite: d.rewrite,
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
            rewrite: f.rewrite,
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
            rewrite: Vec::new(),
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
    /// robots.txt text used instead of a host's own file (spec 002
    /// follow-up). The site's file is then not requested.
    pub custom: Vec<CustomRobots>,
}

impl Default for RobotsSettings {
    fn default() -> Self {
        Self {
            mode: RobotsMode::Respect,
            honour_crawl_delay: true,
            custom: Vec::new(),
        }
    }
}

/// A custom robots.txt for one host, `example.com` or `shop.example.com`;
/// it applies to http and https and every port of that host.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export)]
pub struct CustomRobots {
    pub host: String,
    pub text: String,
}

impl RobotsSettings {
    /// The custom robots.txt text for `host`, matched without case.
    pub fn custom_for(&self, host: &str) -> Option<&str> {
        self.custom
            .iter()
            .find(|c| c.host.trim().eq_ignore_ascii_case(host))
            .map(|c| c.text.as_str())
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
    /// FR-031). PDF text follows the same setting (spec 027 FR-C05).
    pub store_html: bool,
    /// Follow `amphtml` links and check AMP pages (spec 027 part E).
    pub check_amp: bool,
    /// PDF documents (spec 027 part C).
    pub pdf: PdfSettings,
}

impl Default for ExtractionSettings {
    fn default() -> Self {
        Self {
            store_html: true,
            check_amp: false,
            pdf: PdfSettings::default(),
        }
    }
}

/// Reading PDFs (spec 027 FR-C01, FR-C03, FR-C04). Off by default, so the
/// crawl does exactly what it did before unless asked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export)]
pub struct PdfSettings {
    /// Download PDF bodies up to `max_mb` and read them.
    pub crawl: bool,
    /// Hand links found in PDFs to discovery (they are stored either way).
    pub follow_links: bool,
    /// PDF size limit, separate from the page size limit.
    pub max_mb: u32,
    /// Time limit for reading one PDF.
    pub time_limit_secs: u32,
}

impl Default for PdfSettings {
    fn default() -> Self {
        Self {
            crawl: false,
            follow_links: true,
            max_mb: 20,
            time_limit_secs: 5,
        }
    }
}

/// How urgent an issue is (spec 009). Declaration order is display order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, rename = "IssuePriority")]
pub enum Priority {
    High,
    Medium,
    Low,
    Opportunity,
}

impl Priority {
    pub const ALL: [Priority; 4] = [Self::High, Self::Medium, Self::Low, Self::Opportunity];

    pub fn label(self) -> &'static str {
        match self {
            Self::High => "High",
            Self::Medium => "Medium",
            Self::Low => "Low",
            Self::Opportunity => "Opportunity",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|p| p.label().eq_ignore_ascii_case(s.trim()))
    }
}

/// Per-rule overrides of the issue rule catalogue (spec 009, plan D7). Rule
/// ids are checked against the catalogue by `spiderlens-results`, which owns
/// it; unknown ids are ignored there with a notice.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export)]
pub struct IssueSettings {
    pub rules: std::collections::BTreeMap<String, RuleOverride>,
}

impl IssueSettings {
    pub fn is_empty(&self) -> bool {
        self.rules.values().all(RuleOverride::is_empty)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export)]
pub struct RuleOverride {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority: Option<Priority>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub threshold: Option<f64>,
}

impl RuleOverride {
    pub fn is_empty(&self) -> bool {
        self.enabled.is_none() && self.priority.is_none() && self.threshold.is_none()
    }
}

/// Sign-ins, the login page, cookies and the proxy: how the crawl reaches
/// a site (spec 025, data-model.md).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export)]
pub struct AccessSettings {
    /// HTTP sign-ins, one per origin (FR-001).
    pub sign_ins: Vec<SignIn>,
    /// The login page whose session the crawl uses (FR-007). The session
    /// itself is never stored.
    pub login: Option<LoginSettings>,
    /// Sent to the crawl's internal hosts only (FR-015, FR-016).
    pub cookies: Vec<SuppliedCookie>,
    pub cookie_policy: CookiePolicy,
    pub proxy: ProxySettings,
}

/// An HTTP sign-in for one origin (FR-001 to FR-006).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SignIn {
    /// Scheme, host and port only, such as `https://staging.example.com`.
    pub origin: String,
    /// May be `DOMAIN\user` for NTLM.
    pub user: String,
    #[serde(default)]
    #[ts(type = "{ secret: true, value?: string }")]
    pub password: Secret,
    #[serde(default)]
    pub method: AuthMethod,
}

impl SignIn {
    /// The origin in the form URLs give it (`https://host` or
    /// `http://host:8080`), for matching requests.
    pub fn origin_key(&self) -> Option<String> {
        let url = url::Url::parse(self.origin.trim()).ok()?;
        Some(url.origin().ascii_serialization())
    }
}

/// Which challenge a sign-in answers. `Any` takes the strongest the
/// server offers: NTLM, then Digest, then Basic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum AuthMethod {
    #[default]
    Any,
    Basic,
    Digest,
    Ntlm,
}

impl AuthMethod {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Any => "any",
            Self::Basic => "basic",
            Self::Digest => "digest",
            Self::Ntlm => "ntlm",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "any" => Some(Self::Any),
            "basic" => Some(Self::Basic),
            "digest" => Some(Self::Digest),
            "ntlm" => Some(Self::Ntlm),
            _ => None,
        }
    }
}

/// Sign-out links a crawl with a login session never requests (FR-011).
pub const DEFAULT_SIGN_OUT_PATTERNS: [&str; 6] =
    ["logout", "log-out", "signout", "sign-out", "logoff", "log-off"];

fn default_sign_out_patterns() -> Vec<String> {
    DEFAULT_SIGN_OUT_PATTERNS
        .iter()
        .map(|p| (*p).to_string())
        .collect()
}

/// The login page (FR-007 to FR-012). Only these settings are stored; the
/// cookies captured from it stay in memory.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct LoginSettings {
    pub login_url: String,
    /// Plain substrings of the path and query, compared ignoring case.
    #[serde(default = "default_sign_out_patterns")]
    pub sign_out_patterns: Vec<String>,
    #[serde(default)]
    pub check: Option<SignedOutCheck>,
}

impl LoginSettings {
    pub fn new(login_url: impl Into<String>) -> Self {
        Self {
            login_url: login_url.into(),
            sign_out_patterns: default_sign_out_patterns(),
            check: None,
        }
    }
}

/// How the crawl notices that the session ended (FR-012).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SignedOutCheck {
    pub url: String,
    pub when: CheckCondition,
}

/// The condition that means "signed out".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum CheckCondition {
    /// The check URL redirects to a URL containing this text.
    RedirectsTo(String),
    /// The check URL answers this status code.
    Status(u16),
    /// The check URL's page contains this text.
    Contains(String),
}

/// The proxy every request goes through (FR-020 to FR-024).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export)]
pub struct ProxySettings {
    pub mode: ProxyMode,
    pub kind: ProxyKind,
    pub host: String,
    pub port: u16,
    pub user: Option<String>,
    #[ts(type = "{ secret: true, value?: string }")]
    pub password: Secret,
    /// Hosts reached directly: `localhost`, `*.example.com`, `10.0.0.0/8`.
    pub bypass: Vec<String>,
}

impl ProxySettings {
    /// A manual proxy with a user needs its password.
    pub fn needs_password(&self) -> bool {
        self.mode == ProxyMode::Manual
            && self.user.as_deref().is_some_and(|u| !u.trim().is_empty())
            && !self.password.is_set()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ProxyMode {
    /// Direct connections, whatever the shell's environment says.
    #[default]
    None,
    /// The operating system's settings, read when the crawl starts.
    System,
    Manual,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ProxyKind {
    #[default]
    Http,
    Https,
    Socks5,
}

impl ProxyKind {
    pub fn scheme(self) -> &'static str {
        match self {
            Self::Http => "http",
            Self::Https => "https",
            // DNS goes through the proxy too.
            Self::Socks5 => "socks5h",
        }
    }
}

/// `staging.example.com:8443` becomes `STAGING_EXAMPLE_COM_8443`: the
/// suffix of the CLI's environment variables (data-model.md).
pub fn env_suffix(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_uppercase()
            } else {
                '_'
            }
        })
        .collect()
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
        let sign_ins = self
            .access
            .sign_ins
            .iter()
            .filter(|s| !s.password.is_set())
            .map(|s| format!("sign_in:{}", s.origin));
        let proxy = self.access.proxy.needs_password().then(|| "proxy".to_string());
        headers.chain(cookies).chain(sign_ins).chain(proxy).collect()
    }

    /// A readable label for a [`Self::missing_secrets`] key, for prompts
    /// and the resume dialog.
    pub fn secret_label(&self, key: &str) -> String {
        if let Some(origin) = key.strip_prefix("sign_in:") {
            let user = self
                .access
                .sign_ins
                .iter()
                .find(|s| s.origin == origin)
                .map(|s| s.user.as_str())
                .unwrap_or_default();
            let host = url::Url::parse(origin)
                .ok()
                .and_then(|u| u.host_str().map(str::to_string))
                .unwrap_or_else(|| origin.to_string());
            return format!("Password for {host} (user {user})");
        }
        if key == "proxy" {
            let p = &self.access.proxy;
            return format!(
                "Proxy password for user {} at {}:{}",
                p.user.as_deref().unwrap_or_default(),
                p.host,
                p.port
            );
        }
        if let Some(rest) = key.strip_prefix("cookie:")
            && let Some((domain, name)) = rest.rsplit_once('/')
        {
            return format!("Value of cookie {name} for {domain}");
        }
        format!("Value of header {key}")
    }

    /// The CLI's environment variable for a [`Self::missing_secrets`] key
    /// (contracts/cli.md).
    pub fn secret_env_var(key: &str) -> String {
        if let Some(origin) = key.strip_prefix("sign_in:") {
            let host = url::Url::parse(origin)
                .ok()
                .map(|u| match (u.host_str(), u.port()) {
                    (Some(h), Some(p)) => format!("{h}:{p}"),
                    (Some(h), None) => h.to_string(),
                    _ => origin.to_string(),
                })
                .unwrap_or_else(|| origin.to_string());
            return format!("SPIDERLENS_PASSWORD_{}", env_suffix(&host));
        }
        if key == "proxy" {
            return "SPIDERLENS_PROXY_PASSWORD".into();
        }
        if let Some(rest) = key.strip_prefix("cookie:")
            && let Some((_, name)) = rest.rsplit_once('/')
        {
            return format!("SPIDERLENS_COOKIE_{}", env_suffix(name));
        }
        format!("SPIDERLENS_HEADER_{}", env_suffix(key))
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
        for s in &mut self.access.sign_ins {
            if !s.password.is_set()
                && let Some(v) = find(&format!("sign_in:{}", s.origin), false)
            {
                s.password = Secret::new(v);
            }
        }
        if self.access.proxy.needs_password()
            && let Some(v) = find("proxy", false)
        {
            self.access.proxy.password = Secret::new(v);
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

/// AI agent readiness settings (spec 016 data-model, `agent_settings`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export)]
pub struct AgentSettings {
    /// Fetch `/.well-known/ai-catalog.json`, `/.well-known/api-catalog` and
    /// `/.well-known/agent-card.json` once per internal host at the end of
    /// the crawl.
    pub entry_points: bool,
    /// Most distinct Markdown alternates requested per host after the last
    /// page, to check that each answers with Markdown. The rest are listed
    /// as not verified. Needs `entry_points`.
    pub markdown_alternates_max: u32,
    /// Send one MCP discovery request to each MCP endpoint the site's
    /// catalog or cards list (at most 5 per host). Needs `entry_points`.
    pub mcp_discovery: bool,
}

impl Default for AgentSettings {
    fn default() -> Self {
        Self {
            entry_points: true,
            markdown_alternates_max: 50,
            mcp_discovery: true,
        }
    }
}

/// Spelling checks (spec 027 part D). They run over the crawl file after
/// the crawl, so they never slow fetching.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export)]
pub struct SpellingSettings {
    /// Check spelling when the crawl ends.
    pub check: bool,
    /// For pages that do not declare a language, as a BCP 47 tag.
    pub default_lang: String,
    /// Words written in capitals only are names or acronyms more often than
    /// mistakes.
    pub skip_all_caps: bool,
    /// Words never reported, matched without regard to case.
    pub ignore: Vec<String>,
}

impl Default for SpellingSettings {
    fn default() -> Self {
        Self {
            check: false,
            default_lang: "en".into(),
            skip_all_caps: true,
            ignore: Vec::new(),
        }
    }
}

/// JavaScript rendering (spec 014, data-model.md "Settings"). A
/// configuration without this section loads with rendering off.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export)]
pub struct RenderSettings {
    pub mode: RenderMode,
    /// The longest one page may take, 1 to 60 s (FR-010).
    pub max_render_secs: f64,
    /// Waited before the settle rule applies, 0 to `max_render_secs`.
    pub min_wait_secs: f64,
    pub window: RenderWindow,
    pub block: RenderBlock,
    /// Pages rendered, by regex on the URL (same rules as `scope.include`).
    pub include: Vec<String>,
    pub exclude: Vec<String>,
    /// Also render internal 4xx and 5xx HTML pages (FR-004).
    pub render_error_pages: bool,
    /// Pages rendered at once, 1 to 16. Null: total memory GB / 4, clamped.
    pub concurrency: Option<u32>,
    /// Browser memory budget, 256 to 65,536 MB. Null: total memory / 8.
    pub memory_budget_mb: Option<u32>,
    pub screenshots: bool,
    /// FR-027: share of rendered text that is new, 1 to 1,000 %.
    pub js_content_min_percent: u32,
    /// FR-027: words added by rendering, 0 to 100,000.
    pub js_content_min_words: u32,
    /// FR-030: share of text hidden from non-rendering crawlers, 1 to 100 %.
    pub hidden_content_percent: u32,
}

impl Default for RenderSettings {
    fn default() -> Self {
        Self {
            mode: RenderMode::Raw,
            max_render_secs: 10.0,
            min_wait_secs: 0.0,
            window: RenderWindow::default(),
            block: RenderBlock::default(),
            include: Vec::new(),
            exclude: Vec::new(),
            render_error_pages: false,
            concurrency: None,
            memory_budget_mb: None,
            screenshots: false,
            js_content_min_percent: 20,
            js_content_min_words: 50,
            hidden_content_percent: 50,
        }
    }
}

impl RenderSettings {
    /// Whether the crawl renders pages.
    pub fn is_on(&self) -> bool {
        self.mode == RenderMode::Javascript
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum RenderMode {
    /// Raw HTML only, as before spec 014.
    #[default]
    Raw,
    Javascript,
}

/// The rendering window (FR-008).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export)]
pub struct RenderWindow {
    pub preset: WindowPreset,
    /// Required for `custom`: 320 to 3,840 CSS px.
    pub width: Option<u32>,
    /// Required for `custom`: 480 to 4,320 CSS px.
    pub height: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum WindowPreset {
    /// Mobile for a mobile user agent, desktop otherwise.
    #[default]
    Auto,
    /// 411 x 731.
    Mobile,
    /// 1024 x 768.
    Desktop,
    Custom,
}

/// Resource types not loaded while rendering (FR-014).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export)]
pub struct RenderBlock {
    pub images: bool,
    pub media: bool,
    pub fonts: bool,
    pub stylesheets: bool,
    pub third_party: bool,
}

impl Default for RenderBlock {
    fn default() -> Self {
        Self {
            images: true,
            media: true,
            fonts: true,
            stylesheets: false,
            third_party: false,
        }
    }
}

/// Near duplicate content analysis (spec 020, data-model.md).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export)]
pub struct ContentSettings {
    /// Pages at least this similar (percent) are near duplicates (FR-010).
    pub threshold: u8,
    /// Closest matches at least this similar are recorded (FR-011).
    pub report_floor: u8,
    /// Pages with fewer main content words are not compared (FR-004).
    pub min_words: u32,
    /// Leave noindex and canonicalised pages out (FR-001).
    pub indexable_only: bool,
    /// CSS selector of the element holding the main content (FR-003).
    pub content_selector: Option<String>,
    /// CSS selectors of elements left out of the main content (FR-003).
    pub exclude_selectors: Vec<String>,
}

impl Default for ContentSettings {
    fn default() -> Self {
        Self {
            threshold: 90,
            report_floor: 80,
            min_words: 50,
            indexable_only: true,
            content_selector: None,
            exclude_selectors: Vec::new(),
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
    fn issue_settings_store_only_overrides() {
        // Spec 009 FR-022: old profiles load unchanged and defaults are not written.
        let c = CrawlConfig::default();
        let v = serde_json::to_value(&c).unwrap();
        assert!(v.get("issues").is_none());
        let back: CrawlConfig = serde_json::from_value(v).unwrap();
        assert_eq!(back, c);

        let mut c = CrawlConfig::default();
        c.issues.rules.insert(
            "title.over".into(),
            RuleOverride {
                threshold: Some(70.0),
                ..Default::default()
            },
        );
        c.issues
            .rules
            .insert("canonical.missing".into(), RuleOverride::default());
        let v = serde_json::to_value(&c).unwrap();
        assert_eq!(v["issues"]["rules"]["title.over"], json!({ "threshold": 70.0 }));
        let back: CrawlConfig = serde_json::from_value(v).unwrap();
        assert_eq!(back.issues.rules["title.over"].threshold, Some(70.0));
    }

    #[test]
    fn priorities_parse_and_order() {
        assert_eq!(Priority::parse("high"), Some(Priority::High));
        assert_eq!(Priority::parse(" Opportunity "), Some(Priority::Opportunity));
        assert_eq!(Priority::parse("urgent"), None);
        assert!(Priority::High < Priority::Opportunity);
        assert_eq!(serde_json::to_value(Priority::Medium).unwrap(), json!("medium"));
    }

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
                list_depth: 0,
                max_urls_per_subdomain: None,
                max_urls_per_depth: None,
                max_folder_depth: None,
                max_query_params: None,
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
        let r = &c.rendering;
        assert_eq!(r.mode, RenderMode::Raw);
        assert!(!r.is_on());
        assert_eq!(r.max_render_secs, 10.0);
        assert_eq!(r.min_wait_secs, 0.0);
        assert_eq!(r.window.preset, WindowPreset::Auto);
        assert_eq!((r.window.width, r.window.height), (None, None));
        assert!(r.block.images && r.block.media && r.block.fonts);
        assert!(!r.block.stylesheets && !r.block.third_party);
        assert!(r.include.is_empty() && r.exclude.is_empty());
        assert!(!r.render_error_pages);
        assert_eq!(r.concurrency, None);
        assert_eq!(r.memory_budget_mb, None);
        assert!(!r.screenshots);
        assert_eq!(r.js_content_min_percent, 20);
        assert_eq!(r.js_content_min_words, 50);
        assert_eq!(r.hidden_content_percent, 50);
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
            "rendering",
            "links",
            "access",
        ] {
            assert!(v.get(group).is_some(), "missing {group}");
        }
        assert_eq!(v["limits"]["max_urls"], json!(null));
        assert_eq!(v["speed"]["preset"], "polite");
        let r = &v["rendering"];
        assert_eq!(r["mode"], "raw");
        assert_eq!(r["concurrency"], json!(null));
        assert_eq!(r["memory_budget_mb"], json!(null));
        assert_eq!(r["window"]["preset"], "auto");
        assert_eq!(r["window"]["width"], json!(null));
        assert_eq!(r["block"]["images"], true);
        assert_eq!(r["block"]["third_party"], false);
        for key in [
            "max_render_secs",
            "min_wait_secs",
            "include",
            "exclude",
            "render_error_pages",
            "screenshots",
            "js_content_min_percent",
            "js_content_min_words",
            "hidden_content_percent",
        ] {
            assert!(r.get(key).is_some(), "missing rendering.{key}");
        }
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
    fn sign_in_and_proxy_passwords_are_listed_filled_and_never_written() {
        let mut c: CrawlConfig = serde_json::from_value(json!({
            "access": {
                "sign_ins": [{"origin": "https://staging.example.com:8443", "user": "ann", "password": {"secret": true}}],
                "proxy": {"mode": "manual", "host": "proxy.corp", "port": 8080, "user": "ann", "password": {"secret": true}},
                "login": {"login_url": "https://example.com/login"}
            }
        }))
        .unwrap();
        assert_eq!(c.access.sign_ins[0].method, AuthMethod::Any);
        assert_eq!(c.access.login.as_ref().unwrap().sign_out_patterns.len(), 6);
        assert_eq!(
            c.missing_secrets(),
            ["sign_in:https://staging.example.com:8443", "proxy"]
        );
        assert_eq!(
            CrawlConfig::secret_env_var("sign_in:https://staging.example.com:8443"),
            "SPIDERLENS_PASSWORD_STAGING_EXAMPLE_COM_8443"
        );
        assert_eq!(CrawlConfig::secret_env_var("proxy"), "SPIDERLENS_PROXY_PASSWORD");
        assert_eq!(
            c.secret_label("sign_in:https://staging.example.com:8443"),
            "Password for staging.example.com (user ann)"
        );
        let left = c.fill_secrets(&[
            (
                "sign_in:https://staging.example.com:8443".into(),
                "pw-site".into(),
            ),
            ("proxy".into(), "pw-proxy".into()),
        ]);
        assert!(left.is_empty());
        assert_eq!(c.access.sign_ins[0].password.expose(), Some("pw-site"));
        let json = serde_json::to_string(&c).unwrap();
        assert!(!json.contains("pw-site") && !json.contains("pw-proxy"));
        let back: CrawlConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(back.missing_secrets().len(), 2);
    }

    #[test]
    fn missing_fields_take_defaults() {
        let c: CrawlConfig = serde_json::from_value(json!({"speed": {"max_concurrent": 16}})).unwrap();
        assert_eq!(c.speed.max_concurrent, 16);
        assert_eq!(c.speed.max_per_host, 4);
        assert_eq!(c.retry, RetrySettings::default());
        // Configs from before spec 005 keep raw HTML (the default).
        assert!(c.extraction.store_html);
        // Configs from before spec 014 do not render.
        assert_eq!(c.rendering, RenderSettings::default());
        assert_eq!(c.rendering.mode, RenderMode::Raw);
        let c: CrawlConfig =
            serde_json::from_value(json!({"rendering": {"mode": "javascript", "block": {"images": false}}}))
                .unwrap();
        assert!(c.rendering.is_on());
        assert!(!c.rendering.block.images && c.rendering.block.fonts);
        assert_eq!(c.rendering.max_render_secs, 10.0);
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
