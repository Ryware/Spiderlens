//! The known AI crawlers (spec 015 FR-006 to FR-008, contracts/ai-bots.md).
//!
//! One list for the whole app: the AI checks (015), issue rules (009) and
//! agent access (016) all read it from here. The list is data in
//! `ai_bots.json`, embedded at build time, with its own version and date so
//! it can be updated without touching code.

use std::sync::LazyLock;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// What a crawler's visits are for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum BotPurpose {
    /// Collects content to train models.
    Training,
    /// Builds the index an AI search or answer engine cites from.
    Search,
    /// Fetches a page because a person asked an assistant to.
    UserFetch,
    Other,
}

/// Whether the vendor documents that the crawler follows robots.txt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum Honours {
    Yes,
    No,
    /// The vendor says robots.txt rules may not apply (user-requested fetches).
    MayNotApply,
    Unknown,
}

/// Whether the crawler is documented or observed to run JavaScript.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum RunsJs {
    Yes,
    No,
    Unknown,
}

impl RunsJs {
    /// Crawlers not known to run JavaScript are treated as reading raw HTML.
    pub fn reads_raw_html(self) -> bool {
        self != Self::Yes
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct AiBot {
    /// Stable position within the list; a bit in stored per-page masks.
    /// User-defined entries get indexes from 64 up and are never stored in masks.
    pub index: u8,
    pub vendor: String,
    pub name: String,
    /// robots.txt token, lower case.
    pub token: String,
    /// Tokens whose group applies when the crawler has none of its own.
    #[serde(default)]
    pub fallback_tokens: Vec<String>,
    /// Substring that identifies the crawler's user agent; `None` for
    /// tokens with no crawler of their own.
    #[serde(default)]
    pub ua_pattern: Option<String>,
    /// A published user agent string, used only by the explicit probe.
    #[serde(default)]
    pub ua_example: Option<String>,
    pub purpose: BotPurpose,
    pub honours_robots: Honours,
    pub runs_js: RunsJs,
    /// A robots.txt token only (Google-Extended, Applebot-Extended).
    #[serde(default)]
    pub token_only: bool,
    pub docs_url: String,
    #[serde(default)]
    pub ip_ranges_url: Option<String>,
    /// Kept so its index stays reserved; not evaluated.
    #[serde(default)]
    pub retired: bool,
    /// Added by the user rather than shipped with the app.
    #[serde(default)]
    pub custom: bool,
}

#[derive(Debug, Deserialize)]
struct BotList {
    version: u32,
    date: String,
    bots: Vec<AiBot>,
}

static LIST: LazyLock<BotList> = LazyLock::new(|| {
    serde_json::from_str(include_str!("ai_bots.json")).expect("ai_bots.json is valid (checked by tests)")
});

/// Indexes from here up belong to user-defined entries.
pub const CUSTOM_INDEX_START: u8 = 64;

/// Every built-in crawler, retired ones included, in index order.
pub fn all() -> &'static [AiBot] {
    &LIST.bots
}

/// The built-in crawlers that are evaluated.
pub fn active() -> impl Iterator<Item = &'static AiBot> {
    all().iter().filter(|b| !b.retired)
}

/// The list's version, stored with every AI result.
pub fn version() -> u32 {
    LIST.version
}

/// The date the list was last checked against vendor documentation.
pub fn date() -> &'static str {
    &LIST.date
}

/// Problems with user-defined crawler entries.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CustomBotError {
    #[error("crawler {0:?} needs a robots.txt token")]
    NoToken(String),
    #[error("token {0:?} is not a valid robots.txt token (letters, digits, '-' and '_' only)")]
    BadToken(String),
    #[error("token {0:?} is used twice")]
    Duplicate(String),
}

/// Built-in crawlers followed by the user's own entries. A custom entry whose
/// token is already built in is dropped and returned, so the caller can tell
/// the user (built-in entries win).
pub fn merged(custom: &[AiBot]) -> Result<(Vec<AiBot>, Vec<String>), CustomBotError> {
    let mut out: Vec<AiBot> = active().cloned().collect();
    let mut clashes = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for (i, bot) in custom.iter().enumerate() {
        let token = bot.token.trim().to_ascii_lowercase();
        if token.is_empty() {
            return Err(CustomBotError::NoToken(bot.name.clone()));
        }
        if !token
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            return Err(CustomBotError::BadToken(bot.token.clone()));
        }
        if !seen.insert(token.clone()) {
            return Err(CustomBotError::Duplicate(token));
        }
        if out.iter().any(|b| b.token == token) {
            clashes.push(token);
            continue;
        }
        let index =
            CUSTOM_INDEX_START.saturating_add(u8::try_from(i).unwrap_or(u8::MAX - CUSTOM_INDEX_START));
        out.push(AiBot {
            index,
            token,
            custom: true,
            retired: false,
            ..bot.clone()
        });
    }
    Ok((out, clashes))
}

