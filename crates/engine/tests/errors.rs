//! Spec 004 US1 and US3: every URL ends in one named outcome.

mod support;

use spiderlens_engine::crawl::CrawlStore;
use spiderlens_test_fixtures::SiteBuilder;
use support::*;

#[tokio::test]
async fn us1_1_http_answers_are_classed_by_status() {
    let site = SiteBuilder::new()
        .page(E, "/", "", &["/ok", "/gone", "/missing", "/broken"])
        .page(E, "/ok", "", &[])
        .status(E, "/gone", 410)
        .status(E, "/broken", 500)
        .start();
    let d = crawl(&site, spider(&site, "/"), |_| {}).await;
    d.assert_one_final_state();
    let s = &d.summary;
    assert_eq!(s.by_class.get("http_4xx"), Some(&2), "{s:?}");
    assert_eq!(s.by_class.get("http_5xx"), Some(&1), "{s:?}");
    assert_eq!(s.transient_errors, 1);
    assert_eq!(s.permanent_errors, 2);
    assert_eq!(d.page(&site.url(E, "/ok"))["attempts"], 1);
    let gone = d.page(&site.url(E, "/gone"));
    assert_eq!(gone["status"], 410);
    assert_eq!(gone["attempts"], 1, "permanent: requested once");
    assert_eq!(site.count(E, "/gone"), 1, "SC-003");
    let broken = d.page(&site.url(E, "/broken"));
    assert_eq!(broken["attempts"], 3, "transient: three attempts");
    assert!(
        broken["error_class"].is_null(),
        "HTTP answers keep their status only"
    );
}

#[tokio::test]
async fn us1_4_timeouts_say_read() {
    let site = SiteBuilder::new()
        .page(E, "/", "", &["/slow"])
        .hang(E, "/slow")
        .start();
    let d = crawl(&site, spider(&site, "/"), |c| c.retry.attempts = 2).await;
    d.assert_one_final_state();
    let p = d.page(&site.url(E, "/slow"));
    assert_eq!(p["error_class"], "timeout");
    assert_eq!(p["error_reason"], "read");
    assert_eq!(p["attempts"], 2);
    assert!(p["error_message"].as_str().unwrap().contains("time limit"));
}

#[tokio::test]
async fn us1_5_content_problems_name_the_problem() {
    let big = "x".repeat(1_200_000);
    let site = SiteBuilder::new()
        .page(E, "/", "", &["/big", "/badgzip", "/cut"])
        .raw(E, "/big", 200, "text/html", big.as_bytes())
        .raw(E, "/badgzip", 200, "text/html", b"this is not gzip at all")
        .header(E, "/badgzip", "content-encoding", "gzip")
        .truncated(E, "/cut")
        .start();
    let d = crawl(&site, spider(&site, "/"), |c| c.network.max_body_mb = 1).await;
    d.assert_one_final_state();
    let big = d.page(&site.url(E, "/big"));
    assert_eq!(big["error_class"], "content_problem");
    assert_eq!(big["error_reason"], "over_size");
    assert_eq!(big["attempts"], 1, "over size is permanent");
    let gz = d.page(&site.url(E, "/badgzip"));
    assert_eq!(gz["error_class"], "content_problem", "{gz}");
    assert_eq!(gz["error_reason"], "decompression", "{gz}");
    assert_eq!(gz["attempts"], 1);
    let cut = d.page(&site.url(E, "/cut"));
    assert_eq!(cut["error_class"], "content_problem", "{cut}");
    assert_eq!(cut["error_reason"], "truncated", "{cut}");
    assert_eq!(cut["attempts"], 3, "a truncated body is transient");
}

#[tokio::test]
async fn refused_connections_are_connection_errors() {
    let site = SiteBuilder::new().page(E, "/", "", &[]).start();
    // A port nothing listens on.
    let closed = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap();
    let url = format!("http://down.test:{}/x", closed.port());
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("t.spiderlens");
    let mut c = spiderlens_config::CrawlConfig {
        mode: spiderlens_config::Mode::List,
        ..Default::default()
    };
    quick(&mut c);
    allow_slow_refusal(&mut c);
    c.retry.attempts = 2;
    c.robots.mode = spiderlens_config::RobotsMode::Ignore;
    let mut resolve = site.resolve();
    resolve.push(("down.test".into(), closed));
    let crawler =
        spiderlens_engine::crawl::Crawler::create(&path, c, &list(vec![site.url(E, "/"), url.clone()]))
            .unwrap()
            .with_resolve(resolve);
    let d = run(dir, path, crawler, spiderlens_engine::crawl::Control::Run, None).await;
    d.assert_one_final_state();
    let p = d.page(&url);
    assert_eq!(p["error_class"], "connection", "{p}");
    assert_eq!(p["error_reason"], "refused", "{p}");
    assert_eq!(p["attempts"], 2);
}

