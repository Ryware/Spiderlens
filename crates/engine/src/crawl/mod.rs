//! The crawler: frontier, scope and the crawl loop (spec 003).
//!
//! - `scope` decides what is crawled (one function, FR-038 order).
//! - `store` keeps the frontier on disk in the crawl file.
//! - `runner` schedules requests breadth-first and politely.

mod links;
pub mod reason;
mod runner;
pub mod scope;
mod sitemap;
pub mod store;
mod window;

pub use reason::{DiscoverySource, LinkKind, NotCrawledReason, UrlState};
pub use runner::{
    Control, CrawlError, CrawlEvent, CrawlSummary, Crawler, HostSummary, Progress, SitemapSummary, host_key,
};
pub use scope::{ScopeRules, ScopeTest, Verdict, test_url};
pub use store::{CrawlStore, UrlRow};

/// Fetches a host's robots.txt once, for the scope tester (FR-016).
pub async fn fetch_robots(
    config: &spiderlens_config::CrawlConfig,
    url: &url::Url,
) -> Result<crate::robots::RobotsPolicy, crate::CheckError> {
    let opts = crate::CheckOptions::from(config);
    let client = crate::fetch::build_client(&opts)?;
    Ok(crate::robots::fetch(&client, url)
        .await
        .policy(&opts.robots_token))
}
