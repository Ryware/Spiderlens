//! What each side is (FR-003): start URL, date, size, completeness, versions,
//! and the configuration differences that change what is collected (D10).

use rusqlite::{Connection, OptionalExtension};
use serde::Serialize;
use serde_json::Value;
use ts_rs::TS;

use crate::Result;

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct CrawlInfo {
    pub path: String,
    pub start_url: Option<String>,
    /// When the crawl started (RFC 3339), if recorded.
    pub started_at: Option<String>,
    pub app_version: Option<String>,
    pub format_version: Option<u32>,
    /// `completed`, `stopped`, `paused`, `running` or another stored status.
    pub status: String,
    /// Finished on its own, without a stop or a URL limit.
    pub complete: bool,
    pub url_limit_reached: bool,
    /// The file was still being crawled when the comparison read it; the
    /// comparison uses a snapshot.
    pub snapshot: bool,
    #[ts(type = "number")]
    pub urls: u64,
    #[ts(type = "number")]
    pub crawled: u64,
    #[serde(skip)]
    #[ts(skip)]
    pub(crate) config: Option<Value>,
}

impl CrawlInfo {
    pub(crate) fn read(conn: &Connection, path: &std::path::Path) -> Result<Self> {
        let meta = |key: &str| -> Result<Option<String>> {
            Ok(conn
                .query_row("SELECT value FROM meta WHERE key = ?1", [key], |r| r.get(0))
                .optional()?)
        };
        let stored = meta("config")?.and_then(|c| serde_json::from_str::<Value>(&c).ok());
        let status = meta("status")?.unwrap_or_else(|| "running".into());
        let url_limit_reached = meta("url_limit_reached")?.as_deref() == Some("true");
        let (urls, crawled): (i64, i64) = conn.query_row(
            "SELECT COUNT(*), COALESCE(SUM(state = 'crawled'), 0) FROM urls",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        let text = |v: &Option<Value>, k: &str| {
            v.as_ref()
                .and_then(|v| v.get(k))
                .and_then(Value::as_str)
                .map(str::to_string)
        };
        Ok(Self {
            path: path.display().to_string(),
            start_url: meta("start_url")?,
            started_at: text(&stored, "started_at"),
            app_version: text(&stored, "app_version"),
            format_version: meta("format_version")?.and_then(|v| v.parse().ok()),
            complete: status == "completed" && !url_limit_reached,
            snapshot: status == "running",
            status,
            url_limit_reached,
            urls: urls as u64,
            crawled: crawled as u64,
            config: stored.and_then(|v| v.get("config").cloned()),
        })
    }

    /// `scheme://host[:port]` of the start URL.
    pub fn origin(&self) -> Option<String> {
        let u = url::Url::parse(self.start_url.as_deref()?).ok()?;
        Some(u.origin().ascii_serialization())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct ConfigDiff {
    pub field: String,
    pub label: String,
    pub previous: String,
    pub current: String,
}

/// Settings that change what a crawl collects, as JSON paths in the stored
/// configuration (spec 002), with the label users see.
const FIELDS: &[(&str, &str)] = &[
    ("mode", "Mode"),
    ("limits.max_urls", "Maximum URLs"),
    ("limits.max_depth", "Maximum depth"),
    ("scope.include", "Include patterns"),
    ("scope.exclude", "Exclude patterns"),
    ("scope.subdomains", "Subdomains"),
    ("scope.stay_in_folder", "Stay in start folder"),
    ("scope.check_external", "Check external links"),
    ("scope.follow_internal_nofollow", "Follow internal nofollow"),
    ("scope.follow_external_nofollow", "Follow external nofollow"),
    ("scope.query_params", "Query parameters"),
    ("scope.use_sitemaps", "Use sitemaps"),
    ("identity.user_agent", "User agent"),
    ("robots.mode", "Robots.txt"),
    ("network.max_redirects", "Maximum redirects"),
    ("rendering", "JavaScript rendering"),
];

fn at<'a>(v: &'a Value, path: &str) -> Option<&'a Value> {
    path.split('.').try_fold(v, |v, k| v.get(k))
}

fn show(v: Option<&Value>) -> String {
    match v {
        None | Some(Value::Null) => "not set".into(),
        Some(Value::String(s)) => s.clone(),
        Some(Value::Bool(b)) => if *b { "on" } else { "off" }.into(),
        Some(other) => other.to_string(),
    }
}

pub(crate) fn config_diffs(prev: &CrawlInfo, cur: &CrawlInfo) -> Vec<ConfigDiff> {
    let mut out = Vec::new();
    if prev.start_url != cur.start_url {
        out.push(ConfigDiff {
            field: "start_url".into(),
            label: "Start URL".into(),
            previous: prev.start_url.clone().unwrap_or_default(),
            current: cur.start_url.clone().unwrap_or_default(),
        });
    }
    let (Some(p), Some(c)) = (&prev.config, &cur.config) else {
        return out;
    };
    for (path, label) in FIELDS {
        let (a, b) = (at(p, path), at(c, path));
        if a != b {
            out.push(ConfigDiff {
                field: (*path).into(),
                label: (*label).into(),
                previous: show(a),
                current: show(b),
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn info(start: &str, config: Value) -> CrawlInfo {
        CrawlInfo {
            path: String::new(),
            start_url: Some(start.into()),
            started_at: None,
            app_version: None,
            format_version: None,
            status: "completed".into(),
            complete: true,
            url_limit_reached: false,
            snapshot: false,
            urls: 0,
            crawled: 0,
            config: Some(config),
        }
    }

    #[test]
    fn lists_only_fields_that_differ() {
        let a = info(
            "https://a.test/",
            json!({"mode":"spider","limits":{"max_urls":null},"scope":{"exclude":[]}}),
        );
        let b = info(
            "https://b.test/",
            json!({"mode":"spider","limits":{"max_urls":500},"scope":{"exclude":["/x/"]}}),
        );
        let d = config_diffs(&a, &b);
        let fields: Vec<_> = d.iter().map(|d| d.field.as_str()).collect();
        assert_eq!(fields, ["start_url", "limits.max_urls", "scope.exclude"]);
        assert_eq!(d[1].previous, "not set");
        assert_eq!(d[1].current, "500");
        assert!(config_diffs(&a, &a).is_empty());
    }
}
