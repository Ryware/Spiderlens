//! User agent and speed presets (FR-016, FR-017, FR-021).

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::SpeedSettings;

/// Honest identifier including the app version (principle IV).
pub fn spiderlens_user_agent() -> String {
    format!(
        "SpiderLens/{} (+https://github.com/Ryware/spiderlens)",
        env!("CARGO_PKG_VERSION")
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum UaPreset {
    Spiderlens,
    GooglebotSmartphone,
    GooglebotDesktop,
    Bingbot,
    ChromeDesktop,
    ChromeMobile,
    Custom,
}

// Browser and bot strings use a recent Chrome major version; update them
// with each release.
const CHROME: &str = "Chrome/141.0.0.0";

impl UaPreset {
    pub const ALL: [UaPreset; 7] = [
        Self::Spiderlens,
        Self::GooglebotSmartphone,
        Self::GooglebotDesktop,
        Self::Bingbot,
        Self::ChromeDesktop,
        Self::ChromeMobile,
        Self::Custom,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Spiderlens => "SpiderLens",
            Self::GooglebotSmartphone => "Googlebot Smartphone",
            Self::GooglebotDesktop => "Googlebot Desktop",
            Self::Bingbot => "Bingbot",
            Self::ChromeDesktop => "Chrome (desktop)",
            Self::ChromeMobile => "Chrome (mobile)",
            Self::Custom => "Custom",
        }
    }

    /// User agent string, or `None` for `Custom`.
    pub fn user_agent(self) -> Option<String> {
        Some(match self {
            Self::Spiderlens => spiderlens_user_agent(),
            Self::GooglebotSmartphone => format!(
                "Mozilla/5.0 (Linux; Android 6.0.1; Nexus 5X Build/MMB29P) AppleWebKit/537.36 (KHTML, like Gecko) {CHROME} Mobile Safari/537.36 (compatible; Googlebot/2.1; +http://www.google.com/bot.html)"
            ),
            Self::GooglebotDesktop => format!(
                "Mozilla/5.0 AppleWebKit/537.36 (KHTML, like Gecko; compatible; Googlebot/2.1; +http://www.google.com/bot.html) {CHROME} Safari/537.36"
            ),
            Self::Bingbot => format!(
                "Mozilla/5.0 AppleWebKit/537.36 (KHTML, like Gecko; compatible; bingbot/2.0; +http://www.bing.com/bingbot.htm) {CHROME} Safari/537.36"
            ),
            Self::ChromeDesktop => format!(
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) {CHROME} Safari/537.36"
            ),
            Self::ChromeMobile => format!(
                "Mozilla/5.0 (Linux; Android 10; K) AppleWebKit/537.36 (KHTML, like Gecko) {CHROME} Mobile Safari/537.36"
            ),
            Self::Custom => return None,
        })
    }

    /// Token matched against robots.txt `User-agent:` lines.
    pub fn robots_token(self) -> Option<&'static str> {
        Some(match self {
            Self::Spiderlens => "SpiderLens",
            Self::GooglebotSmartphone | Self::GooglebotDesktop => "Googlebot",
            Self::Bingbot => "bingbot",
            // Browsers have no token of their own; only `*` groups apply.
            Self::ChromeDesktop | Self::ChromeMobile => "*",
            Self::Custom => return None,
        })
    }
}

/// Suggested robots token for a custom user agent: its first product name,
/// for example `MyBot` from `MyBot/1.0 (+https://example.com)`.
pub fn suggest_robots_token(user_agent: &str) -> Option<String> {
    let token: String = user_agent
        .split_whitespace()
        .find(|w| !w.starts_with("Mozilla/") && !w.starts_with('('))?
        .split('/')
        .next()?
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .collect();
    (!token.is_empty()).then_some(token)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum SpeedPreset {
    Polite,
    Fast,
    Custom,
}

impl SpeedSettings {
    /// Polite (default): 8 at once, 4 per site, 5 URLs per second.
    pub const POLITE: SpeedSettings = SpeedSettings {
        max_concurrent: 8,
        max_per_host: 4,
        max_urls_per_sec: Some(5.0),
        adaptive: true,
    };

    /// Fast: 64 at once, 32 per site, no rate cap. For sites the user controls.
    pub const FAST: SpeedSettings = SpeedSettings {
        max_concurrent: 64,
        max_per_host: 32,
        max_urls_per_sec: None,
        adaptive: true,
    };

    /// Derived, never stored on its own, so it cannot disagree with the values.
    pub fn preset(&self) -> SpeedPreset {
        if *self == Self::POLITE {
            SpeedPreset::Polite
        } else if *self == Self::FAST {
            SpeedPreset::Fast
        } else {
            SpeedPreset::Custom
        }
    }

    pub fn from_preset(preset: SpeedPreset) -> Option<Self> {
        match preset {
            SpeedPreset::Polite => Some(Self::POLITE),
            SpeedPreset::Fast => Some(Self::FAST),
            SpeedPreset::Custom => None,
        }
    }

    /// True when any value is faster than Polite (FR-024).
    pub fn faster_than_polite(&self) -> bool {
        let p = Self::POLITE;
        self.max_concurrent > p.max_concurrent
            || self.max_per_host > p.max_per_host
            || match (self.max_urls_per_sec, p.max_urls_per_sec) {
                (None, _) => true,
                (Some(a), Some(b)) => a > b,
                (Some(_), None) => false,
            }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn googlebot_token() {
        assert_eq!(UaPreset::GooglebotSmartphone.robots_token(), Some("Googlebot"));
        assert!(
            UaPreset::GooglebotSmartphone
                .user_agent()
                .unwrap()
                .contains("Googlebot/2.1")
        );
        assert_eq!(UaPreset::Custom.user_agent(), None);
    }

    #[test]
    fn every_preset_except_custom_has_string_and_token() {
        for p in UaPreset::ALL {
            assert_eq!(p.user_agent().is_some(), p != UaPreset::Custom, "{p:?}");
            assert_eq!(p.robots_token().is_some(), p != UaPreset::Custom, "{p:?}");
        }
    }

    #[test]
    fn edited_values_are_custom() {
        assert_eq!(SpeedSettings::POLITE.preset(), SpeedPreset::Polite);
        assert_eq!(SpeedSettings::FAST.preset(), SpeedPreset::Fast);
        let edited = SpeedSettings {
            max_per_host: 3,
            ..SpeedSettings::POLITE
        };
        assert_eq!(edited.preset(), SpeedPreset::Custom);
    }

    #[test]
    fn faster_than_polite() {
        assert!(!SpeedSettings::POLITE.faster_than_polite());
        assert!(SpeedSettings::FAST.faster_than_polite());
        let slower = SpeedSettings {
            max_urls_per_sec: Some(1.0),
            ..SpeedSettings::POLITE
        };
        assert!(!slower.faster_than_polite());
    }

    #[test]
    fn suggests_token_from_custom_agent() {
        assert_eq!(
            suggest_robots_token("MyBot/1.0 (+https://x.test)").as_deref(),
            Some("MyBot")
        );
        assert_eq!(
            suggest_robots_token("Mozilla/5.0 (compatible) AcmeCrawler/2").as_deref(),
            Some("AcmeCrawler")
        );
    }
}
