//! Spec 026 full accessibility checks and mobile measures in the render
//! stage (T047). Needs a browser: skipped when none is found, unless
//! `SPIDERLENS_REQUIRE_BROWSER=1`.

use spiderlens_config::{A11yLevel, CrawlConfig, ModeInput, RenderMode, SpeedSettings, WindowPreset};
use spiderlens_engine::crawl::{Control, CrawlStore, Crawler};
use spiderlens_render::{LaunchOptions, browser_for_tests};
use spiderlens_test_fixtures::a11y_site::{
    CONTRAST_OVER_IMAGE, FULL_PAGES, FullPage, MOBILE_PAGES, SCRIPT_ADDED,
};
use spiderlens_test_fixtures::js_site::{JS_HOST, JsSite, JsSiteBuilder};
use tokio::sync::watch;

/// The full-check fixtures this test crawls.
fn full_page(path: &str) -> &'static FullPage {
    FULL_PAGES
        .iter()
        .find(|p| p.path == path)
        .unwrap_or_else(|| panic!("no fixture {path}"))
}

fn site() -> JsSite {
    let mut pages: Vec<(&str, &str)> = vec![
        (full_page("/full/color-contrast").path, full_page("/full/color-contrast").html),
        (full_page("/full/color-contrast-ok").path, full_page("/full/color-contrast-ok").html),
        (full_page("/full/label").path, full_page("/full/label").html),
        (full_page("/full/label-ok").path, full_page("/full/label-ok").html),
        (SCRIPT_ADDED.path, SCRIPT_ADDED.html),
        (CONTRAST_OVER_IMAGE.path, CONTRAST_OVER_IMAGE.html),
    ];
    for m in MOBILE_PAGES {
        pages.push((m.path, m.html));
    }
    let links: String = pages
        .iter()
        .map(|(path, _)| format!("<a href=\"{path}\">{path}</a> "))
        .collect();
    let mut b = JsSiteBuilder::new().page(
        "/",
        &format!(
            "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\">\
             <meta name=\"viewport\" content=\"width=device-width\"><title>Index</title></head>\
             <body><main><h1>Index</h1>{links}</main></body></html>"
        ),
    );
    for (path, html) in pages {
        b = b.page(path, html);
    }
    b.start()
}

fn one<T: rusqlite::types::FromSql>(store: &CrawlStore, sql: &str, id: i64) -> T {
    store.conn().query_row(sql, [id], |r| r.get(0)).unwrap()
}

fn full_results(store: &CrawlStore, id: i64) -> Vec<(String, String)> {
    let mut stmt = store
        .conn()
        .prepare("SELECT check_id, kind FROM a11y_results WHERE url_id = ?1 AND source = 'full' ORDER BY check_id")
        .unwrap();
    stmt.query_map([id], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .map(|r| r.unwrap())
        .collect()
}

fn has(results: &[(String, String)], check: &str, kind: &str) -> bool {
    results.iter().any(|(c, k)| c == check && k == kind)
}

#[tokio::test(flavor = "multi_thread")]
async fn full_checks_and_mobile_measures_are_stored_for_rendered_pages() {
    let Some(browser) = browser_for_tests().expect("browser for tests") else {
        return;
    };
    let site = site();
    let mut c = CrawlConfig {
        speed: SpeedSettings::FAST,
        ..CrawlConfig::default()
    };
    c.rendering.mode = RenderMode::Javascript;
    c.rendering.concurrency = Some(2);
    c.rendering.window.preset = WindowPreset::Mobile;
    c.accessibility.level = A11yLevel::Full;
    c.accessibility.mobile = true;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a11y-render.spiderlens");
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
    let status = crawler.run(rx, None).await.unwrap().status;
    assert_eq!(status, "completed");

    let store = CrawlStore::open_read_only(&path).unwrap();
    let id = |p: &str| store.id_of(&site.url(JS_HOST, p)).unwrap().expect(p);

    let features = store.meta("data_features").unwrap().unwrap();
    for f in ["accessibility_static", "accessibility_full", "mobile_layout"] {
        assert!(features.split(',').any(|x| x == f), "{f} in {features}");
    }
    let engine = store.meta("a11y_engine").unwrap().unwrap();
    assert!(engine.contains("axe-core"), "{engine}");

    // Contrast: the pale text fails, the dark text passes.
    let bad = id("/full/color-contrast");
    assert_eq!(one::<String>(&store, "SELECT full_state FROM a11y_audits WHERE url_id = ?1", bad), "checked");
    assert!(has(&full_results(&store, bad), "color-contrast", "violation"));
    let nodes: i64 = one(
        &store,
        "SELECT COUNT(*) FROM a11y_nodes WHERE result_id IN \
         (SELECT id FROM a11y_results WHERE url_id = ?1 AND check_id = 'color-contrast')",
        bad,
    );
    assert!(nodes >= 1);
    let good = id("/full/color-contrast-ok");
    assert!(!has(&full_results(&store, good), "color-contrast", "violation"));

    // Labels, and the full result replaces the static one for the same field.
    let label = id("/full/label");
    assert!(has(&full_results(&store, label), "label", "violation"));
    let static_label: i64 = one(
        &store,
        "SELECT COUNT(*) FROM a11y_results WHERE url_id = ?1 AND check_id = 'static.form_label'",
        label,
    );
    assert_eq!(static_label, 0, "the full label result replaces static.form_label");
    let label_ok = id("/full/label-ok");
    assert!(!has(&full_results(&store, label_ok), "label", "violation"));

    // An input added by a script after load is seen by the full checks only.
    let late = id("/full/script-added");
    assert!(has(&full_results(&store, late), "label", "violation"));

    // Text over an image cannot be decided: a review, not a violation.
    let over = id("/full/contrast-over-image");
    let results = full_results(&store, over);
    assert!(!has(&results, "color-contrast", "violation"), "{results:?}");
    assert!(has(&results, "color-contrast", "needs_review"), "{results:?}");

    // Mobile measures.
    #[allow(clippy::type_complexity)]
    let measures = |p: &str| -> (String, String, f64, f64, f64, i64) {
        store
            .conn()
            .query_row(
                "SELECT \"window\", measured, content_px, window_px, small_text_share, small_targets \
                 FROM mobile_measures WHERE url_id = ?1",
                [id(p)],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?)),
            )
            .unwrap_or_else(|e| panic!("{p}: {e}"))
    };
    let wider = measures("/mobile/content-wider");
    assert_eq!((wider.0.as_str(), wider.1.as_str()), ("mobile", "yes"));
    assert!(wider.3 <= 480.0, "the mobile window is narrow: {}", wider.3);
    assert!(wider.2 > wider.3 + 5.0, "content {} in a window of {}", wider.2, wider.3);

    let small = measures("/mobile/small-text");
    assert_eq!(small.1, "yes");
    assert!(small.4 > 40.0, "small text share {}", small.4);

    let targets = measures("/mobile/small-targets");
    assert_eq!(targets.1, "yes");
    assert!(targets.5 >= 1, "small targets {}", targets.5);

    let ok = measures("/mobile/ok");
    assert_eq!(ok.1, "yes");
    assert!(ok.2 <= ok.3 + 5.0, "content {} in a window of {}", ok.2, ok.3);
    assert!(ok.4 < 40.0, "small text share {}", ok.4);
    assert_eq!(ok.5, 0);
}
