//! The render stage inside a crawl (spec 014 T019). Needs a browser: skipped
//! when none is found, unless `SPIDERLENS_REQUIRE_BROWSER=1`.
//!
//! The first tests queue pages through the store, as a later render of a
//! finished crawl would; the others render from the fetch stage (T021).

use std::path::Path;

use spiderlens_config::{Change, CrawlConfig, ModeInput, RenderMode, SpeedSettings};
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
    let mut settings = CrawlConfig::default().rendering;
    settings.mode = RenderMode::Javascript;
    settings.concurrency = Some(2);
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
    let before: Vec<u64> = (0..PAGES)
        .map(|i| site.count(JS_HOST, &format!("/slow/{i}")))
        .collect();

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
