//! Spec 023 US4 on a real crawl: link positions, the rules tester, the
//! Links tab filter and position-weighted Link Score.

use std::path::PathBuf;
use std::time::Duration;

use spiderlens_config::{
    CrawlConfig, LinkAnalysisSettings, ModeInput, Position, PositionRule, PositionWeights, SpeedSettings,
};
use spiderlens_engine::crawl::{Control, Crawler};
use spiderlens_results::{ListKind, Reader, UrlScope, ViewQuery};
use spiderlens_test_fixtures::{Site, SiteBuilder};
use tokio::sync::watch;

const E: &str = "example.test";

/// Every page shares a header, a navigation menu and a footer. /story is
/// linked from the home page's content, /legal only from the footer.
fn page(content: &str) -> String {
    format!(
        r#"<!doctype html><html><head><title>A page</title></head><body>
        <header><a href="/">Home</a><nav><a href="/news">News</a></nav></header>
        <div class="mega-menu"><a href="/promo">Promo</a></div>
        <main>{content}</main>
        <footer><a href="/legal">Legal</a></footer></body></html>"#
    )
}

fn site() -> Site {
    SiteBuilder::new()
        .html(E, "/", &page(r#"<p><a href="/story">Read the story</a></p>"#))
        .html(E, "/news", &page(r#"<p><a href="/story">Story</a></p>"#))
        .html(E, "/story", &page("<p>Text</p>"))
        .html(E, "/promo", &page("<p>Promo</p>"))
        .html(E, "/legal", &page("<p>Legal</p>"))
        .start()
}

struct Crawled {
    _dir: tempfile::TempDir,
    path: PathBuf,
    site: Site,
}

async fn crawl(store_html: bool) -> Crawled {
    let site = site();
    let mut c = CrawlConfig {
        speed: SpeedSettings::FAST,
        ..CrawlConfig::default()
    };
    c.extraction.store_html = store_html;
    c.links.position_rules = vec![PositionRule {
        selector: ".mega-menu".into(),
        position: Position::Navigation,
    }];
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("positions.spiderlens");
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

fn id(c: &Crawled, path: &str) -> i64 {
    rusqlite::Connection::open(&c.path)
        .unwrap()
        .query_row("SELECT id FROM urls WHERE url = ?1", [c.site.url(E, path)], |r| {
            r.get(0)
        })
        .unwrap()
}

fn paths(r: &mut Reader, c: &Crawled, filter: &str) -> Vec<String> {
    let page = r
        .open_view(
            &ViewQuery {
                tab: "links".into(),
                filter: filter.into(),
                scope: UrlScope::All,
                search: String::new(),
                sort: None,
            },
            100,
        )
        .unwrap();
    r.materialize(page.view).unwrap();
    let rows = r.rows(page.view, 0, 100).unwrap().rows;
    let base = c.site.url(E, "");
    let mut out: Vec<String> = rows
        .iter()
        .map(|row| {
            format!(
                "/{}",
                row[1]
                    .as_str()
                    .unwrap()
                    .strip_prefix(&base)
                    .unwrap()
                    .trim_start_matches('/')
            )
        })
        .collect();
    out.sort();
    out
}

fn score(r: &mut Reader, c: &Crawled, path: &str) -> f64 {
    r.detail(id(c, path)).unwrap().link_metrics.unwrap().score
}

#[tokio::test]
async fn positions_filter_tester_and_weighting() {
    let c = crawl(true).await;
    let mut r = Reader::open(&c.path).unwrap();
    // Rules travel with the crawl file (FR-021).
    assert_eq!(r.link_settings().position_rules.len(), 1);

    // Positions are shown once worked out, never guessed before.
    let home = id(&c, "/");
    let before = r.list(home, ListKind::Outlinks, 0, 100).unwrap();
    assert!(before.rows.iter().all(|l| l.position.is_none()));
    let run = r.analyse().unwrap();
    assert_eq!(run.pages_without_html, 0);
    assert!(!run.weighted);
    let out = r.list(home, ListKind::Outlinks, 0, 100).unwrap();
    let by_raw = |raw: &str| out.rows.iter().find(|l| l.raw == raw).unwrap().position.clone();
    assert_eq!(by_raw("/").as_deref(), Some("header"));
    assert_eq!(by_raw("/news").as_deref(), Some("navigation"));
    assert_eq!(by_raw("/promo").as_deref(), Some("navigation"), "custom rule");
    assert_eq!(by_raw("/story").as_deref(), Some("content"));
    assert_eq!(by_raw("/legal").as_deref(), Some("footer"));

    // US4 filter: every page but /story is linked only from boilerplate.
    assert_eq!(
        paths(&mut r, &c, "boilerplate_only"),
        ["/", "/legal", "/news", "/promo"]
    );

    // FR-022: the tester shows each link's position and rule, writes nothing.
    let tested = r.test_positions(home, &[]).unwrap().unwrap();
    let promo = tested.iter().find(|l| l.raw == "/promo").unwrap();
    assert_eq!((promo.position, promo.rule), (Position::Content, None));
    let tested = r
        .test_positions(home, &r.link_settings().position_rules.clone())
        .unwrap()
        .unwrap();
    let promo = tested.iter().find(|l| l.raw == "/promo").unwrap();
    assert_eq!((promo.position, promo.rule), (Position::Navigation, Some(0)));

    // FR-025: with weighting on, the footer-only page drops, the content
    // page gains; changing settings recalculates (FR-023).
    let legal = score(&mut r, &c, "/legal");
    let story = score(&mut r, &c, "/story");
    assert!((legal - score(&mut r, &c, "/promo")).abs() < 1e-9);
    r.set_link_settings(LinkAnalysisSettings {
        weighting: Some(PositionWeights::default()),
        ..r.link_settings().clone()
    });
    assert!(score(&mut r, &c, "/legal") < legal);
    assert!(score(&mut r, &c, "/story") >= story);
    assert!(
        score(&mut r, &c, "/legal") < score(&mut r, &c, "/promo"),
        "footer 0.2 < navigation 0.3"
    );
    assert!(r.analyse().unwrap().weighted);
}

#[tokio::test]
async fn pages_without_html_are_unknown() {
    let c = crawl(false).await;
    let mut r = Reader::open(&c.path).unwrap();
    let run = r.analyse().unwrap();
    assert_eq!(run.pages_without_html, 5);
    let out = r.list(id(&c, "/"), ListKind::Outlinks, 0, 100).unwrap();
    assert!(
        out.rows
            .iter()
            .filter(|l| l.kind == "anchor")
            .all(|l| l.position.as_deref() == Some("unknown"))
    );
    assert!(r.test_positions(id(&c, "/"), &[]).unwrap().is_none());
    // Unknown links count as content for the filter.
    assert!(paths(&mut r, &c, "boilerplate_only").is_empty());
}
