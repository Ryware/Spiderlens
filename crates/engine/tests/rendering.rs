//! The render stage inside a crawl (spec 014 T019). Needs a browser: skipped
//! when none is found, unless `SPIDERLENS_REQUIRE_BROWSER=1`.
//!
//! The first tests queue pages through the store, as a later render of a
//! finished crawl would; the others render from the fetch stage (T021).

use std::path::Path;

use spiderlens_config::{Change, CrawlConfig, ModeInput, RenderMode, SpeedSettings};
use spiderlens_engine::crawl::{Control, CrawlStore, Crawler, ResumeOptions};
use spiderlens_render::{BrowserInfo, LaunchOptions, browser_for_tests};
use spiderlens_test_fixtures::js_site::{JS_HOST, JsSite, JsSiteBuilder};
use tokio::sync::watch;

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
    let eligible = ["/", "/throws", "/logs", "/missing-script", "/heavy", "/challenge", "/steady"];
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
    assert!(one::<i64>(&store, "SELECT console_errors FROM renders WHERE url_id = ?1", throws) >= 1);

    // console.error and console.warn.
    let logs = id(&store, &site, "/logs");
    assert!(one::<i64>(&store, "SELECT console_errors FROM renders WHERE url_id = ?1", logs) >= 1);
    assert_eq!(one::<i64>(&store, "SELECT console_warnings FROM renders WHERE url_id = ?1", logs), 1);
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
    assert!(messages.contains(&("error".into(), "fixture error message".into())), "{messages:?}");
    assert!(messages.contains(&("warning".into(), "fixture warning message".into())), "{messages:?}");

    // Scenario 2: the 404 script is a failed resource of that page only.
    let missing = id(&store, &site, "/missing-script");
    assert_eq!(render_state(&store, &site, "/missing-script").unwrap().0, "rendered");
    assert_eq!(one::<i64>(&store, "SELECT resources_failed FROM renders WHERE url_id = ?1", missing), 1);
    let (outcome, status, error): (String, Option<i64>, Option<String>) = store
        .conn_for_tests()
        .query_row(
            "SELECT outcome, status, error FROM render_resources WHERE url_id = ?1 AND url LIKE '%/missing.js'",
            [missing],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!((outcome.as_str(), status, error.as_deref()), ("failed", Some(404), Some("HTTP 404")));
    // The pages the "JavaScript errors" and "Has failed resources" filters list.
    let ids = |sql: &str| -> Vec<i64> {
        let conn = store.conn_for_tests();
        let mut stmt = conn.prepare(sql).unwrap();
        stmt.query_map([], |r| r.get(0)).unwrap().collect::<Result<_, _>>().unwrap()
    };
    let with_errors = ids("SELECT url_id FROM renders WHERE console_errors > 0 ORDER BY url_id");
    assert!(with_errors.contains(&throws) && with_errors.contains(&logs));
    assert_eq!(ids("SELECT url_id FROM renders WHERE resources_failed > 0"), vec![missing]);

    // Scenario 3 (heap limit): not retried, raw row untouched.
    let (state, reason) = render_state(&store, &site, "/heavy").unwrap();
    assert_eq!((state.as_str(), reason.as_deref()), ("failed", Some("page_too_large")));
    let heavy = id(&store, &site, "/heavy");
    assert_eq!(one::<i64>(&store, "SELECT attempts FROM renders WHERE url_id = ?1", heavy), 1);
    assert_eq!(url_state(&store, &site, "/heavy").as_deref(), Some("crawled"));
    let title: String = one(&store, "SELECT title FROM urls WHERE id = ?1", heavy);
    assert_eq!(title, "Heavy");
    assert_eq!(
        one::<i64>(&store, "SELECT COUNT(*) FROM rendered_pages WHERE url_id = ?1", heavy),
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
    let Ok(dir) = std::fs::read_dir("/proc") else { return 0 };
    for e in dir.flatten() {
        let Some(pid) = e.file_name().to_str().and_then(|n| n.parse::<u32>().ok()) else {
            continue;
        };
        let Ok(raw) = std::fs::read(format!("/proc/{pid}/cmdline")) else {
            continue;
        };
        let text = String::from_utf8_lossy(&raw).replace('\0', " ");
        if text.contains(&marker) && !text.contains("--type=") && !text.contains("--render-guard") {
            let _ = std::process::Command::new("kill").args(["-9", &pid.to_string()]).status();
            killed += 1;
        }
    }
    killed
}

/// Seven slow pages: each render takes about a second.
fn slow_site() -> JsSite {
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
    b.script(JS_HOST, "/slow.js", "document.body.setAttribute('data-slow', '1');")
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
    let site = slow_site();
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
        let title: String = one(&store, "SELECT title FROM urls WHERE id = ?1", id(&store, &site, p));
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
        .query_row("SELECT COUNT(*) FROM renders WHERE state = 'rendered'", [], |r| r.get(0))
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
    let site = slow_site();
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
        summary.warnings.iter().any(|w| w.contains("could not be started again")),
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
