//! Comparison settings (spec 012 FR-028): URL mapping rules, tracked
//! elements and thresholds, and which issue settings both crawls use. The
//! desktop app remembers them and saves them as a file the CLI also reads.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// One find-and-replace rule applied to the previous crawl's URLs before
/// matching (FR-006). Rules apply in order; the first that matches wins.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct MapRule {
    pub find: String,
    pub replace: String,
    /// `find` is a regular expression and `replace` may use `$1` groups.
    #[serde(default)]
    pub regex: bool,
}

/// A user override for one tracked element (FR-019). Elements without an
/// override use their defaults.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ElementSetting {
    pub id: String,
    pub on: bool,
    /// Percent for counts, levels for depth, a factor for response time.
    #[serde(default)]
    pub threshold: Option<f64>,
}

/// Which issue settings both crawls are evaluated with (FR-023).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum IssueSettingsChoice {
    #[default]
    Current,
    Previous,
    App,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct CompareSettings {
    #[serde(default)]
    pub mapping: Vec<MapRule>,
    #[serde(default)]
    pub elements: Vec<ElementSetting>,
    #[serde(default)]
    pub issue_settings: IssueSettingsChoice,
}

/// A problem with comparison settings, naming the rule or element.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CompareSettingsError {
    #[error("mapping rule {index}: {message}")]
    Rule { index: usize, message: String },
    #[error("{id}: {message}")]
    Element { id: String, message: String },
    #[error("comparison settings file: {0}")]
    File(String),
}

impl CompareSettings {
    /// Checks every rule compiles and every threshold is a finite number
    /// zero or above. Element ids are checked by the comparison, which owns
    /// the element list.
    pub fn validate(&self) -> Result<(), CompareSettingsError> {
        for (i, r) in self.mapping.iter().enumerate() {
            let index = i + 1;
            if r.find.is_empty() {
                return Err(CompareSettingsError::Rule {
                    index,
                    message: "the text to find is empty".into(),
                });
            }
            if r.regex
                && let Err(e) = regex::Regex::new(&r.find)
            {
                return Err(CompareSettingsError::Rule {
                    index,
                    message: format!("not a valid regular expression: {e}"),
                });
            }
        }
        for e in &self.elements {
            if let Some(t) = e.threshold
                && !(t.is_finite() && t >= 0.0)
            {
                return Err(CompareSettingsError::Element {
                    id: e.id.clone(),
                    message: format!("threshold {t} must be a number zero or above"),
                });
            }
        }
        Ok(())
    }

    pub fn from_json(text: &str) -> Result<Self, CompareSettingsError> {
        let s: Self = serde_json::from_str(text).map_err(|e| CompareSettingsError::File(e.to_string()))?;
        s.validate()?;
        Ok(s)
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("settings serialise")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_and_defaults_missing_fields() {
        let s = CompareSettings {
            mapping: vec![MapRule {
                find: "https://staging.example.com/".into(),
                replace: "https://www.example.com/".into(),
                regex: false,
            }],
            elements: vec![ElementSetting {
                id: "word_count".into(),
                on: true,
                threshold: Some(25.0),
            }],
            issue_settings: IssueSettingsChoice::Previous,
        };
        assert_eq!(CompareSettings::from_json(&s.to_json()).unwrap(), s);
        assert_eq!(
            CompareSettings::from_json("{}").unwrap(),
            CompareSettings::default()
        );
    }

    #[test]
    fn rejects_bad_rules_and_thresholds() {
        let bad_regex = r#"{"mapping":[{"find":"(","replace":"x","regex":true}]}"#;
        assert!(matches!(
            CompareSettings::from_json(bad_regex),
            Err(CompareSettingsError::Rule { index: 1, .. })
        ));
        let empty = r#"{"mapping":[{"find":"","replace":"x"}]}"#;
        assert!(CompareSettings::from_json(empty).is_err());
        let negative = r#"{"elements":[{"id":"depth","on":true,"threshold":-1}]}"#;
        assert!(matches!(
            CompareSettings::from_json(negative),
            Err(CompareSettingsError::Element { .. })
        ));
        assert!(CompareSettings::from_json("not json").is_err());
    }
}
