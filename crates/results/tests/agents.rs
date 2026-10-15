//! Spec 016 User Story 2 on a real crawl: every fixture page appears under
//! exactly the expected Agents filters (SC-005, raw HTML), and a page's
//! findings list its forms, fields, actions and links.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::time::Duration;

use spiderlens_config::{CrawlConfig, ModeInput, SpeedSettings};
use spiderlens_engine::crawl::{Control, Crawler};
use spiderlens_results::{Reader, UrlScope, ViewQuery, tab};
use spiderlens_test_fixtures::{Site, SiteBuilder};
use tokio::sync::watch;

const E: &str = "example.test";

fn html(body: &str) -> String {
    format!("<!doctype html><html><head><title>t</title></head><body>{body}</body></html>")
}

fn site() -> Site {
    let links: String = [
        "/search", "/contact", "/icons", "/fake", "/captcha", "/webmcp", "/md", "/plain",
    ]
    .iter()
    .map(|l| format!("<a href=\"{l}\">x</a>"))
    .collect();
    SiteBuilder::new()
        .html(E, "/", &html(&links))
        .html(
            E,
            "/search",
            &html(r#"<form action="/s"><input name="q" aria-label="Search"><button>Go</button></form>"#),
        )
        .html(
            E,
            "/contact",
            &html(r#"<form method="post"><input name="email"><input name="phone" placeholder="Phone"><button>Send</button></form>"#),
        )
        .html(E, "/icons", &html(r#"<button><svg></svg></button>"#))
        .html(
            E,
            "/fake",
            &html(r##"<a href="#" onclick="x()">Open</a><div onclick="y()">More</div>"##),
        )
        .html(
            E,
            "/captcha",
            &html(r#"<form method="post"><input name="m" aria-label="Message"><div class="g-recaptcha"></div><button>Send</button></form>"#),
        )
        .html(
            E,
            "/webmcp",
            &html(r#"<form toolname="book"><input name="d" aria-label="Day"><button>Book</button></form>"#),
        )
        .html(
            E,
            "/md",
            r#"<!doctype html><html><head><title>md</title><link rel="alternate" type="text/markdown" href="/md.md"></head><body></body></html>"#,
        )
        .html(E, "/plain", &html("<p>Nothing to do here.</p>"))
        .start()
}

struct Crawled {
    _dir: tempfile::TempDir,
    path: PathBuf,
    site: Site,
}

async fn crawl() -> Crawled {
    let site = site();
    let c = CrawlConfig {
        speed: SpeedSettings::FAST,
        ..CrawlConfig::default()
    };
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("agents.spiderlens");
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

fn paths(r: &mut Reader, c: &Crawled, filter: &str) -> BTreeSet<String> {
    let q = ViewQuery {
        tab: "agents".into(),
        filter: filter.into(),
        scope: UrlScope::All,
        search: String::new(),
        sort: None,
        values: None,
        segment: None,
    };
    let page = r.open_view(&q, 500).unwrap();
    r.materialize(page.view).unwrap();
    let rows = r.rows(page.view, 0, 1000).unwrap().rows;
    r.close_view(page.view).unwrap();
    let base = c.site.url(E, "");
    rows.into_iter()
        .map(|row| {
            let url = row[1].as_str().unwrap();
            url.strip_prefix(&base).unwrap_or(url).to_string()
        })
        .collect()
}

#[tokio::test]
async fn us2_filters_list_exactly_the_expected_pages() {
    let c = crawl().await;
    let mut r = Reader::open(&c.path).unwrap();
    let expected: BTreeMap<&str, &[&str]> = BTreeMap::from([
        ("has_forms", &["/search", "/contact", "/captcha", "/webmcp"][..]),
        ("search_forms", &["/search"][..]),
        ("unlabelled_fields", &["/contact"][..]),
        ("placeholder_only_fields", &["/contact"][..]),
        ("nameless_buttons", &["/icons"][..]),
        ("actions_without_link", &["/fake"][..]),
        ("challenge", &["/captcha"][..]),
        ("no_submit", &[][..]),
        ("declared_tools", &["/webmcp"][..]),
        ("tool_missing_description", &["/webmcp"][..]),
        ("has_markdown", &["/md"][..]),
    ]);
    let t = tab("agents").unwrap();
    for f in &t.filters {
        if f.id == "all" || f.id == "no_markdown" || f.id == "with_issues" {
            continue;
        }
        let want = expected
            .get(&*f.id)
            .unwrap_or_else(|| panic!("filter {} has no expectation", f.id));
        let want: BTreeSet<String> = want.iter().map(|s| s.to_string()).collect();
        assert_eq!(paths(&mut r, &c, &f.id), want, "filter {}", f.id);
    }
    let no_md = paths(&mut r, &c, "no_markdown");
    assert_eq!(no_md.len(), 8, "{no_md:?}");
    assert!(!no_md.contains("/md"));

    let counts = r.counts("agents", UrlScope::All).unwrap();
    assert_eq!(counts.counts["has_forms"], Some(4));
    assert_eq!(counts.counts["all"], Some(9));
}

#[tokio::test]
async fn us2_page_findings() {
    let c = crawl().await;
    let mut r = Reader::open(&c.path).unwrap();
    let id: i64 = rusqlite::Connection::open(&c.path)
        .unwrap()
        .query_row(
            "SELECT id FROM urls WHERE url = ?1",
            [c.site.url(E, "/contact")],
            |row| row.get(0),
        )
        .unwrap();
    let f = r.agent_findings(id).unwrap();
    assert!(f.available && f.checked);
    assert_eq!(f.forms.len(), 1);
    assert_eq!(f.forms[0].method, "post");
    let names: Vec<_> = f.forms[0]
        .fields
        .iter()
        .map(|x| (x.name.as_deref().unwrap(), x.name_source.as_str()))
        .collect();
    assert_eq!(names, [("email", "none"), ("phone", "placeholder")]);
    assert_eq!((f.unlabelled_fields, f.placeholder_only_fields), (1, 1));
}
