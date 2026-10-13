//! Spec 004 US4: a failing host is paused without stopping the crawl, and a
//! dead network pauses the crawl.

mod support;

use std::time::Duration;

use spiderlens_config::{CrawlConfig, Mode};
use spiderlens_engine::crawl::{Control, Crawler, RunCommand};
use spiderlens_test_fixtures::SiteBuilder;
use support::*;
use tokio::sync::mpsc;

const A: &str = "a.test";
const B: &str = "b.test";

fn two_hosts(n: usize) -> spiderlens_test_fixtures::SiteBuilder {
    let mut b = SiteBuilder::new();
    for i in 0..n {
        b = b
            .page(A, &format!("/{i}"), "", &[])
            .page(B, &format!("/{i}"), "", &[]);
    }
    b
}

fn urls(site: &spiderlens_test_fixtures::Site, n: usize) -> Vec<String> {
    (0..n)
        .flat_map(|i| [site.url(A, &format!("/{i}")), site.url(B, &format!("/{i}"))])
        .collect()
}

fn breaker(c: &mut CrawlConfig) {
    c.retry.attempts = 1;
    c.retry.host_pause_after = 5;
    c.retry.host_pause_secs = 1.0;
    c.retry.host_pause_max_secs = 4.0;
    c.speed.max_per_host = 1;
    // While a host is down its robots.txt fails too; spec 002 decides that
    // case, so these tests skip robots.txt.
    c.robots.mode = spiderlens_config::RobotsMode::Ignore;
}

#[tokio::test]
async fn us4_1_2_paused_then_resumed_while_the_other_host_continues() {
    let site = two_hosts(30).start();
    site.set_down(A, true);
    let site = std::sync::Arc::new(site);
    let s2 = site.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(1500)).await;
        s2.set_down(A, false);
    });
    let d = crawl(&site, list(urls(&site, 30)), breaker).await;
    d.assert_one_final_state();
    let a_hits: Vec<_> = site
        .hits()
        .into_iter()
        .filter(|h| h.host == A && h.path != "/robots.txt")
        .collect();
    // SC-005: no more than 5 failed requests before the pause.
    let before_pause = a_hits
        .iter()
        .take_while(|h| h.at - a_hits[0].at < Duration::from_millis(900))
        .count();
    assert!(before_pause <= 5, "{before_pause} requests before the pause");
    assert!(
        d.notices
            .iter()
            .any(|n| n.contains("a.test") && n.contains("paused")),
        "{:?}",
        d.notices
    );
    assert!(
        d.notices.iter().any(|n| n.contains("answers again")),
        "{:?}",
        d.notices
    );
    assert_eq!(d.summary.hosts_paused, 1);
    // Every b.test page was crawled, and most a.test pages after recovery.
    let b_ok = (0..30)
        .filter(|i| d.page(&site.url(B, &format!("/{i}")))["status"] == 200)
        .count();
    assert_eq!(b_ok, 30);
    let a_ok = (0..30)
        .filter(|i| d.page(&site.url(A, &format!("/{i}")))["status"] == 200)
        .count();
    assert!(a_ok >= 24, "{a_ok} a.test pages recovered");
}

#[tokio::test]
async fn us4_3_given_up_host_records_host_unavailable() {
    let site = two_hosts(20).start();
    site.set_down(A, true);
    let d = crawl(&site, list(urls(&site, 20)), |c| {
        breaker(c);
        c.retry.host_give_up_after = 2;
        c.retry.host_pause_secs = 0.2;
        c.retry.host_pause_max_secs = 0.4;
    })
    .await;
    d.assert_one_final_state();
    let unavailable = d.summary.by_class.get("host_unavailable").copied().unwrap_or(0);
    assert!(unavailable >= 10, "{:?}", d.summary.by_class);
    assert!(
        d.notices.iter().any(|n| n.contains("Host unavailable")),
        "{:?}",
        d.notices
    );
    let a_requests = site
        .hits()
        .iter()
        .filter(|h| h.host == A && h.path != "/robots.txt")
        .count();
    assert!(
        a_requests <= 8,
        "5 failures, then 1 probe per pause: {a_requests}"
    );
}

#[tokio::test]
async fn us4_5_resume_host_now() {
    let site = two_hosts(10).start();
    site.set_down(A, true);
    let site = std::sync::Arc::new(site);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("t.spiderlens");
    let mut c = CrawlConfig {
        mode: Mode::List,
        ..CrawlConfig::default()
    };
    quick(&mut c);
    breaker(&mut c);
    c.retry.host_pause_secs = 600.0;
    c.retry.host_pause_max_secs = 600.0;
    let crawler = Crawler::create(&path, c, &list(urls(&site, 10)))
        .unwrap()
        .with_resolve(site.resolve());
    let (ctx, crx) = mpsc::unbounded_channel();
    let s2 = site.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(1000)).await;
        s2.set_down(A, false);
        ctx.send(RunCommand::ResumeHost(format!("http://a.test:{}", s2.port())))
            .unwrap();
        // Keep the channel open until the crawl ends.
        tokio::time::sleep(Duration::from_secs(60)).await;
    });
    let started = std::time::Instant::now();
    let d = run(dir, path, crawler, Control::Run, Some(crx)).await;
    assert!(
        started.elapsed() < Duration::from_secs(30),
        "did not wait for the 10-minute pause"
    );
    d.assert_one_final_state();
    assert!(
        d.notices.iter().any(|n| n.contains("resumed by you")),
        "{:?}",
        d.notices
    );
}

#[tokio::test]
async fn fr_029_a_dead_network_pauses_the_crawl() {
    let site = SiteBuilder::new().start();
    let closed = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap();
    let urls: Vec<String> = (0..12)
        .flat_map(|i| {
            [
                format!("http://x.test:{}/{i}", closed.port()),
                format!("http://y.test:{}/{i}", closed.port()),
            ]
        })
        .collect();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("t.spiderlens");
    let mut c = CrawlConfig {
        mode: Mode::List,
        ..CrawlConfig::default()
    };
    quick(&mut c);
    c.robots.mode = spiderlens_config::RobotsMode::Ignore;
    c.retry.host_pause_after = 50;
    let mut resolve = site.resolve();
    resolve.push(("x.test".into(), closed));
    resolve.push(("y.test".into(), closed));
    let crawler = Crawler::create(&path, c, &list(urls))
        .unwrap()
        .with_resolve(resolve);
    let d = run(dir, path, crawler, Control::Run, None).await;
    assert_eq!(d.summary.status, "paused");
    assert_eq!(d.summary.pause_reason.as_deref(), Some("network"));
    assert!(
        d.summary.by_class.is_empty(),
        "FR-015: no URL failed: {:?}",
        d.summary.by_class
    );
    assert!(d.notices.iter().any(|n| n.contains("network")), "{:?}", d.notices);
}
