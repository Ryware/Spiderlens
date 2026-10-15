//! Link analysis settings (spec 023 plan D9): whether the analysis runs on
//! its own when a crawl ends, custom link position rules, and optional
//! weighting of links by position.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export)]
pub struct LinkAnalysisSettings {
    /// Run the analysis when a crawl finishes (FR-012).
    pub auto_analyse: bool,
    /// Checked in order before the built-in landmark rules; the first
    /// matching rule wins (FR-019).
    pub position_rules: Vec<PositionRule>,
    /// None: every followed link counts the same (the default, FR-020).
    pub weighting: Option<PositionWeights>,
}

impl Default for LinkAnalysisSettings {
    fn default() -> Self {
        Self {
            auto_analyse: true,
            position_rules: Vec::new(),
            weighting: None,
        }
    }
}

/// A CSS selector matched against a link and its ancestors.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PositionRule {
    pub selector: String,
    pub position: Position,
}

/// Where on the page a link sits. Stored as its number (data-model.md).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum Position {
    Navigation,
    Header,
    Content,
    Sidebar,
    Footer,
}

impl Position {
    pub const ALL: [Position; 5] = [
        Self::Navigation,
        Self::Header,
        Self::Content,
        Self::Sidebar,
        Self::Footer,
    ];

    /// The stored number; 0 is kept for "unknown".
    pub fn code(self) -> u8 {
        match self {
            Self::Navigation => 1,
            Self::Header => 2,
            Self::Content => 3,
            Self::Sidebar => 4,
            Self::Footer => 5,
        }
    }

    pub fn from_code(code: u8) -> Option<Self> {
        Self::ALL.into_iter().find(|p| p.code() == code)
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Navigation => "navigation",
            Self::Header => "header",
            Self::Content => "content",
            Self::Sidebar => "sidebar",
            Self::Footer => "footer",
        }
    }
}

/// How much of a page's value a link passes, by position, 0 to 1.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export)]
pub struct PositionWeights {
    pub navigation: f32,
    pub header: f32,
    pub content: f32,
    pub sidebar: f32,
    pub footer: f32,
    /// Links on pages whose HTML was not stored.
    pub unknown: f32,
}

impl Default for PositionWeights {
    fn default() -> Self {
        Self {
            navigation: 0.3,
            header: 0.3,
            content: 1.0,
            sidebar: 0.5,
            footer: 0.2,
            unknown: 1.0,
        }
    }
}

impl PositionWeights {
    /// The weight for a stored position number (0 = unknown).
    pub fn for_code(&self, code: u8) -> f32 {
        match Position::from_code(code) {
            Some(Position::Navigation) => self.navigation,
            Some(Position::Header) => self.header,
            Some(Position::Content) => self.content,
            Some(Position::Sidebar) => self.sidebar,
            Some(Position::Footer) => self.footer,
            None => self.unknown,
        }
    }

    pub(crate) fn fields(&self) -> [(&'static str, f32); 6] {
        [
            ("navigation", self.navigation),
            ("header", self.header),
            ("content", self.content),
            ("sidebar", self.sidebar),
            ("footer", self.footer),
            ("unknown", self.unknown),
        ]
    }
}

/// Checks a position rule's selector. The message is the parser's, made
/// readable (FR-022).
pub fn check_selector(selector: &str) -> Result<(), String> {
    if selector.trim().is_empty() {
        return Err("Enter a CSS selector, such as .mega-menu or #related.".into());
    }
    scraper::Selector::parse(selector)
        .map(|_| ())
        .map_err(|e| format!("Not a valid CSS selector: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn defaults_match_spec() {
        let s = LinkAnalysisSettings::default();
        assert!(s.auto_analyse);
        assert!(s.position_rules.is_empty());
        assert_eq!(s.weighting, None);
        let w = PositionWeights::default();
        assert_eq!(
            (w.navigation, w.footer, w.content, w.unknown),
            (0.3, 0.2, 1.0, 1.0)
        );
    }

    #[test]
    fn codes_round_trip() {
        for p in Position::ALL {
            assert_eq!(Position::from_code(p.code()), Some(p));
        }
        assert_eq!(Position::from_code(0), None);
        assert_eq!(PositionWeights::default().for_code(0), 1.0);
        assert_eq!(PositionWeights::default().for_code(5), 0.2);
    }

    #[test]
    fn serde_shape() {
        let s: LinkAnalysisSettings = serde_json::from_value(json!({
            "position_rules": [{"selector": ".mega", "position": "navigation"}],
            "weighting": {"footer": 0.1}
        }))
        .unwrap();
        assert!(s.auto_analyse);
        assert_eq!(s.position_rules[0].position, Position::Navigation);
        let w = s.weighting.unwrap();
        assert_eq!((w.footer, w.navigation), (0.1, 0.3));
    }

    #[test]
    fn selectors_are_checked() {
        assert!(check_selector(".mega-menu a").is_ok());
        assert!(check_selector("div[role=navigation]").is_ok());
        assert!(check_selector("").is_err());
        let e = check_selector("div[").unwrap_err();
        assert!(e.starts_with("Not a valid CSS selector"), "{e}");
    }
}
