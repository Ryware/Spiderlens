//! SpiderLens crawl configuration (spec 002).
//!
//! One typed model shared by the engine, the CLI, the desktop app and the
//! crawl store. Validation lives here too, so every surface reports the same
//! issues. This crate has no dependency on the desktop shell.

pub mod ai_bots;
mod compare;
pub mod custom;
pub mod dirs;
mod links;
mod list_input;
mod model;
mod patterns;
mod presets;
mod profile;
mod recent;
mod secret;
mod segments;
mod site;
mod storage;
mod store;
mod stored;
mod url_norm;
mod validate;

pub use compare::{CompareSettings, CompareSettingsError, ElementSetting, IssueSettingsChoice, MapRule};
pub use custom::{
    CustomRules, ElementKind, ElementLimit, ExtractMode, Extractor, ExtractorKind, Search, SearchCondition,
    SearchTarget, export_rules, import_rules, merge_rules,
};
pub use links::{LinkAnalysisSettings, Position, PositionRule, PositionWeights, check_selector};
pub use list_input::{InvalidLine, ListFormat, ListInput, parse_list_bytes, parse_list_text};
pub use model::{
    AccessSettings, AiChecks, Changefreq, CookiePolicy, CrawlConfig, CustomHeader, ExtractionSettings,
    HeaderTarget, Identity, ImageEntries, IssueSettings, LastmodSource, Limits, LocalSitemap, Mode,
    ModeInput, NetworkSettings, Priority, PrioritySource, QueryMode, QueryParams, RetrySettings, RobotsMode,
    RobotsSettings, RuleOverride, Scope, SitemapGenerateOptions, SitemapSettings, SpeedSettings,
    SubdomainMode, SuppliedCookie, UserAgent,
};
pub use patterns::{Decision, PatternTest, UrlFilter, test_patterns};
pub use presets::{SpeedPreset, UaPreset, spiderlens_user_agent};
pub use profile::{FORMAT_VERSION, ImportedProfile, ProfileError, export_profile, import_profile};
pub use recent::{RECENT_LIMIT, RecentEntry, RecentStore};
pub use secret::{HeaderValue, Secret};
pub use segments::{
    Clause, Field as SegmentField, MAX_SEGMENTS, Match as SegmentMatch, Op as SegmentOp, Segment,
    SegmentColor, SegmentMatcher, UrlFacts, validate_segments,
};
pub use site::{InternalHosts, registrable_domain};
pub use storage::{
    CLEANUP_DAY_CHOICES, CleanupReason, CleanupRecord, DEFAULT_CLEANUP_DAYS, DEFAULT_RESERVE, HISTORY_LIMIT,
    MAX_RESERVE, MIN_RESERVE, StoragePrefs, StorageStore,
};
pub use store::{ProfileEntry, ProfileStore, StoreError};
pub use stored::{Change, ConfigChange, StoredCrawlConfig, unavailable_settings};
pub use url_norm::{LinkProblem, NormError, apply_query_rules, normalize_link, normalize_url, tidy_percent};
pub use validate::{Issue, Severity, validate, validate_config};
