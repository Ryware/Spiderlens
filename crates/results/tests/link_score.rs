//! Spec 023 on a real crawl: Link Score, link counts and the Links tab.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::time::Duration;

use spiderlens_config::{CrawlConfig, ModeInput, SpeedSettings};
use spiderlens_engine::crawl::{Control, Crawler};
use spiderlens_results::{Need, Reader, UrlScope, ViewQuery, tabs};
use spiderlens_test_fixtures::{Site, SiteBuilder};
use tokio::sync::watch;

const E: &str = "example.test";

fn page(links: &[&str]) -> String {
    let a: String = links
        .iter()
        .map(|l| match l.strip_prefix('!') {
            Some(nf) => format!("<a rel=\"nofollow\" href=\"{nf}\">{nf}</a>"),
            None => format!("<a href=\"{l}\">{l}</a>"),
        })
        .collect();
    format!("<!doctype html><html><head><title>A page</title></head><body>{a}</body></html>")
}

/// `/` → hub, a, b, nofollow nf, redir (→ target); hub → /, a, b, c;
/// a, b, c → hub; a also links itself and hub twice; c → deep1 → deep2 →
/// deep3; b → missing (404).
fn site() -> Site {
    SiteBuilder::new()
        .html(E, "/", &page(&["/hub", "/a", "/b", "!/nf", "/redir"]))
        .html(E, "/hub", &page(&["/", "/a", "/b", "/c"]))
        .html(E, "/a", &page(&["/hub", "/hub", "/a"]))
        .html(E, "/b", &page(&["/hub", "/missing"]))
        .html(E, "/c", &page(&["/hub", "/deep1"]))
        .html(E, "/deep1", &page(&["/deep2"]))
        .html(E, "/deep2", &page(&["/deep3"]))
        .html(E, "/deep3", &page(&[]))
        .html(E, "/nf", &page(&[]))
        .html(E, "/target", &page(&[]))
        .redirect(E, "/redir", 301, "/target")
        .start()
}

struct Crawled {
    _dir: tempfile::TempDir,
    path: PathBuf,
    site: Site,
}

async fn crawl() -> Crawled {
    let site = site();
    let mut c = CrawlConfig {
        speed: SpeedSettings::FAST,
        ..CrawlConfig::default()
    };
    // Crawl the nofollow-only page too, so its score can be checked.
    c.scope.follow_internal_nofollow = true;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("links.spiderlens");
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
    Crawled {
        _dir: dir,
        path,
        site,
    }
}

fn q(tab: &str, filter: &str) -> ViewQuery {
    ViewQuery {
        tab: tab.into(),
        filter: filter.into(),
        scope: UrlScope::All,
        search: String::new(),
        sort: None,
        segment: None,
        values: None,
    }
}

/// Path -> column id -> value, for every row of a view.
fn table(
    r: &mut Reader,
    c: &Crawled,
    tab: &str,
    filter: &str,
) -> BTreeMap<String, BTreeMap<&'static str, serde_json::Value>> {
    let t = tabs().iter().find(|t| t.id == tab).unwrap();
    let page = r.open_view(&q(tab, filter), 500).unwrap();
    let total = r.materialize(page.view).unwrap();
    let rows = r.rows(page.view, 0, 1000).unwrap().rows;
    assert_eq!(rows.len() as u32, total);
    r.close_view(page.view).unwrap();
    let base = c.site.url(E, "");
    rows.into_iter()
        .map(|row| {
            let url = row[1].as_str().unwrap().to_string();
            let path = url
                .strip_prefix(&base)
                .map_or(url.clone(), |p| format!("/{}", p.trim_start_matches('/')));
            let cols = t
                .columns
                .iter()
                .zip(&row[1..])
                .map(|(c, v)| (&*c.id, v.clone()))
                .collect();
            (path, cols)
        })
        .collect()
}

fn set(items: &[&str]) -> BTreeSet<String> {
    items.iter().map(|s| s.to_string()).collect()
}

fn num(v: &serde_json::Value) -> f64 {
    v.as_f64().unwrap_or_else(|| panic!("not a number: {v:?}"))
}

