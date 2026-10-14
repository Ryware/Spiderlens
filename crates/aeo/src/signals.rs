//! Machine-readable AI usage and licence signals (spec 015 FR-013, FR-014,
//! research R3). These are preferences: none of them blocks a crawler.

use serde::{Deserialize, Serialize};
use spiderlens_engine::robots::OtherLine;
use ts_rs::TS;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum SignalKind {
    /// `Content-Signal: search=yes, ai-input=no, ai-train=no` (Cloudflare convention).
    ContentSignal,
    /// `Content-Usage: train-ai=n` (IETF AI preferences, Internet-Draft).
    ContentUsage,
    /// `License: <url>` (RSL 1.0).
    License,
    /// Any other line SpiderLens does not act on, typos included.
    Unknown,
}

impl SignalKind {
    pub fn status(self) -> &'static str {
        match self {
            Self::ContentSignal => "Provider convention",
            Self::ContentUsage => "IETF draft standard",
            Self::License => "RSL 1.0",
            Self::Unknown => "Not a recognised robots.txt line",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Signal {
    pub line_no: u32,
    pub kind: SignalKind,
    pub key: String,
    pub value: String,
    /// Parsed `name=value` pairs, in order (Content-Signal and Content-Usage).
    pub preferences: Vec<(String, String)>,
    /// What it asks for, in plain words.
    pub meaning: String,
}

/// Classifies robots.txt lines the matcher does not act on.
pub fn robots_signals(lines: &[OtherLine]) -> Vec<Signal> {
    lines
        .iter()
        .map(|l| {
            let kind = match l.key.to_ascii_lowercase().as_str() {
                "content-signal" => SignalKind::ContentSignal,
                "content-usage" => SignalKind::ContentUsage,
                "license" => SignalKind::License,
                _ => SignalKind::Unknown,
            };
            let preferences = match kind {
                SignalKind::ContentSignal | SignalKind::ContentUsage => preferences(&l.value),
                _ => vec![],
            };
            let meaning = meaning(kind, &l.value, &preferences);
            Signal {
                line_no: l.line_no,
                kind,
                key: l.key.clone(),
                value: l.value.clone(),
                preferences,
                meaning,
            }
        })
        .collect()
}

/// `search=yes, ai-train=no` or `/path train-ai=n` (a leading path is kept
/// as `path`).
fn preferences(value: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for part in value.split([',', ' ']).map(str::trim).filter(|p| !p.is_empty()) {
        match part.split_once('=') {
            Some((k, v)) => out.push((k.trim().to_ascii_lowercase(), v.trim().to_ascii_lowercase())),
            None if part.starts_with('/') => out.push(("path".into(), part.to_string())),
            None => out.push((part.to_ascii_lowercase(), String::new())),
        }
    }
    out
}

fn meaning(kind: SignalKind, value: &str, prefs: &[(String, String)]) -> String {
    let yes_no = |v: &str| match v {
        "yes" | "y" => Some(true),
        "no" | "n" => Some(false),
        _ => None,
    };
    match kind {
        SignalKind::ContentSignal | SignalKind::ContentUsage => {
            let parts: Vec<String> = prefs
                .iter()
                .filter(|(k, _)| k != "path")
                .map(|(k, v)| {
                    let what = match k.as_str() {
                        "search" => "use in search results",
                        "ai-input" => "use as input to AI answers",
                        "ai-train" | "train-ai" => "use to train AI models",
                        "train-genai" => "use to train generative AI",
                        "bulk" => "bulk collection",
                        other => other,
                    };
                    match yes_no(v) {
                        Some(true) => format!("allows {what}"),
                        Some(false) => format!("asks not to {what}"),
                        None => format!("{what}: {v}"),
                    }
                })
                .collect();
            let path = prefs
                .iter()
                .find(|(k, _)| k == "path")
                .map(|(_, p)| format!(" for {p}"))
                .unwrap_or_default();
            if parts.is_empty() {
                "No preference stated".into()
            } else {
                format!(
                    "{}{path}. A preference, not a block.",
                    capitalise(&parts.join(", "))
                )
            }
        }
        SignalKind::License => {
            format!("Points to licence terms at {value} (RSL). A preference, not a block.")
        }
        SignalKind::Unknown => "Ignored by crawlers that follow RFC 9309; check for a typo.".into(),
    }
}

fn capitalise(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().chain(c).collect(),
        None => String::new(),
    }
}

/// Robots meta and header values that limit what AI systems may use or quote
/// (FR-013), with what each vendor documents.
pub fn directive_meaning(directive: &str) -> Option<&'static str> {
    Some(match directive.to_ascii_lowercase().as_str() {
        "nosnippet" => {
            "Google: not used as direct input to AI Overviews and AI Mode. Apple: kept out of AI answers. Bing: may reduce citation quality."
        }
        "max-snippet" => "Google: limits how much text AI Overviews and AI Mode may use.",
        "noarchive" => "Bing: kept out of Copilot answers and grounding. Amazon: not used for training.",
        "nocache" => "Bing: Copilot may show only the URL, title and snippet.",
        "noindex" => "Removed from search indexes, and so from AI search answers that use them.",
        "noai" | "noimageai" => "An informal convention; no major AI vendor documents honouring it.",
        "data-nosnippet" => "Google: these sections are not used in snippets or AI answers.",
        "data-snippet" => "Bing: marks text that may be quoted in answers.",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use spiderlens_engine::robots::RobotsTxt;

    #[test]
    fn classifies_and_explains_robots_lines() {
        // US5.5
        let txt = RobotsTxt::parse(
            b"Content-Signal: search=yes, ai-train=no\nUser-agent: *\nContent-Usage: /docs train-ai=n\nLicense: https://a.test/rsl.xml\nDisalow: /x\n",
        );
        let s = robots_signals(txt.other_lines());
        assert_eq!(
            s.iter().map(|x| (x.line_no, x.kind)).collect::<Vec<_>>(),
            [
                (1, SignalKind::ContentSignal),
                (3, SignalKind::ContentUsage),
                (4, SignalKind::License),
                (5, SignalKind::Unknown)
            ]
        );
        assert_eq!(
            s[0].meaning,
            "Allows use in search results, asks not to use to train AI models. A preference, not a block."
        );
        assert_eq!(
            s[1].meaning,
            "Asks not to use to train AI models for /docs. A preference, not a block."
        );
        assert!(s[2].meaning.contains("https://a.test/rsl.xml"));
        assert!(s[3].meaning.contains("typo"));
    }

    #[test]
    fn directive_meanings() {
        assert!(directive_meaning("NOSNIPPET").unwrap().contains("AI Overviews"));
        assert!(directive_meaning("noai").unwrap().contains("no major AI vendor"));
        assert_eq!(directive_meaning("follow"), None);
    }
}
