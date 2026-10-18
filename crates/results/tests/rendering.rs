//! Spec 014 T028: the existing tabs show rendered values on a rendered
//! crawl, and raw values on request (FR-021). Crawls the user story 1
//! fixture with a browser; skipped when none is found, unless
//! `SPIDERLENS_REQUIRE_BROWSER=1`.

use std::path::PathBuf;

use spiderlens_config::{CrawlConfig, ModeInput, RenderMode, SpeedSettings};
use spiderlens_engine::crawl::{Control, Crawler};
use spiderlens_render::{LaunchOptions, browser_for_tests};
use spiderlens_results::synth::Synth;
use spiderlens_results::{Need, Reader, Sort, UrlScope, Values, ViewQuery, tabs};
use spiderlens_test_fixtures::js_site::{JS_HOST, JsSite, JsSiteBuilder, US3_PATHS};
use tokio::sync::watch;

struct Crawled {
    site: JsSite,
    path: PathBuf,
    _dir: tempfile::TempDir,
}

async fn crawl(mode: RenderMode) -> Option<Crawled> {
    crawl_pages(mode, JsSiteBuilder::new().us1_pages()).await
}

async fn crawl_pages(mode: RenderMode, pages: JsSiteBuilder) -> Option<Crawled> {
    let browser = browser_for_tests().expect("browser for tests")?;
    let site = pages.start();
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
        segment: None,
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

    // FR-034: the JavaScript tab explains that the crawl was not rendered,
    // and lists nothing.
    let fc = r.counts("javascript", UrlScope::All).unwrap();
    assert_eq!(fc.counts["all"], None);
    assert!(
        fc.unavailable["all"].contains("not rendered"),
        "{:?}",
        fc.unavailable
    );
    assert_eq!(r.tab_counts().unwrap().counts["javascript"], 0);
}

/// A synthetic rendered crawl (no browser): rendered titles and noindex,
/// one link only after rendering per page, some raw links gone.
fn synth() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("synth.spiderlens");
    Synth {
        urls: 3_000,
        links_per_page: 4,
        seed: 5,
        rendered: true,
        ..Synth::default()
    }
    .write(&path)
    .unwrap();
    (dir, path)
}

fn all_rows(r: &mut Reader, query: &ViewQuery) -> Vec<Vec<serde_json::Value>> {
    let page = r.open_view(query, 1).unwrap();
    let total = r.materialize(page.view).unwrap();
    let mut rows = Vec::new();
    while (rows.len() as u32) < total {
        rows.extend(r.rows(page.view, rows.len() as u32, 1000).unwrap().rows);
    }
    r.close_view(page.view).unwrap();
    rows
}

/// The rendered link counts subtract gone raw links and add rendered-only
/// ones; checked against the plain definition (raw links still on the
/// page after rendering, plus `rendered_links`) for every URL.
#[test]
fn rendered_link_counts_match_their_definition() {
    let (_dir, path) = synth();
    let conn = rusqlite::Connection::open(&path).unwrap();
    let gone: i64 = conn
        .query_row("SELECT COUNT(*) FROM raw_only_links", [], |r| r.get(0))
        .unwrap();
    assert!(gone > 0, "the synthetic file has gone links");
    let want = |key: &str| -> std::collections::HashMap<i64, i64> {
        let sql = format!(
            "SELECT u.id,
               (SELECT COUNT(*) FROM links l WHERE l.{key} = u.id AND NOT EXISTS
                  (SELECT 1 FROM raw_only_links g WHERE g.from_id = l.from_id AND g.pos = l.pos))
             + (SELECT COUNT(*) FROM rendered_links l WHERE l.{key} = u.id)
             FROM urls u"
        );
        let mut stmt = conn.prepare(&sql).unwrap();
        stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .map(Result::unwrap)
            .collect()
    };
    let (inl, outl) = (want("to_id"), want("from_id"));
    let mut r = Reader::open(&path).unwrap();
    let t = tabs().iter().find(|t| t.id == "internal").unwrap();
    let col = |id: &str| t.columns.iter().position(|c| c.id == id).unwrap() + 1;
    let (ci, co) = (col("inlinks"), col("outlinks"));
    let rows = all_rows(&mut r, &q("internal", "all", Some(Values::Rendered)));
    assert!(rows.len() > 1000);
    for row in &rows {
        let id = row[0].as_i64().unwrap();
        assert_eq!(row[ci].as_i64().unwrap(), inl[&id], "inlinks of {id}");
        assert_eq!(row[co].as_i64().unwrap(), outl[&id], "outlinks of {id}");
    }
}

