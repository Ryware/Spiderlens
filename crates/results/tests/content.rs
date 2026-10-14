//! Spec 007 content tabs on a real crawl with page extraction (spec 005):
//! meta descriptions, headings, images, canonicals, directives,
//! indexability and the detail pane's content sections (T044).

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::time::Duration;

use spiderlens_config::{CrawlConfig, ModeInput, SpeedSettings};
use spiderlens_engine::crawl::{Control, Crawler};
use spiderlens_results::{HtmlMissing, ListKind, Need, Reader, UrlScope, ViewQuery, tabs};
use spiderlens_test_fixtures::{Site, SiteBuilder};
use tokio::sync::watch;

const E: &str = "example.test";

/// A page whose parts default to unique, well-sized values.
#[derive(Default)]
struct Page {
    titles: Option<Vec<&'static str>>,
    descriptions: Option<Vec<String>>,
    h1: Option<Vec<String>>,
    h2: Vec<&'static str>,
    head: &'static str,
    body: &'static str,
}

impl Page {
    fn html(&self, path: &str) -> String {
        let titles = self
            .titles
            .clone()
            .unwrap_or_else(|| vec!["A content page title of decent length"]);
        let descriptions = self.descriptions.clone().unwrap_or_else(|| {
            vec![format!(
                "Description for {path}, written to be long enough for the seventy character check."
            )]
        });
        let h1 = self
            .h1
            .clone()
            .unwrap_or_else(|| vec![format!("Heading for {path}")]);
        let mut head = String::new();
        for t in titles {
            head.push_str(&format!("<title>{t}</title>"));
        }
        for d in descriptions {
            head.push_str(&format!("<meta name=\"description\" content=\"{d}\">"));
        }
        head.push_str(self.head);
        let mut body = String::new();
        for h in h1 {
            body.push_str(&format!("<h1>{h}</h1>"));
        }
        for h in &self.h2 {
            body.push_str(&format!("<h2>{h}</h2>"));
        }
        body.push_str(self.body);
        format!("<!doctype html><html><head>{head}</head><body>{body}</body></html>")
    }
}

const PAGES: &[&str] = &[
    "/no-md",
    "/dup-md-1",
    "/dup-md-2",
    "/long-md",
    "/short-md",
    "/noindex",
    "/xrobots",
    "/googlebot-only",
    "/canon-self",
    "/canon-other",
    "/canon-conflict",
];

