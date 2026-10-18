//! Spec 026 static checks inside a crawl (T046, SC-001 static part): the
//! stored results of every fixture page equal the page's expected findings,
//! and with the level Off nothing is stored.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Duration;

use spiderlens_config::{A11yLevel, CrawlConfig, ModeInput, SpeedSettings};
use spiderlens_engine::crawl::{Control, CrawlStore, Crawler};
use spiderlens_test_fixtures::a11y_site::STATIC_PAGES;
use spiderlens_test_fixtures::{Site, SiteBuilder};
use tokio::sync::watch;

const E: &str = "a11y.test";

fn site() -> Site {
    let links: String = STATIC_PAGES
        .iter()
        .map(|p| format!("<a href=\"{}\">{}</a> ", p.path, p.path))
        .collect();
    let mut b = SiteBuilder::new().html(
        E,
        "/",
        &format!(
            "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\">\
             <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\
             <title>Index</title></head><body><main><h1>Index</h1>{links}</main></body></html>"
        ),
    );
    for p in STATIC_PAGES {
        b = b.html(E, p.path, p.html);
    }
    b.start()
}

struct Crawled {
    _dir: tempfile::TempDir,
    path: PathBuf,
    site: Site,
}

async fn crawl(level: A11yLevel) -> Crawled {
    let site = site();
    let mut c = CrawlConfig {
        speed: SpeedSettings::FAST,
        ..CrawlConfig::default()
    };
    c.accessibility.level = level;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a11y.spiderlens");
    let crawler = Crawler::create(
        &path,
        c,
        &ModeInput::Spider {
            start_url: site.url(E, "/"),
        },
    )
    .unwrap()
    .with_resolve(site.resolve())
    .with_retry_base(Duration::from_millis(10));
    let (_tx, rx) = watch::channel(Control::Run);
    let summary = tokio::time::timeout(Duration::from_secs(120), crawler.run(rx, None))
        .await
        .expect("crawl finishes")
        .expect("crawl succeeds");
    assert_eq!(summary.status, "completed");
    Crawled {
        _dir: dir,
        path,
        site,
    }
}

fn total(store: &CrawlStore, table: &str) -> i64 {
    store
        .conn()
        .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
        .unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn every_page_stores_exactly_its_expected_static_findings() {
    let c = crawl(A11yLevel::Static).await;
    let store = CrawlStore::open(&c.path).unwrap();
    let features = store.meta("data_features").unwrap().unwrap();
    assert!(
        features.split(',').any(|f| f == "accessibility_static"),
        "{features}"
    );
    assert!(!features.split(',').any(|f| f == "accessibility_full"));

    let mut failures = Vec::new();
    for page in STATIC_PAGES {
        let id = store
            .id_of(&c.site.url(E, page.path))
            .unwrap()
            .unwrap_or_else(|| panic!("{} was not crawled", page.path));
        let mut stmt = store
            .conn()
            .prepare(
                "SELECT check_id, nodes_total, kind, source, impact FROM a11y_results \
                 WHERE url_id = ?1 ORDER BY check_id",
            )
            .unwrap();
        let rows: Vec<(String, u32, String, String, Option<String>)> = stmt
            .query_map([id], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
            })
            .unwrap()
            .map(|r| r.unwrap())
            .collect();
        let got: BTreeMap<&str, u32> = rows
            .iter()
            .map(|(check, n, ..)| (check.strip_prefix("static.").unwrap_or(check), *n))
            .collect();
        let want: BTreeMap<&str, u32> = page.expect.iter().copied().collect();
        if got != want {
            failures.push(format!("{}: expected {want:?}, got {got:?}", page.path));
        }
        for (check, n, kind, source, impact) in &rows {
            assert!(check.starts_with("static."), "{check}");
            assert_eq!(
                (kind.as_str(), source.as_str()),
                ("violation", "static"),
                "{check}"
            );
            assert!(impact.is_some(), "{check} has no impact");
            assert!(*n >= 1, "{check}");
        }
        // Every checked page has its audit row, also the clean ones.
        let state: Option<String> = store
            .conn()
            .query_row(
                "SELECT static_state FROM a11y_audits WHERE url_id = ?1",
                [id],
                |r| r.get(0),
            )
            .unwrap_or_else(|e| panic!("{}: no audit row: {e}", page.path));
        assert_eq!(state.as_deref(), Some("checked"), "{}", page.path);
        let checks: i64 = store
            .conn()
            .query_row(
                "SELECT checks_failed FROM a11y_audits WHERE url_id = ?1",
                [id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(checks as usize, page.expect.len(), "{}", page.path);
    }
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));

    // The viewport facts are read from the HTML in the same pass.
    let missing = store
        .id_of(&c.site.url(E, "/static/viewport_missing"))
        .unwrap()
        .unwrap();
    let state: String = store
        .conn()
        .query_row(
            "SELECT viewport_state FROM mobile_measures WHERE url_id = ?1",
            [missing],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(state, "missing");
    let clean = store.id_of(&c.site.url(E, "/clean/1")).unwrap().unwrap();
    let state: String = store
        .conn()
        .query_row(
            "SELECT viewport_state FROM mobile_measures WHERE url_id = ?1",
            [clean],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(state, "ok");
}

#[tokio::test(flavor = "multi_thread")]
async fn with_the_level_off_nothing_is_checked_or_stored() {
    let c = crawl(A11yLevel::Off).await;
    let store = CrawlStore::open(&c.path).unwrap();
    for table in ["a11y_audits", "a11y_results", "a11y_nodes", "mobile_measures"] {
        assert_eq!(total(&store, table), 0, "{table} must stay empty");
    }
    let features = store.meta("data_features").unwrap().unwrap();
    for f in ["accessibility_static", "accessibility_full", "mobile_layout"] {
        assert!(!features.split(',').any(|x| x == f), "{f} in {features}");
    }
    // The pages themselves were still crawled.
    assert!(
        store
            .id_of(&c.site.url(E, "/static/form_label"))
            .unwrap()
            .is_some()
    );
}
