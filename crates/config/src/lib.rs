//! SpiderLens crawl configuration (spec 002).
//!
//! One typed model shared by the engine, the CLI, the desktop app and the
//! crawl store. Validation lives here too, so every surface reports the same
//! issues. This crate has no dependency on the desktop shell.

mod list_input;
mod model;
mod patterns;
mod presets;
mod profile;
mod recent;
mod secret;
mod site;
mod store;
mod stored;
mod url_norm;
mod validate;

pub use list_input::{InvalidLine, ListFormat, ListInput, parse_list_bytes, parse_list_text};
pub use model::{
    CrawlConfig, CustomHeader, ExtractionSettings, HeaderTarget, Identity, Limits, Mode, ModeInput,
    NetworkSettings, QueryMode, QueryParams, RenderBlock, RenderMode, RenderSettings, RenderWindow,
    RetrySettings, RobotsMode, RobotsSettings, Scope, SpeedSettings, SubdomainMode, UserAgent, WindowPreset,
};
pub use patterns::{Decision, PatternTest, UrlFilter, test_patterns};
pub use presets::{SpeedPreset, UaPreset, spiderlens_user_agent};
pub use profile::{FORMAT_VERSION, ImportedProfile, ProfileError, export_profile, import_profile};
pub use recent::{RECENT_LIMIT, RecentEntry, RecentStore};
pub use secret::HeaderValue;
pub use site::{InternalHosts, registrable_domain};
pub use store::{ProfileEntry, ProfileStore, StoreError};
pub use stored::{Change, ConfigChange, StoredCrawlConfig, unavailable_settings};
pub use url_norm::{LinkProblem, NormError, apply_query_rules, normalize_link, normalize_url, tidy_percent};
pub use validate::{Issue, Severity, validate, validate_config};
