//! Spec 008 FR-021, FR-028, FR-029: speed events in the crawl log and the
//! summary, and the limit and per-site pace in progress events.

mod support;

use std::time::Duration;

use spiderlens_config::{CrawlConfig, Mode};
use spiderlens_engine::crawl::pace::{LimitKind, SiteState};
use spiderlens_engine::crawl::{Control, CrawlEvent, CrawlStore, Crawler, Progress};
use spiderlens_test_fixtures::SiteBuilder;
use support::*;
use tokio::sync::{mpsc, watch};

fn pages(n: usize) -> SiteBuilder {
    (0..n).fold(SiteBuilder::new(), |b, i| b.page(E, &format!("/{i}"), "", &[]))
}

#[tokio::test]
async fn fr_021_waits_and_cuts_are_logged_and_summarized() {
    let site = pages(5).retry_after(E, "/limited", 429, "1").start();
    let mut urls = vec![site.url(E, "/limited")];
    urls.extend((0..5).map(|i| site.url(E, &format!("/{i}"))));
    let d = crawl(&site, list(urls), |c| c.retry.attempts = 1).await;

    let log = CrawlStore::open(&d.path).unwrap().log_entries(100).unwrap();
    let kinds: Vec<&str> = log.iter().map(|e| e.kind.as_str()).collect();
    let cut = log.iter().find(|e| e.kind == "speed_cut").expect("cut logged");
    assert!(
        cut.message.contains("answered 429 (overloaded)"),
        "{}",
        cut.message
    );
    let wait = log.iter().find(|e| e.kind == "speed_wait").expect("wait logged");
    assert!(
        wait.message.contains("asked to wait 1") && wait.message.contains("(Retry-After)"),
        "{}",
        wait.message
    );
    assert_eq!(
        kinds.iter().filter(|k| **k == "speed_summary").count(),
        1,
        "{kinds:?}"
    );

    let [note] = d.summary.speed_events.as_slice() else {
        panic!("{:?}", d.summary.speed_events)
    };
    assert_eq!(note.host, format!("http://{E}:{}", site.port()));
    assert!(
        note.summary.contains(
            "cut back once after 429 or 503 answers; asked to wait once (1 s in total, longest 1 s)"
        ),
        "{}",
        note.summary
    );
}

#[tokio::test]
async fn a_quiet_crawl_has_no_speed_events() {
    let site = pages(5).start();
    let urls = (0..5).map(|i| site.url(E, &format!("/{i}"))).collect();
    let d = crawl(&site, list(urls), |_| {}).await;
    assert!(d.summary.speed_events.is_empty(), "{:?}", d.summary.speed_events);
}

/// Runs a crawl and keeps every progress event.
async fn progress_of(
    site: &spiderlens_test_fixtures::Site,
    urls: Vec<String>,
    edit: impl FnOnce(&mut CrawlConfig),
) -> Vec<Progress> {
    let dir = tempfile::tempdir().unwrap();
    let mut c = CrawlConfig {
        mode: Mode::List,
        ..CrawlConfig::default()
    };
    quick(&mut c);
    edit(&mut c);
    let path = dir.path().join("t.spiderlens");
    let crawler = Crawler::create(&path, c, &list(urls))
        .unwrap()
        .with_resolve(site.resolve());
    let (_tx, rx) = watch::channel(Control::Run);
    let (etx, mut erx) = mpsc::unbounded_channel();
    let collect = tokio::spawn(async move {
        let mut out = Vec::new();
        while let Some(e) = erx.recv().await {
            if let CrawlEvent::Progress(p) = e {
                out.push(p);
            }
        }
        out
    });
    tokio::time::timeout(Duration::from_secs(60), crawler.run(rx, Some(etx)))
        .await
        .expect("crawl finishes")
        .unwrap();
    collect.await.unwrap()
}

#[tokio::test]
async fn fr_028_crawl_delay_is_named_with_its_site() {
    // robots.txt asks for 1 s between requests: the crawl says so.
    let site = pages(4).robots(E, "User-agent: *\nCrawl-delay: 1\n").start();
    let urls = (0..4).map(|i| site.url(E, &format!("/{i}"))).collect();
    let events = progress_of(&site, urls, |_| {}).await;
    let host = format!("http://{E}:{}", site.port());
    let limited = events
        .iter()
        .find_map(|p| p.limit.clone().filter(|l| l.kind == LimitKind::CrawlDelay))
        .expect("crawl-delay named");
    assert_eq!(limited.host.as_deref(), Some(host.as_str()));
    assert!(
        limited
            .message
            .contains("robots.txt asks for 1 s between requests"),
        "{}",
        limited.message
    );

    // FR-029: the site's row while it has work.
    let row = events
        .iter()
        .flat_map(|p| p.sites.iter())
        .find(|s| s.host == host && s.waiting > 0 && s.reason.contains("between requests"))
        .expect("site listed");
    assert_eq!(row.state, SiteState::Steady);
    assert_eq!(row.allowance, 1);
    // Every site with work is counted, and the last event has none left.
    assert!(events.iter().any(|p| p.sites_total == 1));
    let last = events.last().unwrap();
    assert!(last.sites.is_empty() && last.limit.is_none(), "{last:?}");
}

#[tokio::test]
async fn fr_028_a_users_cap_is_named_before_the_sites() {
    let site = (0..40)
        .fold(SiteBuilder::new(), |b, i| b.page(E, &format!("/{i}"), "", &[]))
        .start();
    let urls = (0..40).map(|i| site.url(E, &format!("/{i}"))).collect();
    let events = progress_of(&site, urls, |c| {
        c.speed.max_urls_per_sec = Some(10.0);
    })
    .await;
    assert!(
        events
            .iter()
            .any(|p| p.limit.as_ref().is_some_and(|l| l.kind == LimitKind::Rate)),
        "{:?}",
        events
            .iter()
            .map(|p| p.limit.as_ref().map(|l| l.kind))
            .collect::<Vec<_>>()
    );
}
