//! Issue settings applied to the catalogue (FR-021 to FR-024, plan D7).

use std::collections::HashMap;

use serde::Serialize;
use spiderlens_config::{IssueSettings, Priority};
use ts_rs::TS;

use super::{catalogue, fmt_num};

/// One rule as configured: in catalogue order, so the index is the rule's
/// id in temp tables.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ResolvedRule {
    pub enabled: bool,
    pub priority: Priority,
    /// The threshold in force, for rules that have one.
    pub threshold: Option<f64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedSettings {
    pub rules: Vec<ResolvedRule>,
    /// The overrides these were resolved from, as stored with the crawl.
    pub source: IssueSettings,
}

impl Default for ResolvedSettings {
    fn default() -> Self {
        resolve(&IssueSettings::default()).0
    }
}

impl ResolvedSettings {
    /// Threshold values by rule id, for the `thr()` SQL function.
    pub fn thresholds(&self) -> HashMap<&'static str, f64> {
        catalogue()
            .iter()
            .zip(&self.rules)
            .filter_map(|(r, s)| s.threshold.map(|t| (r.id, t)))
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct SettingsNotice {
    pub rule: String,
    /// True when the value was rejected; false for an ignored unknown rule.
    pub error: bool,
    pub message: String,
}

/// Applies `s` to the catalogue. Unknown rule ids are ignored with a notice
/// (Edge case: settings for a rule that no longer exists); thresholds out of
/// range are rejected with a notice and the default stays (FR-024).
pub fn resolve(s: &IssueSettings) -> (ResolvedSettings, Vec<SettingsNotice>) {
    let mut notices = Vec::new();
    for id in s.rules.keys() {
        if super::get(id).is_none() {
            notices.push(SettingsNotice {
                rule: id.clone(),
                error: false,
                message: format!("Issue settings for \"{id}\" were ignored: this version has no such rule."),
            });
        }
    }
    let rules = catalogue()
        .iter()
        .map(|r| {
            let o = s.rules.get(r.id).copied().unwrap_or_default();
            let threshold = r.threshold.map(|t| match o.threshold {
                Some(v) if v.is_finite() && v >= t.min && v <= t.max => v,
                Some(v) => {
                    notices.push(SettingsNotice {
                        rule: r.id.into(),
                        error: true,
                        message: format!(
                            "{}: {} is outside {} to {} {}; the default {} is used.",
                            super::display_name(r, None),
                            fmt_num(v),
                            fmt_num(t.min),
                            fmt_num(t.max),
                            t.unit.label(),
                            fmt_num(t.default)
                        ),
                    });
                    t.default
                }
                None => t.default,
            });
            if o.threshold.is_some() && r.threshold.is_none() {
                notices.push(SettingsNotice {
                    rule: r.id.into(),
                    error: false,
                    message: format!("{} has no threshold; the value was ignored.", r.name),
                });
            }
            ResolvedRule {
                enabled: o.enabled.unwrap_or(true),
                priority: o.priority.unwrap_or(r.priority),
                threshold,
            }
        })
        .collect();
    (
        ResolvedSettings {
            rules,
            source: s.clone(),
        },
        notices,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use spiderlens_config::RuleOverride;

    fn with(id: &str, o: RuleOverride) -> IssueSettings {
        let mut s = IssueSettings::default();
        s.rules.insert(id.into(), o);
        s
    }

    #[test]
    fn defaults_come_from_the_catalogue() {
        let (r, n) = resolve(&IssueSettings::default());
        assert!(n.is_empty());
        let i = super::super::index_of("title.over").unwrap();
        assert_eq!(
            r.rules[i],
            ResolvedRule {
                enabled: true,
                priority: Priority::Medium,
                threshold: Some(60.0)
            }
        );
        assert_eq!(r.thresholds()["title.over"], 60.0);
    }

    #[test]
    fn overrides_apply_and_bad_values_are_rejected() {
        let i = super::super::index_of("title.over").unwrap();
        let (r, n) = resolve(&with(
            "title.over",
            RuleOverride {
                threshold: Some(70.0),
                priority: Some(Priority::High),
                enabled: Some(false),
            },
        ));
        assert!(n.is_empty());
        assert_eq!(
            r.rules[i],
            ResolvedRule {
                enabled: false,
                priority: Priority::High,
                threshold: Some(70.0)
            }
        );

        // US4 scenario 5: a negative length is rejected and the default stays.
        let (r, n) = resolve(&with(
            "title.over",
            RuleOverride {
                threshold: Some(-5.0),
                ..Default::default()
            },
        ));
        assert_eq!(r.rules[i].threshold, Some(60.0));
        assert!(
            n[0].error && n[0].message.contains("outside 10 to 1,000 characters"),
            "{n:?}"
        );
    }

    #[test]
    fn unknown_rules_are_ignored_with_a_notice() {
        let (_, n) = resolve(&with(
            "title.gone",
            RuleOverride {
                enabled: Some(false),
                ..Default::default()
            },
        ));
        assert_eq!(n.len(), 1);
        assert!(!n[0].error && n[0].message.contains("title.gone"));
    }
}