/// In rendered mode the first screen is read in two steps (ids, then
/// columns); it must equal the start of the stored order.
#[test]
fn rendered_first_screen_matches_the_full_order() {
    let (_dir, path) = synth();
    let mut r = Reader::open(&path).unwrap();
    for (column, desc) in [
        ("title", false),
        ("title", true),
        ("inlinks", true),
        ("status_code", false),
    ] {
        let mut query = q("internal", "all", Some(Values::Rendered));
        query.sort = Some(Sort {
            column: column.into(),
            desc,
        });
        let first = r.open_view(&query, 60).unwrap();
        r.close_view(first.view).unwrap();
        let full = all_rows(&mut r, &query);
        assert_eq!(first.rows, full[..60].to_vec(), "{column} desc={desc}");
    }
}

/// Counts in rendered mode: the Page titles counts follow the rendered
/// titles; tabs that read no rendered field count as in raw mode. Their
/// "With issues" filter (spec 009) is left out: the issue rules read the
/// rendered titles too.
#[test]
fn rendered_counts_follow_the_rendered_values() {
    let (_dir, path) = synth();
    let mut r = Reader::open(&path).unwrap();
    for tab in ["internal", "external", "response-codes", "url", "images"] {
        let mut raw = r
            .counts_with(tab, UrlScope::All, Some(Values::Raw))
            .unwrap()
            .counts;
        let mut rendered = r
            .counts_with(tab, UrlScope::All, Some(Values::Rendered))
            .unwrap()
            .counts;
        raw.remove("with_issues");
        rendered.remove("with_issues");
        assert_eq!(raw, rendered, "{tab}");
    }
    let raw = r
        .counts_with("page-titles", UrlScope::All, Some(Values::Raw))
        .unwrap()
        .counts;
    let rendered = r
        .counts_with("page-titles", UrlScope::All, Some(Values::Rendered))
        .unwrap()
        .counts;
    assert_eq!(raw["all"], rendered["all"]);
    assert_ne!(
        raw["duplicate"], rendered["duplicate"],
        "a third of the titles change after rendering"
    );
    for (counts, values) in [(raw, Values::Raw), (rendered, Values::Rendered)] {
        let listed = all_rows(&mut r, &q("page-titles", "duplicate", Some(values))).len() as u64;
        assert_eq!(counts["duplicate"], Some(listed), "{values:?}");
    }
}

// ---------------------------------------------------------------------------
// Spec 014 US3 (T041, T045): the JavaScript tab, the detail data, the diff.
// ---------------------------------------------------------------------------

/// A synthetic rendered crawl with `render_comparisons` rows written by
/// hand (no browser), for the JavaScript tab's filters and labels.
fn synth_with_comparisons() -> (tempfile::TempDir, PathBuf, Vec<i64>) {
    let (dir, path) = synth();
    let conn = rusqlite::Connection::open(&path).unwrap();
    let ids: Vec<i64> = conn
        .prepare("SELECT url_id FROM renders ORDER BY url_id LIMIT 8")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(ids.len(), 8);
    // (words raw, rendered, links rendered only, title diff, noindex diff, dependence)
    let rows: [(i64, i64, i64, &str, &str, f64); 5] = [
        (100, 200, 3, "same", "same", 55.0),    // ids[0]: content, hidden
        (100, 140, 0, "added", "same", 10.0),   // ids[1]: +40 words only; title added, hidden
        (1000, 1100, 0, "same", "same", 9.0),   // ids[2]: +10 %, not content
        (100, 100, 0, "same", "removed", 0.0),  // ids[3]: noindex only in the original
        (100, 100, 0, "changed", "added", 0.0), // ids[4]: title updated, noindex only rendered
    ];
    for (id, (raw, rendered, js_links, title, noindex, dependence)) in ids.iter().zip(rows) {
        conn.execute(
            "INSERT INTO render_comparisons (url_id, words_raw, words_rendered, words_change, links_raw,
               links_rendered, links_rendered_only, links_raw_only, title_diff, description_diff, h1_diff,
               canonical_diff, noindex_diff, nofollow_diff, hreflang_diff, structured_rendered_only,
               dependence)
             VALUES (?1, ?2, ?3, ?3 - ?2, 10, 10 + ?4, ?4, 0, ?5, 'same', 'same', 'same', ?6, 'same', 'same',
               0, ?7)",
            rusqlite::params![id, raw, rendered, js_links, title, noindex, dependence],
        )
        .unwrap();
    }
    conn.execute(
        "UPDATE renders SET state = 'failed', reason = 'crash' WHERE url_id = ?1",
        [ids[5]],
    )
    .unwrap();
    conn.execute(
        "UPDATE renders SET console_errors = 2, resources_blocked = 1 WHERE url_id = ?1",
        [ids[6]],
    )
    .unwrap();
    conn.execute(
        "UPDATE renders SET js_redirect = '/elsewhere', bot_challenge = 1 WHERE url_id = ?1",
        [ids[7]],
    )
    .unwrap();
    (dir, path, ids)
}

