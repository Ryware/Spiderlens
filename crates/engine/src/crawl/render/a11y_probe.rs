//! The accessibility and mobile probes of the render stage (spec 026 plan
//! D2, `contracts/probes.md`).
//!
//! The probes are built once per crawl ([`probes_for`]) and cloned into each
//! render job. Their JSON results are parsed on the render task, never on
//! the coordinator, which only writes the rows.

use std::borrow::Cow;
use std::sync::LazyLock;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use spiderlens_config::{A11yLevel, AccessibilitySettings};
use spiderlens_render::{PageProbe, ProbeOutcome};

use crate::crawl::store::{A11yNodeRow, A11yRow};

/// The axe-core version in `assets/axe/axe.min.js` (stored as `a11y_engine`).
pub(crate) const AXE_VERSION: &str = "4.14.0";

/// Probe names; they key the outcomes.
pub(crate) const PROBE_A11Y: &str = "a11y";
pub(crate) const PROBE_MOBILE: &str = "mobile_layout";

/// `axe.min.js` followed by the runner, as one function expression. Built
/// once per process (about 650 KB).
static AXE_PROBE: LazyLock<String> = LazyLock::new(|| {
    format!(
        "(async (args) => {{ {axe}\n; return await ({runner})(args); }})",
        axe = include_str!("../../../assets/axe/axe.min.js"),
        runner = include_str!("../../../assets/a11y_runner.js"),
    )
});

const MOBILE_PROBE: &str = include_str!("../../../assets/mobile_layout.js");

/// The mobile probe is cheap; it never gets more than this.
const MOBILE_BUDGET: Duration = Duration::from_secs(1);

/// The probes a crawl's render jobs carry and the shared cap of all of them
/// (`RenderJob::probes_max_total`). `mobile_window` is whether the rendering
/// window is a mobile one. Empty when nothing is asked for.
pub(crate) fn probes_for(
    settings: &AccessibilitySettings,
    mobile_window: bool,
) -> (Vec<PageProbe>, Duration) {
    let total = Duration::from_secs(u64::from(settings.budget_secs.max(1)));
    let mut probes = Vec::new();
    if settings.level == A11yLevel::Off {
        return (probes, total);
    }
    if settings.mobile && mobile_window {
        probes.push(PageProbe {
            name: PROBE_MOBILE.to_string(),
            script: MOBILE_PROBE.to_string(),
            args_json: format!(
                "{{\"small_text_px\":{},\"tap_target_px\":{},\"examples\":20}}",
                settings.small_text_px, settings.tap_target_px
            ),
            budget: MOBILE_BUDGET.min(total),
        });
    }
    if settings.level == A11yLevel::Full {
        probes.push(PageProbe {
            name: PROBE_A11Y.to_string(),
            script: AXE_PROBE.clone(),
            args_json: "{\"node_cap\":20,\"page_cap\":500,\"excerpt\":300}".to_string(),
            budget: total,
        });
    }
    (probes, total)
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct OwnedNode {
    pub selector: String,
    pub excerpt: String,
    /// The node's extra data as a JSON string.
    pub data: Option<String>,
    pub bbox: Option<[f64; 4]>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct OwnedA11yResult {
    pub id: String,
    /// `violation` or `needs_review`.
    pub kind: &'static str,
    pub impact: Option<String>,
    /// WCAG tags and `best-practice`, space separated.
    pub wcag: String,
    pub total: u32,
    pub nodes: Vec<OwnedNode>,
}

impl OwnedA11yResult {
    /// The row the store writes.
    pub(crate) fn as_row(&self) -> A11yRow<'_> {
        A11yRow {
            check_id: Cow::Borrowed(&self.id),
            kind: self.kind,
            source: "full",
            impact: self.impact.as_deref(),
            wcag: Cow::Borrowed(&self.wcag),
            nodes_total: self.total,
            nodes: self
                .nodes
                .iter()
                .map(|n| A11yNodeRow {
                    selector: Cow::Borrowed(&n.selector),
                    excerpt: Cow::Borrowed(&n.excerpt),
                    data: n.data.clone(),
                    bbox: n.bbox,
                })
                .collect(),
        }
    }
}

/// A parsed `a11y` probe result.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct A11yFull {
    pub ms: i64,
    pub results: Vec<OwnedA11yResult>,
}

#[derive(Deserialize)]
struct RunnerOut {
    #[serde(default)]
    ms: f64,
    #[serde(default)]
    results: Vec<RunnerResult>,
}