#[tokio::test]
async fn link_score_counts_and_links_tab() {
    let c = crawl().await;
    let mut r = Reader::open(&c.path).unwrap();
    assert!(r.features().has(Need::Analysis));
    let internal = table(&mut r, &c, "internal", "all");
    let score = |p: &str| num(&internal[p]["link_score"]);
    let int = |p: &str, col: &str| internal[p][col].as_i64().unwrap_or_else(|| panic!("{p} {col}"));

    // US1: the hub is the strongest page; every page linked only nofollow
    // gets nothing through that link.
    let top = internal
        .iter()
        .max_by(|a, b| num(&a.1["link_score"]).total_cmp(&num(&b.1["link_score"])))
        .unwrap();
    assert_eq!(top.0, "/hub");
    assert_eq!(score("/hub"), 100.0);
    assert_eq!(int("/hub", "link_rank"), 1);
    assert!(score("/hub") > score("/a") && score("/a") > score("/deep1"));
    assert_eq!(score("/nf"), 0.0, "nofollow passes nothing");
    // Redirects pass their value on (FR-003).
    assert!(score("/target") > score("/nf"));
    assert_eq!(int("/target", "followed_inlinks"), 0);

    // FR-006 counts: duplicates count once as unique, every time as all;
    // self links are not inlinks.
    assert_eq!(int("/hub", "unique_inlinks"), 4);
    assert_eq!(int("/hub", "all_inlinks"), 5);
    assert_eq!(int("/hub", "followed_inlinks"), 4);
    assert_eq!(int("/a", "unique_inlinks"), 2);
    assert_eq!(int("/nf", "unique_inlinks"), 1);
    assert_eq!(int("/nf", "followed_inlinks"), 0);
    assert_eq!(int("/a", "unique_outlinks"), 1);
    assert_eq!(int("/", "unique_outlinks"), 5);
    let pages = 10.0; // the internal HTML pages (/missing is a 404, /redir a redirect)
    assert!((num(&internal["/hub"]["pct_linking"]) - (4.0 * 100.0 / pages)).abs() < 0.051);

    // US3: Links tab filters (SC-007).
    let mut links = |f: &str| table(&mut r, &c, "links", f).into_keys().collect::<BTreeSet<_>>();
    assert_eq!(links("all").len(), 10);
    assert_eq!(links("depth_4"), set(&["/deep2", "/deep3"]));
    assert_eq!(links("no_followed"), set(&["/nf", "/target"]));
    assert_eq!(
        links("one_followed"),
        set(&["/", "/c", "/deep1", "/deep2", "/deep3"])
    );
    assert_eq!(links("no_outlinks"), set(&["/deep3", "/nf", "/target"]));
    assert_eq!(links("nofollow_outlinks"), set(&["/"]));

    // FR-008: the detail pane.
    let hub_id: i64 = rusqlite::Connection::open(&c.path)
        .unwrap()
        .query_row(
            "SELECT id FROM urls WHERE url = ?1",
            [c.site.url(E, "/hub")],
            |r| r.get(0),
        )
        .unwrap();
    let m = r.detail(hub_id).unwrap().link_metrics.expect("hub has metrics");
    assert_eq!((m.rank, m.of), (1, 12));
    assert_eq!((m.unique_inlinks, m.all_inlinks, m.unique_outlinks), (4, 5, 4));

    // FR-005: the same file gives the same scores.
    let again = table(&mut Reader::open(&c.path).unwrap(), &c, "internal", "all");
    for (p, cols) in &internal {
        assert_eq!(cols["link_score"], again[p]["link_score"], "{p}");
    }
}

#[tokio::test]
async fn running_crawl_shows_pending() {
    let c = crawl().await;
    let conn = rusqlite::Connection::open(&c.path).unwrap();
    conn.execute("UPDATE meta SET value = 'running' WHERE key = 'status'", [])
        .unwrap();
    drop(conn);
    let mut r = Reader::open(&c.path).unwrap();
    assert!(!r.features().has(Need::Analysis));
    // The column reads as no value, never 0, and a filter needing it says why.
    let internal = table(&mut r, &c, "internal", "all");
    assert!(internal["/hub"]["link_score"].is_null());
    let err = r.open_view(&q("links", "one_followed"), 10).err().unwrap();
    assert!(err.to_string().contains("when the crawl ends"), "{err}");
}
