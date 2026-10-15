use std::time::{Duration, Instant};

use spiderlens_test_fixtures::pdf::PdfSpec;

use super::*;

fn read_now(bytes: &[u8]) -> PdfFacts {
    read(bytes, Instant::now() + Duration::from_secs(30), &progress())
}

#[test]
fn properties_pages_text_and_links() {
    let spec = PdfSpec {
        author: Some("Finance team".into()),
        subject: Some("Results".into()),
        keywords: Some("annual, report".into()),
        created: Some("D:20250131120000+02'00'".into()),
        lang: Some("en-GB".into()),
        tagged: true,
        links: vec![
            (2, "https://example.test/only-from-pdf".into()),
            (12, "/relative".into()),
        ],
        ..PdfSpec::titled("Annual Report 2025", 12)
    };
    let f = read_now(&spec.build());
    assert_eq!(f.status, PdfStatus::Read, "{:?}", f.detail);
    assert_eq!(f.title.as_deref(), Some("Annual Report 2025"));
    assert_eq!(f.author.as_deref(), Some("Finance team"));
    assert_eq!(f.subject.as_deref(), Some("Results"));
    assert_eq!(f.keywords.as_deref(), Some("annual, report"));
    assert_eq!(f.created.as_deref(), Some("2025-01-31T12:00:00+02:00"));
    assert_eq!(f.lang.as_deref(), Some("en-GB"));
    assert!(f.tagged);
    assert!(!f.encrypted);
    assert_eq!(f.pages, Some(12));
    assert_eq!(f.pages_read, 12);
    // "Page N of the report with some words": 8 words a page.
    assert_eq!(f.words, Some(96));
    assert!(!f.no_text());
    assert!(f.text.contains("Page 7 of the report"));
    assert_eq!(
        f.links,
        vec![
            PdfLink {
                uri: "https://example.test/only-from-pdf".into(),
                page: 2
            },
            PdfLink {
                uri: "/relative".into(),
                page: 12
            },
        ]
    );
}

#[test]
fn a_scan_has_no_text() {
    let f = read_now(
        &PdfSpec {
            scanned: true,
            ..PdfSpec::titled("Scan", 3)
        }
        .build(),
    );
    assert_eq!(f.status, PdfStatus::Read, "{:?}", f.detail);
    assert_eq!(f.words, Some(0));
    assert!(f.no_text());
}

#[test]
fn a_password_keeps_the_properties_and_says_so() {
    let f = read_now(
        &PdfSpec {
            password: Some("secret".into()),
            ..PdfSpec::titled("Locked", 2)
        }
        .build(),
    );
    assert_eq!(f.status, PdfStatus::Encrypted, "{:?}", f.detail);
    assert!(f.encrypted);
    // Here even the page tree is encrypted: nothing else is readable, and
    // nothing is made up.
    assert_eq!(f.title, None);
    assert_eq!(f.words, None);
    assert!(f.links.is_empty());
}

#[test]
fn owner_only_protection_reads_like_any_pdf() {
    // Most "protected" PDFs restrict printing or copying with an empty
    // user password: they open for everyone.
    let f = read_now(
        &PdfSpec {
            password: Some(String::new()),
            ..PdfSpec::titled("Restricted", 2)
        }
        .build(),
    );
    assert_eq!(f.status, PdfStatus::Read, "{:?}", f.detail);
    assert!(f.encrypted);
    assert_eq!(f.title.as_deref(), Some("Restricted"));
    assert_eq!(f.pages, Some(2));
    assert_eq!(f.pages_read, 2);
}

#[test]
fn damaged_and_non_pdf_bytes_never_panic() {
    let whole = PdfSpec::titled("Damaged", 5).build();
    for cut in [whole.len() / 2, whole.len() / 3, 200, 20] {
        let f = read_now(&whole[..cut]);
        assert_ne!(f.status, PdfStatus::Read, "cut at {cut}");
        assert!(f.detail.is_some());
    }
    let f = read_now(b"<html>not a pdf</html>");
    assert_eq!(f.status, PdfStatus::Failed);
    assert_eq!(f.detail.as_deref(), Some("Not a PDF file"));
}

#[test]
fn the_deadline_stops_between_pages() {
    let f = read(&PdfSpec::titled("Slow", 4).build(), Instant::now(), &progress());
    assert_eq!(f.status, PdfStatus::PartlyRead);
    assert_eq!(f.detail.as_deref(), Some("Time limit reached"));
    assert_eq!(f.title.as_deref(), Some("Slow"));
    assert_eq!(f.pages, Some(4));
    assert_eq!(f.pages_read, 0);
    assert_eq!(f.words, None);
}

#[test]
fn the_watchdog_keeps_what_was_read() {
    let p = progress();
    let _ = read(
        &PdfSpec::titled("Watched", 2).build(),
        Instant::now() + Duration::from_secs(30),
        &p,
    );
    let f = timed_out(&p);
    assert_eq!(f.status, PdfStatus::PartlyRead);
    assert_eq!(f.title.as_deref(), Some("Watched"));
    let f = timed_out(&progress());
    assert_eq!(f.status, PdfStatus::Failed);
}

#[test]
fn pdf_dates() {
    assert_eq!(pdf_date("D:2025"), "2025-01-01");
    assert_eq!(pdf_date("D:20250131"), "2025-01-31");
    assert_eq!(pdf_date("D:20250131120000Z"), "2025-01-31T12:00:00Z");
    assert_eq!(pdf_date("D:20250131120000-05'30"), "2025-01-31T12:00:00-05:30");
    assert_eq!(pdf_date("yesterday"), "yesterday");
}

#[test]
fn utf16_titles() {
    let mut b = vec![0xFE, 0xFF];
    for u in "דוח שנתי".encode_utf16() {
        b.extend(u.to_be_bytes());
    }
    assert_eq!(decode_text(&b), "דוח שנתי");
}
