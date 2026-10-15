//! Spec 005 acceptance scenarios: crawl a fixture site and check what the
//! crawl file holds for each page, through `CrawlStore::page_report`.

use std::path::PathBuf;
use std::time::Duration;

use serde_json::Value;
use spiderlens_config::{CrawlConfig, ModeInput, SpeedSettings};
use spiderlens_engine::crawl::{Control, CrawlStore, Crawler};
use spiderlens_test_fixtures::{Site, SiteBuilder};
use tokio::sync::watch;

const E: &str = "example.test";

const FULL: &str = r##"<!doctype html>
<html lang="en-GB">
<head>
  <title>  Full   page </title>
  <meta name="description" content="Everything a page can have">
  <meta name="robots" content="noindex, max-snippet:50">
  <link rel="canonical" href="/full">
  <link rel="alternate" hreflang="fr" href="/fr">
  <link rel="next" href="/full?page=2">
  <script type="application/ld+json">{"@context":"https://schema.org","@type":"Article","headline":"Hi"}</script>
</head>
<body>
  <h1>Main heading</h1>
  <h2>First</h2><h2>Second</h2><h3>Third level</h3>
  <p>Some words to count here.</p>
  <a href="/about" rel="nofollow sponsored">About <b>us</b></a>
  <a href="#top">Top</a>
  <a href="mailto:a@b.test">Mail</a>
  <a href="/img-link"><img src="/logo.png" alt="Logo" width="120" height="40"></a>
  <img src="/photo.jpg" srcset="/photo-2x.jpg 2x">
  <div itemscope itemtype="https://schema.org/Person"><span itemprop="name">Ada</span></div>
</body>
</html>"##;

struct Done {
    _dir: tempfile::TempDir,
    path: PathBuf,
}

impl Done {
    fn store(&self) -> CrawlStore {
        CrawlStore::open(&self.path).unwrap()
    }

    fn report(&self, site: &Site, path: &str) -> Value {
        let url = site.url(E, path);
        self.store()
            .page_report(&url)
            .unwrap()
            .unwrap_or_else(|| panic!("{url} not in the crawl file"))
    }
}

async fn crawl(site: &Site, start: &str, edit: impl FnOnce(&mut CrawlConfig)) -> Done {
    let mut c = CrawlConfig {
        speed: SpeedSettings::FAST,
        ..CrawlConfig::default()
    };
    edit(&mut c);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("t.spiderlens");
    let input = ModeInput::Spider {
        start_url: site.url(E, start),
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
    Done { _dir: dir, path }
}

fn texts(v: &Value) -> Vec<&str> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|t| t["text"].as_str().unwrap())
        .collect()
}

fn link<'a>(r: &'a Value, kind: &str, raw: &str) -> &'a Value {
    r["links"]
        .as_array()
        .unwrap()
        .iter()
        .find(|l| l["kind"] == kind && l["raw"] == raw)
        .unwrap_or_else(|| panic!("no {kind} link {raw} in {:#}", r["links"]))
}

