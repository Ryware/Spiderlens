//! The crawler: frontier, scope and the crawl loop (spec 003).
//!
//! - `scope` decides what is crawled (one function, FR-038 order).
//! - `store` keeps the frontier on disk in the crawl file.
//! - `runner` schedules requests breadth-first and politely.
//! - `speed` adapts how many requests each site gets at once (spec 008).

pub mod file;
mod health;
pub mod migrate;
pub mod reason;
pub mod retry;
mod runner;
pub mod scope;
mod sitemap;
mod speed;
pub mod store;
mod window;

pub use file::{Access, CrawlInfo, LocalError, LocalErrorKind, LockNote, WriterLock};
pub use reason::{DiscoverySource, LinkKind, NotCrawledReason, UrlState};
pub use runner::{
    Control, CrawlError, CrawlEvent, CrawlSummary, Crawler, HostSummary, Notice, Opened, PausedHost,
    Progress, ResumeOptions, RunCommand, SitemapSummary, Upgrade, default_file_name, host_key, retry_preview,
};
pub use scope::{ScopeRules, ScopeTest, Verdict, test_url};
pub use store::{CrawlStore, LogEntry, UrlRow};

/// The scope tester (spec 003 FR-016): what the crawler would do with each
/// of `inputs` if the start page linked to it. With `check_robots`, each
/// host's robots.txt is fetched once (one request per host, nothing else);
/// a host whose robots.txt cannot be read counts as disallowing everything.
/// Shared by the CLI (`scope test`) and the desktop app (`scope_test`).
pub async fn scope_test(
    config: &spiderlens_config::CrawlConfig,
    start: &url::Url,
    inputs: &[String],
    check_robots: bool,
) -> Result<Vec<ScopeTest>, String> {
    let rules = ScopeRules::new(config, std::slice::from_ref(start))?;
    let mut policies = std::collections::HashMap::new();
    let mut out = Vec::with_capacity(inputs.len());
    for input in inputs {
        let robots = config.robots.mode != spiderlens_config::RobotsMode::Ignore;
        let policy = match rules.normalize(start, input) {
            Ok(url) if check_robots && robots => {
                let key = host_key(&url);
                if !policies.contains_key(&key) {
                    let p = fetch_robots(config, &url)
                        .await
                        .unwrap_or_else(|_| crate::robots::RobotsPolicy::disallow_all());
                    policies.insert(key.clone(), p);
                }
                policies.get(&key)
            }
            _ => None,
        };
        out.push(test_url(&rules, start, input, policy, config.robots.mode));
    }
    Ok(out)
}

/// Fetches a host's robots.txt once, for the scope tester (FR-016).
pub async fn fetch_robots(
    config: &spiderlens_config::CrawlConfig,
    url: &url::Url,
) -> Result<crate::robots::RobotsPolicy, crate::CheckError> {
    let opts = crate::CheckOptions::from(config);
    let client = crate::fetch::build_client(&opts)?;
    Ok(crate::robots::fetch(&client, url, &opts.headers)
        .await
        .policy(&opts.robots_token))
}
