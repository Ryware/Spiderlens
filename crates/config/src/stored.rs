//! The configuration stored inside a crawl file (FR-037 to FR-042;
//! research R11). Spec 006 writes and reads it; this module defines it.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

use crate::{CrawlConfig, ModeInput, RenderSettings, RetrySettings, SpeedSettings};

pub const STORED_FORMAT_VERSION: u32 = 1;

/// Only speed, retry and the URL limit may change during or between runs of
/// a crawl (spec 006 FR-011), plus the rendering settings when a finished
/// crawl is rendered later (spec 014 FR-006, plan owner point 2). The type
/// cannot express any other change, so a resumed crawl can never change
/// which URLs are in scope.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum Change {
    Speed(SpeedSettings),
    Retry(RetrySettings),
    /// Blank (null) means no limit.
    MaxUrls(#[ts(type = "number | null")] Option<u64>),
    /// The settings of a later rendering run of a finished crawl (spec 014).
    Rendering(RenderSettings),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ConfigChange {
    #[ts(type = "string")]
    pub at: jiff::Timestamp,
    pub change: Change,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct StoredCrawlConfig {
    pub format_version: u32,
    pub app_version: String,
    #[ts(type = "string")]
    pub started_at: jiff::Timestamp,
    /// The configuration as it was when the crawl started. Never edited.
    pub config: CrawlConfig,
    pub input: ModeInput,
    /// Internal hosts after following the start URL's redirects (FR-003).
    pub resolved_hosts: Vec<String>,
    #[serde(default)]
    pub changes: Vec<ConfigChange>,
}

impl StoredCrawlConfig {
    pub fn new(
        config: CrawlConfig,
        input: ModeInput,
        resolved_hosts: Vec<String>,
        started_at: jiff::Timestamp,
    ) -> Self {
        Self {
            format_version: STORED_FORMAT_VERSION,
            app_version: env!("CARGO_PKG_VERSION").into(),
            started_at,
            config,
            input,
            resolved_hosts,
            changes: Vec::new(),
        }
    }

    /// Records a mid-crawl change (FR-040).
    pub fn record(&mut self, at: jiff::Timestamp, change: Change) {
        self.changes.push(ConfigChange { at, change });
    }

    /// The configuration in force now: the original plus changes in order.
    pub fn effective(&self) -> CrawlConfig {
        let mut c = self.config.clone();
        for change in &self.changes {
            match &change.change {
                Change::Speed(s) => c.speed = *s,
                Change::Retry(r) => c.retry = *r,
                Change::MaxUrls(m) => c.limits.max_urls = *m,
                Change::Rendering(r) => c.rendering = r.clone(),
            }
        }
        c
    }

    /// For "New crawl with this configuration" and "Save as profile"
    /// (FR-039): the effective settings and the start URL, if any.
    pub fn to_new_crawl(&self) -> (CrawlConfig, ModeInput) {
        (self.effective(), self.input.clone())
    }
}

/// Settings the current version has but a stored configuration lacks,
/// because an older version wrote it (FR-042). Paths like `retry.retry_429`.
pub fn unavailable_settings(stored_config: &Value) -> Vec<String> {
    let current = serde_json::to_value(CrawlConfig::default()).expect("defaults serialize");
    let mut missing = Vec::new();
    diff_keys(&current, stored_config, "", &mut missing);
    missing
}

fn diff_keys(current: &Value, stored: &Value, prefix: &str, out: &mut Vec<String>) {
    let Value::Object(cur) = current else { return };
    for (key, cur_value) in cur {
        // Output-only fields are not settings.
        if key == "preset" && prefix == "speed" {
            continue;
        }
        let path = if prefix.is_empty() {
            key.clone()
        } else {
            format!("{prefix}.{key}")
        };
        match stored.get(key) {
            None => out.push(path),
            Some(s) => diff_keys(cur_value, s, &path, out),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn stored() -> StoredCrawlConfig {
        StoredCrawlConfig::new(
            CrawlConfig::default(),
            ModeInput::Spider {
                start_url: "https://a.test/".into(),
            },
            vec!["a.test".into()],
            "2026-10-11T10:00:00Z".parse().unwrap(),
        )
    }

    #[test]
    fn url_limit_can_change() {
        let mut s = stored();
        s.record("2026-10-11T10:05:00Z".parse().unwrap(), Change::MaxUrls(Some(10)));
        assert_eq!(s.effective().limits.max_urls, Some(10));
        assert_eq!(s.config.limits.max_urls, None, "the original is never edited");
    }

    #[test]
    fn changes_apply_in_order() {
        let mut s = stored();
        s.record(
            "2026-10-11T10:05:00Z".parse().unwrap(),
            Change::Speed(SpeedSettings::FAST),
        );
        let retry = RetrySettings {
            attempts: 1,
            ..Default::default()
        };
        s.record("2026-10-11T10:06:00Z".parse().unwrap(), Change::Retry(retry));
        s.record(
            "2026-10-11T10:07:00Z".parse().unwrap(),
            Change::Speed(SpeedSettings::POLITE),
        );
        let e = s.effective();
        assert_eq!(e.speed, SpeedSettings::POLITE);
        assert_eq!(e.retry.attempts, 1);
        assert_eq!(s.config, CrawlConfig::default(), "original is never edited");
    }

    #[test]
    fn rendering_later_is_a_recorded_change() {
        let mut s = stored();
        let mut r = crate::RenderSettings {
            mode: crate::RenderMode::Javascript,
            ..Default::default()
        };
        r.screenshots = true;
        s.record(
            "2026-10-12T09:00:00Z".parse().unwrap(),
            Change::Rendering(r.clone()),
        );
        assert_eq!(s.effective().rendering, r);
        assert!(!s.config.rendering.is_on(), "the original is never edited");
        let back: StoredCrawlConfig = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
        assert_eq!(back, s);
        let v = serde_json::to_value(&s.changes[0].change).unwrap();
        assert_eq!(v["rendering"]["mode"], "javascript");
    }

    #[test]
    fn round_trips_through_json() {
        let mut s = stored();
        s.record(
            "2026-10-11T10:05:00Z".parse().unwrap(),
            Change::Speed(SpeedSettings::FAST),
        );
        let back: StoredCrawlConfig = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
        assert_eq!(back, s);
    }

    #[test]
    fn to_profile_keeps_effective_settings() {
        let mut s = stored();
        s.record(
            "2026-10-11T10:05:00Z".parse().unwrap(),
            Change::Speed(SpeedSettings::FAST),
        );
        let (config, input) = s.to_new_crawl();
        assert_eq!(config.speed, SpeedSettings::FAST);
        assert_eq!(input, s.input);
    }

    #[test]
    fn missing_fields_reported() {
        let mut old = serde_json::to_value(CrawlConfig::default()).unwrap();
        old["retry"].as_object_mut().unwrap().remove("retry_429");
        old.as_object_mut().unwrap().remove("robots");
        let missing = unavailable_settings(&old);
        assert_eq!(missing, vec!["retry.retry_429", "robots"]);
        assert!(
            unavailable_settings(&json!(serde_json::to_value(CrawlConfig::default()).unwrap())).is_empty()
        );
    }
}