#[tokio::test]
async fn us1_6_malformed_links_are_recorded_without_a_request() {
    let site = SiteBuilder::new()
        .html(E, "/", r#"<a href="http://exa mple.test:99999/">bad</a>"#)
        .start();
    let d = crawl(&site, spider(&site, "/"), |_| {}).await;
    d.assert_one_final_state();
    assert_eq!(
        d.summary
            .by_reason
            .get(&spiderlens_engine::crawl::NotCrawledReason::MalformedUrl),
        Some(&1)
    );
    assert_eq!(site.page_hits().len(), 1, "no request for the malformed link");
}

#[tokio::test]
async fn us3_6_invalid_redirect_target() {
    let site = SiteBuilder::new()
        .page(E, "/", "", &["/r"])
        .redirect(E, "/r", 301, "http://exa mple.test:99999/")
        .start();
    let d = crawl(&site, spider(&site, "/"), |_| {}).await;
    d.assert_one_final_state();
    let r = d.page(&site.url(E, "/r"));
    assert_eq!(r["error_class"], "redirect_problem");
    assert_eq!(r["error_reason"], "invalid_target");
    assert!(r["error_message"].as_str().unwrap().contains("exa mple"), "{r}");
    assert_eq!(r["status"], 301, "the hop's status is kept");
}

#[tokio::test]
async fn us3_2_following_stops_at_the_hop_limit() {
    let site = SiteBuilder::new()
        .page(E, "/", "", &["/r0"])
        .redirect(E, "/r0", 301, "/r1")
        .redirect(E, "/r1", 302, "/r2")
        .redirect(E, "/r2", 301, "/r3")
        .redirect(E, "/r3", 301, "/r4")
        .page(E, "/r4", "", &[])
        .start();
    let d = crawl(&site, spider(&site, "/"), |c| c.network.max_redirects = 3).await;
    d.assert_one_final_state();
    assert_eq!(site.count(E, "/r3"), 1, "the third hop is followed");
    assert_eq!(site.count(E, "/r4"), 0, "the fourth is not");
    let r4 = d.page(&site.url(E, "/r4"));
    assert_eq!(r4["reason"], "redirect_limit");
    // The chain is still recorded hop by hop.
    let r3 = d.page(&site.url(E, "/r3"));
    assert_eq!(r3["redirect_to"], site.url(E, "/r4"));
}

#[tokio::test]
async fn us3_5_a_redirect_to_a_crawled_page_links_to_it() {
    let site = SiteBuilder::new()
        .page(E, "/", "", &["/a", "/old"])
        .page(E, "/a", "", &[])
        .redirect(E, "/old", 301, "/a")
        .start();
    let d = crawl(&site, spider(&site, "/"), |_| {}).await;
    assert_eq!(site.count(E, "/a"), 1);
    let old = d.page(&site.url(E, "/old"));
    assert_eq!(old["redirect_to"], site.url(E, "/a"));
}

#[tokio::test]
async fn log_and_attempts_survive_in_the_file() {
    let site = SiteBuilder::new()
        .page(E, "/", "", &["/broken"])
        .status(E, "/broken", 503)
        .start();
    let d = crawl(&site, spider(&site, "/"), |_| {}).await;
    let store = CrawlStore::open(&d.path).unwrap();
    let log = store.log_entries(100).unwrap();
    let kinds: Vec<&str> = log.iter().map(|e| e.kind.as_str()).collect();
    assert_eq!(kinds.iter().filter(|k| **k == "retry").count(), 2, "{kinds:?}");
    assert_eq!(kinds.iter().filter(|k| **k == "failure").count(), 1, "{kinds:?}");
    assert_eq!(d.summary.retries, 2);
}
