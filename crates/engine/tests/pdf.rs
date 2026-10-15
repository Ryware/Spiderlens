//! Spec 027 part C acceptance scenarios: PDFs on a fixture site, read into
//! the crawl file's `pdfs` table, their links into `links`.

use std::path::PathBuf;
use std::time::Duration;

use rusqlite::{Connection, OptionalExtension};
use spiderlens_config::{CrawlConfig, ModeInput, SpeedSettings};
use spiderlens_engine::crawl::store::connect_read_only;
use spiderlens_engine::crawl::{Control, Crawler};
use spiderlens_test_fixtures::pdf::PdfSpec;
use spiderlens_test_fixtures::{Site, SiteBuilder};
use tokio::sync::watch;

const E: &str = "example.test";
const PDF: &str = "application/pdf";

fn site() -> Site {
    let report = PdfSpec {
        author: Some("Ada".into()),
        lang: Some("en".into()),
        links: vec![(2, "/only-from-pdf".into()), (1, "https://other.test/x".into())],
        ..PdfSpec::titled("Annual report", 3)
    };
    let scan = PdfSpec {
        scanned: true,
        ..PdfSpec::titled("Scanned", 2)
    };
    let locked = PdfSpec {
        password: Some("secret".into()),
        ..PdfSpec::titled("Locked", 2)
    };
    let big = PdfSpec {
        padding: 2 * 1024 * 1024,
        ..PdfSpec::titled("Big", 1)
    };
    SiteBuilder::new()
        .page(
            E,
            "/",
            "",
            &[
                "/report.pdf",
                "/scan.pdf",
                "/locked.pdf",
                "/broken.pdf",
                "/big.pdf",
                "/hidden.pdf",
                "/file.pdf",
            ],
        )
        .raw(E, "/report.pdf", 200, PDF, &report.build())
        .raw(E, "/scan.pdf", 200, PDF, &scan.build())
        .raw(E, "/locked.pdf", 200, PDF, &locked.build())
        .raw(E, "/broken.pdf", 200, PDF, b"%PDF-1.4\nthis is not really a PDF")
        .raw(E, "/big.pdf", 200, PDF, &big.build())
        .raw(E, "/hidden.pdf", 200, PDF, &PdfSpec::titled("Hidden", 1).build())
        .header(E, "/hidden.pdf", "X-Robots-Tag", "noindex")
        // A generic type: known as a PDF by its path.
        .raw(
            E,
            "/file.pdf",
            200,
            "application/octet-stream",
            &PdfSpec::titled("Generic", 1).build(),
        )
        .page(E, "/only-from-pdf", "", &[])
        .start()
}

/// status, title, page count, word count, no text.
type PdfRow = (String, Option<String>, Option<i64>, Option<i64>, bool);

struct Done {
    _dir: tempfile::TempDir,
    path: PathBuf,
}

impl Done {
    fn conn(&self) -> Connection {
        connect_read_only(&self.path).unwrap()
    }

    fn pdf(&self, site: &Site, path: &str) -> Option<PdfRow> {
        self.conn()
            .query_row(
                "SELECT p.status, p.title, p.page_count, p.word_count, p.no_text
                 FROM pdfs p JOIN urls u ON u.id = p.url_id WHERE u.url = ?1",
                [site.url(E, path)],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
            )
            .optional()
            .unwrap()
    }

    fn state(&self, site: &Site, path: &str) -> Option<String> {
        self.conn()
            .query_row(
                "SELECT state FROM urls WHERE url = ?1",
                [site.url(E, path)],
                |r| r.get(0),
            )
            .optional()
            .unwrap()
    }
}

async fn crawl(site: &Site, edit: impl FnOnce(&mut CrawlConfig)) -> Done {
    let mut c = CrawlConfig {
        speed: SpeedSettings::FAST,
        ..CrawlConfig::default()
    };
    edit(&mut c);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("t.spiderlens");
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
    Done { _dir: dir, path }
}

fn reading(c: &mut CrawlConfig) {
    c.extraction.pdf.crawl = true;
    c.extraction.pdf.max_mb = 1;
}

