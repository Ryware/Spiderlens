//! Spec 004 US2 (retries) and US6 (headers on every request).

mod support;

use std::time::Duration;

use spiderlens_config::{CustomHeader, HeaderTarget, HeaderValue};
use spiderlens_engine::crawl::{Control, CrawlStore, Crawler};
use spiderlens_test_fixtures::SiteBuilder;
use support::*;
use tokio::sync::watch;

fn gaps(site: &spiderlens_test_fixtures::Site, host: &str, path: &str) -> Vec<Duration> {
    let times: Vec<_> = site
        .hits()
        .into_iter()
        .filter(|h| h.host == host && h.path == path)
        .map(|h| h.at)
        .collect();
    times.windows(2).map(|w| w[1] - w[0]).collect()
}

#[tokio::test]
async fn us2_1_succeeds_on_the_third_attempt_with_growing_waits() {
    let site = SiteBuilder::new()
        .page(E, "/", "", &["/flaky"])
        .flaky(E, "/flaky", 503, 2, &["/after"])
        .page(E, "/after", "", &[])
        .start();
    let d = crawl(&site, spider(&site, "/"), |_| {}).await;
    d.assert_one_final_state();
    let p = d.page(&site.url(E, "/flaky"));
    assert_eq!(p["status"], 200);
    assert_eq!(p["attempts"], 3);
    assert!(p["error_class"].is_null());
    let g = gaps(&site, E, "/flaky");
    assert_eq!(g.len(), 2);
    // Base 0.1 s in tests: about 0.1 s then 0.2 s, within 20%.
    assert!(
        g[0] >= Duration::from_millis(78) && g[0] < Duration::from_millis(600),
        "{g:?}"
    );
    assert!(
        g[1] >= Duration::from_millis(158) && g[1] < Duration::from_millis(800),
        "{g:?}"
    );
    assert_eq!(
        site.count(E, "/after"),
        1,
        "links of the successful answer are followed"
    );
    assert_eq!(
        site.count(E, "/should-not-be-followed"),
        0,
        "failed answers add no links"
    );
    assert_eq!(d.summary.recovered, 1);
    assert!(d.summary.by_class.is_empty(), "SC-002: not an error");
}

#[tokio::test]
async fn us2_2_and_sc_003_attempt_counts() {
    let site = SiteBuilder::new()
        .page(E, "/", "", &["/always", "/missing", "/notimpl"])
        .status(E, "/always", 500)
        .status(E, "/notimpl", 501)
        .start();
    let d = crawl(&site, spider(&site, "/"), |_| {}).await;
    assert_eq!(site.count(E, "/always"), 3);
    assert_eq!(site.count(E, "/missing"), 1);
    assert_eq!(site.count(E, "/notimpl"), 1, "501 is permanent");
    let p = d.page(&site.url(E, "/always"));
    assert_eq!(p["status"], 500);
    assert_eq!(p["attempts"], 3);
}

#[tokio::test]
async fn us2_3_retry_after_holds_the_whole_host() {
    let site = SiteBuilder::new()
        .page(E, "/", "", &["/limited", "/b", "/c", "/d"])
        .retry_after(E, "/limited", 429, "1")
        .page(E, "/b", "", &[])
        .page(E, "/c", "", &[])
        .page(E, "/d", "", &[])
        .start();
    let d = crawl(&site, spider(&site, "/"), |c| c.retry.attempts = 2).await;
    d.assert_one_final_state();
    let hits = site.page_hits();
    let first = hits.iter().position(|h| h.path == "/limited").unwrap();
    let at = hits[first].at;
    // SC-004: nothing else goes to the host for the second it asked for,
    // except requests already on their way.
    let early: Vec<_> = hits[first + 1..]
        .iter()
        .filter(|h| h.at - at < Duration::from_millis(950))
        .filter(|h| h.at - at > Duration::from_millis(50))
        .map(|h| h.path.clone())
        .collect();
    assert!(early.is_empty(), "requests during Retry-After: {early:?}");
    let g = gaps(&site, E, "/limited");
    assert!(g[0] >= Duration::from_millis(990), "{g:?}");
}