#[derive(Deserialize)]
struct RunnerResult {
    id: String,
    #[serde(default)]
    kind: String,
    #[serde(default)]
    impact: Option<String>,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    total: u32,
    #[serde(default)]
    nodes: Vec<RunnerNode>,
}

#[derive(Deserialize)]
struct RunnerNode {
    #[serde(default)]
    selector: String,
    #[serde(default)]
    excerpt: String,
    #[serde(default)]
    data: Option<serde_json::Value>,
    #[serde(default, rename = "box")]
    bbox: Option<[f64; 4]>,
}

/// Parses the `a11y` probe's JSON.
pub(crate) fn parse_a11y(json: &str) -> Result<A11yFull, String> {
    let out: RunnerOut = serde_json::from_str(json).map_err(|e| e.to_string())?;
    let results = out
        .results
        .into_iter()
        .map(|r| OwnedA11yResult {
            kind: if r.kind == "violation" {
                "violation"
            } else {
                "needs_review"
            },
            wcag: r
                .tags
                .iter()
                .filter(|t| t.starts_with("wcag") || t.as_str() == "best-practice")
                .map(String::as_str)
                .collect::<Vec<_>>()
                .join(" "),
            id: r.id,
            impact: r.impact,
            total: r.total,
            nodes: r
                .nodes
                .into_iter()
                .map(|n| OwnedNode {
                    selector: n.selector,
                    excerpt: n.excerpt,
                    data: n.data.and_then(|d| serde_json::to_string(&d).ok()),
                    bbox: n.bbox,
                })
                .collect(),
        })
        .collect();
    Ok(A11yFull {
        ms: out.ms.round() as i64,
        results,
    })
}

#[derive(Deserialize)]
struct MobileOut {
    window_px: f64,
    content_px: f64,
    #[serde(default)]
    widest: Option<String>,
    #[serde(default)]
    small_text_share: f64,
    #[serde(default)]
    small_targets: i64,
    #[serde(default)]
    examples: Vec<MobileExample>,
}

#[derive(Deserialize, Serialize)]
struct MobileExample {
    selector: String,
    w: f64,
    h: f64,
}

/// A parsed `mobile_layout` probe result.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct MobileLayout {
    pub window_px: f64,
    pub content_px: f64,
    pub widest: Option<String>,
    pub small_text_share: f64,
    pub small_targets: i64,
    /// A JSON array of `{selector, w, h}`.
    pub examples: String,
}

/// Parses the `mobile_layout` probe's JSON.
pub(crate) fn parse_mobile(json: &str) -> Result<MobileLayout, String> {
    let out: MobileOut = serde_json::from_str(json).map_err(|e| e.to_string())?;
    Ok(MobileLayout {
        window_px: out.window_px,
        content_px: out.content_px,
        widest: out.widest,
        small_text_share: out.small_text_share,
        small_targets: out.small_targets,
        examples: serde_json::to_string(&out.examples).unwrap_or_else(|_| "[]".to_string()),
    })
}

/// What the `a11y` probe came to for one page.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum FullOutcome {
    Checked(A11yFull),
    TimedOut {
        ms: i64,
    },
    /// `too_large`, `probe_failed` or `bot_challenge`.
    NotChecked(&'static str),
}

/// What the `mobile_layout` probe came to for one page.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum MobileOutcome {
    Measured(MobileLayout),
    TimedOut,
    Failed,
}

/// The parsed probe results of one page.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct ProbeReport {
    pub full: Option<FullOutcome>,
    pub mobile: Option<MobileOutcome>,
}

/// Parses a job's probe outcomes. A page flagged as a bot challenge keeps no
/// probe data (FR-014).
pub(crate) fn digest(probes: Vec<(String, ProbeOutcome)>, bot_challenge: bool) -> ProbeReport {
    let mut report = ProbeReport::default();
    for (name, outcome) in probes {
        match name.as_str() {
            PROBE_A11Y => {
                report.full = Some(if bot_challenge {
                    FullOutcome::NotChecked("bot_challenge")
                } else {
                    match outcome {
                        ProbeOutcome::Ok(json) => match parse_a11y(&json) {
                            Ok(a) => FullOutcome::Checked(a),
                            Err(_) => FullOutcome::NotChecked("probe_failed"),
                        },
                        ProbeOutcome::TimedOut { spent } => FullOutcome::TimedOut {
                            ms: spent.as_millis() as i64,
                        },
                        ProbeOutcome::TooLarge => FullOutcome::NotChecked("too_large"),
                        ProbeOutcome::Failed(_) => FullOutcome::NotChecked("probe_failed"),
                    }
                });
            }
            PROBE_MOBILE if !bot_challenge => {
                report.mobile = Some(match outcome {
                    ProbeOutcome::Ok(json) => {
                        parse_mobile(&json).map_or(MobileOutcome::Failed, MobileOutcome::Measured)
                    }
                    ProbeOutcome::TimedOut { .. } => MobileOutcome::TimedOut,
                    ProbeOutcome::TooLarge | ProbeOutcome::Failed(_) => MobileOutcome::Failed,
                });
            }
            _ => {}
        }
    }
    report
}

