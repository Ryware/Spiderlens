//! Spec 014 T028: the existing tabs show rendered values on a rendered
//! crawl, and raw values on request (FR-021). Crawls the user story 1
//! fixture with a browser; skipped when none is found, unless
//! `SPIDERLENS_REQUIRE_BROWSER=1`.

use std::path::PathBuf;

use spiderlens_config::{CrawlConfig, ModeInput, RenderMode, SpeedSettings};
use spiderlens_engine::crawl::{Control, Crawler};
use spiderlens_render::{LaunchOptions, browser_for_tests};
use spiderlens_results::{Need, Reader, UrlScope, Values, ViewQuery, tabs};
use spiderlens_test_fixtures::js_site::{JS_HOST, JsSite, JsSiteBuilder};
use tokio::sync::watch;

struct Crawled {
    site: JsSite,
    path: PathBuf,
    _dir: tempfile::TempDir,
}

async fn crawl(mode: RenderMode) -> Option<Crawled> {
    let browser = browser_for_tests().expect("browser for tests")?;
    let site = JsSiteBuilder::new().us1_pages().start();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("us1.spiderlens");
    let mut c = CrawlConfig {
        speed: SpeedSettings::FAST,
        ..CrawlConfig::default()
    };
    c.rendering.mode = mode;
    c.rendering.concurrency = Some(2);
    let crawler = Crawler::create(
        &path,
        c,
        &ModeInput::Spider {
            start_url: site.url(JS_HOST, "/"),
        },
    )
    .unwrap()
    .with_resolve(site.resolve())
    .with_browser(browser)
    .with_render_launch(LaunchOptions::default());
    let (_tx, rx) = watch::channel(Control::Run);
    assert_eq!(crawler.run(rx, None).await.unwrap().status, "completed");
    Some(Crawled {
        site,
        path,
        _dir: dir,
    })
}

fn q(tab: &str, filter: &str, values: Option<Values>) -> ViewQuery {
    ViewQuery {
        tab: tab.into(),
        filter: filter.into(),
        scope: UrlScope::All,
        search: String::new(),
        sort: None,
        values,
    }
}

/// The value of `column` on the row of `path`, or None when the row is not listed.
fn cell(
    r: &mut Reader,
    site: &JsSite,
    query: &ViewQuery,
    path: &str,
    column: &str,
) -> Option<serde_json::Value> {
    let t = tabs().iter().find(|t| t.id == query.tab).unwrap();
    let i = t.columns.iter().position(|c| c.id == column).unwrap() + 1;
    let page = r.open_view(query, 500).unwrap();
    r.materialize(page.view).unwrap();
    let rows = r.rows(page.view, 0, 1000).unwrap().rows;
    r.close_view(page.view).unwrap();
    let want = site.url(JS_HOST, path);
    rows.into_iter()
        .find(|row| row[1].as_str() == Some(want.as_str()))
        .map(|row| row[i].clone())
}

fn text(v: Option<serde_json::Value>) -> String {
    v.expect("row listed").as_str().unwrap_or_default().to_string()
}

fn int(v: Option<serde_json::Value>) -> i64 {
    v.expect("row listed").as_i64().unwrap()
}

fn noindex_count(r: &mut Reader, values: Option<Values>) -> u64 {
    r.counts_with("directives", UrlScope::All, values).unwrap().counts["noindex"].unwrap()
}

#[tokio::test]
async fn tabs_show_rendered_values_by_default_and_raw_on_request() {
    let Some(c) = crawl(RenderMode::Javascript).await else {
        return;
    };
    let mut r = Reader::open(&c.path).unwrap();
    assert!(r.features().has(Need::Rendering));
    let s = &c.site;
    let rendered = Some(Values::Rendered);
    let raw = Some(Values::Raw);

    // US1 scenario 1: the Page titles tab.
    for v in [None, rendered] {
        assert_eq!(
            text(cell(
                &mut r,
                s,
                &q("page-titles", "all", v),
                "/title-changed",
                "title"
            )),
            "Red Shoes | Shop"
        );
        assert_eq!(
            text(cell(
                &mut r,
                s,
                &q("page-titles", "all", v),
                "/title-changed",
                "values"
            )),
            "Rendered"
        );
    }
    assert_eq!(
        text(cell(
            &mut r,
            s,
            &q("page-titles", "all", raw),
            "/title-changed",
            "title"
        )),
        "Loading"
    );
    assert_eq!(
        text(cell(
            &mut r,
            s,
            &q("page-titles", "all", raw),
            "/title-changed",
            "values"
        )),
        "Raw"
    );
    // Search reads the shown values.
    let mut search = q("page-titles", "all", rendered);
    search.search = "red shoes".into();
    assert!(cell(&mut r, s, &search, "/title-changed", "title").is_some());
    search.values = raw;
    assert!(cell(&mut r, s, &search, "/title-changed", "title").is_none());

    // Scenario 4: a noindex added by a script makes the page non-indexable.
    let idx = |r: &mut Reader, v| {
        text(cell(
            r,
            s,
            &q("internal", "all", v),
            "/noindex-added",
            "indexability",
        ))
    };
    assert!(
        idx(&mut r, rendered).starts_with("Non-indexable"),
        "{}",
        idx(&mut r, rendered)
    );
    assert_eq!(idx(&mut r, raw), "Indexable");
    assert_eq!(noindex_count(&mut r, rendered), 1);
    assert_eq!(noindex_count(&mut r, None), 1);
    assert_eq!(noindex_count(&mut r, raw), 0);

    // Scenario 2: links a script inserted count only in rendered mode.
    let out = |r: &mut Reader, v| int(cell(r, s, &q("internal", "all", v), "/js-nav", "outlinks"));
    assert_eq!(out(&mut r, rendered), 2);
    assert_eq!(out(&mut r, raw), 0);
    let inl = |r: &mut Reader, v| int(cell(r, s, &q("internal", "all", v), "/only-js-1", "inlinks"));
    assert_eq!(inl(&mut r, rendered), 1);
    assert_eq!(inl(&mut r, raw), 0);

    // A page without a rendered version keeps its raw values in rendered mode.
    assert_eq!(
        text(cell(
            &mut r,
            s,
            &q("page-titles", "all", rendered),
            "/only-js-1",
            "title"
        )),
        "Only JS 1"
    );
}

#[tokio::test]
async fn a_raw_crawl_always_shows_raw_values() {
    let Some(c) = crawl(RenderMode::Raw).await else {
        return;
    };
    let mut r = Reader::open(&c.path).unwrap();
    assert!(!r.features().has(Need::Rendering));
    let s = &c.site;
    for v in [None, Some(Values::Rendered)] {
        assert_eq!(
            text(cell(
                &mut r,
                s,
                &q("page-titles", "all", v),
                "/title-changed",
                "title"
            )),
            "Loading"
        );
        assert_eq!(
            text(cell(
                &mut r,
                s,
                &q("page-titles", "all", v),
                "/title-changed",
                "values"
            )),
            "Raw"
        );
    }
    assert_eq!(noindex_count(&mut r, Some(Values::Rendered)), 0);
}
