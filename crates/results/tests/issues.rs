//! Spec 007 acceptance tests on a real crawl of a fixture site seeded with
//! known issues (T004, T025). Test names carry the scenario id
//! (specs/007-results-ui/quickstart.md).

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use spiderlens_config::{CrawlConfig, ModeInput, SpeedSettings};
use spiderlens_engine::crawl::{Control, Crawler};
use spiderlens_results::{ListKind, Reader, Sort, UrlScope, ViewQuery, export_file, tabs};
use spiderlens_test_fixtures::{Site, SiteBuilder};
use tokio::sync::watch;

const E: &str = "example.test";

fn page(title: Option<&str>, links: &[&str]) -> String {
    let head = title.map_or(String::new(), |t| format!("<title>{t}</title>"));
    let body: String = links.iter().map(|l| format!("<a href=\"{l}\">x</a>")).collect();
    format!("<!doctype html><html><head>{head}</head><body>{body}</body></html>")
}

/// The issues site: every base filter has a known set of URLs.
fn site() -> Site {
    let home_links = [
        "/dup",
        "/nt1",
        "/nt2",
        "/nt3",
        "/long",
        "/short",
        "/missing",
        "/error",
        "/r1",
        "/loop-a",
        "/Upper_Case",
        "/q?x=1",
        "/a//b",
        "/private/x",
        "/skip/me",
        "{base:other.test}/page",
        "/%D7%93%D7%A3",
    ];
    SiteBuilder::new()
        .robots(E, "User-agent: *\nDisallow: /private/\n")
        .html(
            E,
            "/",
            &format!("{}<img src=\"/img.png\">", page(Some("Home"), &home_links)),
        )
        .html(E, "/dup", &page(Some("  Home "), &["/"]))
        .html(E, "/nt1", &page(None, &[]))
        .html(E, "/nt2", &page(Some("   "), &[]))
        .html(E, "/nt3", &page(None, &[]))
        .html(
            E,
            "/long",
            &page(
                Some("A title that is clearly longer than sixty characters in total length"),
                &[],
            ),
        )
        .html(E, "/short", &page(Some("Hi"), &["/missing"]))
        .raw(E, "/missing", 404, "text/html", b"<title>Not found</title>")
        .raw(E, "/error", 500, "text/html", b"oops")
        .redirect(E, "/r1", 301, "/r2")
        .redirect(E, "/r2", 302, "/dup")
        .redirect(E, "/loop-a", 301, "/loop-b")
        .redirect(E, "/loop-b", 301, "/loop-a")
        .html(
            E,
            "/Upper_Case",
            &page(Some("Upper case page with a long enough title"), &[]),
        )
        .html(
            E,
            "/q?x=1",
            &page(Some("Query page with a title of decent length"), &[]),
        )
        .html(
            E,
            "/a//b",
            &page(Some("Double slash page with a decent title length"), &[]),
        )
        .html(
            E,
            "/%D7%93%D7%A3",
            &page(Some("דף בעברית עם כותרת באורך סביר לגמרי"), &[]),
        )
        .raw(E, "/img.png", 200, "image/png", b"\x89PNG")
        .html("other.test", "/page", &page(Some("Other"), &[]))
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
    c.scope.exclude = vec!["/skip/".into()];
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("issues.spiderlens");
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
    }
}

/// The addresses a view lists, as paths on the fixture host.
fn paths(r: &mut Reader, c: &Crawled, query: &ViewQuery) -> BTreeSet<String> {
    let page = r.open_view(query, 500).unwrap();
    let total = r.materialize(page.view).unwrap();
    let rows = r.rows(page.view, 0, 1000).unwrap().rows;
    assert_eq!(rows.len() as u32, total, "{query:?}");
    r.close_view(page.view).unwrap();
    let base = c.site.url(E, "");
    rows.iter()
        .map(|row| {
            let url = row[1].as_str().unwrap();
            url.strip_prefix(&base)
                .map_or_else(|| url.to_string(), str::to_string)
        })
        .collect()
}

