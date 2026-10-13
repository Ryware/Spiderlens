//! Shared helpers for the spec 004 crawl tests.
#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::Value;
use spiderlens_config::{CrawlConfig, ListInput, Mode, ModeInput, SpeedSettings};
use spiderlens_engine::crawl::{Control, CrawlEvent, CrawlStore, CrawlSummary, Crawler, RunCommand};
use spiderlens_test_fixtures::Site;
use tokio::sync::{mpsc, watch};

pub const E: &str = "example.test";

/// Fast crawl and short waits, so retry timing can be measured in tests.
pub fn quick(c: &mut CrawlConfig) {
    c.speed = SpeedSettings::FAST;
    c.retry.base_wait_secs = 0.1;
    c.retry.max_wait_secs = 2.0;
    c.network.read_timeout_secs = 1.0;
    c.network.connect_timeout_secs = 1.0;
}

/// For requests to a closed port: Windows retries a refused connection for
/// about two seconds before reporting it, which a 1 s limit would turn into
/// a timeout.
pub fn allow_slow_refusal(c: &mut CrawlConfig) {
    c.network.read_timeout_secs = 10.0;
    c.network.connect_timeout_secs = 10.0;
}

pub struct Done {
    pub dir: tempfile::TempDir,
    pub path: PathBuf,
    pub summary: CrawlSummary,
    pub notices: Vec<String>,
}

impl Done {
    /// Everything stored for one URL (`spiderlens page --json`).
    pub fn page(&self, url: &str) -> Value {
        CrawlStore::open(&self.path)
            .unwrap()
            .page_report(url)
            .unwrap()
            .unwrap_or_else(|| panic!("{url} not recorded"))
    }

    /// US1.8 / SC-008: every discovered URL has exactly one final state.
    pub fn assert_one_final_state(&self) {
        let s = &self.summary;
        assert_eq!(s.queued, 0, "nothing left queued or waiting: {s:?}");
        assert_eq!(s.crawled + s.not_crawled, s.discovered, "{s:?}");
    }
}

pub fn spider(site: &Site, path: &str) -> ModeInput {
    ModeInput::Spider {
        start_url: site.url(E, path),
    }
}

pub fn list(urls: Vec<String>) -> ModeInput {
    ModeInput::List(ListInput {
        urls,
        ..ListInput::default()
    })
}

pub async fn crawl_in(
    dir: tempfile::TempDir,
    site: &Site,
    input: ModeInput,
    edit: impl FnOnce(&mut CrawlConfig),
) -> Done {
    let mut c = CrawlConfig {
        mode: match input {
            ModeInput::Spider { .. } => Mode::Spider,
            ModeInput::List(_) => Mode::List,
        },
        ..CrawlConfig::default()
    };
    quick(&mut c);
    edit(&mut c);
    let path = dir.path().join("t.spiderlens");
    let crawler = Crawler::create(&path, c, &input)
        .unwrap()
        .with_resolve(site.resolve())
        .with_retry_base(Duration::from_millis(10));
    run(dir, path, crawler, Control::Run, None).await
}

pub async fn crawl(site: &Site, input: ModeInput, edit: impl FnOnce(&mut CrawlConfig)) -> Done {
    crawl_in(tempfile::tempdir().unwrap(), site, input, edit).await
}

/// Runs a prepared crawler, collecting notices. `commands` lets a test send
/// run commands while it runs.
pub async fn run(
    dir: tempfile::TempDir,
    path: PathBuf,
    crawler: Crawler,
    _initial: Control,
    commands: Option<mpsc::UnboundedReceiver<RunCommand>>,
) -> Done {
    let (_tx, rx) = watch::channel(Control::Run);
    let (etx, mut erx) = mpsc::unbounded_channel();
    let collect = tokio::spawn(async move {
        let mut notices = Vec::new();
        while let Some(e) = erx.recv().await {
            if let CrawlEvent::Notice(n) = e {
                notices.push(n.message);
            }
        }
        notices
    });
    let summary = tokio::time::timeout(
        Duration::from_secs(120),
        crawler.run_with(rx, commands, Some(etx)),
    )
    .await
    .expect("crawl finishes")
    .unwrap();
    let notices = collect.await.unwrap();
    Done {
        dir,
        path,
        summary,
        notices,
    }
}

pub fn reopen(path: &Path, site: &Site) -> Crawler {
    Crawler::open(path).unwrap().with_resolve(site.resolve())
}
