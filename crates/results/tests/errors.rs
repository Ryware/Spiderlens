//! Spec 004 in the results table: the Attempts column, the transient and
//! permanent failure filters, and the retry fields of the detail pane.

use std::collections::BTreeSet;
use std::time::Duration;

use spiderlens_config::{CrawlConfig, ModeInput, SpeedSettings};
use spiderlens_engine::crawl::{Control, Crawler};
use spiderlens_results::{Reader, UrlScope, ViewQuery, tabs};
use spiderlens_test_fixtures::SiteBuilder;
use tokio::sync::watch;

const E: &str = "example.test";

fn q(filter: &str) -> ViewQuery {
    ViewQuery {
        tab: "response-codes".into(),
        filter: filter.into(),
        scope: UrlScope::All,
        search: String::new(),
        sort: None,
        segment: None,
    }
}

/// (path, row values after the id) for every row of a view.
fn rows(r: &mut Reader, base: &str, query: &ViewQuery) -> Vec<(String, Vec<serde_json::Value>)> {
    let page = r.open_view(query, 500).unwrap();
    r.materialize(page.view).unwrap();
    let rows = r.rows(page.view, 0, 1000).unwrap().rows;
    r.close_view(page.view).unwrap();
    rows.into_iter()
        .map(|row| {
            let url = row[1].as_str().unwrap().to_string();
            (url.strip_prefix(base).unwrap_or(&url).to_string(), row.clone())
        })
        .collect()
}

fn paths(r: &mut Reader, base: &str, filter: &str) -> BTreeSet<String> {
    rows(r, base, &q(filter)).into_iter().map(|(p, _)| p).collect()
}

fn set(items: &[&str]) -> BTreeSet<String> {
    items.iter().map(|s| s.to_string()).collect()
}

#[tokio::test]
async fn attempts_and_failure_filters() {
    let site = SiteBuilder::new()
        .page(E, "/", "", &["/ok", "/flaky", "/down", "/missing", "/not-impl"])
        .page(E, "/ok", "", &[])
        .flaky(E, "/flaky", 503, 1, &[])
        .status(E, "/down", 503)
        .status(E, "/not-impl", 501)
        .start();
    let mut c = CrawlConfig {
        speed: SpeedSettings::FAST,
        ..CrawlConfig::default()
    };
    c.retry.base_wait_secs = 0.1;
    c.retry.max_wait_secs = 1.0;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("errors.spiderlens");
    let input = ModeInput::Spider {
        start_url: site.url(E, "/"),
    };
    let crawler = Crawler::create(&path, c, &input)
        .unwrap()
        .with_resolve(site.resolve());
    let (_tx, rx) = watch::channel(Control::Run);
    tokio::time::timeout(Duration::from_secs(60), crawler.run(rx, None))
        .await
        .expect("crawl finishes")
        .unwrap();

    let mut r = Reader::open(&path).unwrap();
    let base = site.url(E, "");
    assert_eq!(paths(&mut r, &base, "transient"), set(&["/down"]));
    assert_eq!(paths(&mut r, &base, "permanent"), set(&["/missing", "/not-impl"]));
    assert_eq!(paths(&mut r, &base, "retried"), set(&["/down", "/flaky"]));

    let tab = tabs().iter().find(|t| t.id == "response-codes").unwrap();
    let col = 1 + tab.columns.iter().position(|c| c.id == "attempts").unwrap();
    let all = rows(&mut r, &base, &q("all"));
    let attempts = |p: &str| all.iter().find(|(path, _)| path == p).unwrap().1[col].clone();
    assert_eq!(attempts("/ok"), 1);
    assert_eq!(attempts("/flaky"), 2);
    assert_eq!(attempts("/down"), 3);
    assert_eq!(attempts("/not-impl"), 1, "501 is permanent: not retried");

    let counts = r.counts("response-codes", UrlScope::All).unwrap();
    assert_eq!(counts.counts["transient"], Some(1));
    assert_eq!(counts.counts["permanent"], Some(2));
    assert!(counts.unavailable.is_empty(), "{:?}", counts.unavailable);
    // One filter per error class: none of these happened here.
    assert_eq!(counts.counts["class_timeout"], Some(0));
    assert_eq!(counts.counts["class_dns"], Some(0));

    let id = all.iter().find(|(p, _)| p == "/down").unwrap().1[0]
        .as_i64()
        .unwrap();
    let d = r.detail(id).unwrap();
    assert_eq!(d.attempts, Some(3));
    assert!(!d.manual_retry);
    assert!(!d.unavailable.contains_key("attempts"));
}