#[cfg(test)]
mod tests {
    use super::*;

    const A11Y_JSON: &str = r##"{
      "engine": "4.14.0",
      "ms": 412.4,
      "results": [
        {
          "id": "color-contrast", "kind": "violation", "impact": "serious",
          "tags": ["cat.color", "wcag2aa", "wcag143"], "total": 3,
          "nodes": [
            { "selector": "main > p.note", "excerpt": "<p class=\"note\">",
              "data": { "fg": "#999999", "bg": "#ffffff", "ratio": 2.85, "expected": 4.5 },
              "box": [16, 820, 380, 40] },
            { "selector": "main > p.other", "excerpt": "<p>" }
          ]
        },
        { "id": "region", "kind": "needs_review", "impact": null, "tags": ["best-practice"], "total": 1,
          "nodes": [] }
      ]
    }"##;

    const MOBILE_JSON: &str = r#"{ "window_px": 411, "content_px": 1200, "widest": "table#prices",
      "small_text_share": 62.5, "small_targets": 14,
      "examples": [ { "selector": "nav a:nth-child(3)", "w": 16, "h": 16 } ] }"#;

    fn settings(level: A11yLevel) -> AccessibilitySettings {
        AccessibilitySettings {
            level,
            ..AccessibilitySettings::default()
        }
    }

    #[test]
    fn the_axe_json_becomes_results_with_tags_nodes_and_boxes() {
        let a = parse_a11y(A11Y_JSON).unwrap();
        assert_eq!(a.ms, 412);
        assert_eq!(a.results.len(), 2);
        let cc = &a.results[0];
        assert_eq!(cc.id, "color-contrast");
        assert_eq!(cc.kind, "violation");
        assert_eq!(cc.impact.as_deref(), Some("serious"));
        // Only the WCAG tags and best practice are kept.
        assert_eq!(cc.wcag, "wcag2aa wcag143");
        assert_eq!(cc.total, 3);
        assert_eq!(cc.nodes.len(), 2);
        assert_eq!(cc.nodes[0].selector, "main > p.note");
        assert_eq!(cc.nodes[0].excerpt, "<p class=\"note\">");
        assert_eq!(cc.nodes[0].bbox, Some([16.0, 820.0, 380.0, 40.0]));
        let data: serde_json::Value = serde_json::from_str(cc.nodes[0].data.as_deref().unwrap()).unwrap();
        assert_eq!(data["ratio"], 2.85);
        assert_eq!(cc.nodes[1].data, None);
        assert_eq!(cc.nodes[1].bbox, None);

        let region = &a.results[1];
        assert_eq!(region.kind, "needs_review");
        assert_eq!(region.impact, None);
        assert_eq!(region.wcag, "best-practice");
    }

    #[test]
    fn a_result_converts_to_the_row_the_store_writes() {
        let a = parse_a11y(A11Y_JSON).unwrap();
        let row = a.results[0].as_row();
        assert_eq!(row.check_id, "color-contrast");
        assert_eq!((row.kind, row.source, row.impact), ("violation", "full", Some("serious")));
        assert_eq!(row.nodes_total, 3);
        assert_eq!(row.nodes.len(), 2);
        assert_eq!(row.nodes[0].bbox, Some([16.0, 820.0, 380.0, 40.0]));
    }

    #[test]
    fn an_unknown_kind_is_a_review_and_missing_fields_default() {
        let a = parse_a11y(r#"{"results":[{"id":"x","kind":"incomplete"}]}"#).unwrap();
        assert_eq!(a.ms, 0);
        assert_eq!(a.results[0].kind, "needs_review");
        assert_eq!(a.results[0].total, 0);
        assert!(a.results[0].nodes.is_empty());
        assert!(parse_a11y("not json").is_err());
        assert!(parse_a11y(r#"{"results":[{"kind":"violation"}]}"#).is_err());
    }

    #[test]
    fn the_mobile_json_is_parsed_with_its_examples_kept_as_json() {
        let m = parse_mobile(MOBILE_JSON).unwrap();
        assert_eq!((m.window_px, m.content_px), (411.0, 1200.0));
        assert_eq!(m.widest.as_deref(), Some("table#prices"));
        assert_eq!((m.small_text_share, m.small_targets), (62.5, 14));
        let examples: Vec<serde_json::Value> = serde_json::from_str(&m.examples).unwrap();
        assert_eq!(examples.len(), 1);
        assert_eq!(examples[0]["selector"], "nav a:nth-child(3)");
        assert_eq!(examples[0]["w"], 16.0);

        let bare = parse_mobile(r#"{"window_px":411,"content_px":411}"#).unwrap();
        assert_eq!((bare.widest, bare.small_targets, bare.examples.as_str()), (None, 0, "[]"));
        assert!(parse_mobile(r#"{"content_px":1}"#).is_err());
    }

    #[test]
    fn off_gives_no_probes() {
        for mobile_window in [false, true] {
            let (probes, _) = probes_for(&settings(A11yLevel::Off), mobile_window);
            assert!(probes.is_empty(), "mobile window: {mobile_window}");
        }
    }

    #[test]
    fn static_gives_only_the_mobile_probe_and_only_in_a_mobile_window() {
        let (probes, _) = probes_for(&settings(A11yLevel::Static), false);
        assert!(probes.is_empty());
        let (probes, _) = probes_for(&settings(A11yLevel::Static), true);
        let names: Vec<&str> = probes.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, [PROBE_MOBILE]);
        assert!(probes[0].args_json.contains("\"small_text_px\":12"));
        assert!(probes[0].args_json.contains("\"tap_target_px\":24"));

        let mut no_mobile = settings(A11yLevel::Static);
        no_mobile.mobile = false;
        assert!(probes_for(&no_mobile, true).0.is_empty());
    }

    #[test]
    fn full_gives_the_a11y_probe_and_the_mobile_probe_in_a_mobile_window() {
        let s = settings(A11yLevel::Full);
        let (probes, total) = probes_for(&s, false);
        let names: Vec<&str> = probes.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, [PROBE_A11Y]);
        assert_eq!(total, Duration::from_secs(5));
        assert_eq!(probes[0].budget, Duration::from_secs(5));
        assert!(probes[0].args_json.contains("\"node_cap\":20"));

        let (probes, _) = probes_for(&s, true);
        let names: Vec<&str> = probes.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, [PROBE_MOBILE, PROBE_A11Y]);
        assert!(probes[0].budget <= Duration::from_secs(1));

        let longer = AccessibilitySettings {
            budget_secs: 12,
            ..s
        };
        let (probes, total) = probes_for(&longer, false);
        assert_eq!(total, Duration::from_secs(12));
        assert_eq!(probes[0].budget, Duration::from_secs(12));
    }

    #[test]
    fn digest_turns_outcomes_into_a_report_and_drops_data_on_a_bot_challenge() {
        let both = || {
            vec![
                (PROBE_A11Y.to_string(), ProbeOutcome::Ok(A11Y_JSON.to_string())),
                (PROBE_MOBILE.to_string(), ProbeOutcome::Ok(MOBILE_JSON.to_string())),
            ]
        };
        let report = digest(both(), false);
        assert!(matches!(report.full, Some(FullOutcome::Checked(ref a)) if a.results.len() == 2));
        assert!(matches!(report.mobile, Some(MobileOutcome::Measured(ref m)) if m.small_targets == 14));

        let report = digest(
            vec![
                (
                    PROBE_A11Y.to_string(),
                    ProbeOutcome::TimedOut {
                        spent: Duration::from_millis(5000),
                    },
                ),
                (PROBE_MOBILE.to_string(), ProbeOutcome::Failed("boom".into())),
            ],
            false,
        );
        assert_eq!(report.full, Some(FullOutcome::TimedOut { ms: 5000 }));
        assert_eq!(report.mobile, Some(MobileOutcome::Failed));

        let report = digest(vec![(PROBE_A11Y.to_string(), ProbeOutcome::TooLarge)], false);
        assert_eq!(report.full, Some(FullOutcome::NotChecked("too_large")));
        let report = digest(vec![(PROBE_A11Y.to_string(), ProbeOutcome::Ok("{".into()))], false);
        assert_eq!(report.full, Some(FullOutcome::NotChecked("probe_failed")));

        let report = digest(both(), true);
        assert_eq!(report.full, Some(FullOutcome::NotChecked("bot_challenge")));
        assert_eq!(report.mobile, None);
    }
}
