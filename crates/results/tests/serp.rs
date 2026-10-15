//! Spec 027 part A: title and description pixel widths in the Page titles
//! and Meta description tabs (FR-A01, FR-A02).

use std::collections::BTreeSet;
use std::time::Duration;

use spiderlens_config::{CrawlConfig, ModeInput, SpeedSettings};
use spiderlens_engine::crawl::{Control, Crawler};
use spiderlens_results::serp::{SerpStyle, px_width};
use spiderlens_results::{Reader, UrlScope, ViewQuery, tabs};
use spiderlens_test_fixtures::{Site, SiteBuilder};
use tokio::sync::watch;

const E: &str = "example.test";

/// Wide capitals: 25 W's are about 470 px, 33 about 620 px.
const WIDE: &str = "WWWWWWWWWWWWWWWWWWWWWWWWWWWWWWWWW";
/// Narrow letters: 60 characters, well under 600 px.
const NARROW: &str = "illi illi illi illi illi illi illi illi illi illi illi illi";
/// Between the mobile (580) and desktop (600) title limits.
const BETWEEN: &str = "Running Shoes For Road And Trail With Free Returns Policy Today";
const SHORT: &str = "Shoes";
const DESC_BETWEEN: &str = "Shop lightweight running shoes for road and trail with free delivery, easy \
     returns within thirty days and a price match on every pair we sell";

fn page(title: &str, desc: &str) -> String {
    format!(
        "<!doctype html><html><head><title>{title}</title><meta name=\"description\" content=\"{desc}\"></head><body><h1>x</h1></body></html>"
    )
}

struct Crawled {
    _dir: tempfile::TempDir,
    path: std::path::PathBuf,
    site: Site,
}

async fn crawl() -> Crawled {
    let ok_desc = "A description of a sensible length for the page.";
    let links: String = ["/wide", "/narrow", "/between", "/short"]
        .iter()
        .map(|p| format!("<a href=\"{p}\">{p}</a>"))
        .collect();
    let site = SiteBuilder::new()
        .html(
            E,
            "/",
            &page("The home page of our running shoe shop", ok_desc)
                .replace("</body>", &format!("{links}</body>")),
        )
        .html(E, "/wide", &page(WIDE, ok_desc))
        .html(E, "/narrow", &page(NARROW, DESC_BETWEEN))
        .html(E, "/between", &page(BETWEEN, ok_desc))
        .html(E, "/short", &page(SHORT, ok_desc))
        .start();
    let c = CrawlConfig {
        speed: SpeedSettings::FAST,
        ..CrawlConfig::default()
    };
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("serp.spiderlens");
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

fn rows(r: &mut Reader, c: &Crawled, query: &ViewQuery) -> Vec<(String, Vec<serde_json::Value>)> {
    let page = r.open_view(query, 500).unwrap();
    r.materialize(page.view).unwrap();
    let rows = r.rows(page.view, 0, 1000).unwrap().rows;
    r.close_view(page.view).unwrap();
    let base = c.site.url(E, "");
    rows.into_iter()
        .map(|row| {
            let url = row[1].as_str().unwrap().to_string();
            (
                url.strip_prefix(&base).unwrap_or(&url).to_string(),
                row[1..].to_vec(),
            )
        })
        .collect()
}

fn paths(r: &mut Reader, c: &Crawled, query: &ViewQuery) -> BTreeSet<String> {
    rows(r, c, query).into_iter().map(|(p, _)| p).collect()
}

fn set(items: &[&str]) -> BTreeSet<String> {
    items.iter().map(|s| s.to_string()).collect()
}

fn cell(r: &mut Reader, c: &Crawled, tab: &str, path: &str, column: &str) -> serde_json::Value {
    let t = tabs().iter().find(|t| t.id == tab).unwrap();
    let i = t.columns.iter().position(|col| col.id == column).unwrap();
    rows(r, c, &q(tab, "all"))
        .into_iter()
        .find(|(p, _)| p == path)
        .unwrap_or_else(|| panic!("{path} not in {tab}"))
        .1[i]
        .clone()
}

#[test]
fn fixture_titles_sit_where_the_test_needs_them() {
    let w = |t| px_width(t, SerpStyle::Title);
    assert!(w(WIDE) > 600, "{}", w(WIDE));
    assert!(w(NARROW) < 580, "{}", w(NARROW));
    assert!(w(BETWEEN) > 580 && w(BETWEEN) <= 600, "{}", w(BETWEEN));
    assert!(w(SHORT) < 200);
    let d = px_width(DESC_BETWEEN, SerpStyle::Description);
    assert!(d > 680 && d <= 990, "{d}");
}

#[tokio::test(flavor = "multi_thread")]
async fn pixel_columns_and_filters() {
    let c = crawl().await;
    let mut r = Reader::open(&c.path).unwrap();

    assert_eq!(
        cell(&mut r, &c, "page-titles", "/wide", "title_px"),
        px_width(WIDE, SerpStyle::Title)
    );
    assert_eq!(cell(&mut r, &c, "page-titles", "/wide", "title_cut_desktop"), 1);
    assert_eq!(
        cell(&mut r, &c, "page-titles", "/between", "title_cut_desktop"),
        0
    );
    assert_eq!(cell(&mut r, &c, "page-titles", "/between", "title_cut_mobile"), 1);

    assert_eq!(
        paths(&mut r, &c, &q("page-titles", "px_over_desktop")),
        set(&["/wide"])
    );
    assert_eq!(
        paths(&mut r, &c, &q("page-titles", "px_over_mobile")),
        set(&["/wide", "/between"])
    );
    assert_eq!(paths(&mut r, &c, &q("page-titles", "px_below")), set(&["/short"]));
    // Character length alone would not catch the wide title.
    assert!(!paths(&mut r, &c, &q("page-titles", "over_60")).contains("/wide"));

    assert_eq!(
        cell(&mut r, &c, "meta-description", "/narrow", "desc_px"),
        px_width(DESC_BETWEEN, SerpStyle::Description)
    );
    assert!(paths(&mut r, &c, &q("meta-description", "px_over_desktop")).is_empty());
    assert_eq!(
        paths(&mut r, &c, &q("meta-description", "px_over_mobile")),
        set(&["/narrow"])
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn pixel_columns_sort() {
    let c = crawl().await;
    let mut r = Reader::open(&c.path).unwrap();
    let mut query = q("page-titles", "all");
    query.sort = Some(spiderlens_results::Sort {
        column: "title_px".into(),
        desc: true,
    });
    let order: Vec<String> = rows(&mut r, &c, &query).into_iter().map(|(p, _)| p).collect();
    assert_eq!(order.first().map(String::as_str), Some("/wide"));
    assert_eq!(order.last().map(String::as_str), Some("/short"));
}
