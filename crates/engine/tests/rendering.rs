//! The render stage inside a crawl (spec 014 T019). Needs a browser: skipped
//! when none is found, unless `SPIDERLENS_REQUIRE_BROWSER=1`.
//!
//! The first tests queue pages through the store, as a later render of a
//! finished crawl would; the others render from the fetch stage (T021).

use std::path::Path;

use spiderlens_config::{
    Change, CrawlConfig, ModeInput, RenderBlock, RenderMode, RenderWindow, RobotsMode, SpeedSettings,
    UaPreset, WindowPreset,
};
use spiderlens_engine::crawl::render::progress::{RenderLimiting, RenderProgress};
use spiderlens_engine::crawl::{
    Control, CrawlEvent, CrawlStore, CrawlSummary, Crawler, Progress, ResumeOptions,
};
use spiderlens_render::{BrowserInfo, LaunchOptions, browser_for_tests};
use spiderlens_test_fixtures::js_site::{JS_HOST, JsSite, JsSiteBuilder};
use std::time::Duration;

use tokio::sync::{mpsc, watch};

/// Tests run one at a time: the raw-crawl test reads the process-wide
/// browser launch counter.
static SERIAL: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

fn browser() -> Option<BrowserInfo> {
    browser_for_tests().expect("browser for tests")
}

/// Three pages linked in the raw HTML; a script changes each title and adds
/// a heading.
fn site() -> JsSite {
    let page = |links: &str| {
        format!(
            "<!doctype html><html><head><title>Loading</title><script src=\"/app.js\"></script></head>\
             <body><p>Raw words here.</p>{links}</body></html>"
        )
    };
    JsSiteBuilder::new()
        .page("/", &page("<a href=\"/a\">A</a><a href=\"/b\">B</a>"))
        .page("/a", &page(""))
        .page("/b", &page(""))
        .script(
            JS_HOST,
            "/app.js",
            "document.addEventListener('DOMContentLoaded', () => {\
               document.title = 'Rendered ' + location.pathname;\
               document.body.insertAdjacentHTML('beforeend', '<h1>Hello from JavaScript</h1>');\
             });",
        )
        .cache_control(JS_HOST, "/app.js", "max-age=600")
        .start()
}

fn config(store_html: bool) -> CrawlConfig {
    let mut c = CrawlConfig {
        speed: SpeedSettings::FAST,
        ..CrawlConfig::default()
    };
    c.extraction.store_html = store_html;
    c
}

/// Crawls raw, then queues every crawled HTML page for rendering.
async fn crawl_then_queue(site: &JsSite, path: &Path, store_html: bool) -> Vec<i64> {
    let crawler = Crawler::create(
        path,
        config(store_html),
        &ModeInput::Spider {
            start_url: site.url(JS_HOST, "/"),
        },
    )
    .unwrap()
    .with_resolve(site.resolve());
    let (_tx, rx) = watch::channel(Control::Run);
    let s = crawler.run(rx, None).await.unwrap();
    assert_eq!(s.status, "completed");
    let store = CrawlStore::open(path).unwrap();
    let ids: Vec<i64> = ["/", "/a", "/b"]
        .iter()
        .map(|p| store.id_of(&site.url(JS_HOST, p)).unwrap().expect("crawled"))
        .collect();
    for id in &ids {
        store.enqueue_render(*id, None).unwrap();
    }
    ids
}

async fn render(site: &JsSite, path: &Path, browser: BrowserInfo) -> String {
    render_with(site, path, browser, Some(2)).await
}

async fn render_with(site: &JsSite, path: &Path, browser: BrowserInfo, concurrency: Option<u32>) -> String {
    let mut settings = CrawlConfig::default().rendering;
    settings.mode = RenderMode::Javascript;
    settings.concurrency = concurrency;
    let crawler = Crawler::open_with(
        path,
        &ResumeOptions {
            changes: vec![Change::Rendering(settings)],
            ..ResumeOptions::default()
        },
    )
    .unwrap()
    .with_resolve(site.resolve())
    .with_browser(browser)
    .with_render_launch(LaunchOptions::default());
    let (_tx, rx) = watch::channel(Control::Run);
    crawler.run(rx, None).await.unwrap().status
}

fn one<T: rusqlite::types::FromSql>(store: &CrawlStore, sql: &str, id: i64) -> T {
    store.conn_for_tests().query_row(sql, [id], |r| r.get(0)).unwrap()
}

#[tokio::test]
async fn queued_pages_are_rendered_and_the_crawl_completes_after_the_renders() {
    let _serial = SERIAL.lock().await;
    let Some(browser) = browser() else { return };
    let site = site();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("render.spiderlens");
    let ids = crawl_then_queue(&site, &path, true).await;
    let main_requests = site.count(JS_HOST, "/a");

    assert_eq!(render(&site, &path, browser).await, "completed");

    let store = CrawlStore::open_read_only(&path).unwrap();
    for (id, p) in ids.iter().zip(["/", "/a", "/b"]) {
        let state: String = one(&store, "SELECT state FROM renders WHERE url_id = ?1", *id);
        assert_eq!(state, "rendered", "{p}");
        let title: String = one(&store, "SELECT title FROM rendered_pages WHERE url_id = ?1", *id);
        assert_eq!(title, format!("Rendered {p}"));
        let raw_title: String = one(&store, "SELECT title FROM urls WHERE id = ?1", *id);
        assert_eq!(raw_title, "Loading", "raw rows are never updated (FR-020)");
        let diff: String = one(
            &store,
            "SELECT title_diff FROM render_comparisons WHERE url_id = ?1",
            *id,
        );
        assert_eq!(diff, "changed");
        let h1: String = one(
            &store,
            "SELECT origin FROM rendered_text WHERE url_id = ?1 AND kind = 'heading'",
            *id,
        );
        assert_eq!(h1, "rendered");
        let html = String::from_utf8(store.rendered_html(*id).unwrap().unwrap()).unwrap();
        assert!(html.contains("Hello from JavaScript"));
        let attempts: i64 = one(&store, "SELECT attempts FROM renders WHERE url_id = ?1", *id);
        assert_eq!(attempts, 1);
    }
    // The script went through the engine and its shared cache: at most the
    // two renders that may run at once fetched it; the third page got it
    // from the cache.
    let fetched = site.count(JS_HOST, "/app.js");
    assert!((1..=2).contains(&fetched), "{fetched}");
    let outcomes: Vec<String> = store
        .conn_for_tests()
        .prepare("SELECT outcome FROM render_resources WHERE url LIKE '%/app.js' ORDER BY url_id")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .map(|r| r.unwrap())
        .collect();
    assert_eq!(outcomes.len(), 3);
    assert!(
        outcomes.iter().filter(|o| *o == "cached").count() >= 1,
        "{outcomes:?}"
    );
    // The main documents were not requested again (plan D3).
    assert_eq!(site.count(JS_HOST, "/a"), main_requests);
    assert!(store.meta("fetch_completed_at").unwrap().is_some());
    let renderer = store.meta("renderer").unwrap().unwrap();
    assert!(renderer.contains("\"source\""), "{renderer}");
    assert!(!renderer.contains('/'), "no file path in the file: {renderer}");
    assert_eq!(store.meta("render_version").unwrap().as_deref(), Some("1"));
}