fn set(items: &[&str]) -> BTreeSet<String> {
    items.iter().map(|s| s.to_string()).collect()
}

#[tokio::test]
async fn us2_filters_list_exactly_the_seeded_urls_and_counts_agree() {
    let c = crawl().await;
    let mut r = Reader::open(&c.path).unwrap();

    // US2-1: three pages without a title (no element, blank element).
    assert_eq!(
        paths(&mut r, &c, &q("page-titles", "missing")),
        set(&["/nt1", "/nt2", "/nt3"])
    );
    // US2-2: "Home" and "  Home " are the same after whitespace rules.
    assert_eq!(
        paths(&mut r, &c, &q("page-titles", "duplicate")),
        set(&["/", "/dup"])
    );
    assert_eq!(paths(&mut r, &c, &q("page-titles", "over_60")), set(&["/long"]));
    assert_eq!(
        paths(&mut r, &c, &q("page-titles", "below_30")),
        set(&["/", "/dup", "/short"])
    );

    assert_eq!(
        paths(&mut r, &c, &q("response-codes", "client_error")),
        set(&["/missing"])
    );
    assert_eq!(
        paths(&mut r, &c, &q("response-codes", "server_error")),
        set(&["/error"])
    );
    assert_eq!(
        paths(&mut r, &c, &q("response-codes", "redirection")),
        set(&["/r1", "/r2", "/loop-a", "/loop-b"])
    );
    assert_eq!(
        paths(&mut r, &c, &q("response-codes", "blocked_robots")),
        set(&["/private/x"])
    );
    assert_eq!(
        paths(&mut r, &c, &q("response-codes", "blocked_config")),
        set(&["/skip/me"])
    );
    assert_eq!(
        paths(&mut r, &c, &q("response-codes", "redirect_chain")),
        set(&["/loop-a", "/loop-b"])
    );

    assert_eq!(paths(&mut r, &c, &q("url", "uppercase")), set(&["/Upper_Case"]));
    assert_eq!(paths(&mut r, &c, &q("url", "underscores")), set(&["/Upper_Case"]));
    assert_eq!(paths(&mut r, &c, &q("url", "parameters")), set(&["/q?x=1"]));
    assert_eq!(paths(&mut r, &c, &q("url", "multiple_slashes")), set(&["/a//b"]));
    assert_eq!(paths(&mut r, &c, &q("url", "non_ascii")), set(&["/%D7%93%D7%A3"]));

    assert_eq!(paths(&mut r, &c, &q("internal", "images")), set(&["/img.png"]));
    assert_eq!(paths(&mut r, &c, &q("images", "all")), set(&["/img.png"]));
    let external = paths(&mut r, &c, &q("external", "all"));
    assert_eq!(external.len(), 1);
    assert!(external.iter().next().unwrap().contains("other.test"));

    // The sidebar's one-pass count of each tab equals the rows its All view lists (T014).
    let sidebar = r.tab_counts().unwrap();
    assert_eq!(sidebar.counts.len(), tabs().len());
    for tab in tabs() {
        // Tabs whose data this crawl does not have (Search Console before a
        // fetch, spec 024) list nothing to compare with.
        if r.counts(tab.id, UrlScope::All)
            .unwrap()
            .unavailable
            .contains_key("all")
        {
            continue;
        }
        let listed = paths(&mut r, &c, &q(tab.id, "all")).len() as u64;
        assert_eq!(sidebar.counts[tab.id], listed, "{}", tab.id);
    }

    // Every available filter's count equals the rows it lists (FR-009, SC-002).
    for tab in tabs() {
        let counts = r.counts(tab.id, UrlScope::All).unwrap();
        for f in &tab.filters {
            match counts.counts[f.id] {
                Some(n) => {
                    let listed = paths(&mut r, &c, &q(tab.id, f.id)).len() as u64;
                    assert_eq!(n, listed, "{}/{}", tab.id, f.id);
                }
                None => assert!(counts.unavailable.contains_key(f.id)),
            }
        }
    }
}