#[tokio::test]
async fn us2_4_retry_after_is_capped_and_logged() {
    let site = SiteBuilder::new()
        .page(E, "/", "", &["/slowdown"])
        .retry_after(E, "/slowdown", 503, "600")
        .start();
    let d = crawl(&site, spider(&site, "/"), |c| {
        c.retry.attempts = 2;
        c.retry.max_wait_secs = 1.0;
    })
    .await;
    let g = gaps(&site, E, "/slowdown");
    assert!(
        g[0] >= Duration::from_millis(990) && g[0] < Duration::from_secs(5),
        "{g:?}"
    );
    let log = CrawlStore::open(&d.path).unwrap().log_entries(50).unwrap();
    assert!(
        log.iter()
            .any(|e| e.kind == "retry_after_capped" && e.message.contains("10 min")),
        "{log:?}"
    );
}

#[tokio::test]
async fn us2_6_other_urls_go_ahead_while_a_retry_waits() {
    let site = SiteBuilder::new()
        .page(E, "/", "", &["/flaky", "/a", "/b", "/c"])
        .flaky(E, "/flaky", 503, 1, &[])
        .page(E, "/a", "", &[])
        .page(E, "/b", "", &[])
        .page(E, "/c", "", &[])
        .start();
    crawl(&site, spider(&site, "/"), |c| c.retry.base_wait_secs = 0.5).await;
    let order: Vec<String> = site.page_hits().into_iter().map(|h| h.path).collect();
    let last = order.iter().rposition(|p| p == "/flaky").unwrap();
    for p in ["/a", "/b", "/c"] {
        let i = order.iter().position(|x| x == p).unwrap();
        assert!(i < last, "{p} waited for the retry: {order:?}");
    }
}

#[tokio::test]
async fn fr_023_a_pause_during_the_wait_keeps_the_attempts() {
    let site = SiteBuilder::new()
        .page(E, "/", "", &["/flaky"])
        .flaky(E, "/flaky", 503, 1, &[])
        .start();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("t.spiderlens");
    let mut c = spiderlens_config::CrawlConfig::default();
    quick(&mut c);
    c.retry.base_wait_secs = 3.0;
    c.retry.max_wait_secs = 10.0;
    let crawler = Crawler::create(&path, c, &spider(&site, "/"))
        .unwrap()
        .with_resolve(site.resolve());
    let (tx, rx) = watch::channel(Control::Run);
    let run = tokio::spawn(crawler.run(rx, None));
    tokio::time::sleep(Duration::from_millis(800)).await;
    tx.send(Control::Pause).unwrap();
    let s = run.await.unwrap().unwrap();
    assert_eq!(s.status, "paused");
    assert_eq!(s.queued, 1, "the waiting URL is kept");
    assert_eq!(site.count(E, "/flaky"), 1);
    // Resume: the retry happens once, and counts as the second attempt.
    let d = support::run(dir, path.clone(), reopen(&path, &site), Control::Run, None).await;
    d.assert_one_final_state();
    assert_eq!(site.count(E, "/flaky"), 2, "SC-009: no lost or extra request");
    assert_eq!(d.page(&site.url(E, "/flaky"))["attempts"], 2);
}

#[tokio::test]
async fn us6_headers_and_user_agent_on_retries() {
    let site = SiteBuilder::new()
        .page(E, "/", "", &["/flaky"])
        .flaky(E, "/flaky", 503, 2, &[])
        .start();
    crawl(&site, spider(&site, "/"), |c| {
        c.identity.headers.push(CustomHeader {
            name: "X-Stage".into(),
            value: HeaderValue::secret("token-1"),
            send_to: HeaderTarget::All,
        });
    })
    .await;
    let hits: Vec<_> = site.hits().into_iter().filter(|h| h.path == "/flaky").collect();
    assert_eq!(hits.len(), 3);
    for h in &hits {
        assert!(h.user_agent.starts_with("SpiderLens/"), "{}", h.user_agent);
        assert!(
            h.headers.iter().any(|(k, v)| k == "x-stage" && v == "token-1"),
            "{:?}",
            h.headers
        );
    }
    let robots = site.hits().into_iter().find(|h| h.path == "/robots.txt").unwrap();
    assert!(
        robots.headers.iter().any(|(k, _)| k == "x-stage"),
        "robots.txt too"
    );
}
