//! Spec 027 part C: the PDFs tab and the PDF section of the detail pane, on
//! a real crawl of PDFs served by the fixture site.

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::time::Duration;

use spiderlens_config::{CrawlConfig, ModeInput, SpeedSettings};
use spiderlens_engine::crawl::{Control, Crawler};
use spiderlens_results::{Need, Reader, UrlScope, ViewQuery, tabs};
use spiderlens_test_fixtures::pdf::PdfSpec;
use spiderlens_test_fixtures::{Site, SiteBuilder};
use tokio::sync::watch;

const E: &str = "example.test";
const PDF: &str = "application/pdf";

fn site() -> Site {
    let scan = PdfSpec {
        scanned: true,
        ..PdfSpec::titled("Scanned", 2)
    };
    let untitled = PdfSpec {
        title: None,
        tagged: true,
        ..PdfSpec::titled("", 1)
    };
    let locked = PdfSpec {
        password: Some("secret".into()),
        ..PdfSpec::titled("Locked", 1)
    };
    let report = PdfSpec {
        author: Some("Ada".into()),
        tagged: true,
        links: vec![(1, "/".into())],
        ..PdfSpec::titled("Annual report", 3)
    };
    SiteBuilder::new()
        .page(
            E,
            "/",
            "",
            &[
                "/report.pdf",
                "/scan.pdf",
                "/untitled.pdf",
                "/locked.pdf",
                "/broken.pdf",
                "/big.pdf",
                "/hidden.pdf",
            ],
        )
        .raw(E, "/report.pdf", 200, PDF, &report.build())
        .raw(E, "/scan.pdf", 200, PDF, &scan.build())
        .raw(E, "/untitled.pdf", 200, PDF, &untitled.build())
        .raw(E, "/locked.pdf", 200, PDF, &locked.build())
        .raw(E, "/broken.pdf", 200, PDF, b"%PDF-1.4 broken")
        .raw(
            E,
            "/big.pdf",
            200,
            PDF,
            &PdfSpec {
                padding: 2 * 1024 * 1024,
                ..PdfSpec::titled("Big", 1)
            }
            .build(),
        )
        .raw(E, "/hidden.pdf", 200, PDF, &PdfSpec::titled("Hidden", 1).build())
        .header(E, "/hidden.pdf", "X-Robots-Tag", "noindex")
        .start()
}

struct Crawled {
    _dir: tempfile::TempDir,
    path: PathBuf,
    site: Site,
}

async fn crawl(read_pdfs: bool) -> Crawled {
    let site = site();
    let mut c = CrawlConfig {
        speed: SpeedSettings::FAST,
        ..CrawlConfig::default()
    };
    c.extraction.store_html = true;
    c.extraction.pdf.crawl = read_pdfs;
    c.extraction.pdf.max_mb = 1;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("pdfs.spiderlens");
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

fn q(filter: &str) -> ViewQuery {
    ViewQuery {
        tab: "pdfs".into(),
        filter: filter.into(),
        scope: UrlScope::All,
        search: String::new(),
        sort: None,
    }
}

/// Each row as (path, values after the id).
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
            (
                url.strip_prefix(&base).map_or(url.clone(), str::to_string),
                row[1..].to_vec(),
            )
        })
        .collect()
}

fn paths(r: &mut Reader, c: &Crawled, filter: &str) -> BTreeSet<String> {
    rows(r, c, &q(filter)).into_iter().map(|(p, _)| p).collect()
}

fn set(items: &[&str]) -> BTreeSet<String> {
    items.iter().map(|s| s.to_string()).collect()
}

fn cell(r: &mut Reader, c: &Crawled, path: &str, column: &str) -> serde_json::Value {
    let t = tabs().iter().find(|t| t.id == "pdfs").unwrap();
    let i = t.columns.iter().position(|col| col.id == column).unwrap();
    rows(r, c, &q("all"))
        .into_iter()
        .find(|(p, _)| p == path)
        .unwrap_or_else(|| panic!("{path} not in PDFs"))
        .1[i]
        .clone()
}