fn site() -> Site {
    let dup =
        "The same description on two pages, long enough to pass the seventy character check.".to_string();
    let long_h1 = "A first heading that goes on and on well past the seventy character limit".to_string();
    let long_alt = "x".repeat(110);
    let links: String = PAGES.iter().map(|p| format!("<a href=\"{p}\">{p}</a>")).collect();
    let home = Page {
        h2: vec!["Section one"],
        ..Page::default()
    };
    let home_html = home.html("/").replace(
        "</body>",
        &format!("<img src=\"/a.png\" alt=\"A photo\">{links}</body>"),
    );
    let mut b = SiteBuilder::new().html(E, "/", &home_html);
    let pages: Vec<(&str, Page)> = vec![
        (
            "/no-md",
            Page {
                descriptions: Some(vec![]),
                h1: Some(vec![]),
                body: "<img src=\"/b.png\">",
                ..Page::default()
            },
        ),
        (
            "/dup-md-1",
            Page {
                descriptions: Some(vec![dup.clone()]),
                h1: Some(vec!["Shared heading".into()]),
                h2: vec!["Shared subheading"],
                body: "<img src=\"/c.png\" alt=\"\">",
                ..Page::default()
            },
        ),
        (
            "/dup-md-2",
            Page {
                descriptions: Some(vec![dup]),
                h1: Some(vec!["Shared heading".into()]),
                h2: vec!["Shared subheading"],
                ..Page::default()
            },
        ),
        (
            "/long-md",
            Page {
                descriptions: Some(vec!["y".repeat(170)]),
                h1: Some(vec![long_h1]),
                ..Page::default()
            },
        ),
        (
            "/short-md",
            Page {
                titles: Some(vec!["Same words in title and heading", "Second title"]),
                descriptions: Some(vec!["Too short.".into(), "Another one.".into()]),
                h1: Some(vec![
                    "same words in TITLE and heading".into(),
                    "Another heading".into(),
                ]),
                ..Page::default()
            },
        ),
        (
            "/noindex",
            Page {
                head: "<meta name=\"robots\" content=\"noindex, nofollow\">",
                ..Page::default()
            },
        ),
        ("/xrobots", Page::default()),
        (
            "/googlebot-only",
            Page {
                head: "<meta name=\"googlebot\" content=\"nosnippet\">",
                ..Page::default()
            },
        ),
        (
            "/canon-self",
            Page {
                head: "<link rel=\"canonical\" href=\"/canon-self\">",
                ..Page::default()
            },
        ),
        (
            "/canon-other",
            Page {
                head: "<link rel=\"canonical\" href=\"/\">",
                ..Page::default()
            },
        ),
        (
            "/canon-conflict",
            Page {
                head: "<link rel=\"canonical\" href=\"/\"><link rel=\"canonical\" href=\"/no-md\">",
                ..Page::default()
            },
        ),
    ];
    for (path, page) in &pages {
        b = b.html(E, path, &page.html(path));
    }
    let d_body = format!("<img src=\"/d.png\" alt=\"{long_alt}\">");
    b = b
        .html(
            E,
            "/long-md",
            &pages[3]
                .1
                .html("/long-md")
                .replace("</body>", &format!("{d_body}</body>")),
        )
        .header(E, "/xrobots", "X-Robots-Tag", "noarchive, max-snippet:50")
        .raw(E, "/a.png", 200, "image/png", b"\x89PNG small")
        .raw(E, "/b.png", 200, "image/png", b"\x89PNG small")
        .raw(E, "/c.png", 200, "image/png", b"\x89PNG small")
        .raw(E, "/d.png", 200, "image/png", &vec![7u8; 150 * 1024]);
    b.start()
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
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("content.spiderlens");
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
        values: None,
        sort: None,
    }
}

/// Each listed row as (path, row values after the id).
fn rows(r: &mut Reader, c: &Crawled, query: &ViewQuery) -> Vec<(String, Vec<serde_json::Value>)> {
    let page = r.open_view(query, 500).unwrap();
    let total = r.materialize(page.view).unwrap();
    let rows = r.rows(page.view, 0, 1000).unwrap().rows;
    assert_eq!(rows.len() as u32, total, "{query:?}");
    r.close_view(page.view).unwrap();
    let base = c.site.url(E, "");
    rows.into_iter()
        .map(|row| {
            let url = row[1].as_str().unwrap().to_string();
            let path = url.strip_prefix(&base).map_or(url.clone(), str::to_string);
            (path, row[1..].to_vec())
        })
        .collect()
}

fn paths(r: &mut Reader, c: &Crawled, query: &ViewQuery) -> BTreeSet<String> {
    rows(r, c, query).into_iter().map(|(p, _)| p).collect()
}

fn set(items: &[&str]) -> BTreeSet<String> {
    items.iter().map(|s| s.to_string()).collect()
}

/// The value of one column on one row of a view.
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

fn id_of(r: &mut Reader, c: &Crawled, path: &str) -> i64 {
    let page = r.open_view(&q("internal", "all"), 500).unwrap();
    let want = c.site.url(E, path);
    page.rows
        .iter()
        .find(|row| row[1].as_str() == Some(want.as_str()))
        .map(|row| row[0].as_i64().unwrap())
        .unwrap_or_else(|| panic!("{path} not listed"))
}

