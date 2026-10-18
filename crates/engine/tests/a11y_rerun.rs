//! Spec 026 T061: the static accessibility checks on a finished crawl
//! (`accessibility::rerun_static`) give the findings the crawl itself would,
//! send no requests and refuse files they cannot write.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use spiderlens_config::{A11yLevel, AccessibilitySettings, CrawlConfig, ModeInput, SpeedSettings};
use spiderlens_engine::accessibility::{AccessibilityError, rerun_static};
use spiderlens_engine::crawl::{Control, CrawlStore, Crawler, WriterLock};
use spiderlens_test_fixtures::a11y_site::STATIC_PAGES;
use spiderlens_test_fixtures::{Site, SiteBuilder};
use tokio::sync::watch;

const E: &str = "a11y.test";

fn site() -> Site {
    let links: Vec<&str> = STATIC_PAGES.iter().map(|p| p.path).collect();
    let mut b = SiteBuilder::new().page(E, "/", "", &links);
    for p in STATIC_PAGES {
        b = b.html(E, p.path, p.html);
    }
    b.start()
}

async fn crawl(site: &Site, edit: impl FnOnce(&mut CrawlConfig)) -> (tempfile::TempDir, PathBuf) {
    let mut c = CrawlConfig {
        speed: SpeedSettings::FAST,
        ..CrawlConfig::default()
    };
    // The checks are run afterwards, not during the crawl.
    c.accessibility.level = A11yLevel::Off;
    edit(&mut c);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("t.spiderlens");
    let input = ModeInput::Spider {
        start_url: site.url(E, "/"),
    };
    let crawler = Crawler::create(&path, c, &input)
        .unwrap()
        .with_resolve(site.resolve())
        .with_retry_base(Duration::from_millis(10));
    let (_tx, rx) = watch::channel(Control::Run);
    tokio::time::timeout(Duration::from_secs(60), crawler.run(rx, None))
        .await
        .expect("crawl finishes")
        .unwrap();
    (dir, path)
}

/// `address -> [(check id, elements)]` of the stored static results.
fn stored(path: &Path) -> BTreeMap<String, Vec<(String, u32)>> {
    let store = CrawlStore::open(path).unwrap();
    let mut stmt = store
        .conn()
        .prepare(
            "SELECT u.url, r.check_id, r.nodes_total FROM a11y_results r JOIN urls u ON u.id = r.url_id \
             WHERE r.source = 'static' AND r.kind = 'violation' ORDER BY u.url, r.check_id",
        )
        .unwrap();
    let rows = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, u32>(2)?,
            ))
        })
        .unwrap();
    let mut out: BTreeMap<String, Vec<(String, u32)>> = BTreeMap::new();
    for row in rows {
        let (url, check, n) = row.unwrap();
        out.entry(url).or_default().push((check, n));
    }
    out
}

fn count(path: &Path, sql: &str) -> i64 {
    let store = CrawlStore::open(path).unwrap();
    store.conn().query_row(sql, [], |r| r.get(0)).unwrap()
}

#[tokio::test]
async fn rerun_gives_the_fixture_findings_and_sends_no_requests() {
    let site = site();
    let (_d, path) = crawl(&site, |_| {}).await;
    assert_eq!(
        count(&path, "SELECT COUNT(*) FROM a11y_audits"),
        0,
        "level off: nothing checked"
    );

    let hits_before = site.hits().len();
    let mut calls = Vec::new();
    let summary = rerun_static(&path, &AccessibilitySettings::default(), |done, total| {
        calls.push((done, total))
    })
    .unwrap();
    assert_eq!(site.hits().len(), hits_before, "a re-check sends no requests");
    assert_eq!(
        summary.pages as usize,
        STATIC_PAGES.len() + 1,
        "every page plus the home page"
    );
    assert_eq!(summary.skipped_no_html, 0);
    let with_findings = STATIC_PAGES.iter().filter(|p| !p.expect.is_empty()).count() as u64;
    assert!(summary.with_findings >= with_findings, "{summary:?}");
    assert_eq!(calls.last().map(|c| c.0), Some(summary.pages));

    let found = stored(&path);
    for page in STATIC_PAGES {
        let mut want: Vec<(String, u32)> = page
            .expect
            .iter()
            .map(|(id, n)| (format!("static.{id}"), *n))
            .collect();
        want.sort();
        let got = found.get(&site.url(E, page.path)).cloned().unwrap_or_default();
        assert_eq!(got, want, "{}", page.path);
    }
    assert_eq!(
        count(
            &path,
            "SELECT COUNT(*) FROM a11y_audits WHERE static_state = 'checked'"
        ),
        summary.pages as i64
    );
    assert!(
        count(
            &path,
            "SELECT COUNT(*) FROM mobile_measures WHERE viewport_state IS NOT NULL"
        ) > 0,
        "mobile facts are stored too"
    );
    let store = CrawlStore::open(&path).unwrap();
    assert!(
        store
            .meta("data_features")
            .unwrap()
            .unwrap_or_default()
            .contains("accessibility_static")
    );
}

#[tokio::test]
async fn running_twice_replaces_the_results() {
    let site = site();
    let (_d, path) = crawl(&site, |_| {}).await;
    let settings = AccessibilitySettings::default();
    let first = rerun_static(&path, &settings, |_, _| {}).unwrap();
    let before = stored(&path);
    let rows = count(&path, "SELECT COUNT(*) FROM a11y_results");
    let second = rerun_static(&path, &settings, |_, _| {}).unwrap();
    assert_eq!(first, second);
    assert_eq!(stored(&path), before);
    assert_eq!(
        count(&path, "SELECT COUNT(*) FROM a11y_results"),
        rows,
        "no duplicate rows"
    );
}

#[tokio::test]
async fn a_crawl_without_stored_html_is_refused() {
    let site = site();
    let (_d, path) = crawl(&site, |c| c.extraction.store_html = false).await;
    match rerun_static(&path, &AccessibilitySettings::default(), |_, _| {}) {
        Err(AccessibilityError::NotPossible(m)) => assert!(m.contains("stored page HTML"), "{m}"),
        other => panic!("expected a refusal, got {other:?}"),
    }
}

#[tokio::test]
async fn a_file_another_process_holds_is_refused() {
    let site = site();
    let (_d, path) = crawl(&site, |_| {}).await;
    let _lock = WriterLock::acquire(&path).unwrap();
    match rerun_static(&path, &AccessibilitySettings::default(), |_, _| {}) {
        Err(AccessibilityError::NotPossible(m)) => assert!(m.contains("view-only"), "{m}"),
        other => panic!("expected a refusal, got {other:?}"),
    }
    assert_eq!(
        count(&path, "SELECT COUNT(*) FROM a11y_audits"),
        0,
        "nothing was written"
    );
}