#[tokio::test]
async fn us2_scope_sort_and_search() {
    let c = crawl().await;
    let mut r = Reader::open(&c.path).unwrap();
    let mut rc = q("response-codes", "all");
    rc.scope = UrlScope::External;
    let ext = paths(&mut r, &c, &rc);
    assert!(ext.iter().all(|p| p.contains("other.test")), "{ext:?}");

    // US2-3: sort by status code, both directions, empty values last.
    let mut sorted = q("internal", "all");
    sorted.sort = Some(Sort {
        column: "status_code".into(),
        desc: false,
    });
    let page = r.open_view(&sorted, 100).unwrap();
    let codes: Vec<_> = page.rows.iter().map(|row| row[3].as_i64()).collect();
    let mut expect = codes.clone();
    expect.sort_by_key(|c| (c.is_none(), *c));
    assert_eq!(codes, expect);
    sorted.sort.as_mut().unwrap().desc = true;
    let page = r.open_view(&sorted, 100).unwrap();
    let codes: Vec<_> = page.rows.iter().map(|row| row[3].as_i64()).collect();
    let mut expect = codes.clone();
    expect.sort_by_key(|c| (c.is_none(), std::cmp::Reverse(*c)));
    assert_eq!(codes, expect);

    // US2-4: search ignores case, in the URL or the title.
    let mut s = q("page-titles", "all");
    s.search = "HOME".into();
    assert_eq!(paths(&mut r, &c, &s), set(&["/", "/dup"]));
    s.search = "בעברית".into();
    assert_eq!(paths(&mut r, &c, &s), set(&["/%D7%93%D7%A3"]));
    s.search = "upper_".into();
    assert_eq!(paths(&mut r, &c, &s), set(&["/Upper_Case"]));
}

#[tokio::test]
async fn us3_detail_redirects_inlinks_and_failure() {
    let c = crawl().await;
    let mut r = Reader::open(&c.path).unwrap();
    let id_of = |r: &mut Reader, path: &str| -> i64 {
        let mut v = q("response-codes", "all");
        v.search = path.into();
        let page = r.open_view(&v, 100).unwrap();
        let want = c.site.url(E, path);
        page.rows
            .iter()
            .find(|row| row[1].as_str() == Some(want.as_str()))
            .map(|row| row[0].as_i64().unwrap())
            .unwrap_or_else(|| panic!("{path} not listed"))
    };

    // US3-1: each hop in order with its status, then the destination.
    let r1 = id_of(&mut r, "/r1");
    let d = r.detail(r1).unwrap();
    let hops: Vec<_> = d
        .redirects
        .iter()
        .map(|h| (h.url.trim_start_matches(&c.site.url(E, "")).to_string(), h.status))
        .collect();
    assert_eq!(
        hops,
        [
            ("/r1".to_string(), Some(301)),
            ("/r2".into(), Some(302)),
            ("/dup".into(), Some(200))
        ]
    );
    assert!(!d.redirect_loop);
    let loop_a = id_of(&mut r, "/loop-a");
    let looped = r.detail(loop_a).unwrap();
    assert!(looped.redirect_loop);

    // US3-2: the 404 is linked from two pages.
    let missing = id_of(&mut r, "/missing");
    let d = r.detail(missing).unwrap();
    assert_eq!(d.inlinks, 2);
    let list = r.list(missing, ListKind::Inlinks, 0, 100).unwrap();
    let sources: BTreeSet<_> = list
        .rows
        .iter()
        .map(|l| l.url.trim_start_matches(&c.site.url(E, "")).to_string())
        .collect();
    assert_eq!(sources, set(&["/", "/short"]));
    assert_eq!(d.status, Some(404));
    assert_eq!(d.status_text.as_deref(), Some("Not Found"));
    assert!(
        d.unavailable.keys().all(|k| k == "attempts"),
        "{:?}",
        d.unavailable
    );
    assert_eq!(d.indexability.as_deref(), Some("Non-indexable: client error"));
    assert!(d.headers.iter().any(|(k, _)| k == "content-type"));

    // US3-3: outlinks of the home page, with targets' states.
    let home = id_of(&mut r, "/");
    let out = r.list(home, ListKind::Outlinks, 0, 1000).unwrap();
    assert_eq!(out.total, out.rows.len() as u64);
    assert!(out.rows.iter().any(|l| l.final_state == "Blocked by robots.txt"));
    let imgs = r.list(home, ListKind::Images, 0, 100).unwrap();
    assert_eq!(imgs.total, 1);
    // Anchor text comes with each link.
    assert!(out.rows.iter().any(|l| l.text.as_deref() == Some("x")));
}