/// The built-in crawler whose user agent string contains `user_agent`'s
/// pattern (case-insensitive), for log and probe matching.
pub fn by_user_agent(user_agent: &str) -> Option<&'static AiBot> {
    let ua = user_agent.to_ascii_lowercase();
    active().find(|b| {
        b.ua_pattern
            .as_deref()
            .is_some_and(|p| ua.contains(&p.to_ascii_lowercase()))
    })
}

/// The built-in crawler with this robots.txt token.
pub fn by_token(token: &str) -> Option<&'static AiBot> {
    let token = token.to_ascii_lowercase();
    all().iter().find(|b| b.token == token)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_parses_with_version_and_date() {
        assert!(version() >= 1);
        assert!(jiff::civil::Date::strptime("%Y-%m-%d", date()).is_ok());
        assert!(all().len() >= 25);
    }

    #[test]
    fn indexes_are_dense_and_tokens_unique() {
        for (i, b) in all().iter().enumerate() {
            assert_eq!(usize::from(b.index), i, "{} index", b.name);
            assert!(b.index < CUSTOM_INDEX_START);
            assert_eq!(
                b.token,
                b.token.to_ascii_lowercase(),
                "{} token is lower case",
                b.name
            );
            assert_eq!(
                b.token,
                b.name.to_ascii_lowercase(),
                "{} token matches name",
                b.name
            );
            assert!(!b.custom);
        }
        let mut tokens: Vec<_> = all().iter().map(|b| &b.token).collect();
        tokens.sort();
        tokens.dedup();
        assert_eq!(tokens.len(), all().len());
    }

    #[test]
    fn every_entry_is_documented() {
        for b in all() {
            assert!(b.docs_url.starts_with("https://"), "{} docs", b.name);
            assert_eq!(
                b.token_only,
                b.ua_pattern.is_none(),
                "{}: token-only entries have no UA pattern",
                b.name
            );
            if b.token_only {
                assert!(b.ua_example.is_none(), "{}", b.name);
            }
            for f in &b.fallback_tokens {
                assert_eq!(f, &f.to_ascii_lowercase());
            }
        }
    }

    #[test]
    fn covers_the_required_crawlers() {
        // research.md R1: every vendor splits purposes; the spec names these.
        for token in [
            "gptbot",
            "oai-searchbot",
            "chatgpt-user",
            "claudebot",
            "claude-searchbot",
            "claude-user",
            "perplexitybot",
            "perplexity-user",
            "google-extended",
            "googlebot",
            "applebot",
            "applebot-extended",
            "bingbot",
            "meta-externalagent",
            "ccbot",
            "amazonbot",
            "bytespider",
            "duckassistbot",
            "mistralai-user",
        ] {
            assert!(by_token(token).is_some(), "{token} missing");
        }
        assert_eq!(by_token("applebot").unwrap().fallback_tokens, ["googlebot"]);
        assert_eq!(
            by_token("chatgpt-user").unwrap().honours_robots,
            Honours::MayNotApply
        );
        assert!(by_token("google-extended").unwrap().token_only);
    }

    #[test]
    fn finds_crawler_by_user_agent() {
        let ua = "Mozilla/5.0 AppleWebKit/537.36 (KHTML, like Gecko; compatible; GPTBot/1.1; +https://openai.com/gptbot)";
        assert_eq!(by_user_agent(ua).unwrap().name, "GPTBot");
        assert!(by_user_agent("Mozilla/5.0 (Windows NT 10.0) Chrome/141").is_none());
        // Every published example identifies its own entry.
        for b in active() {
            if let Some(ex) = &b.ua_example {
                assert_eq!(by_user_agent(ex).map(|f| &f.name), Some(&b.name), "{}", b.name);
            }
        }
    }

    fn custom(token: &str) -> AiBot {
        AiBot {
            index: 0,
            vendor: "Acme".into(),
            name: "AcmeBot".into(),
            token: token.into(),
            fallback_tokens: vec![],
            ua_pattern: Some("AcmeBot".into()),
            ua_example: None,
            purpose: BotPurpose::Search,
            honours_robots: Honours::Unknown,
            runs_js: RunsJs::Unknown,
            token_only: false,
            docs_url: "https://acme.example/bot".into(),
            ip_ranges_url: None,
            retired: false,
            custom: false,
        }
    }

    #[test]
    fn custom_entries_merge_after_built_in_ones() {
        let (bots, clashes) = merged(&[custom("AcmeBot"), custom("GPTBot")]).unwrap();
        assert_eq!(clashes, ["gptbot"]);
        let acme = bots.iter().find(|b| b.token == "acmebot").unwrap();
        assert!(acme.custom);
        assert_eq!(acme.index, CUSTOM_INDEX_START);
        assert_eq!(bots.iter().filter(|b| b.token == "gptbot").count(), 1);
    }

    #[test]
    fn custom_entries_are_validated() {
        assert_eq!(
            merged(&[custom(" ")]),
            Err(CustomBotError::NoToken("AcmeBot".into()))
        );
        assert_eq!(
            merged(&[custom("acme bot")]),
            Err(CustomBotError::BadToken("acme bot".into()))
        );
        assert_eq!(
            merged(&[custom("x"), custom("X")]),
            Err(CustomBotError::Duplicate("x".into()))
        );
    }
}