#[tokio::test]
async fn pdfs_are_read_with_their_properties_and_reasons() {
    let site = site();
    let d = crawl(&site, reading).await;

    // C1.1: properties, page count and words.
    let (status, title, pages, words, no_text) = d.pdf(&site, "/report.pdf").unwrap();
    assert_eq!(
        (status.as_str(), title.as_deref(), pages),
        ("read", Some("Annual report"), Some(3))
    );
    assert!(words.unwrap() > 10);
    assert!(!no_text);
    let (author, lang, links): (Option<String>, Option<String>, i64) = d
        .conn()
        .query_row(
            "SELECT author, lang, link_count FROM pdfs p JOIN urls u ON u.id = p.url_id WHERE u.url = ?1",
            [site.url(E, "/report.pdf")],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!(
        (author.as_deref(), lang.as_deref(), links),
        (Some("Ada"), Some("en"), 2)
    );

    // C1.2: a scan has pages but no text.
    let (status, _, pages, _, no_text) = d.pdf(&site, "/scan.pdf").unwrap();
    assert_eq!((status.as_str(), pages, no_text), ("read", Some(2), true));

    // C1.3: a password-protected PDF says so.
    assert_eq!(d.pdf(&site, "/locked.pdf").unwrap().0, "encrypted");

    // C1.4: a damaged file is recorded, the crawl goes on.
    assert_eq!(d.pdf(&site, "/broken.pdf").unwrap().0, "failed");

    // C1.5: over the size limit, nothing past it is read.
    let (status, _, pages, _, _) = d.pdf(&site, "/big.pdf").unwrap();
    assert_eq!((status.as_str(), pages), ("over_size_limit", None));

    // A generic content type with a .pdf path is still read.
    assert_eq!(d.pdf(&site, "/file.pdf").unwrap().1.as_deref(), Some("Generic"));

    // Response facts stay with the URL: resource type and X-Robots-Tag.
    let (kind, noindex): (String, bool) = d
        .conn()
        .query_row(
            "SELECT resource_type, noindex FROM urls WHERE url = ?1",
            [site.url(E, "/hidden.pdf")],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!((kind.as_str(), noindex), ("pdf", true));
    assert_eq!(d.pdf(&site, "/hidden.pdf").unwrap().0, "read");
}

#[tokio::test]
async fn links_in_pdfs_are_followed_and_shown_as_from_the_pdf() {
    let site = site();
    let d = crawl(&site, reading).await;
    // C2.1: a page linked only from a PDF is found.
    assert_eq!(d.state(&site, "/only-from-pdf").as_deref(), Some("crawled"));
    let (source, pos, followed): (String, i64, bool) = d
        .conn()
        .query_row(
            "SELECT l.source, l.pos, l.followed FROM links l
             JOIN urls f ON f.id = l.from_id JOIN urls t ON t.id = l.to_id
             WHERE f.url = ?1 AND t.url = ?2",
            [site.url(E, "/report.pdf"), site.url(E, "/only-from-pdf")],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!((source.as_str(), pos, followed), ("pdf", 2, true));
}

#[tokio::test]
async fn links_in_pdfs_are_kept_but_not_followed_when_switched_off() {
    let site = site();
    let d = crawl(&site, |c| {
        reading(c);
        c.extraction.pdf.follow_links = false;
    })
    .await;
    // C2.2
    assert_eq!(d.state(&site, "/only-from-pdf"), None);
    let (target, followed, reason): (String, bool, String) = d
        .conn()
        .query_row(
            "SELECT l.target, l.followed, l.skip_reason FROM links l JOIN urls f ON f.id = l.from_id
             WHERE f.url = ?1 AND l.raw = '/only-from-pdf'",
            [site.url(E, "/report.pdf")],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!(target, site.url(E, "/only-from-pdf"));
    assert!(!followed);
    assert_eq!(reason, "pdf_links_not_followed");
}

#[tokio::test]
async fn with_pdfs_off_nothing_changes() {
    let site = site();
    let d = crawl(&site, |_| {}).await;
    // C1.6: checked like any other file, no PDF rows, no reads.
    let rows: i64 = d
        .conn()
        .query_row("SELECT COUNT(*) FROM pdfs", [], |r| r.get(0))
        .unwrap();
    assert_eq!(rows, 0);
    let kind: String = d
        .conn()
        .query_row(
            "SELECT resource_type FROM urls WHERE url = ?1",
            [site.url(E, "/report.pdf")],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(kind, "other");
    assert_eq!(d.state(&site, "/only-from-pdf"), None);
}