fn id_of(r: &mut Reader, c: &Crawled, path: &str) -> i64 {
    let page = r.open_view(&q("all"), 500).unwrap();
    let want = c.site.url(E, path);
    page.rows
        .iter()
        .find(|row| row[1].as_str() == Some(want.as_str()))
        .map(|row| row[0].as_i64().unwrap())
        .unwrap_or_else(|| panic!("{path} not listed"))
}

#[tokio::test]
async fn pdf_filters_list_exactly_the_seeded_documents() {
    let c = crawl(true).await;
    let mut r = Reader::open(&c.path).unwrap();
    assert!(r.features().has(Need::Pdf));
    assert_eq!(
        paths(&mut r, &c, "all"),
        set(&[
            "/report.pdf",
            "/scan.pdf",
            "/untitled.pdf",
            "/locked.pdf",
            "/broken.pdf",
            "/big.pdf",
            "/hidden.pdf"
        ])
    );
    // Encrypted, damaged and oversize PDFs have no readable title either.
    assert_eq!(
        paths(&mut r, &c, "missing_title"),
        set(&["/untitled.pdf", "/locked.pdf", "/broken.pdf", "/big.pdf"])
    );
    assert_eq!(paths(&mut r, &c, "no_text"), set(&["/scan.pdf"]));
    assert_eq!(paths(&mut r, &c, "encrypted"), set(&["/locked.pdf"]));
    assert_eq!(paths(&mut r, &c, "size_limit"), set(&["/big.pdf"]));
    assert_eq!(paths(&mut r, &c, "unreadable"), set(&["/broken.pdf"]));
    assert_eq!(paths(&mut r, &c, "noindex"), set(&["/hidden.pdf"]));
    assert!(paths(&mut r, &c, "untagged").contains("/scan.pdf"));
    assert!(!paths(&mut r, &c, "untagged").contains("/report.pdf"));

    assert_eq!(cell(&mut r, &c, "/report.pdf", "pdf_title"), "Annual report");
    assert_eq!(cell(&mut r, &c, "/report.pdf", "pdf_pages"), 3);
    assert_eq!(cell(&mut r, &c, "/report.pdf", "pdf_status"), "Read");
    assert_eq!(cell(&mut r, &c, "/big.pdf", "pdf_status"), "Over size limit");
    assert_eq!(
        cell(&mut r, &c, "/locked.pdf", "pdf_status"),
        "Password protected"
    );
}

#[tokio::test]
async fn the_detail_pane_shows_what_was_read() {
    let c = crawl(true).await;
    let mut r = Reader::open(&c.path).unwrap();
    let id = id_of(&mut r, &c, "/report.pdf");
    let d = r.detail(id).unwrap();
    let p = d.pdf.expect("a PDF section");
    assert_eq!(p.title.as_deref(), Some("Annual report"));
    assert_eq!(p.author.as_deref(), Some("Ada"));
    assert_eq!((p.pages, p.pages_read, p.links), (Some(3), Some(3), 1));
    assert!(p.tagged && p.text_stored && !p.encrypted);
    // The stored text is the PDF's, not page HTML.
    assert!(!d.html_stored);
    // The PDF's link shows as an inlink of the home page.
    let home_id = id_of_any(&mut r, &c, "/");
    let home = r.detail(home_id).unwrap();
    assert!(home.inlinks >= 1);
}

fn id_of_any(r: &mut Reader, c: &Crawled, path: &str) -> i64 {
    let query = ViewQuery {
        tab: "internal".into(),
        ..q("all")
    };
    let page = r.open_view(&query, 500).unwrap();
    let want = c.site.url(E, path);
    page.rows
        .iter()
        .find(|row| row[1].as_str() == Some(want.as_str()))
        .map(|row| row[0].as_i64().unwrap())
        .unwrap()
}

#[tokio::test]
async fn a_crawl_that_did_not_read_pdfs_says_so() {
    let c = crawl(false).await;
    let mut r = Reader::open(&c.path).unwrap();
    assert!(!r.features().has(Need::Pdf));
    // The PDFs are still listed, from their content type.
    assert_eq!(paths(&mut r, &c, "all").len(), 7);
    let id = id_of(&mut r, &c, "/report.pdf");
    let d = r.detail(id).unwrap();
    assert!(d.pdf.is_none());
    assert!(d.unavailable.contains_key("pdf"));
}