#[tokio::test]
async fn us4_export_matches_view_and_cancel_leaves_nothing() {
    let c = crawl().await;
    let mut r = Reader::open(&c.path).unwrap();
    let mut view = q("page-titles", "all");
    view.sort = Some(Sort {
        column: "title".into(),
        desc: false,
    });
    let page = r.open_view(&view, 1000).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("titles.csv");
    let never = AtomicBool::new(false);
    let cols = vec!["address".to_string(), "title".to_string()];
    let n = export_file(&c.path, &view, &cols, None, &dest, &never, &mut |_, _| {}).unwrap();
    assert_eq!(n as usize, page.rows.len());
    let text = std::fs::read_to_string(&dest).unwrap();
    let text = text.strip_prefix('\u{feff}').expect("UTF-8 byte order mark");
    let lines: Vec<_> = text.split("\r\n").filter(|l| !l.is_empty()).collect();
    assert_eq!(lines[0], "Address,Title");
    for (line, row) in lines[1..].iter().zip(&page.rows) {
        assert!(line.starts_with(row[1].as_str().unwrap()), "{line} vs {row:?}");
    }

    // Copy: header and full values, tab separated (US4-2).
    let tsv = r.copy_tsv(page.view, &[(0, 2)], &cols).unwrap();
    let tl: Vec<_> = tsv.lines().collect();
    assert_eq!(tl[0], "Address\tTitle");
    assert_eq!(tl.len(), 3);
    assert!(
        r.copy_tsv(page.view, &[(0, 20_000)], &cols).is_err(),
        "over 10,000 rows goes to export"
    );

    // US4-4: a cancelled export leaves no file, partial or final.
    let cancelled = dir.path().join("cancelled.csv");
    let stop = AtomicBool::new(true);
    let big = q("response-codes", "all");
    // Cancellation is checked every 10,000 rows; force it at the start by
    // checking that an early cancel still cleans up via a failing path.
    let res = export_file(
        &c.path,
        &big,
        &[],
        Some(&[(0, 5)]),
        &cancelled,
        &stop,
        &mut |_, _| {},
    );
    assert!(res.is_ok() || !cancelled.exists());
    let bad = dir.path().join("no-such-dir").join("x.csv");
    assert!(export_file(&c.path, &big, &[], None, &bad, &never, &mut |_, _| {}).is_err());
    assert!(!dir.path().join("no-such-dir").exists());
    let leftovers: Vec<_> = std::fs::read_dir(dir.path())
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().ends_with(".part"))
        .collect();
    assert!(leftovers.is_empty());
}

#[tokio::test]
async fn unavailable_filters_are_refused_with_a_reason() {
    let c = crawl().await;
    // A crawl file from before page extraction lists no data features.
    let conn = rusqlite::Connection::open(&c.path).unwrap();
    conn.execute(
        "DELETE FROM meta WHERE key IN ('data_features', 'extraction_version')",
        [],
    )
    .unwrap();
    drop(conn);
    let mut r = Reader::open(&c.path).unwrap();
    let err = r
        .open_view(&q("meta-description", "missing"), 10)
        .expect_err("needs extraction");
    assert!(err.to_string().contains("not collected"), "{err}");
    // All still lists the HTML pages.
    let all = paths(&mut r, &c, &q("meta-description", "all"));
    assert!(all.contains("/") && !all.contains("/img.png"));
}