#[tokio::test]
async fn content_filters_list_exactly_the_seeded_pages() {
    let c = crawl(true).await;
    let mut r = Reader::open(&c.path).unwrap();
    for need in [Need::Extraction, Need::Timing, Need::RawHtml] {
        assert!(r.features().has(need), "{need:?}");
    }

    let md = |f| q("meta-description", f);
    assert_eq!(paths(&mut r, &c, &md("missing")), set(&["/no-md"]));
    assert_eq!(
        paths(&mut r, &c, &md("duplicate")),
        set(&["/dup-md-1", "/dup-md-2"])
    );
    assert_eq!(paths(&mut r, &c, &md("over_155")), set(&["/long-md"]));
    assert_eq!(paths(&mut r, &c, &md("below_70")), set(&["/short-md"]));
    assert_eq!(paths(&mut r, &c, &md("multiple")), set(&["/short-md"]));
    assert_eq!(
        cell(&mut r, &c, "meta-description", "/dup-md-1", "occurrences"),
        2
    );

    assert_eq!(paths(&mut r, &c, &q("h1", "missing")), set(&["/no-md"]));
    assert_eq!(
        paths(&mut r, &c, &q("h1", "duplicate")),
        set(&["/dup-md-1", "/dup-md-2"])
    );
    assert_eq!(paths(&mut r, &c, &q("h1", "over_70")), set(&["/long-md"]));
    assert_eq!(paths(&mut r, &c, &q("h1", "multiple")), set(&["/short-md"]));
    assert_eq!(cell(&mut r, &c, "h1", "/short-md", "h1_2"), "Another heading");
    assert_eq!(
        paths(&mut r, &c, &q("h2", "duplicate")),
        set(&["/dup-md-1", "/dup-md-2"])
    );
    assert_eq!(cell(&mut r, &c, "h2", "/", "h2"), "Section one");

    assert_eq!(
        paths(&mut r, &c, &q("page-titles", "same_as_h1")),
        set(&["/short-md"])
    );
    assert_eq!(
        paths(&mut r, &c, &q("page-titles", "multiple")),
        set(&["/short-md"])
    );

    // Images: by alt text and by size (FR-023, checked images only).
    assert_eq!(paths(&mut r, &c, &q("images", "missing_alt")), set(&["/b.png"]));
    assert_eq!(paths(&mut r, &c, &q("images", "empty_alt")), set(&["/c.png"]));
    assert_eq!(paths(&mut r, &c, &q("images", "alt_over_100")), set(&["/d.png"]));
    assert_eq!(paths(&mut r, &c, &q("images", "over_100kb")), set(&["/d.png"]));
    assert_eq!(cell(&mut r, &c, "images", "/a.png", "alt"), "A photo");

    let canon = |f| q("canonicals", f);
    assert_eq!(
        paths(&mut r, &c, &canon("contains")),
        set(&["/canon-self", "/canon-other", "/canon-conflict"])
    );
    assert_eq!(paths(&mut r, &c, &canon("self")), set(&["/canon-self"]));
    assert_eq!(
        paths(&mut r, &c, &canon("canonicalised")),
        set(&["/canon-other", "/canon-conflict"])
    );
    assert_eq!(
        paths(&mut r, &c, &canon("conflicting")),
        set(&["/canon-conflict"])
    );
    assert_eq!(
        cell(&mut r, &c, "canonicals", "/canon-other", "canonical_html"),
        c.site.url(E, "/")
    );

    // Directives: meta and header combined; a googlebot-only directive does
    // not apply to this crawl's robots token (FR-013).
    let dir = |f| q("directives", f);
    assert_eq!(paths(&mut r, &c, &dir("noindex")), set(&["/noindex"]));
    assert_eq!(paths(&mut r, &c, &dir("nofollow")), set(&["/noindex"]));
    assert_eq!(paths(&mut r, &c, &dir("noarchive")), set(&["/xrobots"]));
    assert_eq!(paths(&mut r, &c, &dir("max_preview")), set(&["/xrobots"]));
    assert!(paths(&mut r, &c, &dir("nosnippet")).is_empty());
    assert_eq!(
        cell(&mut r, &c, "directives", "/xrobots", "x_robots_tag"),
        "noarchive, max-snippet:50"
    );
    assert_eq!(
        cell(&mut r, &c, "directives", "/googlebot-only", "meta_robots"),
        "googlebot: nosnippet"
    );

    // Indexability: noindex and canonicalised pages are not indexable.
    assert_eq!(cell(&mut r, &c, "directives", "/", "indexability"), "Indexable");
    assert_eq!(
        cell(&mut r, &c, "directives", "/noindex", "indexability"),
        "Non-indexable: noindex"
    );
    assert_eq!(
        cell(&mut r, &c, "canonicals", "/canon-other", "indexability"),
        "Non-indexable: canonicalised"
    );
    assert_eq!(
        cell(&mut r, &c, "canonicals", "/canon-self", "indexability"),
        "Indexable"
    );

    // Internal: content and timing columns are filled in.
    assert!(cell(&mut r, &c, "internal", "/", "word_count").as_i64().unwrap() > 0);
    assert!(cell(&mut r, &c, "internal", "/", "size").as_i64().unwrap() > 100);
    assert!(cell(&mut r, &c, "internal", "/", "response_time").is_i64());

    // Every filter is available, and its count equals the rows it lists.
    for tab in tabs() {
        let counts = r.counts(tab.id, UrlScope::All).unwrap();
        for f in &tab.filters {
            let n = counts.counts[f.id].unwrap_or_else(|| panic!("{}/{} unavailable", tab.id, f.id));
            assert_eq!(
                n,
                paths(&mut r, &c, &q(tab.id, f.id)).len() as u64,
                "{}/{}",
                tab.id,
                f.id
            );
        }
    }
}

