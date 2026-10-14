//! Tracked elements (spec 012 FR-016..FR-020, plan D5).
//!
//! An element is data, like a 007 tab column: the SQL that reads its value
//! from a crawl file, how two values are compared, and its default
//! threshold. Where a results column exists its expression is reused, so a
//! value in the comparison is the value the results table shows.

use serde::Serialize;
use spiderlens_config::ElementSetting;
use spiderlens_results::{Need, columns as c};
use ts_rs::TS;

use crate::{Error, Result};

/// How two values of an element are compared.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ElementKind {
    /// Text after whitespace normalisation, case-sensitive (FR-017).
    Text,
    /// A sorted set stored as one string; order never matters.
    Set,
    /// A number; changed when the difference reaches a percentage of the
    /// previous value.
    Percent,
    /// A number; changed when the difference reaches an absolute amount.
    Absolute,
    /// A duration; changed when the current value is the threshold times
    /// the previous one or more (slower only).
    Factor,
    /// Any difference.
    Exact,
}

/// Which pairs an element applies to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Applies {
    /// Both sides are successfully fetched internal HTML pages.
    Content,
    /// Both sides are internal URLs with a final state.
    Response,
}

pub(crate) struct Element {
    pub id: &'static str,
    pub label: &'static str,
    pub kind: ElementKind,
    pub expr: &'static str,
    /// A second value compared alongside (structured data block hash).
    pub hash_expr: Option<&'static str>,
    pub need: Need,
    pub applies: Applies,
    pub on: bool,
    pub threshold: Option<f64>,
}

const H1_ALL: &str = "(SELECT group_concat(text, char(31)) FROM (SELECT t.text AS text FROM page_text t \
     WHERE t.url_id = u.id AND t.kind = 'heading' AND t.level = 1 ORDER BY t.text))";
const H2: &str = "(SELECT p.h2_1 FROM pages p WHERE p.url_id = u.id)";
const DIRECTIVES: &str = "(SELECT group_concat(v, char(31)) FROM (SELECT DISTINCT d.directive || COALESCE(':' || d.value, '') AS v \
     FROM directives d WHERE d.url_id = u.id AND (d.crawler = 'all' OR d.crawler = robots_token()) ORDER BY v))";
const HREFLANG: &str = "(SELECT group_concat(v, char(31)) FROM (SELECT DISTINCT COALESCE(l.lang, '') || '=' || COALESCE(t.url, l.target, l.raw) AS v \
     FROM links l LEFT JOIN urls t ON t.id = l.to_id WHERE l.from_id = u.id AND l.kind = 'hreflang' ORDER BY v))";
const SD_TYPES: &str = "(SELECT group_concat(v, char(31)) FROM (SELECT DISTINCT j.value AS v \
     FROM structured_data s, json_each(sd_types(s.kind, s.raw)) j WHERE s.url_id = u.id ORDER BY v))";
const SD_HASH: &str = "(SELECT sd_hash(group_concat(h, ',')) FROM (SELECT sd_hash(s.raw) AS h \
     FROM structured_data s WHERE s.url_id = u.id ORDER BY h))";
const CONTENT: &str = "(SELECT p.text_hash FROM pages p WHERE p.url_id = u.id)";
const INTERNAL_OUTLINKS: &str = "(SELECT COUNT(*) FROM links l JOIN urls t ON t.id = l.to_id \
     WHERE l.from_id = u.id AND l.kind = 'anchor' AND t.internal = 1)";

const fn el(
    id: &'static str,
    label: &'static str,
    kind: ElementKind,
    expr: &'static str,
    need: Need,
    applies: Applies,
) -> Element {
    Element {
        id,
        label,
        kind,
        expr,
        hash_expr: None,
        need,
        applies,
        on: true,
        threshold: None,
    }
}

/// The element list in display order, with defaults (FR-016, FR-018,
/// FR-020).
pub(crate) fn defaults() -> Vec<Element> {
    use Applies::*;
    use ElementKind::*;
    let mut v = vec![
        el("title", "Page title", Text, c::TITLE.expr, Need::Base, Content),
        el(
            "meta_description",
            "Meta description",
            Text,
            c::META_DESCRIPTION.expr,
            Need::Extraction,
            Content,
        ),
        el("h1", "First H1", Text, c::H1.expr, Need::Extraction, Content),
        el("h1_all", "All H1s", Set, H1_ALL, Need::Extraction, Content),
        el("h2", "First H2", Text, H2, Need::Extraction, Content),
        el(
            "word_count",
            "Word count",
            Percent,
            c::WORD_COUNT.expr,
            Need::Extraction,
            Content,
        ),
        el(
            "status_code",
            "Status code",
            Exact,
            c::STATUS_CODE.expr,
            Need::Base,
            Response,
        ),
        el(
            "final_state",
            "Final state",
            Text,
            c::FINAL_STATE.expr,
            Need::Base,
            Response,
        ),
        el(
            "indexability",
            "Indexability",
            Text,
            c::INDEXABILITY.expr,
            Need::Extraction,
            Response,
        ),
        el(
            "canonical",
            "Canonical",
            Text,
            "u.canonical",
            Need::Extraction,
            Content,
        ),
        el(
            "directives",
            "Robots directives",
            Set,
            DIRECTIVES,
            Need::Extraction,
            Content,
        ),
        el(
            "redirect_target",
            "Redirect target",
            Text,
            c::REDIRECT_TARGET.expr,
            Need::Base,
            Response,
        ),
        el("hreflang", "Hreflang", Set, HREFLANG, Need::Extraction, Content),
        Element {
            hash_expr: Some(SD_HASH),
            ..el(
                "structured_data",
                "Structured data",
                Set,
                SD_TYPES,
                Need::Extraction,
                Content,
            )
        },
        el("content", "Content", Exact, CONTENT, Need::Extraction, Content),
        el(
            "depth",
            "Crawl depth",
            Absolute,
            c::DEPTH.expr,
            Need::Base,
            Response,
        ),
        el(
            "inlinks",
            "Unique inlinks",
            Percent,
            c::INLINKS.expr,
            Need::Base,
            Content,
        ),
        el(
            "outlinks",
            "Internal outlinks",
            Percent,
            INTERNAL_OUTLINKS,
            Need::Base,
            Content,
        ),
        Element {
            on: false,
            ..el(
                "response_time",
                "Response time",
                Factor,
                c::RESPONSE_TIME.expr,
                Need::Timing,
                Response,
            )
        },
    ];
    for e in &mut v {
        e.threshold = match e.id {
            "word_count" => Some(10.0),
            "inlinks" | "outlinks" => Some(20.0),
            "depth" => Some(1.0),
            "response_time" => Some(2.0),
            _ => None,
        };
    }
    v
}

