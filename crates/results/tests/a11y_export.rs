//! Spec 026 T064: the accessibility exports of a crawl of the fixture site,
//! in CSV and JSON, each starting with the disclaimer (FR-025, FR-026).

use std::path::PathBuf;
use std::time::Duration;

use spiderlens_config::{CrawlConfig, ModeInput, SpeedSettings, WcagLevel, WcagTarget, WcagVersion};
use spiderlens_engine::crawl::{Control, Crawler};
use spiderlens_results::{A11Y_DISCLAIMER, A11Y_EXPORTS, A11yFormat, export_a11y_file, write_a11y};
use spiderlens_test_fixtures::a11y_site::STATIC_PAGES;
use spiderlens_test_fixtures::{Site, SiteBuilder};
use tokio::sync::watch;

const E: &str = "a11y.test";
const PAGES: [&str; 3] = ["/static/form_label", "/static/zoom_disabled", "/static/mixed"];

fn site() -> Site {
    let anchors: String = PAGES.iter().map(|p| format!("<a href=\"{p}\">{p}</a>")).collect();
    let home = format!(
        "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\">\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\"><title>Home</title></head>\
         <body><main>{anchors}</main></body></html>"
    );
    let mut b = SiteBuilder::new().html(E, "/", &home);
    for path in PAGES {
        let page = STATIC_PAGES
            .iter()
            .find(|p| p.path == path)
            .expect("fixture page");
        b = b.html(E, path, page.html);
    }
    b.start()
}

async fn crawl(site: &Site) -> (tempfile::TempDir, PathBuf) {
    let c = CrawlConfig {
        speed: SpeedSettings::FAST,
        ..CrawlConfig::default()
    };
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
    (dir, path)
}

fn export(path: &std::path::Path, id: &str, format: A11yFormat, target: Option<WcagTarget>) -> (u64, String) {
    let mut out = Vec::new();
    let n = write_a11y(path, id, format, target, &mut out).unwrap();
    (n, String::from_utf8(out).unwrap())
}

#[tokio::test]
async fn csv_starts_with_the_disclaimer_and_has_a_row_per_page_and_check() {
    let site = site();
    let (_d, path) = crawl(&site).await;
    let (n, csv) = export(&path, "a11y_violations", A11yFormat::Csv, None);
    let lines: Vec<&str> = csv.split("\r\n").collect();
    assert_eq!(lines[0], format!("# {A11Y_DISCLAIMER}"));
    assert_eq!(
        lines[1],
        "address,check_id,title,kind,source,impact,wcag,elements"
    );
    // form_label, zoom_disabled and the three checks of the mixed page.
    assert_eq!(n, 5, "{csv}");
    let form = format!(
        "{},static.form_label,Form field without a label,violation,static,critical,wcag2a wcag131 wcag412,4",
        site.url(E, "/static/form_label")
    );
    assert!(lines.contains(&form.as_str()), "{csv}");
}

#[tokio::test]
async fn json_has_a_disclaimer_field_and_the_same_rows() {
    let site = site();
    let (_d, path) = crawl(&site).await;
    let (n, csv) = export(&path, "a11y_violations", A11yFormat::Csv, None);
    let (m, json) = export(&path, "a11y_violations", A11yFormat::Json, None);
    assert_eq!(n, m, "{csv}");
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["disclaimer"], A11Y_DISCLAIMER);
    let rows = v["rows"].as_array().unwrap();
    assert_eq!(rows.len() as u64, n);
    let form = rows
        .iter()
        .find(|r| r["check_id"] == "static.form_label")
        .expect("form_label row");
    assert_eq!(form["address"], site.url(E, "/static/form_label"));
    assert_eq!(form["elements"], 4);
    assert_eq!(form["impact"], "critical");
}

#[tokio::test]
async fn the_level_export_follows_the_target() {
    let site = site();
    let (_d, path) = crawl(&site).await;
    let (_, all) = export(&path, "a11y_violations", A11yFormat::Csv, None);
    assert!(
        all.contains("static.zoom_disabled"),
        "zoom_disabled is WCAG 2.0 AA"
    );
    let a = WcagTarget {
        version: WcagVersion::V2_0,
        level: WcagLevel::A,
    };
    let (_, level_a) = export(&path, "a11y_violations_level", A11yFormat::Csv, Some(a));
    assert!(!level_a.contains("static.zoom_disabled"), "{level_a}");
    assert!(level_a.contains("static.form_label"), "{level_a}");
    let aa = WcagTarget {
        version: WcagVersion::V2_2,
        level: WcagLevel::Aa,
    };
    let (_, level_aa) = export(&path, "a11y_violations_level", A11yFormat::Csv, Some(aa));
    assert!(level_aa.contains("static.zoom_disabled"), "{level_aa}");
}

#[tokio::test]
async fn needs_review_is_empty_for_static_checks_and_the_summary_counts_pages() {
    let site = site();
    let (_d, path) = crawl(&site).await;
    let (n, csv) = export(&path, "a11y_needs_review", A11yFormat::Csv, None);
    assert_eq!(n, 0, "static checks never need review");
    assert!(csv.starts_with(&format!("# {A11Y_DISCLAIMER}\r\n")));

    let (n, json) = export(&path, "a11y_summary", A11yFormat::Json, None);
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["disclaimer"], A11Y_DISCLAIMER);
    let rows = v["rows"].as_array().unwrap();
    assert_eq!(rows.len() as u64, n);
    let form = rows
        .iter()
        .find(|r| r["check_id"] == "static.form_label")
        .unwrap();
    assert_eq!(form["pages"], 2, "form_label page and the mixed page");
    assert_eq!(form["elements"], 5);
    assert_eq!(form["title"], "Form field without a label");
    // Critical checks come before minor ones.
    let first = rows.first().unwrap()["impact"].as_str().unwrap();
    assert_eq!(first, "critical");
}

#[tokio::test]
async fn files_are_written_through_a_part_file_and_unknown_ids_fail() {
    let site = site();
    let (d, path) = crawl(&site).await;
    for (id, _) in A11Y_EXPORTS {
        let dest = d.path().join(format!("{id}.csv"));
        export_a11y_file(&path, id, A11yFormat::Csv, None, &dest).unwrap();
        assert!(dest.exists());
        assert!(!d.path().join(format!("{id}.csv.part")).exists());
    }
    let mut out = Vec::new();
    assert!(write_a11y(&path, "a11y_nope", A11yFormat::Csv, None, &mut out).is_err());
}