#[tokio::test]
async fn pages_without_stored_html_are_not_rendered() {
    let _serial = SERIAL.lock().await;
    let Some(browser) = browser() else { return };
    let site = site();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("nohtml.spiderlens");
    let ids = crawl_then_queue(&site, &path, false).await;
    assert_eq!(render(&site, &path, browser).await, "completed");
    let store = CrawlStore::open_read_only(&path).unwrap();
    for id in ids {
        let reason: String = one(&store, "SELECT reason FROM renders WHERE url_id = ?1", id);
        assert_eq!(reason, "html_not_stored");
    }
    assert_eq!(
        store
            .conn_for_tests()
            .query_row("SELECT COUNT(*) FROM rendered_pages", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        0
    );
}

/// One crawl with rendering on from the start (T021).
async fn crawl_rendering(
    site: &JsSite,
    path: &Path,
    browser: BrowserInfo,
    edit: impl FnOnce(&mut CrawlConfig),
) -> String {
    let mut c = config(false);
    c.rendering.mode = RenderMode::Javascript;
    c.rendering.concurrency = Some(2);
    edit(&mut c);
    let crawler = Crawler::create(
        path,
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
    crawler.run(rx, None).await.unwrap().status
}

fn render_state(store: &CrawlStore, site: &JsSite, p: &str) -> Option<(String, Option<String>)> {
    let id = store.id_of(&site.url(JS_HOST, p)).unwrap().expect("discovered");
    store
        .conn_for_tests()
        .query_row("SELECT state, reason FROM renders WHERE url_id = ?1", [id], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .ok()
}

#[tokio::test]
async fn the_fetch_stage_queues_eligible_pages() {
    let _serial = SERIAL.lock().await;
    let Some(browser) = browser() else { return };
    let page = |links: &str| {
        format!(
            "<!doctype html><html><head><title>Page</title></head><body><p>Words.</p>{links}</body></html>"
        )
    };
    let site = JsSiteBuilder::new()
        .page(
            "/",
            &page("<a href=\"/a\">A</a><a href=\"/skip/b\">B</a><a href=\"/gone\">Gone</a><a href=\"/data.json\">J</a>"),
        )
        .page("/a", &page(""))
        .page("/skip/b", &page(""))
        .json(JS_HOST, "/data.json", "{}")
        .start();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("queue.spiderlens");
    let status = crawl_rendering(&site, &path, browser.clone(), |c| {
        c.rendering.exclude = vec!["/skip/".into()];
    })
    .await;
    assert_eq!(status, "completed");
    let store = CrawlStore::open_read_only(&path).unwrap();
    assert_eq!(render_state(&store, &site, "/").unwrap().0, "rendered");
    assert_eq!(render_state(&store, &site, "/a").unwrap().0, "rendered");
    assert_eq!(
        render_state(&store, &site, "/skip/b").unwrap(),
        ("not_rendered".to_string(), Some("outside_rules".to_string()))
    );
    // A 404 page (error pages are not rendered by default) and a JSON
    // response get no row.
    assert_eq!(render_state(&store, &site, "/gone"), None);
    assert_eq!(render_state(&store, &site, "/data.json"), None);
    let count = |sql: &str| -> i64 { store.conn_for_tests().query_row(sql, [], |r| r.get(0)).unwrap() };
    // "Store page HTML" off: inputs waited in render_inputs and are gone.
    assert_eq!(count("SELECT COUNT(*) FROM render_inputs"), 0);
    assert_eq!(count("SELECT COUNT(*) FROM bodies"), 0);
    assert_eq!(count("SELECT COUNT(*) FROM rendered_bodies"), 0);
    // Each page was requested once (plan D3).
    assert_eq!(site.count(JS_HOST, "/a"), 1);

    // With error pages on, the 404 page is rendered too.
    let path = dir.path().join("errors.spiderlens");
    let status = crawl_rendering(&site, &path, browser, |c| {
        c.rendering.render_error_pages = true;
        c.extraction.store_html = true;
    })
    .await;
    assert_eq!(status, "completed");
    let store = CrawlStore::open_read_only(&path).unwrap();
    assert_eq!(render_state(&store, &site, "/gone").unwrap().0, "rendered");
    assert_eq!(render_state(&store, &site, "/skip/b").unwrap().0, "rendered");
    let count = |sql: &str| -> i64 { store.conn_for_tests().query_row(sql, [], |r| r.get(0)).unwrap() };
    assert_eq!(count("SELECT COUNT(*) FROM render_inputs"), 0);
    assert_eq!(count("SELECT COUNT(*) FROM rendered_bodies"), 4);
}

fn us1_site() -> JsSite {
    JsSiteBuilder::new().us1_pages().start()
}

fn id(store: &CrawlStore, site: &JsSite, p: &str) -> i64 {
    store
        .id_of(&site.url(JS_HOST, p))
        .unwrap()
        .unwrap_or_else(|| panic!("{p} not discovered"))
}

fn url_state(store: &CrawlStore, site: &JsSite, p: &str) -> Option<String> {
    let id = store.id_of(&site.url(JS_HOST, p)).unwrap()?;
    Some(one(store, "SELECT state FROM urls WHERE id = ?1", id))
}

/// US1 scenarios 1, 2, 4, 5, 6 and the JavaScript redirect (T024).
#[tokio::test]
async fn us1_rendered_values_are_stored_next_to_the_raw_ones() {
    let _serial = SERIAL.lock().await;
    let Some(browser) = browser() else { return };
    let site = us1_site();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("us1.spiderlens");
    assert_eq!(crawl_rendering(&site, &path, browser, |_| {}).await, "completed");
    let store = CrawlStore::open_read_only(&path).unwrap();

    // Scenario 1: both titles are stored.
    let t = id(&store, &site, "/title-changed");
    let rendered: String = one(&store, "SELECT title FROM rendered_pages WHERE url_id = ?1", t);
    assert_eq!(rendered, "Red Shoes | Shop");
    let raw: String = one(&store, "SELECT title FROM urls WHERE id = ?1", t);
    assert_eq!(raw, "Loading");

    // Scenario 2: links inserted by a script are rendered-only, and the
    // pages they point to are crawled and rendered.
    let nav = id(&store, &site, "/js-nav");
    let targets: Vec<(String, String, i64)> = store
        .conn_for_tests()
        .prepare(
            "SELECT u.url, l.kind, l.followed FROM rendered_links l JOIN urls u ON u.id = l.to_id
             WHERE l.from_id = ?1 ORDER BY l.rpos",
        )
        .unwrap()
        .query_map([nav], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .unwrap()
        .map(|r| r.unwrap())
        .collect();
    assert_eq!(
        targets,
        vec![
            (site.url(JS_HOST, "/only-js-1"), "anchor".into(), 1),
            (site.url(JS_HOST, "/only-js-2"), "anchor".into(), 1),
        ]
    );
    let raw_links: i64 = one(&store, "SELECT COUNT(*) FROM links WHERE from_id = ?1", nav);
    assert_eq!(raw_links, 0, "raw link rows are not touched (FR-020)");
    for p in ["/only-js-1", "/only-js-2"] {
        assert_eq!(url_state(&store, &site, p).as_deref(), Some("crawled"), "{p}");
        assert_eq!(render_state(&store, &site, p).unwrap().0, "rendered", "{p}");
        let depth: i64 = one(
            &store,
            "SELECT depth FROM urls WHERE id = ?1",
            id(&store, &site, p),
        );
        assert_eq!(depth, 2, "the page's depth plus one");
    }
    let only: i64 = one(
        &store,
        "SELECT links_rendered_only FROM render_comparisons WHERE url_id = ?1",
        nav,
    );
    assert_eq!(only, 2);

    // Scenario 4: JSON-LD added by a script is marked rendered-only.
    let ld = id(&store, &site, "/jsonld-added");
    let origin: String = one(
        &store,
        "SELECT origin FROM rendered_structured_data WHERE url_id = ?1",
        ld,
    );
    assert_eq!(origin, "rendered");
    let raw_blocks: i64 = one(
        &store,
        "SELECT COUNT(*) FROM structured_data WHERE url_id = ?1",
        ld,
    );
    assert_eq!(raw_blocks, 0);

    // Scenario 5: noindex only in the rendered HTML.
    let ni = id(&store, &site, "/noindex-added");
    let rendered_noindex: bool = one(&store, "SELECT noindex FROM rendered_pages WHERE url_id = ?1", ni);
    let raw_noindex: bool = one(&store, "SELECT noindex FROM urls WHERE id = ?1", ni);
    assert!(rendered_noindex && !raw_noindex);
    let diff: String = one(
        &store,
        "SELECT noindex_diff FROM render_comparisons WHERE url_id = ?1",
        ni,
    );
    assert_eq!(diff, "added");

    // The JavaScript redirect is recorded, not followed by the browser,
    // and its target is discovered like a redirect.
    let jr = id(&store, &site, "/js-redirect");
    let target = id(&store, &site, "/redirect-target");
    let (to, to_id): (String, i64) = store
        .conn_for_tests()
        .query_row(
            "SELECT js_redirect, js_redirect_id FROM renders WHERE url_id = ?1",
            [jr],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(to, site.url(JS_HOST, "/redirect-target"));
    let (state, ms): (String, i64) = store
        .conn_for_tests()
        .query_row(
            "SELECT state, render_ms FROM renders WHERE url_id = ?1",
            [jr],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(state, "rendered", "a script navigation ends the render ({ms} ms)");
    assert!(ms < 5_000, "{ms} ms");
    assert_eq!(to_id, target);
    let kind: String = one(
        &store,
        "SELECT kind FROM rendered_links WHERE from_id = ?1 AND to_id IS NOT NULL",
        jr,
    );
    assert_eq!(kind, "js_redirect");
    let source: String = one(&store, "SELECT source FROM urls WHERE id = ?1", target);
    assert_eq!(source, "redirect");
    assert_eq!(
        url_state(&store, &site, "/redirect-target").as_deref(),
        Some("crawled")
    );
    assert_eq!(
        site.count(JS_HOST, "/redirect-target"),
        1,
        "fetched by the crawl only"
    );

    // Scenario 6: a server-rendered page renders the same.
    let sr = id(&store, &site, "/server-rendered");
    let (title, links): (String, i64) = store
        .conn_for_tests()
        .query_row(
            "SELECT title_diff, links_rendered_only FROM render_comparisons WHERE url_id = ?1",
            [sr],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!((title.as_str(), links), ("same", 0));
    let extra: i64 = one(
        &store,
        "SELECT COUNT(*) FROM rendered_links WHERE from_id = ?1",
        sr,
    );
    assert_eq!(extra, 0);
    for p in ["/", "/title-changed", "/js-nav", "/server-rendered"] {
        assert_eq!(site.count(JS_HOST, p), 1, "{p} requested once");
    }
}

/// US1 scenario 3: rendering off.
#[tokio::test]
async fn us1_rendering_off_starts_no_browser_and_finds_no_script_links() {
    let _serial = SERIAL.lock().await;
    let site = us1_site();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("raw.spiderlens");
    let before = spiderlens_render::launch_count();
    let crawler = Crawler::create(
        &path,
        config(true),
        &ModeInput::Spider {
            start_url: site.url(JS_HOST, "/"),
        },
    )
    .unwrap()
    .with_resolve(site.resolve());
    let (_tx, rx) = watch::channel(Control::Run);
    assert_eq!(crawler.run(rx, None).await.unwrap().status, "completed");
    assert_eq!(spiderlens_render::launch_count(), before, "no browser started");
    let store = CrawlStore::open_read_only(&path).unwrap();
    for table in ["renders", "rendered_pages", "rendered_links", "render_inputs"] {
        let n: i64 = store
            .conn_for_tests()
            .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 0, "{table}");
    }
    for p in ["/only-js-1", "/only-js-2", "/redirect-target"] {
        assert_eq!(url_state(&store, &site, p), None, "{p}");
        assert_eq!(site.count(JS_HOST, p), 0, "{p}");
    }
    let t = id(&store, &site, "/title-changed");
    let raw: String = one(&store, "SELECT title FROM urls WHERE id = ?1", t);
    assert_eq!(raw, "Loading");
}

// ---------------------------------------------------------------------
// User story 2: rendering never slows the raw crawl (T032 to T037)
// ---------------------------------------------------------------------

/// A crawl of the slow-render page set: `/` and `/slow/0 .. /slow/{n-1}`,
/// each settling after `settle`.
fn slow_site(pages: u64, settle: Duration) -> JsSite {
    JsSiteBuilder::new().slow_pages(pages, settle).start()
}

fn slow_crawler(site: &JsSite, path: &Path, browser: BrowserInfo, concurrency: u32) -> Crawler {
    let mut c = config(false);
    c.rendering.mode = RenderMode::Javascript;
    c.rendering.concurrency = Some(concurrency);
    Crawler::create(
        path,
        c,
        &ModeInput::Spider {
            start_url: site.url(JS_HOST, "/"),
        },
    )
    .unwrap()
    .with_resolve(site.resolve())
    .with_browser(browser)
    .with_render_launch(LaunchOptions::default())
}

fn resumed(site: &JsSite, path: &Path, browser: BrowserInfo) -> Crawler {
    Crawler::open_with(path, &ResumeOptions::default())
        .unwrap()
        .with_resolve(site.resolve())
        .with_browser(browser)
        .with_render_launch(LaunchOptions::default())
}

/// Runs a crawl and hands every progress event to `on_event` while it runs.
async fn run_observed(
    crawler: Crawler,
    control: &watch::Sender<Control>,
    mut on_event: impl FnMut(&Progress, &watch::Sender<Control>),
) -> CrawlSummary {
    let (etx, mut erx) = mpsc::unbounded_channel();
    let rx = control.subscribe();
    let (summary, ()) = tokio::join!(crawler.run(rx, Some(etx)), async {
        while let Some(e) = erx.recv().await {
            if let CrawlEvent::Progress(p) = e {
                on_event(&p, control);
            }
        }
    });
    summary.unwrap()
}

fn slow_fetched(site: &JsSite, pages: u64) -> u64 {
    (0..pages)
        .map(|i| site.count(JS_HOST, &format!("/slow/{i}")))
        .sum()
}

fn render_rows(store: &CrawlStore, state: &str) -> i64 {
    store
        .conn_for_tests()
        .query_row("SELECT COUNT(*) FROM renders WHERE state = ?1", [state], |r| {
            r.get(0)
        })
        .unwrap()
}

/// T032: with 20/15 as the marks and slow renders, fetching stops at the
/// mark and starts again below the low one; the crawl still completes and
/// every page is rendered once.
#[tokio::test]
async fn us2_fetching_pauses_when_the_render_queue_is_full_and_resumes() {
    let _serial = SERIAL.lock().await;
    let Some(browser) = browser() else { return };
    const PAGES: u64 = 60;
    let site = slow_site(PAGES, Duration::from_millis(1000));
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("pressure.spiderlens");
    let crawler = slow_crawler(&site, &path, browser, 4).with_render_queue_marks(20, 15);
    let (control, _keep) = watch::channel(Control::Run);

    let mut max_queued = 0;
    let mut held_at: Option<u64> = None;
    let mut seen_released = false;
    let site_ref = &site;
    let summary = run_observed(crawler, &control, |p, _| {
        let r = p
            .render
            .as_ref()
            .expect("a rendering crawl reports render progress");
        max_queued = max_queued.max(r.queued);
        if r.limiting == Some(RenderLimiting::QueueFull) {
            held_at.get_or_insert_with(|| slow_fetched(site_ref, PAGES));
        } else if held_at.is_some() {
            seen_released = true;
        }
    })
    .await;

    assert_eq!(summary.status, "completed");
    let held_at = held_at.expect("the queue reached the mark and fetching paused");
    assert!(
        held_at < PAGES,
        "pages were still unfetched at the mark ({held_at})"
    );
    assert!(
        max_queued <= 20 + 64,
        "the queue stays near the mark: peak {max_queued} (requests already open may still land)"
    );
    assert!(seen_released, "fetching started again below the low mark");
    assert!(
        slow_fetched(&site, PAGES) == PAGES,
        "every page was fetched in the end"
    );
    let store = CrawlStore::open_read_only(&path).unwrap();
    assert_eq!(render_rows(&store, "rendered"), PAGES as i64 + 1);
    assert_eq!(
        render_rows(&store, "queued") + render_rows(&store, "rendering"),
        0
    );
}

/// T033, US2 scenario 5: pause while pages are rendering, resume; every
/// eligible page is rendered exactly once and the resume asks the site for
/// no page again.
#[tokio::test]
async fn us2_pause_and_resume_render_every_page_exactly_once() {
    let _serial = SERIAL.lock().await;
    let Some(browser) = browser() else { return };
    const PAGES: u64 = 12;
    let site = slow_site(PAGES, Duration::from_millis(700));
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("pause.spiderlens");
    let (control, _keep) = watch::channel(Control::Run);
    let summary = run_observed(
        slow_crawler(&site, &path, browser.clone(), 2),
        &control,
        |p, c| {
            if p.render
                .as_ref()
                .is_some_and(|r| r.rendering > 0 && r.rendered >= 2)
            {
                let _ = c.send(Control::Pause);
            }
        },
    )
    .await;
    assert_eq!(summary.status, "paused");
    {
        let store = CrawlStore::open_read_only(&path).unwrap();
        assert_eq!(
            render_rows(&store, "rendering"),
            0,
            "open renders finished before the pause"
        );
        assert!(render_rows(&store, "queued") > 0, "the queue was kept");
        assert!(render_rows(&store, "rendered") >= 2);
    }
    let (control, _keep) = watch::channel(Control::Run);
    let summary = run_observed(resumed(&site, &path, browser), &control, |_, _| {}).await;
    assert_eq!(summary.status, "completed");

    let store = CrawlStore::open_read_only(&path).unwrap();
    assert_eq!(render_rows(&store, "rendered"), PAGES as i64 + 1);
    let stamps: i64 = store
        .conn_for_tests()
        .query_row(
            "SELECT COUNT(rendered_at) FROM renders WHERE state = 'rendered'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(stamps, PAGES as i64 + 1, "one rendered_at per page");
    let rows: i64 = store
        .conn_for_tests()
        .query_row("SELECT COUNT(*) FROM rendered_pages", [], |r| r.get(0))
        .unwrap();
    assert_eq!(rows, PAGES as i64 + 1, "one rendered version per page");
    let over: i64 = store
        .conn_for_tests()
        .query_row("SELECT COUNT(*) FROM renders WHERE attempts <> 1", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(over, 0, "no page was rendered twice");
    for i in 0..PAGES {
        let path = format!("/slow/{i}");
        assert_eq!(
            site.count(JS_HOST, &path),
            1,
            "{path} requested once, resume included"
        );
    }
}

/// T033: stop marks every waiting render "Crawl stopped" and leaves none
/// queued or rendering.
#[tokio::test]
async fn us2_stop_marks_waiting_renders_as_crawl_stopped() {
    let _serial = SERIAL.lock().await;
    let Some(browser) = browser() else { return };
    const PAGES: u64 = 12;
    let site = slow_site(PAGES, Duration::from_millis(700));
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("stop.spiderlens");
    let (control, _keep) = watch::channel(Control::Run);
    let summary = run_observed(slow_crawler(&site, &path, browser, 2), &control, |p, c| {
        if p.render
            .as_ref()
            .is_some_and(|r| r.rendering > 0 && r.rendered >= 2)
        {
            let _ = c.send(Control::Stop);
        }
    })
    .await;
    assert_eq!(summary.status, "stopped");
    let store = CrawlStore::open_read_only(&path).unwrap();
    assert_eq!(
        render_rows(&store, "queued") + render_rows(&store, "rendering"),
        0
    );
    let stopped: i64 = store
        .conn_for_tests()
        .query_row(
            "SELECT COUNT(*) FROM renders WHERE state = 'not_rendered' AND reason = 'crawl_stopped'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(stopped > 0, "waiting renders were marked Crawl stopped");
    assert_eq!(
        summary
            .render
            .as_ref()
            .and_then(|r| r.by_reason.get("crawl_stopped")),
        Some(&(stopped as u64))
    );
}

/// The process ids of the browser processes this test started: our own
/// children whose command line has the debugging pipe or port flag.
fn browser_children() -> Vec<u32> {
    let me = std::process::id();
    let mut out = Vec::new();
    for e in std::fs::read_dir("/proc").into_iter().flatten().flatten() {
        let Ok(pid) = e.file_name().to_string_lossy().parse::<u32>() else {
            continue;
        };
        let Ok(stat) = std::fs::read_to_string(format!("/proc/{pid}/stat")) else {
            continue;
        };
        let ppid = stat
            .rsplit_once(')')
            .and_then(|(_, rest)| rest.split_whitespace().nth(1))
            .and_then(|p| p.parse::<u32>().ok());
        let cmd = std::fs::read_to_string(format!("/proc/{pid}/cmdline")).unwrap_or_default();
        if ppid == Some(me) && cmd.contains("remote-debugging") {
            out.push(pid);
        }
    }
    out
}

/// T034 (US2 scenario 3, part of SC-006): the site is asked for each
/// rendered page's own path once, also when a render had to be tried again
/// after the browser died.
#[cfg(target_os = "linux")]
#[tokio::test]
async fn us2_a_rendered_page_is_requested_once_also_after_a_crash_retry() {
    let _serial = SERIAL.lock().await;
    let Some(browser) = browser() else { return };
    const PAGES: u64 = 8;
    let site = slow_site(PAGES, Duration::from_millis(1500));
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("once.spiderlens");
    let (control, _keep) = watch::channel(Control::Run);
    let mut killed = false;
    let summary = run_observed(slow_crawler(&site, &path, browser, 2), &control, |p, _| {
        if !killed && p.render.as_ref().is_some_and(|r| r.rendering > 0) {
            // Our own child only: the browser this crawl started.
            for pid in browser_children() {
                let _ = std::process::Command::new("kill")
                    .args(["-9", &pid.to_string()])
                    .status();
                killed = true;
            }
        }
    })
    .await;
    assert_eq!(summary.status, "completed");
    assert!(killed, "the browser was killed while rendering");
    let store = CrawlStore::open_read_only(&path).unwrap();
    let retried: i64 = store
        .conn_for_tests()
        .query_row("SELECT COUNT(*) FROM renders WHERE attempts > 1", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert!(retried > 0, "at least one render was tried again after the crash");
    assert_eq!(
        render_rows(&store, "queued") + render_rows(&store, "rendering"),
        0
    );
    assert_eq!(site.count(JS_HOST, "/"), 1, "/ requested once");
    for i in 0..PAGES {
        let p = format!("/slow/{i}");
        assert_eq!(site.count(JS_HOST, &p), 1, "{p} requested once");
        let in_log = site
            .log()
            .iter()
            .filter(|e| e.host == JS_HOST && e.path == p)
            .count();
        assert_eq!(in_log, 1, "{p} appears once in the request log");
    }
}

/// T035, US2 scenario 4: render progress adds up, is reported while
/// rendering goes on after fetching is done, and does not exist for raw
/// crawls.
#[tokio::test]
async fn us2_render_progress_adds_up_and_raw_crawls_have_none() {
    let _serial = SERIAL.lock().await;
    let Some(browser) = browser() else { return };
    const PAGES: u64 = 10;
    let site = slow_site(PAGES, Duration::from_millis(400));
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("progress.spiderlens");
    let (control, _keep) = watch::channel(Control::Run);
    let mut events: Vec<RenderProgress> = Vec::new();
    let mut last = None;
    let summary = run_observed(slow_crawler(&site, &path, browser, 2), &control, |p, _| {
        if let Some(r) = &p.render {
            events.push(r.clone());
        }
        last = Some(p.clone());
    })
    .await;
    assert_eq!(summary.status, "completed");
    let last = last.expect("progress was reported");
    let r = last
        .render
        .as_ref()
        .expect("render progress on a rendering crawl");
    let total = PAGES + 1;
    assert_eq!((r.rendered, r.queued, r.rendering), (total, 0, 0));
    assert_eq!(r.limit, 2);
    assert!(r.avg_pages_per_sec > 0.0);
    assert!(
        events
            .iter()
            .all(|r| r.rendered + r.queued + u64::from(r.rendering) <= total),
        "rendered, waiting and rendering never exceed the pages queued"
    );
    assert!(events.iter().all(|r| r.rendering <= r.limit));
    assert!(
        events
            .iter()
            .any(|r| r.fetching_done && (r.queued > 0 || r.rendering > 0)),
        "rendering went on after fetching was done"
    );
    assert!(
        events
            .iter()
            .any(|r| r.limiting == Some(RenderLimiting::Concurrency))
    );
    let rendered_json = serde_json::to_value(&last).unwrap();
    assert!(rendered_json["render"].is_object());
    assert_eq!(rendered_json["render"]["rendered"], total);

    // Fields agree with the file.
    let store = CrawlStore::open_read_only(&path).unwrap();
    assert_eq!(render_rows(&store, "rendered") as u64, r.rendered);
}

/// The key set of a crawl summary before spec 014.
const PRE_FEATURE_SUMMARY_KEYS: &[&str] = &[
    "active_ms",
    "by_class",
    "by_reason",
    "crawled",
    "discovered",
    "external_blocked",
    "external_checked",
    "failed_by_class",
    "hosts",
    "hosts_paused",
    "manual_retries",
    "not_crawled",
    "pause_reason",
    "permanent_errors",
    "queued",
    "recovered",
    "retries",
    "sitemaps",
    "speed_events",
    "status",
    "transient_errors",
    "url_limit_reached",
    "warnings",
];

/// Every table migration step 16 creates for rendering.
const RENDERING_TABLES: &[&str] = &[
    "renders",
    "render_inputs",
    "rendered_pages",
    "rendered_text",
    "rendered_directives",
    "rendered_structured_data",
    "rendered_links",
    "raw_only_links",
    "render_comparisons",
    "render_resources",
    "console_entries",
    "rendered_bodies",
    "screenshots",
];

/// T037, FR-002, US2 scenario 1: with rendering off, no browser starts, no
/// rendering row exists, there is no render progress or summary, and the
/// summary has exactly the keys it had before the feature.
#[tokio::test]
async fn us2_rendering_off_leaves_no_trace() {
    let _serial = SERIAL.lock().await;
    let site = us1_site();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("off.spiderlens");
    let before = spiderlens_render::launch_count();
    let crawler = Crawler::create(
        &path,
        config(true),
        &ModeInput::Spider {
            start_url: site.url(JS_HOST, "/"),
        },
    )
    .unwrap()
    .with_resolve(site.resolve());
    let (control, _keep) = watch::channel(Control::Run);
    let mut progress_events = 0;
    let summary = run_observed(crawler, &control, |p, _| {
        progress_events += 1;
        assert!(p.render.is_none(), "no render progress for a raw crawl");
        let json = serde_json::to_value(p).unwrap();
        assert!(
            json.get("render").is_none(),
            "no render key in the progress payload"
        );
    })
    .await;
    assert_eq!(summary.status, "completed");
    assert!(progress_events > 0);
    assert_eq!(
        spiderlens_render::launch_count(),
        before,
        "the launch counter stays at 0"
    );
    assert!(summary.render.is_none());

    let json = serde_json::to_value(&summary).unwrap();
    let mut keys: Vec<&str> = json.as_object().unwrap().keys().map(String::as_str).collect();
    keys.sort_unstable();
    assert_eq!(
        keys, PRE_FEATURE_SUMMARY_KEYS,
        "the summary JSON keys are unchanged"
    );

    let store = CrawlStore::open_read_only(&path).unwrap();
    for table in RENDERING_TABLES {
        let n: i64 = store
            .conn_for_tests()
            .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 0, "{table} has no rows");
    }
    for key in ["renderer", "fetch_completed_at"] {
        assert_eq!(store.meta(key).unwrap(), None, "meta.{key}");
    }
}

// ---------------------------------------------------------------------
// T038: the raw fetch rate while rendering (SC-002, second half)
// ---------------------------------------------------------------------

/// `bench-site`, started on a free port and stopped when dropped.
struct BenchSite {
    child: std::process::Child,
    port: u16,
}

impl BenchSite {
    fn start(pages: u64) -> Self {
        // The binary sits next to the test's own directory; build it when it
        // is not there yet, in the profile the test was built with.
        let exe = std::env::current_exe().unwrap();
        let profile_dir = exe.parent().and_then(Path::parent).unwrap().to_path_buf();
        let bin = profile_dir.join("bench-site");
        if !bin.exists() {
            let mut build = std::process::Command::new(env!("CARGO"));
            build.args(["build", "-p", "spiderlens-test-fixtures", "--bin", "bench-site"]);
            if profile_dir.ends_with("release") {
                build.arg("--release");
            }
            assert!(build.status().unwrap().success(), "bench-site builds");
        }
        let port = std::net::TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        let child = std::process::Command::new(bin)
            .args([
                "--spa",
                "--pages",
                &pages.to_string(),
                "--port",
                &port.to_string(),
            ])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap();
        let site = Self { child, port };
        for _ in 0..100 {
            if site.get("/__stats").is_some() {
                return site;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        panic!("bench-site did not start");
    }

    fn get(&self, path: &str) -> Option<String> {
        use std::io::{Read, Write};
        let mut s = std::net::TcpStream::connect(("127.0.0.1", self.port)).ok()?;
        write!(s, "GET {path} HTTP/1.0\r\nHost: bench\r\n\r\n").ok()?;
        let mut out = String::new();
        s.read_to_string(&mut out).ok()?;
        Some(out.split_once("\r\n\r\n")?.1.to_string())
    }

    fn stats(&self) -> serde_json::Value {
        serde_json::from_str(&self.get("/__stats").expect("stats")).expect("stats are JSON")
    }
}

impl Drop for BenchSite {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// One crawl of the bench site. Returns the pages per second the site saw
/// its page requests arrive at (first to last), and how many renders were
/// done when fetching ended. With rendering on, the crawl is stopped once
/// every page is fetched: the renders still waiting then are not what is
/// measured. `cap` is a URLs-per-second cap.
async fn spa_fetch_rate(
    site: &BenchSite,
    pages: u64,
    browser: Option<BrowserInfo>,
    cap: Option<f64>,
) -> (f64, u64) {
    let _ = site.get("/__reset");
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("spa.spiderlens");
    let mut c = config(false);
    c.speed.max_urls_per_sec = cap;
    if browser.is_some() {
        c.rendering.mode = RenderMode::Javascript;
    }
    let mut crawler = Crawler::create(
        &path,
        c,
        &ModeInput::Spider {
            start_url: format!("http://127.0.0.1:{}/p/0", site.port),
        },
    )
    .unwrap();
    if let Some(b) = browser.clone() {
        crawler = crawler
            .with_browser(b)
            .with_render_launch(LaunchOptions::default());
    }
    let (control, _keep) = watch::channel(Control::Run);
    let mut rendered = 0;
    let summary = run_observed(crawler, &control, |p, c| {
        if browser.is_some() && site.stats()["pages"].as_u64() >= Some(pages) {
            rendered = p.render.as_ref().map_or(0, |r| r.rendered);
            let _ = c.send(Control::Stop);
        }
    })
    .await;
    assert!(matches!(summary.status.as_str(), "completed" | "stopped"));
    let stats = site.stats();
    assert_eq!(stats["pages"], pages, "every page was requested once");
    let ms = stats["last_page_ms"].as_u64().unwrap() - stats["first_page_ms"].as_u64().unwrap();
    (pages as f64 / (ms as f64 / 1000.0), rendered)
}

/// Fetching while rendering runs as fast as with rendering off, until the
/// queue mark (10,000 pages never reach 50,000 waiting renders).
///
/// Two cases, three alternating rounds each, medians compared:
/// - capped at 300 URLs per second, as a crawl is held back by a site or
///   a network and not by this computer: asserted, at least 95% of the
///   rate with rendering off;
/// - uncapped: only printed. The site, the crawler and the browser share
///   the CPUs of one machine here, so the browser's own work takes cycles
///   from the other two.
///
/// Run it on its own, in release mode, and record the numbers in
/// `specs/014-javascript-rendering/benchmarks.md`:
///
/// `SPIDERLENS_BROWSER=... cargo test --release -p spiderlens-engine --test rendering -- --ignored --nocapture spa_raw_fetch_rate`
#[tokio::test]
#[ignore = "benchmark: needs a browser and a quiet machine"]
async fn spa_raw_fetch_rate_while_rendering_is_not_lower() {
    let _serial = SERIAL.lock().await;
    let browser = browser().expect("a browser is needed for the benchmark");
    const PAGES: u64 = 10_000;
    const ROUNDS: usize = 3;
    let site = BenchSite::start(PAGES);
    let median = |v: &mut Vec<f64>| {
        v.sort_by(|a, b| a.partial_cmp(b).unwrap());
        v[v.len() / 2]
    };
    let mut medians = Vec::new();
    for cap in [Some(300.0), None] {
        let (mut off, mut on) = (Vec::new(), Vec::new());
        for round in 0..ROUNDS {
            let (rate_off, _) = spa_fetch_rate(&site, PAGES, None, cap).await;
            let (rate_on, rendered) = spa_fetch_rate(&site, PAGES, Some(browser.clone()), cap).await;
            println!(
                "cap {cap:?}, round {}: off {rate_off:.0} pages/s, on {rate_on:.0} pages/s \
                 ({rendered} rendered when fetching ended)",
                round + 1
            );
            off.push(rate_off);
            on.push(rate_on);
        }
        let (off, on) = (median(&mut off), median(&mut on));
        println!(
            "cap {cap:?}, median: off {off:.0} pages/s, on {on:.0} pages/s ({:+.1}%)",
            (on / off - 1.0) * 100.0
        );
        medians.push((cap, off, on));
    }
    let (_, off, on) = medians[0];
    assert!(
        on >= off * 0.95,
        "fetching while rendering ({on:.0}/s) is slower than with rendering off ({off:.0}/s)"
    );
}

// ---------------------------------------------------------------------------
// US5: know when rendering went wrong (T057, T058, T063, T064)
// ---------------------------------------------------------------------------

fn us5_site() -> JsSite {
    JsSiteBuilder::new().us5_pages().start()
}

/// Everything the failure pages leave in the file, per path.
#[tokio::test]
async fn us5_console_failed_resources_page_too_large_and_bot_challenge_are_stored() {
    let _serial = SERIAL.lock().await;
    let Some(browser) = browser() else { return };
    let site = us5_site();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("us5.spiderlens");
    let status = crawl_rendering(&site, &path, browser, |c| {
        c.rendering.max_render_secs = 20.0;
    })
    .await;
    assert_eq!(status, "completed", "the crawl finishes whatever the pages do");
    let store = CrawlStore::open_read_only(&path).unwrap();

    // Every eligible page ends in exactly one final state.
    let eligible = [
        "/",
        "/throws",
        "/logs",
        "/missing-script",
        "/heavy",
        "/challenge",
        "/steady",
    ];
    for p in eligible {
        let (state, _) = render_state(&store, &site, p).unwrap_or_else(|| panic!("{p} has no render row"));
        assert!(
            ["rendered", "timed_out", "failed", "not_rendered"].contains(&state.as_str()),
            "{p}: {state}"
        );
    }
    let rows: i64 = store
        .conn_for_tests()
        .query_row("SELECT COUNT(*) FROM renders", [], |r| r.get(0))
        .unwrap();
    assert_eq!(rows as usize, eligible.len());

    // Scenario 1: the uncaught exception, with message and location.
    assert_eq!(render_state(&store, &site, "/throws").unwrap().0, "rendered");
    let throws = id(&store, &site, "/throws");
    let (level, message, source, line): (String, String, Option<String>, Option<i64>) = store
        .conn_for_tests()
        .query_row(
            "SELECT level, message, source_url, line FROM console_entries WHERE url_id = ?1",
            [throws],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .unwrap();
    assert_eq!(level, "exception");
    assert!(message.contains("boom from the fixture"), "{message}");
    assert!(source.is_some_and(|s| s.ends_with("/throws")));
    assert!(line.is_some_and(|l| l >= 1));
    assert!(
        one::<i64>(
            &store,
            "SELECT console_errors FROM renders WHERE url_id = ?1",
            throws
        ) >= 1
    );

    // console.error and console.warn.
    let logs = id(&store, &site, "/logs");
    assert!(
        one::<i64>(
            &store,
            "SELECT console_errors FROM renders WHERE url_id = ?1",
            logs
        ) >= 1
    );
    assert_eq!(
        one::<i64>(
            &store,
            "SELECT console_warnings FROM renders WHERE url_id = ?1",
            logs
        ),
        1
    );
    let messages: Vec<(String, String)> = {
        let conn = store.conn_for_tests();
        let mut stmt = conn
            .prepare("SELECT level, message FROM console_entries WHERE url_id = ?1 ORDER BY pos")
            .unwrap();
        stmt.query_map([logs], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap()
    };
    assert!(
        messages.contains(&("error".into(), "fixture error message".into())),
        "{messages:?}"
    );
    assert!(
        messages.contains(&("warning".into(), "fixture warning message".into())),
        "{messages:?}"
    );

    // Scenario 2: the 404 script is a failed resource of that page only.
    let missing = id(&store, &site, "/missing-script");
    assert_eq!(
        render_state(&store, &site, "/missing-script").unwrap().0,
        "rendered"
    );
    assert_eq!(
        one::<i64>(
            &store,
            "SELECT resources_failed FROM renders WHERE url_id = ?1",
            missing
        ),
        1
    );
    let (outcome, status, error): (String, Option<i64>, Option<String>) = store
        .conn_for_tests()
        .query_row(
            "SELECT outcome, status, error FROM render_resources WHERE url_id = ?1 AND url LIKE '%/missing.js'",
            [missing],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!(
        (outcome.as_str(), status, error.as_deref()),
        ("failed", Some(404), Some("HTTP 404"))
    );
    // The pages the "JavaScript errors" and "Has failed resources" filters list.
    let ids = |sql: &str| -> Vec<i64> {
        let conn = store.conn_for_tests();
        let mut stmt = conn.prepare(sql).unwrap();
        stmt.query_map([], |r| r.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap()
    };
    let with_errors = ids("SELECT url_id FROM renders WHERE console_errors > 0 ORDER BY url_id");
    assert!(with_errors.contains(&throws) && with_errors.contains(&logs));
    assert_eq!(
        ids("SELECT url_id FROM renders WHERE resources_failed > 0"),
        vec![missing]
    );

    // Scenario 3 (heap limit): not retried, raw row untouched.
    let (state, reason) = render_state(&store, &site, "/heavy").unwrap();
    assert_eq!(
        (state.as_str(), reason.as_deref()),
        ("failed", Some("page_too_large"))
    );
    let heavy = id(&store, &site, "/heavy");
    assert_eq!(
        one::<i64>(&store, "SELECT attempts FROM renders WHERE url_id = ?1", heavy),
        1
    );
    assert_eq!(url_state(&store, &site, "/heavy").as_deref(), Some("crawled"));
    let title: String = one(&store, "SELECT title FROM urls WHERE id = ?1", heavy);
    assert_eq!(title, "Heavy");
    assert_eq!(
        one::<i64>(
            &store,
            "SELECT COUNT(*) FROM rendered_pages WHERE url_id = ?1",
            heavy
        ),
        0,
        "a failed render stores no rendered values"
    );
    assert_eq!(site.count(JS_HOST, "/heavy"), 1, "the page is requested once");

    // T064: only the challenge page is flagged.
    let flagged = ids("SELECT url_id FROM renders WHERE bot_challenge = 1");
    assert_eq!(flagged, vec![id(&store, &site, "/challenge")]);

    // The summary's counts add up to the eligible pages.
    let summary = spiderlens_engine::crawl::CrawlSummary::read(&path).unwrap();
    let r = summary.render.expect("rendering summary");
    assert_eq!(r.by_state.values().sum::<u64>() as usize, eligible.len());
    assert_eq!(r.by_state.get("failed"), Some(&1));
    assert_eq!(r.by_reason.get("page_too_large"), Some(&1));
    assert!(r.js_errors >= 2);
    assert_eq!(r.failed_resources, 1);
}

/// Kills every browser process of this test process (not the guard).
#[cfg(target_os = "linux")]
fn kill_browsers() -> usize {
    let marker = format!("spiderlens-render-{}-", std::process::id());
    let mut killed = 0;
    let Ok(dir) = std::fs::read_dir("/proc") else {
        return 0;
    };
    for e in dir.flatten() {
        let Some(pid) = e.file_name().to_str().and_then(|n| n.parse::<u32>().ok()) else {
            continue;
        };
        let Ok(raw) = std::fs::read(format!("/proc/{pid}/cmdline")) else {
            continue;
        };
        let text = String::from_utf8_lossy(&raw).replace('\0', " ");
        if text.contains(&marker) && !text.contains("--type=") && !text.contains("--render-guard") {
            let _ = std::process::Command::new("kill")
                .args(["-9", &pid.to_string()])
                .status();
            killed += 1;
        }
    }
    killed
}

/// Seven slow pages: each render takes about a second.
fn us5_slow_site() -> JsSite {
    let page = |links: &str| {
        format!(
            "<!doctype html><html><head><title>Slow</title><script src=\"/slow.js\"></script></head>\
             <body><h1>Slow</h1><p>Some words on a slow page.</p>{links}</body></html>"
        )
    };
    let links: String = (1..=6).map(|i| format!("<a href=\"/p{i}\">P{i}</a>")).collect();
    let mut b = JsSiteBuilder::new().page("/", &page(&links));
    for i in 1..=6 {
        b = b.page(&format!("/p{i}"), &page(""));
    }
    b.script(
        JS_HOST,
        "/slow.js",
        "document.body.setAttribute('data-slow', '1');",
    )
    .delay(JS_HOST, "/slow.js", std::time::Duration::from_secs(1))
    .start()
}

/// SC-008, SC-009: the browser is killed three times during the crawl. The
/// crawl completes, every eligible page ends in one final state, no raw data
/// is lost and no page is requested twice.
#[cfg(target_os = "linux")]
#[tokio::test]
async fn us5_killing_the_browser_repeatedly_leaves_every_page_in_one_final_state() {
    let _serial = SERIAL.lock().await;
    let Some(browser) = browser() else { return };
    let site = us5_slow_site();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("killed.spiderlens");
    let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let flag = stop.clone();
    let killer = std::thread::spawn(move || {
        let mut kills = 0;
        while kills < 3 && !flag.load(std::sync::atomic::Ordering::Relaxed) {
            std::thread::sleep(std::time::Duration::from_millis(1500));
            if kill_browsers() > 0 {
                kills += 1;
            }
        }
        kills
    });
    let status = crawl_rendering(&site, &path, browser, |_| {}).await;
    stop.store(true, std::sync::atomic::Ordering::Relaxed);
    let kills = killer.join().unwrap();
    assert_eq!(status, "completed");
    assert!(kills >= 1, "the test killed no browser");

    let store = CrawlStore::open_read_only(&path).unwrap();
    let pages = ["/", "/p1", "/p2", "/p3", "/p4", "/p5", "/p6"];
    for p in pages {
        let (state, reason) = render_state(&store, &site, p).unwrap_or_else(|| panic!("{p}"));
        assert!(
            matches!(
                (state.as_str(), reason.as_deref()),
                ("rendered", None)
                    | ("timed_out", None)
                    | ("failed", Some("crash" | "renderer_error"))
                    | ("not_rendered", Some("browser_unavailable"))
            ),
            "{p}: {state} {reason:?}"
        );
        // Raw data is untouched, and the page was requested once.
        assert_eq!(url_state(&store, &site, p).as_deref(), Some("crawled"), "{p}");
        let title: String = one(
            &store,
            "SELECT title FROM urls WHERE id = ?1",
            id(&store, &site, p),
        );
        assert_eq!(title, "Slow");
        assert_eq!(site.count(JS_HOST, p), 1, "{p} was requested more than once");
    }
    let open: i64 = store
        .conn_for_tests()
        .query_row(
            "SELECT COUNT(*) FROM renders WHERE state IN ('queued', 'rendering')",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(open, 0);
    let rendered: i64 = store
        .conn_for_tests()
        .query_row("SELECT COUNT(*) FROM renders WHERE state = 'rendered'", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert!(rendered >= 1, "the browser was started again and rendered pages");
}

/// FR-046: a browser that cannot be started again turns the rest into
/// `browser_unavailable` and the crawl says so in a warning.
#[cfg(target_os = "linux")]
#[tokio::test]
async fn us5_a_browser_that_cannot_be_relaunched_makes_the_rest_browser_unavailable() {
    use std::os::unix::fs::PermissionsExt;
    let _serial = SERIAL.lock().await;
    let Some(mut browser) = browser() else { return };
    let site = us5_slow_site();
    let dir = tempfile::tempdir().unwrap();
    // The browser is started through a wrapper that the test removes.
    let wrapper = dir.path().join("browser-wrapper");
    std::fs::write(
        &wrapper,
        format!("#!/bin/sh\nexec \"{}\" \"$@\"\n", browser.path.display()),
    )
    .unwrap();
    std::fs::set_permissions(&wrapper, std::fs::Permissions::from_mode(0o755)).unwrap();
    browser.path = wrapper.clone();

    let path = dir.path().join("gone.spiderlens");
    let removed = wrapper.clone();
    let killer = std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(2500));
        let _ = std::fs::remove_file(&removed);
        for _ in 0..20 {
            if kill_browsers() > 0 {
                return true;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        false
    });
    let mut c = config(false);
    c.rendering.mode = RenderMode::Javascript;
    c.rendering.concurrency = Some(1);
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
    let summary = crawler.run(rx, None).await.unwrap();
    assert!(killer.join().unwrap(), "the test killed no browser");
    assert_eq!(summary.status, "completed");
    assert!(
        summary
            .warnings
            .iter()
            .any(|w| w.contains("could not be started again")),
        "{:?}",
        summary.warnings
    );
    let store = CrawlStore::open_read_only(&path).unwrap();
    let unavailable: i64 = store
        .conn_for_tests()
        .query_row(
            "SELECT COUNT(*) FROM renders WHERE state = 'not_rendered' AND reason = 'browser_unavailable'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(unavailable >= 1);
    let open: i64 = store
        .conn_for_tests()
        .query_row(
            "SELECT COUNT(*) FROM renders WHERE state IN ('queued', 'rendering')",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(open, 0, "every eligible page is in one final state");
    for p in ["/", "/p1", "/p6"] {
        assert_eq!(url_state(&store, &site, p).as_deref(), Some("crawled"), "{p}");
        assert_eq!(site.count(JS_HOST, p), 1);
    }
}

// ---------------------------------------------------------------------------
// User story 4: rendering settings (T047 to T052, T055).

fn us4_site() -> JsSite {
    JsSiteBuilder::new().us4_pages().start()
}

/// The rendering patterns that leave one page of the US4 site to render.
fn only(pattern: &str) -> impl FnOnce(&mut CrawlConfig) {
    let pattern = pattern.to_string();
    move |c| c.rendering.include = vec![pattern]
}

/// `render_resources` outcome of the resource whose address ends with
/// `suffix`, on the page `p`.
fn resource_outcome(store: &CrawlStore, site: &JsSite, p: &str, suffix: &str) -> Option<String> {
    let id = id(store, site, p);
    store
        .conn_for_tests()
        .query_row(
            "SELECT outcome FROM render_resources WHERE url_id = ?1 AND url LIKE ?2",
            rusqlite::params![id, format!("%{suffix}")],
            |r| r.get(0),
        )
        .ok()
}

/// The text a script wrote into the width page and the window it was
/// rendered in.
async fn width_of(
    site: &JsSite,
    dir: &Path,
    name: &str,
    browser: BrowserInfo,
    edit: impl FnOnce(&mut CrawlConfig),
) -> (String, i64) {
    let path = dir.join(name);
    let status = crawl_rendering(site, &path, browser, |c| {
        c.rendering.include = vec!["/width$".into()];
        edit(c);
    })
    .await;
    assert_eq!(status, "completed");
    let store = CrawlStore::open_read_only(&path).unwrap();
    let w = id(&store, site, "/width");
    let text: String = one(
        &store,
        "SELECT text FROM rendered_text WHERE url_id = ?1 AND text LIKE 'innerWidth=%'",
        w,
    );
    (
        text,
        one(&store, "SELECT window_w FROM renders WHERE url_id = ?1", w),
    )
}

/// US4 scenario 7: the page sees the window the settings ask for (T047).
#[tokio::test]
async fn us4_the_page_sees_the_window_the_settings_ask_for() {
    let _serial = SERIAL.lock().await;
    let Some(browser) = browser() else { return };
    let site = us4_site();
    let dir = tempfile::tempdir().unwrap();

    // `auto` with SpiderLens' desktop user agent: 1024 x 768.
    let (text, w) = width_of(&site, dir.path(), "auto.spiderlens", browser.clone(), |_| {}).await;
    assert_eq!((text.as_str(), w), ("innerWidth=1024", 1024));

    // `auto` follows a mobile user agent.
    let (text, w) = width_of(&site, dir.path(), "phone.spiderlens", browser.clone(), |c| {
        c.identity.user_agent.preset = UaPreset::GooglebotSmartphone;
    })
    .await;
    assert_eq!((text.as_str(), w), ("innerWidth=411", 411));

    // The mobile preset, whatever the user agent.
    let (text, _) = width_of(&site, dir.path(), "mobile.spiderlens", browser.clone(), |c| {
        c.rendering.window.preset = WindowPreset::Mobile;
    })
    .await;
    assert_eq!(text, "innerWidth=411");

    // A custom size.
    let (text, w) = width_of(&site, dir.path(), "custom.spiderlens", browser, |c| {
        c.rendering.window = RenderWindow {
            preset: WindowPreset::Custom,
            width: Some(800),
            height: Some(600),
        };
    })
    .await;
    assert_eq!((text.as_str(), w), ("innerWidth=800", 800));
}

/// US4 scenarios 3 and 4, FR-014 (T048).
#[tokio::test]
async fn us4_blocked_resource_types_are_not_requested_and_are_listed() {
    let _serial = SERIAL.lock().await;
    let Some(browser) = browser() else { return };
    let site = us4_site();
    let dir = tempfile::tempdir().unwrap();
    let run = |name: &str, block: RenderBlock| {
        let path = dir.path().join(name);
        let browser = browser.clone();
        let site = &site;
        async move {
            site.reset();
            let status = crawl_rendering(site, &path, browser, |c| {
                c.rendering.include = vec!["/resources$".into()];
                c.rendering.block = block;
                c.robots.mode = RobotsMode::Ignore;
            })
            .await;
            assert_eq!(status, "completed");
            CrawlStore::open_read_only(&path).unwrap()
        }
    };

    // Defaults: images, media and fonts are blocked; scripts, stylesheets
    // and third-party scripts load.
    let store = run("default.spiderlens", RenderBlock::default()).await;
    for (host_path, suffix) in [
        ("/img/a.png", "/img/a.png"),
        ("/media/a.mp4", "/media/a.mp4"),
        ("/fonts/a.woff2", "/fonts/a.woff2"),
    ] {
        assert_eq!(site.count(JS_HOST, host_path), 0, "{host_path} was requested");
        assert_eq!(
            resource_outcome(&store, &site, "/resources", suffix).as_deref(),
            Some("blocked_setting"),
            "{suffix}"
        );
    }
    assert!(site.count(JS_HOST, "/js/own.js") >= 1);
    assert!(site.count(JS_HOST, "/css/site.css") >= 1);
    assert!(
        site.count("cdn.test", "/cdn/lib.js") >= 1,
        "third-party scripts load by default"
    );
    let blocked: i64 = one(
        &store,
        "SELECT resources_blocked FROM renders WHERE url_id = ?1",
        id(&store, &site, "/resources"),
    );
    assert!(blocked >= 3, "{blocked}");

    // "Load all resources".
    let none = RenderBlock {
        images: false,
        media: false,
        fonts: false,
        stylesheets: false,
        third_party: false,
    };
    let store = run("all.spiderlens", none).await;
    for (host_path, suffix) in [
        ("/img/a.png", "/img/a.png"),
        ("/media/a.mp4", "/media/a.mp4"),
        ("/fonts/a.woff2", "/fonts/a.woff2"),
    ] {
        assert!(
            site.count(JS_HOST, host_path) >= 1,
            "{host_path} was not requested"
        );
        assert_ne!(
            resource_outcome(&store, &site, "/resources", suffix).as_deref(),
            Some("blocked_setting"),
            "{suffix}"
        );
    }

    // FR-014: third-party and stylesheet blocking.
    let store = run(
        "third.spiderlens",
        RenderBlock {
            stylesheets: true,
            third_party: true,
            ..none
        },
    )
    .await;
    assert_eq!(site.count("cdn.test", "/cdn/lib.js"), 0);
    assert_eq!(site.count(JS_HOST, "/css/site.css"), 0);
    for suffix in ["/cdn/lib.js", "/css/site.css"] {
        assert_eq!(
            resource_outcome(&store, &site, "/resources", suffix).as_deref(),
            Some("blocked_setting"),
            "{suffix}"
        );
    }
    assert!(site.count(JS_HOST, "/js/own.js") >= 1, "own scripts still load");
}

/// US4 scenario 5, FR-015, FR-016 and SC-006 (T049).
#[tokio::test]
async fn us4_resources_are_polite() {
    let _serial = SERIAL.lock().await;
    let Some(browser) = browser() else { return };
    let site = us4_site();
    let dir = tempfile::tempdir().unwrap();

    // `respect`: the disallowed script is never requested, is listed with
    // its rule and flags the page.
    let path = dir.path().join("respect.spiderlens");
    let status = crawl_rendering(&site, &path, browser.clone(), only("/resources$")).await;
    assert_eq!(status, "completed");
    let store = CrawlStore::open_read_only(&path).unwrap();
    assert_eq!(site.count(JS_HOST, "/private/app.js"), 0);
    assert_eq!(
        resource_outcome(&store, &site, "/resources", "/private/app.js").as_deref(),
        Some("blocked_robots")
    );
    let rule: Option<String> = one(
        &store,
        "SELECT robots_rule FROM render_resources WHERE url_id = ?1 AND url LIKE '%/private/app.js'",
        id(&store, &site, "/resources"),
    );
    assert!(rule.is_some_and(|r| r.contains("/private/")));
    let flagged: i64 = one(
        &store,
        "SELECT resources_blocked FROM renders WHERE url_id = ?1",
        id(&store, &site, "/resources"),
    );
    assert!(flagged >= 1, "the page has blocked resources");

    // `ignore_report`: requested, and still listed with the rule.
    site.reset();
    let path = dir.path().join("report.spiderlens");
    let status = crawl_rendering(&site, &path, browser.clone(), |c| {
        c.rendering.include = vec!["/resources$".into()];
        c.robots.mode = RobotsMode::IgnoreReport;
    })
    .await;
    assert_eq!(status, "completed");
    let store = CrawlStore::open_read_only(&path).unwrap();
    assert_eq!(site.count(JS_HOST, "/private/app.js"), 1);
    assert_eq!(
        resource_outcome(&store, &site, "/resources", "/private/app.js").as_deref(),
        Some("loaded")
    );
    let rule: Option<String> = one(
        &store,
        "SELECT robots_rule FROM render_resources WHERE url_id = ?1 AND url LIKE '%/private/app.js'",
        id(&store, &site, "/resources"),
    );
    assert!(rule.is_some(), "the rule is reported");

    // The per-site cap counts pages and resources together.
    site.reset();
    let path = dir.path().join("cap.spiderlens");
    let status = crawl_rendering(&site, &path, browser.clone(), |c| {
        c.rendering.include = vec!["/many$".into()];
        c.speed = SpeedSettings {
            max_concurrent: 8,
            max_per_host: 3,
            max_urls_per_sec: None,
            adaptive: false,
        };
    })
    .await;
    assert_eq!(status, "completed");
    let peak = site.peak_open(JS_HOST);
    assert!((2..=3).contains(&peak), "peak open requests to the host: {peak}");

    // A 429 with Retry-After holds the host: the next request waits.
    site.reset();
    let path = dir.path().join("retry.spiderlens");
    let status = crawl_rendering(&site, &path, browser, only("/retry$")).await;
    assert_eq!(status, "completed");
    let at = |p: &str| {
        site.log()
            .into_iter()
            .find(|e| e.host == JS_HOST && e.path == p)
            .map(|e| e.at_ms)
    };
    let a = at("/retry/a.js").expect("the 429 script was requested");
    let b = at("/retry/b.js").expect("the second script was requested");
    assert!(
        b >= a + 1500,
        "Retry-After: 2 held the host for only {} ms",
        b - a
    );
}

/// US4 scenarios 1 and 2, SC-007 (T050).
#[tokio::test]
async fn us4_pages_end_when_settled_or_at_the_cap_and_the_minimum_wait_holds() {
    let _serial = SERIAL.lock().await;
    let Some(browser) = browser() else { return };
    let site = us4_site();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settle.spiderlens");
    let status = crawl_rendering(&site, &path, browser.clone(), |c| {
        c.rendering.include = vec!["/settles$|/polls$".into()];
        c.rendering.max_render_secs = 5.0;
        c.extraction.store_html = true;
    })
    .await;
    assert_eq!(status, "completed");
    let store = CrawlStore::open_read_only(&path).unwrap();

    let settles = id(&store, &site, "/settles");
    assert_eq!(render_state(&store, &site, "/settles").unwrap().0, "rendered");
    let ms: i64 = one(&store, "SELECT render_ms FROM renders WHERE url_id = ?1", settles);
    assert!(
        (2000..4500).contains(&ms),
        "settled page took {ms} ms, not the 5 s cap"
    );
    let h2: i64 = one(
        &store,
        "SELECT COUNT(*) FROM rendered_text WHERE url_id = ?1 AND text = 'Settled after two seconds'",
        settles,
    );
    assert_eq!(h2, 1);

    let polls = id(&store, &site, "/polls");
    assert_eq!(render_state(&store, &site, "/polls").unwrap().0, "timed_out");
    let ms: i64 = one(&store, "SELECT render_ms FROM renders WHERE url_id = ?1", polls);
    assert!(ms <= 5000 + 2000, "the cap held for {ms} ms");
    assert_eq!(
        one::<i64>(
            &store,
            "SELECT COUNT(*) FROM rendered_pages WHERE url_id = ?1",
            polls
        ),
        1,
        "the DOM at the cap is stored"
    );
    assert_eq!(
        one::<i64>(
            &store,
            "SELECT COUNT(*) FROM rendered_bodies WHERE url_id = ?1",
            polls
        ),
        1
    );

    // The minimum wait holds a fast page.
    let quick = JsSiteBuilder::new()
        .page(
            "/",
            "<!doctype html><html><head><title>Quick</title></head><body><h1>Quick</h1><p>Nothing happens.</p></body></html>",
        )
        .start();
    let path = dir.path().join("minwait.spiderlens");
    let status = crawl_rendering(&quick, &path, browser, |c| c.rendering.min_wait_secs = 3.0).await;
    assert_eq!(status, "completed");
    let store = CrawlStore::open_read_only(&path).unwrap();
    let home = id(&store, &quick, "/");
    let ms: i64 = one(&store, "SELECT render_ms FROM renders WHERE url_id = ?1", home);
    assert!(ms >= 3000, "{ms} ms");
}

/// Crawls raw, then queues the given pages for rendering (T051).
async fn crawl_then_queue_paths(site: &JsSite, path: &Path, paths: &[String]) {
    let crawler = Crawler::create(
        path,
        config(true),
        &ModeInput::Spider {
            start_url: site.url(JS_HOST, "/"),
        },
    )
    .unwrap()
    .with_resolve(site.resolve());
    let (_tx, rx) = watch::channel(Control::Run);
    let s = crawler.run(rx, None).await.unwrap();
    assert_eq!(s.status, "completed");
    let store = CrawlStore::open(path).unwrap();
    for p in paths {
        let id = store.id_of(&site.url(JS_HOST, p)).unwrap().expect("crawled");
        store.enqueue_render(id, None).unwrap();
    }
}

/// US4 scenario 6, FR-037 (T051). Each slow page holds one request to the
/// site open while it renders, so the site's peak is the renders at once.
#[tokio::test]
async fn us4_the_stage_renders_as_many_pages_at_once_as_the_setting_says() {
    let _serial = SERIAL.lock().await;
    let Some(browser) = browser() else { return };
    let site = JsSiteBuilder::new().us4_slow_pages(6).start();
    let paths: Vec<String> = (0..6).map(|n| format!("/slow/{n}")).collect();
    let dir = tempfile::tempdir().unwrap();
    for (name, concurrency) in [("two.spiderlens", 2), ("one.spiderlens", 1)] {
        let path = dir.path().join(name);
        crawl_then_queue_paths(&site, &path, &paths).await;
        site.reset();
        assert_eq!(
            render_with(&site, &path, browser.clone(), Some(concurrency)).await,
            "completed"
        );
        assert_eq!(
            u64::from(concurrency),
            site.peak_open(JS_HOST),
            "peak open renders with concurrency {concurrency}"
        );
        let store = CrawlStore::open_read_only(&path).unwrap();
        for p in &paths {
            assert_eq!(render_state(&store, &site, p).unwrap().0, "rendered", "{p}");
        }
    }
}

/// US4 scenario 8 (T052).
#[tokio::test]
async fn us4_include_patterns_limit_which_pages_are_rendered() {
    let _serial = SERIAL.lock().await;
    let Some(browser) = browser() else { return };
    let site = us4_site();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("include.spiderlens");
    let status = crawl_rendering(&site, &path, browser, only("/products/")).await;
    assert_eq!(status, "completed");
    let store = CrawlStore::open_read_only(&path).unwrap();
    for p in ["/products/", "/products/a", "/products/b"] {
        assert_eq!(render_state(&store, &site, p).unwrap().0, "rendered", "{p}");
    }
    for p in ["/", "/plain", "/other", "/settles", "/width"] {
        assert_eq!(
            render_state(&store, &site, p).unwrap(),
            ("not_rendered".to_string(), Some("outside_rules".to_string())),
            "{p}"
        );
    }
}

/// FR-017 (T055): a cacheable script is fetched once for 50 pages, one
/// marked `no-store` every time.
#[tokio::test]
async fn us4_cacheable_scripts_are_fetched_once_and_no_store_scripts_every_time() {
    let _serial = SERIAL.lock().await;
    let Some(browser) = browser() else { return };
    let site = JsSiteBuilder::new().us4_cache_pages(50).start();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("cache.spiderlens");
    // One page at a time, so no two renders ask for the script together.
    let status = crawl_rendering(&site, &path, browser, |c| c.rendering.concurrency = Some(1)).await;
    assert_eq!(status, "completed");
    assert_eq!(site.count(JS_HOST, "/shared/max-age.js"), 1);
    assert_eq!(site.count(JS_HOST, "/shared/no-store.js"), 50);
    let store = CrawlStore::open_read_only(&path).unwrap();
    let count = |sql: &str| -> i64 { store.conn_for_tests().query_row(sql, [], |r| r.get(0)).unwrap() };
    assert_eq!(
        count("SELECT COUNT(*) FROM render_resources WHERE url LIKE '%/max-age.js' AND outcome = 'cached'"),
        49
    );
    assert_eq!(
        count("SELECT COUNT(*) FROM render_resources WHERE url LIKE '%/max-age.js' AND outcome = 'loaded'"),
        1
    );
    assert_eq!(
        count("SELECT COUNT(*) FROM render_resources WHERE url LIKE '%/no-store.js' AND outcome = 'cached'"),
        0
    );
}