#[tokio::test]
async fn us1_us2_us3_full_page_is_stored() {
    let site = SiteBuilder::new()
        .html(E, "/full", FULL)
        .header(E, "/full", "X-Robots-Tag", "googlebot: nosnippet")
        .page(E, "/about", "", &[])
        .page(E, "/fr", "", &[])
        .page(E, "/img-link", "", &[])
        .start();
    let done = crawl(&site, "/full", |_| {}).await;
    let r = done.report(&site, "/full");
    let page = &r["page"];

    // US1: titles, description, headings, language, words.
    assert_eq!(texts(&r["titles"]), ["Full page"]);
    assert_eq!(r["title"], "Full page");
    assert_eq!(page["meta_description"], "Everything a page can have");
    assert_eq!(page["h1_1"], "Main heading");
    assert_eq!(page["h1_count"], 1);
    assert_eq!(page["h2_1"], "First");
    assert_eq!(page["h2_2"], "Second");
    assert_eq!(page["h2_count"], 2);
    assert_eq!(page["lang"], "en-GB");
    assert!(page["word_count"].as_i64().unwrap() >= 10);
    assert!(page["title_hash"].is_i64());
    assert_eq!(r["resource_type"], "html");
    assert!(r["response_ms"].is_i64());
    assert_eq!(r["size_bytes"], FULL.len());

    // US2: directives from meta and header, canonical.
    assert_eq!(r["noindex"], 1);
    let dirs = r["directives"].as_array().unwrap();
    assert!(
        dirs.iter()
            .any(|d| d["directive"] == "max-snippet" && d["value"] == "50")
    );
    assert!(
        dirs.iter()
            .any(|d| d["directive"] == "nosnippet" && d["source"] == "header" && d["crawler"] == "googlebot")
    );
    assert_eq!(r["canonical"], site.url(E, "/full"));
    assert_eq!(r["canonical_count"], 1);

    // US3: links with their context.
    let about = link(&r, "anchor", "/about");
    assert_eq!(about["text"], "About us");
    assert_eq!(about["nofollow"], 1);
    assert_eq!(about["sponsored"], 1);
    assert_eq!(about["url"], site.url(E, "/about"));
    assert_eq!(link(&r, "anchor", "#top")["skip_reason"], "fragment");
    assert_eq!(
        link(&r, "anchor", "mailto:a@b.test")["skip_reason"],
        "unsupported_scheme"
    );
    let logo = link(&r, "image", "/logo.png");
    assert_eq!(logo["alt"], "Logo");
    assert_eq!(logo["width"], 120);
    assert_eq!(logo["in_link"], 1);
    assert_eq!(link(&r, "image", "/photo.jpg")["srcset"], 0);
    assert_eq!(link(&r, "image", "/photo-2x.jpg")["srcset"], 1);
    assert_eq!(link(&r, "hreflang", "/fr")["lang"], "fr");
    link(&r, "next", "/full?page=2");

    // US5: structured data, both kinds.
    let sd = r["structured_data"].as_array().unwrap();
    assert!(sd.iter().any(|b| b["kind"] == "json_ld" && b["parseable"] == 1));
    assert!(sd.iter().any(|b| b["kind"] == "microdata"));

    // US6: headers and raw HTML.
    assert!(
        r["headers"]
            .as_array()
            .unwrap()
            .iter()
            .any(|h| h[0] == "x-robots-tag")
    );
    assert_eq!(r["html_stored"], true);
    let store = done.store();
    let id = store.id_of(&site.url(E, "/full")).unwrap().unwrap();
    assert_eq!(store.page_html(id).unwrap().unwrap(), FULL.as_bytes());

    // Duplicate-filter indexes exist once the run has ended.
    let conn = rusqlite::Connection::open(&done.path).unwrap();
    let n: i64 = conn
        .query_row(
            "SELECT count(*) FROM sqlite_master WHERE name IN ('pages_title_hash','pages_md_hash')",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(n, 2);
}

#[tokio::test]
async fn us2_pdf_directives_and_canonical_from_headers() {
    let site = SiteBuilder::new()
        .page(E, "/", "", &["/doc.pdf"])
        .raw(E, "/doc.pdf", 200, "application/pdf", b"%PDF-1.4 not really")
        .header(E, "/doc.pdf", "X-Robots-Tag", "noindex, nofollow")
        .header(E, "/doc.pdf", "Link", "<{base:example.test}/>; rel=\"canonical\"")
        .start();
    let done = crawl(&site, "/", |_| {}).await;
    let r = done.report(&site, "/doc.pdf");
    assert_eq!(r["resource_type"], "other");
    assert_eq!(r["noindex"], 1);
    assert_eq!(r["nofollow"], 1);
    assert_eq!(r["canonical"], site.url(E, "/"));
    let canon = link(&r, "canonical", &site.url(E, "/"));
    assert_eq!(canon["source"], "header");
    assert!(r["page"].is_null(), "no page row for a PDF");
}

#[tokio::test]
async fn meta_refresh_is_a_redirect() {
    let site = SiteBuilder::new()
        .html(
            E,
            "/old",
            r#"<html><head><meta http-equiv="refresh" content="0; url=/new"><title>Moved</title></head></html>"#,
        )
        .page(E, "/new", "", &[])
        .start();
    let done = crawl(&site, "/old", |_| {}).await;
    let r = done.report(&site, "/old");
    assert_eq!(r["redirect_kind"], "meta_refresh");
    assert_eq!(r["redirect_to"], site.url(E, "/new"));
    assert_eq!(r["page"]["meta_refresh_delay"].as_f64(), Some(0.0));
    assert_eq!(done.report(&site, "/new")["state"], "crawled");
}

#[tokio::test]
async fn us4_truncated_body_is_partial_but_extracted() {
    let mut html = String::from("<html><head><title>Huge</title></head><body><h1>Top</h1>");
    while html.len() < 1_200_000 {
        html.push_str("<p>filler words go here and here</p>\n");
    }
    html.push_str("<h2>Never seen</h2></body></html>");
    let site = SiteBuilder::new().html(E, "/huge", &html).start();
    let done = crawl(&site, "/huge", |c| c.network.max_body_mb = 1).await;
    let r = done.report(&site, "/huge");
    assert_eq!(r["title"], "Huge");
    assert_eq!(r["page"]["partial"], 1);
    assert_eq!(r["page"]["h1_1"], "Top");
    assert!(r["page"]["h2_1"].is_null());
}

#[tokio::test]
async fn us4_windows_1252_without_charset() {
    let (bytes, _, _) =
        encoding_rs::WINDOWS_1252.encode("<html><head><title>Café crème</title></head></html>");
    let site = SiteBuilder::new()
        .raw(E, "/latin", 200, "text/html", &bytes)
        .start();
    let done = crawl(&site, "/latin", |_| {}).await;
    let r = done.report(&site, "/latin");
    assert_eq!(r["title"], "Café crème");
    assert_eq!(r["page"]["encoding"], "windows-1252");
    assert_eq!(r["page"]["encoding_errors"], 0);
}

#[tokio::test]
async fn us6_raw_html_can_be_turned_off() {
    let site = SiteBuilder::new().html(E, "/", FULL).start();
    let done = crawl(&site, "/", |c| c.extraction.store_html = false).await;
    let r = done.report(&site, "/");
    assert_eq!(r["html_stored"], false);
    assert_eq!(r["page"]["html_stored"], 0);
    assert_eq!(r["title"], "Full page");
}

/// A page shaped like a real one: shared header, navigation and footer
/// around some unique text, about 3 KB.
fn realistic(n: usize, pages: usize) -> String {
    let nav: String = (0..pages)
        .map(|i| format!("<li><a href=\"/p/{i}\" class=\"nav-link\">Section {i}</a></li>\n"))
        .collect();
    let words = [
        "crawl", "index", "page", "search", "engine", "link", "title", "content", "site", "rank",
    ];
    let text: String = (0..250)
        .map(|i| words[(i * 7 + n * 3 + i / 5) % words.len()])
        .collect::<Vec<_>>()
        .join(" ");
    format!(
        "<!doctype html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n<title>Page {n} | Example</title>\n\
         <meta name=\"description\" content=\"About page {n} of the example site\">\n\
         <link rel=\"stylesheet\" href=\"/assets/site.css\">\n</head>\n<body>\n<header class=\"site-header\">\
         <a href=\"/p/0\" class=\"logo\">Example</a></header>\n<nav><ul>\n{nav}</ul></nav>\n<main>\n\
         <h1>Page {n}</h1>\n<p>{text}</p>\n</main>\n<footer class=\"site-footer\"><p>Copyright Example. \
         All rights reserved.</p></footer>\n</body>\n</html>\n"
    )
}

#[tokio::test]
async fn sc_005_raw_html_is_identical_and_small() {
    const PAGES: usize = 30;
    let bodies: Vec<String> = (0..PAGES).map(|n| realistic(n, PAGES)).collect();
    let mut b = SiteBuilder::new();
    for (n, body) in bodies.iter().enumerate() {
        b = b.html(E, &format!("/p/{n}"), body);
    }
    let site = b.start();
    let done = crawl(&site, "/p/0", |_| {}).await;
    let store = done.store();
    let mut original = 0usize;
    for (n, body) in bodies.iter().enumerate() {
        let id = store.id_of(&site.url(E, &format!("/p/{n}"))).unwrap().unwrap();
        assert_eq!(store.page_html(id).unwrap().unwrap(), body.as_bytes(), "page {n}");
        original += body.len();
    }
    let conn = rusqlite::Connection::open(&done.path).unwrap();
    let stored = conn
        .query_row("SELECT sum(length(body)) FROM bodies", [], |r| r.get::<_, i64>(0))
        .unwrap() as usize;
    let ratio = stored as f64 / original as f64;
    assert!(
        ratio <= 0.30,
        "stored HTML is {:.0}% of the original",
        ratio * 100.0
    );
}

/// SC-004: a 50 MB page with 100,000 links is extracted in under 10 s.
/// Run in release: `cargo test --release -p spiderlens-engine --test
/// extraction -- --ignored sc_004`.
#[test]
#[ignore = "benchmark; run in release"]
fn sc_004_huge_page_is_extracted_in_time() {
    use spiderlens_engine::extract::{PageInput, extract_page};
    let mut html = String::from("<!doctype html><html><head><title>Huge</title></head><body>");
    let filler = "lorem ipsum dolor sit amet ".repeat(16);
    let mut n = 0;
    while html.len() < 50 * 1024 * 1024 {
        if n < 100_000 {
            html.push_str(&format!("<p><a href=\"/l/{n}\">link {n}</a> {filler}</p>\n"));
            n += 1;
        } else {
            html.push_str(&format!("<p>{filler}</p>\n"));
        }
    }
    html.push_str("</body></html>");
    assert_eq!(n, 100_000, "the page holds 100,000 links");
    let url = url::Url::parse("https://example.test/").unwrap();
    let started = std::time::Instant::now();
    let d = extract_page(&PageInput {
        url: &url,
        charset: Some("utf-8"),
        body: html.as_bytes(),
        truncated: false,
        robots_token: "SpiderLens",
        keep_text: false,
        a11y: false,
        hook: None,
    });
    let took = started.elapsed();
    println!(
        "{} MB, {} refs in {took:?}",
        html.len() / 1024 / 1024,
        d.refs.len()
    );
    assert_eq!(d.refs.len(), 100_000);
    assert!(!d.partial);
    assert!(took < Duration::from_secs(10), "took {took:?}");
}

/// Constitution and spec 025 FR-026: cookie values set by the site never
/// reach the crawl file; names and attributes stay.
#[tokio::test]
async fn set_cookie_values_are_not_stored() {
    let site = SiteBuilder::new()
        .page(E, "/", "", &[])
        .header(E, "/", "Set-Cookie", "sid=TOKEN-4f7a91; Path=/; Secure; HttpOnly")
        .header(E, "/", "Set-Cookie", "theme=dark")
        .start();
    let done = crawl(&site, "/", |_| {}).await;
    let r = done.report(&site, "/");
    let cookies: Vec<&str> = r["headers"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|h| h[0].as_str().unwrap().eq_ignore_ascii_case("set-cookie"))
        .map(|h| h[1].as_str().unwrap())
        .collect();
    assert_eq!(
        cookies,
        ["sid=[removed]; Path=/; Secure; HttpOnly", "theme=[removed]"]
    );

    drop(r);
    let bytes = std::fs::read(&done.path).unwrap();
    let found = |needle: &[u8]| bytes.windows(needle.len()).any(|w| w == needle);
    assert!(!found(b"TOKEN-4f7a91"), "a cookie value reached the crawl file");
    assert!(!found(b"theme=dark"));

    // A file written before the rule, viewed only, shows no value either.
    let conn = rusqlite::Connection::open(&done.path).unwrap();
    conn.execute(
        "UPDATE headers SET headers = ?1",
        [r#"[["Set-Cookie","old=OLD-VALUE; Path=/"]]"#],
    )
    .unwrap();
    drop(conn);
    let r = done.report(&site, "/");
    assert_eq!(r["headers"][0][1], "old=[removed]; Path=/");
}