/// Defaults with the user's overrides applied; unknown ids are refused.
pub(crate) fn resolve(overrides: &[ElementSetting]) -> Result<Vec<Element>> {
    let mut v = defaults();
    for o in overrides {
        let e = v
            .iter_mut()
            .find(|e| e.id == o.id)
            .ok_or_else(|| Error::invalid(format!("unknown element {:?}", o.id)))?;
        e.on = o.on;
        if let Some(t) = o.threshold {
            if e.threshold.is_none() {
                return Err(Error::invalid(format!("{} has no threshold", e.label)));
            }
            if !(t.is_finite() && t >= 0.0) {
                return Err(Error::invalid(format!(
                    "{}: threshold must be zero or above",
                    e.label
                )));
            }
            e.threshold = Some(t);
        }
    }
    Ok(v)
}

impl Element {
    /// Workspace column holding the value.
    pub fn col(&self) -> String {
        format!("e_{}", self.id)
    }

    pub fn hash_col(&self) -> Option<String> {
        self.hash_expr.map(|_| format!("h_{}", self.id))
    }

    /// SQL over `p` (previous) and `c` (current) side rows that is true when
    /// the element changed. Thresholds are numbers we own, written as
    /// literals.
    pub fn changed_sql(&self) -> String {
        let col = self.col();
        let (p, c) = (format!("p.{col}"), format!("c.{col}"));
        let base = match self.kind {
            ElementKind::Text => {
                format!("COALESCE(cmp_norm({p}), '') IS NOT COALESCE(cmp_norm({c}), '')")
            }
            ElementKind::Set => format!("COALESCE({p}, '') IS NOT COALESCE({c}, '')"),
            ElementKind::Exact => format!("{p} IS NOT {c}"),
            ElementKind::Percent => {
                let t = self.threshold.unwrap_or(0.0);
                format!(
                    "({p} IS NULL) <> ({c} IS NULL) OR ({p} IS NOT NULL AND abs({c} - {p}) > 0 \
                     AND abs({c} - {p}) * 100.0 >= {t:?} * max(abs({p}), 1))"
                )
            }
            ElementKind::Absolute => {
                let t = self.threshold.unwrap_or(0.0);
                format!(
                    "({p} IS NULL) <> ({c} IS NULL) OR ({p} IS NOT NULL AND abs({c} - {p}) > 0 \
                     AND abs({c} - {p}) >= {t:?})"
                )
            }
            ElementKind::Factor => {
                let t = self.threshold.unwrap_or(1.0);
                format!("{p} > 0 AND {c} IS NOT NULL AND {c} >= {p} * {t:?} AND {c} > {p}")
            }
        };
        match self.hash_col() {
            Some(h) => format!("(({base}) OR p.{h} IS NOT c.{h})"),
            None => format!("({base})"),
        }
    }

    pub fn applies_sql(&self) -> &'static str {
        match self.applies {
            Applies::Content => "p.html_ok = 1 AND c.html_ok = 1",
            Applies::Response => "p.internal = 1 AND c.internal = 1 AND p.reach = 0 AND c.reach = 0",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_unique_and_overrides_apply() {
        let d = defaults();
        let mut ids: Vec<_> = d.iter().map(|e| e.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), d.len());
        assert!(
            !d.iter().find(|e| e.id == "response_time").unwrap().on,
            "FR-020 off by default"
        );

        let r = resolve(&[ElementSetting {
            id: "word_count".into(),
            on: true,
            threshold: Some(30.0),
        }])
        .unwrap();
        assert_eq!(
            r.iter().find(|e| e.id == "word_count").unwrap().threshold,
            Some(30.0)
        );
        assert!(
            resolve(&[ElementSetting {
                id: "nope".into(),
                on: true,
                threshold: None
            }])
            .is_err()
        );
        assert!(
            resolve(&[ElementSetting {
                id: "title".into(),
                on: true,
                threshold: Some(1.0)
            }])
            .is_err(),
            "text elements take no threshold"
        );
    }
}
