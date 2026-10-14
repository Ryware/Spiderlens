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
    pub ai_checks: AiChecks,
    /// URL groups shown across the results (spec 027 part B). Results-only:
    /// they never change what is crawled.
    pub segments: Vec<crate::segments::Segment>,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(default)]
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
    /// Seed the crawl with the sitemaps listed in the start host's robots.txt.
    pub use_sitemaps: bool,
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
            use_sitemaps: false,
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
        assert!(!c.scope.stay_in_folder && !c.scope.use_sitemaps);
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
        ] {
            assert!(v.get(group).is_some(), "missing {group}");
        }
        assert_eq!(v["limits"]["max_urls"], json!(null));
        assert_eq!(v["speed"]["preset"], "polite");
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
