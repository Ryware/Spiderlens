//! Spec 004 US5: retry only the transient failures.

mod support;

use spiderlens_engine::crawl::{Control, Crawler, retry_preview};
use spiderlens_test_fixtures::SiteBuilder;
use support::*;

#[tokio::test]
async fn us5_retries_only_transient_failures_in_place() {
    let site = SiteBuilder::new()
        .page(E, "/", "", &["/missing", "/flaky1", "/flaky2"])
        .flaky(E, "/flaky1", 503, 1, &["/found-later"])
        .flaky(E, "/flaky2", 503, 1, &[])
        .page(E, "/found-later", "", &[])
        .start();
    let d = crawl(&site, spider(&site, "/"), |c| c.retry.attempts = 1).await;
    assert_eq!(d.summary.by_class.get("http_5xx"), Some(&2));
    assert_eq!(site.count(E, "/found-later"), 0);

    // US5.1: the counts by class before anything changes.
    let preview = retry_preview(&d.path).unwrap();
    assert_eq!(preview, vec![("http_5xx".to_string(), 2)]);

    let (crawler, n) = Crawler::retry_failed(&d.path).unwrap();
    assert_eq!(n, 2);
    let path = d.path.clone();
    let r = run(
        d.dir,
        path,
        crawler.with_resolve(site.resolve()),
        Control::Run,
        None,
    )
    .await;
    r.assert_one_final_state();
    // SC-006: every transient failure again, no permanent one.
    assert_eq!(site.count(E, "/flaky1"), 2);
    assert_eq!(site.count(E, "/flaky2"), 2);
    assert_eq!(site.count(E, "/missing"), 1);
    // US5.3: new result in place, total attempts, marked manual, links followed.
    let p = r.page(&site.url(E, "/flaky1"));
    assert_eq!(p["status"], 200);
    assert_eq!(p["attempts"], 2);
    assert_eq!(p["manual_retry"], 1);
    assert_eq!(site.count(E, "/found-later"), 1);
    assert_eq!(
        r.summary.by_class.get("http_5xx"),
        None,
        "{:?}",
        r.summary.by_class
    );
    // The 404s stay as they were: /missing, and the link on the first
    // run's 503 error page (processed then, as for any final answer).
    assert_eq!(
        r.summary.by_class.get("http_4xx"),
        Some(&2),
        "{:?}",
        r.summary.by_class
    );
    assert_eq!(r.summary.manual_retries, 1);
    assert_eq!(r.summary.recovered, 2);
    // US5.4: nothing left to retry.
    assert!(retry_preview(&r.path).unwrap().is_empty());
    let (_, n) = Crawler::retry_failed(&r.path).unwrap();
    assert_eq!(n, 0);
}