#[tokio::test]
async fn detail_shows_headers_headings_and_link_text() {
    let c = crawl(true).await;
    let mut r = Reader::open(&c.path).unwrap();

    let xr = id_of(&mut r, &c, "/xrobots");
    let d = r.detail(xr).unwrap();
    assert!(
        d.unavailable.is_empty() || d.unavailable.keys().all(|k| k == "attempts"),
        "{:?}",
        d.unavailable
    );
    assert!(
        d.headers
            .iter()
            .any(|(k, v)| k == "x-robots-tag" && v == "noarchive, max-snippet:50")
    );
    assert_eq!(d.indexability.as_deref(), Some("Indexable"));
    assert!(d.size.is_some() && d.response_ms.is_some());
    assert!(d.html_stored);

    let home = id_of(&mut r, &c, "/");
    let h = r.list(home, ListKind::Headings, 0, 100).unwrap();
    let got: Vec<_> = h.headings.iter().map(|h| (h.level, h.text.as_str())).collect();
    assert_eq!(got, [(1, "Heading for /"), (2, "Section one")]);
    assert_eq!(h.total, 2);

    let out = r.list(home, ListKind::Outlinks, 0, 100).unwrap();
    let no_md = out.rows.iter().find(|l| l.url.ends_with("/no-md")).unwrap();
    assert_eq!(no_md.text.as_deref(), Some("/no-md"));
    let imgs = r.list(home, ListKind::Images, 0, 100).unwrap();
    assert_eq!(imgs.rows[0].text.as_deref(), Some("A photo"));
    let no_alt = id_of(&mut r, &c, "/no-md");
    let imgs = r.list(no_alt, ListKind::Images, 0, 100).unwrap();
    assert_eq!(imgs.rows[0].text, None, "missing alt stays null");
}

#[tokio::test]
async fn page_html_reads_the_stored_source() {
    let c = crawl(true).await;
    let mut r = Reader::open(&c.path).unwrap();
    let home = id_of(&mut r, &c, "/");
    let h = r.page_html(home).unwrap();
    assert_eq!(h.missing, None);
    let html = h.html.unwrap();
    assert!(html.starts_with("<!doctype html><html><head><title>"), "{html}");
    assert!(html.contains("<img src=\"/a.png\" alt=\"A photo\">"));
    assert!(html.ends_with("</body></html>"));
    assert_eq!(h.bytes, html.len() as u64);
    assert_eq!(h.encoding.as_deref(), Some("UTF-8"));
    assert!(!h.encoding_errors);

    // An image has no stored body.
    let img = id_of(&mut r, &c, "/a.png");
    let h = r.page_html(img).unwrap();
    assert_eq!((h.html, h.missing), (None, Some(HtmlMissing::NoBody)));

    assert!(r.page_html(999_999).is_err());
}

#[tokio::test]
async fn crawl_without_stored_html_says_so() {
    let c = crawl(false).await;
    let r = Reader::open(&c.path).unwrap();
    assert!(r.features().has(Need::Extraction));
    assert!(!r.features().has(Need::RawHtml));
    let mut r = r;
    let home = id_of(&mut r, &c, "/");
    let d = r.detail(home).unwrap();
    assert!(!d.html_stored);
    assert!(d.unavailable.contains_key("raw_html"));
    let h = r.page_html(home).unwrap();
    assert_eq!((h.html, h.missing), (None, Some(HtmlMissing::NotStored)));
}