#[test]
fn javascript_filters_count_and_list_the_right_pages() {
    let (_dir, path, ids) = synth_with_comparisons();
    let mut r = Reader::open(&path).unwrap();
    let fc = r.counts("javascript", UrlScope::All).unwrap();
    let rendered_pages: i64 = rusqlite::Connection::open(&path)
        .unwrap()
        .query_row("SELECT COUNT(*) FROM renders", [], |r| r.get(0))
        .unwrap();
    let n = |f: &str| fc.counts[f].unwrap_or_else(|| panic!("{f} unavailable"));
    assert_eq!(n("all") as i64, rendered_pages);
    assert_eq!(n("rendered") as i64, rendered_pages - 1);
    assert_eq!(n("failed"), 1);
    assert_eq!(n("not_rendered") + n("timed_out"), 0);
    assert_eq!(n("js_links"), 1);
    assert_eq!(
        n("js_content"),
        1,
        "only the first page grew by 20 % and 50 words"
    );
    assert_eq!(n("title_only_rendered"), 1);
    assert_eq!(n("title_updated"), 1);
    assert_eq!(n("noindex_only_original"), 1);
    assert_eq!(n("noindex_only_rendered"), 1);
    assert_eq!(
        n("hidden_content"),
        2,
        "dependence over 50 %, or a title only after rendering"
    );
    assert_eq!(n("js_errors"), 1);
    assert_eq!(n("blocked_resources"), 1);
    assert_eq!(n("failed_resources"), 0);
    assert_eq!(n("js_redirect"), 1);
    assert_eq!(n("bot_challenge"), 1);
    // FR-027, FR-030: the thresholds are in the labels.
    assert_eq!(
        fc.labels["js_content"],
        "Contains JavaScript content (over 20% and 50 words)"
    );
    assert_eq!(
        fc.labels["hidden_content"],
        "Content hidden from non-rendering crawlers (over 50%)"
    );
    // Every count equals the rows its filter lists.
    for f in [
        "all",
        "rendered",
        "js_content",
        "hidden_content",
        "failed",
        "js_errors",
    ] {
        assert_eq!(
            all_rows(&mut r, &q("javascript", f, None)).len() as u64,
            n(f),
            "{f}"
        );
    }
    // The columns of FR-025 hold the stored values.
    let t = tabs().iter().find(|t| t.id == "javascript").unwrap();
    let col = |id: &str| t.columns.iter().position(|c| c.id == id).unwrap() + 1;
    let rows = all_rows(&mut r, &q("javascript", "js_content", None));
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0][0].as_i64(), Some(ids[0]));
    assert_eq!(rows[0][col("words_raw")].as_i64(), Some(100));
    assert_eq!(rows[0][col("words_rendered")].as_i64(), Some(200));
    assert_eq!(rows[0][col("words_change")].as_i64(), Some(100));
    assert_eq!(rows[0][col("js_links")].as_i64(), Some(3));
    assert_eq!(rows[0][col("js_dependence")].as_f64(), Some(55.0));
    assert_eq!(rows[0][col("render_status")].as_str(), Some("Rendered"));
    let failed = all_rows(&mut r, &q("javascript", "failed", None));
    assert_eq!(failed[0][col("render_status")].as_str(), Some("Failed (crash)"));
}

