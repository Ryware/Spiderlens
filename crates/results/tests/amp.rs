//! Spec 027 part E: AMP checks on a crawl of a fixture site, through the AMP
//! tab and the detail pane (SC-E1).

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::time::Duration;

use spiderlens_config::{CrawlConfig, ModeInput, SpeedSettings};
use spiderlens_engine::crawl::{Control, Crawler};
use spiderlens_results::{Need, Reader, UrlScope, ViewQuery, tabs};
use spiderlens_test_fixtures::{Site, SiteBuilder};
use tokio::sync::watch;

const E: &str = "example.test";

fn amp(canonical: Option<&str>, head: &str, body: &str) -> String {
    let canonical = canonical.map_or(String::new(), |c| {
        format!("<link rel=\"canonical\" href=\"{c}\">")
    });
    format!(
        "<!doctype html><html ⚡ lang=\"en\"><head><meta charset=\"utf-8\">\
         <script async src=\"https://cdn.ampproject.org/v0.js\"></script><title>AMP</title>{canonical}\
         <meta name=\"viewport\" content=\"width=device-width\">\
         <style amp-boilerplate>body{{visibility:hidden}}</style>{head}</head><body>{body}</body></html>"
    )
}

fn page(head: &str, links: &[&str]) -> String {
    let a: String = links.iter().map(|l| format!("<a href=\"{l}\">{l}</a>")).collect();
    format!("<!doctype html><html><head><title>Page</title>{head}</head><body>{a}</body></html>")
}

