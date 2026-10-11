//! Profile file format, export and import (FR-034, FR-035; research R8).

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::CrawlConfig;

pub const FORMAT: &str = "spiderlens-profile";
pub const FORMAT_VERSION: u32 = 1;

#[derive(Debug, Serialize, Deserialize)]
struct ProfileFile {
    format: String,
    format_version: u32,
    #[serde(default)]
    app_version: String,
    name: String,
    #[serde(default)]
    start_url: Option<String>,
    #[serde(default)]
    config: CrawlConfig,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ImportedProfile {
    pub name: String,
    pub config: CrawlConfig,
    pub start_url: Option<String>,
    /// Settings in the file that this version does not know, as paths.
    pub ignored: Vec<String>,
    /// App version that wrote the file.
    pub written_by: String,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ProfileError {
    #[error("This is not a SpiderLens profile file.")]
    Foreign,
    #[error("The profile file is damaged: {0}")]
    Corrupt(String),
}

/// Serializes a profile. Secret header values are written as placeholders.
pub fn export_profile(name: &str, config: &CrawlConfig, start_url: Option<&str>) -> String {
    let file = ProfileFile {
        format: FORMAT.into(),
        format_version: FORMAT_VERSION,
        app_version: env!("CARGO_PKG_VERSION").into(),
        name: name.into(),
        start_url: start_url.map(str::to_string),
        config: config.clone(),
    };
    serde_json::to_string_pretty(&file).expect("profile serializes")
}

/// Loads a profile. Unknown settings (from newer versions) are listed in
/// `ignored`; missing settings take defaults; foreign or damaged files are
/// refused without side effects.
pub fn import_profile(text: &str) -> Result<ImportedProfile, ProfileError> {
    let value: Value = serde_json::from_str(text).map_err(|_| ProfileError::Foreign)?;
    if value.get("format").and_then(Value::as_str) != Some(FORMAT) {
        return Err(ProfileError::Foreign);
    }
    let mut ignored = Vec::new();
    let file: ProfileFile = serde_ignored::deserialize(value, |path| ignored.push(path.to_string()))
        .map_err(|e| ProfileError::Corrupt(e.to_string()))?;
    if file.name.trim().is_empty() {
        return Err(ProfileError::Corrupt("the profile has no name".into()));
    }
    Ok(ImportedProfile {
        name: file.name,
        config: file.config,
        start_url: file.start_url,
        ignored,
        written_by: file.app_version,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CustomHeader, HeaderTarget, HeaderValue, SpeedSettings};

    fn custom() -> CrawlConfig {
        let mut c = CrawlConfig::default();
        c.limits.max_urls = Some(500);
        c.scope.exclude.push(r"\?sort=".into());
        c.speed = SpeedSettings::FAST;
        c.identity.headers.push(CustomHeader {
            name: "X-Staging-Key".into(),
            value: HeaderValue::Plain("abc".into()),
            send_to: HeaderTarget::Internal,
        });
        c
    }

    #[test]
    fn round_trip() {
        let c = custom();
        let p = import_profile(&export_profile("Client A", &c, Some("https://a.test/"))).unwrap();
        assert_eq!(p.config, c);
        assert_eq!(p.name, "Client A");
        assert_eq!(p.start_url.as_deref(), Some("https://a.test/"));
        assert!(p.ignored.is_empty());
    }

    #[test]
    fn secrets_are_not_exported() {
        let mut c = CrawlConfig::default();
        c.identity.headers.push(CustomHeader {
            name: "Authorization".into(),
            value: HeaderValue::secret("Bearer s3cret"),
            send_to: HeaderTarget::Internal,
        });
        let text = export_profile("x", &c, None);
        assert!(!text.contains("s3cret"));
        let back = import_profile(&text).unwrap();
        assert_eq!(back.config.identity.headers[0].value, HeaderValue::Secret(None));
    }

    #[test]
    fn unknown_settings_are_listed_and_known_ones_load() {
        let text = r#"{"format":"spiderlens-profile","format_version":2,"app_version":"9.0.0","name":"New",
            "config":{"limits":{"max_urls":10},"ai":{"model":"x"},"speed":{"max_concurrent":16,"turbo":true}}}"#;
        let p = import_profile(text).unwrap();
        assert_eq!(p.config.limits.max_urls, Some(10));
        assert_eq!(p.config.speed.max_concurrent, 16);
        assert_eq!(p.ignored, vec!["config.ai", "config.speed.turbo"]);
    }

    #[test]
    fn foreign_and_corrupt_files_are_refused() {
        assert_eq!(import_profile("not json"), Err(ProfileError::Foreign));
        assert_eq!(
            import_profile(r#"{"format":"other"}"#),
            Err(ProfileError::Foreign)
        );
        let bad = r#"{"format":"spiderlens-profile","format_version":1,"name":"x","config":{"limits":{"max_urls":"lots"}}}"#;
        assert!(matches!(import_profile(bad), Err(ProfileError::Corrupt(_))));
    }
}