#[test]
fn a_changed_threshold_in_the_crawl_settings_moves_the_filters() {
    let (_dir, path, _) = synth_with_comparisons();
    // The thresholds are read from the configuration stored with the crawl.
    let mut config = CrawlConfig::default();
    config.rendering.mode = RenderMode::Javascript;
    config.rendering.js_content_min_percent = 5;
    config.rendering.js_content_min_words = 30;
    config.rendering.hidden_content_percent = 80;
    spiderlens_results::synth::store_config(&path, config).unwrap();
    let mut r = Reader::open(&path).unwrap();
    let fc = r.counts("javascript", UrlScope::All).unwrap();
    // +100 words and +40 words (40 %) now count; +100 words at 10 % too.
    assert_eq!(fc.counts["js_content"], Some(3));
    // Dependence 55 % is below 80 %; only the added title is left.
    assert_eq!(fc.counts["hidden_content"], Some(1));
    assert_eq!(
        fc.labels["js_content"],
        "Contains JavaScript content (over 5% and 30 words)"
    );
}

#[test]
fn link_lists_mark_where_each_link_was_found() {
    let (_dir, path) = synth();
    let id: i64 = rusqlite::Connection::open(&path)
        .unwrap()
        .query_row("SELECT from_id FROM raw_only_links LIMIT 1", [], |r| r.get(0))
        .unwrap();
    let mut r = Reader::open(&path).unwrap();
    let origins = |r: &mut Reader, values| -> Vec<String> {
        let page = r
            .list_values(id, spiderlens_results::ListKind::Outlinks, None, values, 0, 100)
            .unwrap();
        assert_eq!(page.total as usize, page.rows.len());
        let mut o: Vec<String> = page
            .rows
            .iter()
            .map(|x| x.origin.clone().unwrap_or_default())
            .collect();
        o.sort();
        o
    };
    // Four raw links, one of them gone after rendering, and one only after.
    assert_eq!(
        origins(&mut r, Some(Values::Rendered)),
        ["both", "both", "both", "raw", "rendered"]
    );
    // The raw version has no link found only after rendering.
    assert_eq!(
        origins(&mut r, Some(Values::Raw)),
        ["both", "both", "both", "raw"]
    );
    // A page that was not rendered has no origins.
    let other: i64 = rusqlite::Connection::open(&path)
        .unwrap()
        .query_row(
            "SELECT id FROM urls WHERE id NOT IN (SELECT url_id FROM renders) AND id IN (SELECT from_id FROM links) LIMIT 1",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let page = r
        .list_values(
            other,
            spiderlens_results::ListKind::Outlinks,
            None,
            Some(Values::Rendered),
            0,
            100,
        )
        .unwrap();
    assert!(!page.rows.is_empty());
    assert!(page.rows.iter().all(|x| x.origin.is_none()));
}

#[test]
fn resources_and_console_lists_page_through_their_rows() {
    let (_dir, path) = synth();
    let id: i64 = rusqlite::Connection::open(&path)
        .unwrap()
        .query_row("SELECT url_id FROM renders ORDER BY url_id LIMIT 1", [], |r| {
            r.get(0)
        })
        .unwrap();
    {
        let conn = rusqlite::Connection::open(&path).unwrap();
        for pos in 0..5i64 {
            conn.execute(
                "INSERT INTO render_resources (url_id, pos, url, type, host, third_party, outcome, status, bytes)
                 VALUES (?1, ?2, 'https://cdn.test/a' || ?2 || '.js', 'script', 'cdn.test', 1, 'loaded', 200, 1000)",
                rusqlite::params![id, pos],
            )
            .unwrap();
        }
        conn.execute(
            "INSERT INTO console_entries (url_id, pos, level, message, source_url, line, col)
             VALUES (?1, 0, 'error', 'boom', 'https://js.test/app.js', 12, 3)",
            [id],
        )
        .unwrap();
    }
    let mut r = Reader::open(&path).unwrap();
    let res = r.list(id, spiderlens_results::ListKind::Resources, 1, 2).unwrap();
    assert_eq!(res.total, 5);
    assert_eq!(res.resources.len(), 2);
    assert_eq!(res.resources[0].url, "https://cdn.test/a1.js");
    assert!(res.resources[0].third_party);
    assert_eq!(res.resources[0].status, Some(200));
    assert_eq!(res.resources[0].bytes, Some(1000));
    let con = r.list(id, spiderlens_results::ListKind::Console, 0, 10).unwrap();
    assert_eq!(con.total, 1);
    assert_eq!(con.console[0].message, "boom");
    assert_eq!(con.console[0].line, Some(12));
    assert!(con.resources.is_empty());
}

/// The paths each page of the US3 set must appear under, besides All: the
/// exact set of JavaScript filters (SC-005, US3 scenarios 1 to 7).
fn expected_filters() -> Vec<(&'static str, Vec<&'static str>)> {
    vec![
        ("/", vec!["rendered"]),
        ("/title-changed", vec!["rendered", "title_updated"]),
        (
            "/title-added",
            vec!["rendered", "title_only_rendered", "hidden_content"],
        ),
        (
            "/description-added",
            vec!["rendered", "description_only_rendered"],
        ),
        ("/description-changed", vec!["rendered", "description_updated"]),
        // An element a script removes counts as updated (US3 scenario 5).
        ("/h1-removed", vec!["rendered", "h1_updated"]),
        (
            "/h1-added",
            vec!["rendered", "h1_only_rendered", "hidden_content"],
        ),
        ("/h1-changed", vec!["rendered", "h1_updated"]),
        ("/canonical-changed", vec!["rendered", "canonical_mismatch"]),
        ("/canonical-added", vec!["rendered", "canonical_only_rendered"]),
        ("/noindex-added", vec!["rendered", "noindex_only_rendered"]),
        ("/noindex-removed", vec!["rendered", "noindex_only_original"]),
        ("/nofollow-added", vec!["rendered", "nofollow_only_rendered"]),
        ("/nofollow-removed", vec!["rendered", "nofollow_only_original"]),
        ("/links-added", vec!["rendered", "js_links"]),
        ("/text-grown", vec!["rendered", "js_content", "hidden_content"]),
        ("/jsonld-added", vec!["rendered", "structured_only_rendered"]),
        ("/hreflang-changed", vec!["rendered"]),
        ("/unchanged", vec!["rendered"]),
    ]
}

#[tokio::test]
async fn every_difference_page_appears_in_exactly_its_filters() {
    let Some(c) = crawl_pages(RenderMode::Javascript, JsSiteBuilder::new().us3_pages()).await else {
        return;
    };
    let mut r = Reader::open(&c.path).unwrap();
    let s = &c.site;
    let base = s.url(JS_HOST, "");
    let t = tabs().iter().find(|t| t.id == "javascript").unwrap();
    let mut found: std::collections::BTreeMap<String, std::collections::BTreeSet<String>> =
        std::collections::BTreeMap::new();
    for f in &t.filters {
        if f.id == "with_issues" {
            continue;
        }
        for row in all_rows(&mut r, &q("javascript", &f.id, None)) {
            let url = row[1].as_str().unwrap();
            let path = url.strip_prefix(&base).unwrap_or(url).to_string();
            if f.id == "all" {
                found.entry(path).or_default();
            } else {
                found.entry(path).or_default().insert(f.id.to_string());
            }
        }
    }
    // Every page of the set is listed, and nothing else.
    assert_eq!(found.len(), US3_PATHS.len() + 1, "{:?}", found.keys());
    for (path, want) in expected_filters() {
        let got: Vec<&str> = found[path].iter().map(String::as_str).collect();
        let mut want = want;
        want.sort_unstable();
        assert_eq!(got, want, "{path}");
    }

    // The raw and rendered values shown for the pages (US3 scenarios 1 to 7).
    let id = |path: &str| -> i64 {
        let url = s.url(JS_HOST, path);
        let conn = rusqlite::Connection::open(&c.path).unwrap();
        conn.query_row("SELECT id FROM urls WHERE url = ?1", [url], |row| row.get(0))
            .unwrap()
    };
    let mut cmp = |path: &str| {
        let i = id(path);
        let d = r.detail(i).unwrap();
        let render = d.render.expect("render detail");
        assert_eq!(render.state, "rendered", "{path}");
        render.comparison.expect("comparison")
    };
    let title = cmp("/title-changed").title;
    assert_eq!(
        (title.raw.as_deref(), title.rendered.as_deref()),
        (Some("Loading"), Some("Red Shoes | Shop"))
    );
    let title = cmp("/title-added").title;
    assert_eq!(
        (title.diff.as_str(), title.raw, title.rendered.as_deref()),
        ("added", None, Some("Added title"))
    );
    let d = cmp("/description-changed").description;
    assert_eq!(
        (d.raw.as_deref(), d.rendered.as_deref()),
        (Some("Old description"), Some("New description"))
    );
    let h = cmp("/h1-changed").h1;
    assert_eq!(
        (h.raw.as_deref(), h.rendered.as_deref()),
        (Some("Old heading"), Some("New heading"))
    );
    let k = cmp("/canonical-changed").canonical;
    assert!(k.raw.unwrap().ends_with("/canonical-changed"));
    assert_eq!(k.rendered.unwrap(), s.url(JS_HOST, "/"));
    let n = cmp("/noindex-added").noindex;
    assert_eq!(
        (n.diff.as_str(), n.raw, n.rendered.as_deref()),
        ("added", None, Some("noindex"))
    );
    let n = cmp("/nofollow-removed").nofollow;
    assert_eq!(
        (n.diff.as_str(), n.raw.as_deref(), n.rendered),
        ("removed", Some("nofollow"), None)
    );
    let l = cmp("/links-added");
    assert_eq!(
        (l.links_raw, l.links_rendered, l.links_rendered_only),
        (28, 40, 12)
    );
    let g = cmp("/text-grown");
    assert!((50..60).contains(&g.words_raw), "{}", g.words_raw);
    assert!((900..910).contains(&g.words_rendered), "{}", g.words_rendered);
    assert!(g.dependence > 90.0, "{}", g.dependence);
    assert_eq!(cmp("/jsonld-added").structured_rendered_only, 1);
    assert_eq!(cmp("/hreflang-changed").hreflang.diff, "changed");
    let u = cmp("/unchanged");
    assert_eq!(u.title.diff, "same");
    assert_eq!(u.words_change, 0);
    assert_eq!(u.dependence, 0.0);
}

#[tokio::test]
async fn the_html_diff_shows_what_a_script_changed() {
    let Some(c) = crawl_pages(RenderMode::Javascript, JsSiteBuilder::new().us3_pages()).await else {
        return;
    };
    let mut r = Reader::open(&c.path).unwrap();
    assert!(r.features().has(Need::RenderedHtml));
    let id = |path: &str| -> i64 {
        let conn = rusqlite::Connection::open(&c.path).unwrap();
        conn.query_row(
            "SELECT id FROM urls WHERE url = ?1",
            [c.site.url(JS_HOST, path)],
            |row| row.get(0),
        )
        .unwrap()
    };
    // The rendered version is the serialised DOM, the raw one what was sent.
    let raw = r
        .page_html_version(id("/title-changed"), spiderlens_results::HtmlVersion::Raw)
        .unwrap();
    let rendered = r
        .page_html_version(id("/title-changed"), spiderlens_results::HtmlVersion::Rendered)
        .unwrap();
    assert!(raw.html.unwrap().contains("<title>Loading</title>"));
    assert!(rendered.html.unwrap().contains("<title>Red Shoes | Shop</title>"));

    // A changed title is one hunk with both lines.
    let d = r.html_diff(id("/title-changed"), 0, 100).unwrap();
    assert_eq!(d.total_hunks, 1, "{d:?}");
    assert_eq!(d.hunks[0].raw_lines, ["Loading"]);
    assert_eq!(d.hunks[0].rendered_lines, ["Red Shoes | Shop"]);

    // A page without a script has no hunks, though the browser wrote its
    // DOCTYPE and attributes its own way.
    assert_eq!(r.html_diff(id("/"), 0, 100).unwrap().total_hunks, 0);
    assert_eq!(r.html_unified(id("/")).unwrap(), "");

    // A script that only sets an attribute on <body> is one changed line.
    let u = r.html_unified(id("/unchanged")).unwrap();
    assert!(u.starts_with("--- raw\n+++ rendered\n"), "{u}");
    assert!(u.contains("data-ready"), "{u}");
}