fn site() -> Site {
    let big = format!("<style amp-custom>{}</style>", "a".repeat(75_001));
    SiteBuilder::new()
        .html(
            E,
            "/",
            &page(
                r#"<link rel="canonical" href="/"><link rel="amphtml" href="/article.amp">"#,
                &[
                    "/script.amp",
                    "/bigcss.amp",
                    "/mail.amp",
                    "/other",
                    "/other2",
                    "/lonely",
                ],
            ),
        )
        .html(E, "/article.amp", &amp(Some("/"), "", "<p>Valid</p>"))
        .html(
            E,
            "/script.amp",
            &amp(Some("/lonely"), "", "<script>alert(1)</script><object></object>"),
        )
        .html(E, "/bigcss.amp", &amp(Some("/bigcss.amp"), &big, "<p>Big</p>"))
        .html(
            E,
            "/mail.amp",
            "<!doctype html><html ⚡4email><head><meta charset=\"utf-8\">\
             <script async src=\"https://cdn.ampproject.org/v0.js\"></script>\
             <style amp4email-boilerplate>body{visibility:hidden}</style></head><body>Mail</body></html>",
        )
        .html(
            E,
            "/other",
            &page(r#"<link rel="amphtml" href="/bare.amp">"#, &[]),
        )
        .html(
            E,
            "/bare.amp",
            "<!doctype html><html amp><head><title>Bare</title></head><body>x</body></html>",
        )
        .html(E, "/other2", &page(r#"<link rel="amphtml" href="/notamp">"#, &[]))
        .html(E, "/notamp", &page("", &[]))
        .html(E, "/lonely", &page("", &[]))
        .start()
}

struct Crawled {
    _dir: tempfile::TempDir,
    path: PathBuf,
    site: Site,
}

async fn crawl(check_amp: bool) -> Crawled {
    let site = site();
    let mut c = CrawlConfig {
        speed: SpeedSettings::FAST,
        ..CrawlConfig::default()
    };
    c.extraction.check_amp = check_amp;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("amp.spiderlens");
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
    }
}

fn paths(r: &mut Reader, c: &Crawled, tab: &str, filter: &str) -> BTreeSet<String> {
    let page = r.open_view(&q(tab, filter), 500).unwrap();
    r.materialize(page.view).unwrap();
    let rows = r.rows(page.view, 0, 1000).unwrap().rows;
    r.close_view(page.view).unwrap();
    let base = c.site.url(E, "");
    rows.iter()
        .map(|row| row[1].as_str().unwrap().strip_prefix(&base).unwrap().to_string())
        .collect()
}

fn set(items: &[&str]) -> BTreeSet<String> {
    items.iter().map(|s| s.to_string()).collect()
}

fn id_of(r: &mut Reader, c: &Crawled, path: &str) -> i64 {
    let page = r.open_view(&q("amp", "all"), 500).unwrap();
    let want = c.site.url(E, path);
    page.rows
        .iter()
        .find(|row| row[1].as_str() == Some(want.as_str()))
        .map(|row| row[0].as_i64().unwrap())
        .unwrap_or_else(|| panic!("{path} not in the AMP tab"))
}

#[tokio::test]
async fn amp_filters_pass_and_fail_exactly_where_expected() {
    let c = crawl(true).await;
    let mut r = Reader::open(&c.path).unwrap();
    assert!(r.features().has(Need::Amp));
    let p = |r: &mut Reader, f: &str| paths(r, &c, "amp", f);
    assert_eq!(
        p(&mut r, "all"),
        set(&[
            "/article.amp",
            "/bare.amp",
            "/bigcss.amp",
            "/mail.amp",
            "/notamp",
            "/script.amp"
        ])
    );
    assert_eq!(
        p(&mut r, "failing"),
        set(&["/bare.amp", "/bigcss.amp", "/script.amp"])
    );
    assert_eq!(p(&mut r, "missing_marker"), set(&["/notamp"]));
    for f in ["runtime", "boilerplate", "viewport", "canonical"] {
        assert_eq!(p(&mut r, f), set(&["/bare.amp"]), "{f}");
    }
    assert_eq!(p(&mut r, "unpaired"), set(&["/script.amp"]));
    assert_eq!(p(&mut r, "author_script"), set(&["/script.amp"]));
    assert_eq!(p(&mut r, "disallowed"), set(&["/script.amp"]));
    assert_eq!(p(&mut r, "style_size"), set(&["/bigcss.amp"]));

    // Every count equals the rows it lists.
    let tab = tabs().iter().find(|t| t.id == "amp").unwrap();
    let counts = r.counts("amp", UrlScope::All).unwrap();
    for f in &tab.filters {
        assert_eq!(
            counts.counts[f.id],
            Some(p(&mut r, f.id).len() as u64),
            "{}",
            f.id
        );
    }
}

#[tokio::test]
async fn details_list_failed_checks_with_positions_and_the_pairing() {
    let c = crawl(true).await;
    let mut r = Reader::open(&c.path).unwrap();

    let ok = id_of(&mut r, &c, "/article.amp");
    let d = r.detail(ok).unwrap().amp.unwrap();
    assert_eq!(d.format, "website");
    assert!(d.failures.is_empty());
    assert_eq!(d.paired, Some(true));

    let bad = id_of(&mut r, &c, "/script.amp");
    let d = r.detail(bad).unwrap().amp.unwrap();
    let checks: Vec<&str> = d.failures.iter().map(|f| f.check.as_str()).collect();
    assert_eq!(checks, ["author_script", "disallowed"]);
    assert!(d.failures.iter().all(|f| f.pos.is_some()));
    assert_eq!(d.paired, Some(false));

    let mail = id_of(&mut r, &c, "/mail.amp");
    let d = r.detail(mail).unwrap().amp.unwrap();
    assert_eq!((d.format.as_str(), d.paired), ("email", None));

    let not = id_of(&mut r, &c, "/notamp");
    assert!(r.detail(not).unwrap().amp.is_none());
}

#[tokio::test]
async fn with_amp_off_amphtml_links_are_not_followed() {
    let c = crawl(false).await;
    let mut r = Reader::open(&c.path).unwrap();
    assert!(!r.features().has(Need::Amp));
    let all = paths(&mut r, &c, "internal", "all");
    // Only reachable through amphtml links.
    for p in ["/article.amp", "/bare.amp", "/notamp"] {
        assert!(!all.contains(p), "{p} was crawled");
    }
    assert!(all.contains("/script.amp"));
    let counts = r.counts("amp", UrlScope::All).unwrap();
    assert_eq!(counts.counts["failing"], None);
    assert!(counts.unavailable["failing"].contains("Check AMP"));
    assert_eq!(counts.counts["all"], None);
    assert!(r.open_view(&q("amp", "all"), 10).is_err());
    let id = {
        let page = r.open_view(&q("internal", "all"), 500).unwrap();
        let want = c.site.url(E, "/script.amp");
        page.rows
            .iter()
            .find(|row| row[1].as_str() == Some(want.as_str()))
            .unwrap()[0]
            .as_i64()
            .unwrap()
    };
    assert!(r.detail(id).unwrap().amp.is_none());
}
