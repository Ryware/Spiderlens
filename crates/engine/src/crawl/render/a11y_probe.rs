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
pub(crate) fn probes_for(settings: &AccessibilitySettings, mobile_window: bool) -> (Vec<PageProbe>, Duration) {
    let total = Duration::from_secs(u64::from(settings.budget_secs.max(1)));
    let mut probes = Vec::new();
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
            kind: if r.kind == "violation" { "violation" } else { "needs_review" },
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
    TimedOut { ms: i64 },
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
