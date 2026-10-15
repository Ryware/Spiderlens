//! Spec 010 bulk export tests (T011 to T013) on the report fixture crawl.

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use calamine::{Data, Reader as _, Xlsx, open_workbook};
use spiderlens_config::{CrawlConfig, ModeInput, SpeedSettings};
use spiderlens_engine::crawl::{Control, Crawler};
use spiderlens_results::bulk::{BulkFormat, BulkOptions, run_bulk, run_bulk_with};
use spiderlens_results::issues::ResolvedSettings;
use spiderlens_results::{ExportError, ListKind, Reader, UrlScope, ViewQuery, export_file, tab, tabs};
use spiderlens_test_fixtures::{Site, report};
use tokio::sync::watch;

struct Crawled {
    dir: tempfile::TempDir,
    path: PathBuf,
    _site: Site,
}

async fn crawl() -> Crawled {
    let site = report::site();
    let c = CrawlConfig {
        speed: SpeedSettings::FAST,
        ..CrawlConfig::default()
    };
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("bulk.spiderlens");
    let input = ModeInput::Spider {
        start_url: site.url(report::HOST, "/"),
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
        dir,
        path,
        _site: site,
    }
}

fn opts(out: &Path, format: BulkFormat, tables: &[&str]) -> BulkOptions {
    BulkOptions {
        tables: tables.iter().map(|s| s.to_string()).collect(),
        format,
        out: out.to_path_buf(),
        overwrite: false,
    }
}

fn single_export(crawl: &Path, tab_id: &str, dir: &Path) -> String {
    let t = tab(tab_id).unwrap();
    let cols: Vec<String> = t.columns.iter().map(|c| c.id.to_string()).collect();
    let q = ViewQuery {
        tab: tab_id.into(),
        filter: "all".into(),
        scope: UrlScope::All,
        search: String::new(),
        sort: None,
        segment: None,
        values: None,
    };
    let dest = dir.join(format!("single-{}.csv", tab_id.replace(':', "-")));
    export_file(
        crawl,
        &q,
        &cols,
        None,
        None,
        &dest,
        &AtomicBool::new(false),
        &mut |_, _| {},
    )
    .unwrap();
    std::fs::read_to_string(dest).unwrap()
}

#[tokio::test]
async fn us4_csv_folder_matches_single_exports() {
    let c = crawl().await;
    let out = c.dir.path().join("audit-csv");
    let mut seen = 0;
    let res = run_bulk(
        &c.path,
        &ResolvedSettings::default(),
        &opts(&out, BulkFormat::Csv, &["all"]),
        &mut |p| {
            assert!(p.tables_done <= p.tables_total);
            seen += 1;
        },
        &AtomicBool::new(false),
    )
    .unwrap();
    assert!(seen > 0, "progress is reported");
    assert!(out.join("index.csv").exists());
    assert!(!c.dir.path().join(".audit-csv.partial").exists());
    // FR-020, SC-007: every tab equals its single export, byte for byte.
    // Tabs this crawl cannot show (AMP not checked) are left out.
    let features = Reader::open(&c.path).unwrap().features().clone();
    let shown: Vec<_> = tabs()
        .iter()
        .filter(|t| t.filter("all").is_none_or(|f| features.has(f.need)))
        .collect();
    for t in &shown {
        let bulk = std::fs::read_to_string(out.join(format!("tab-{}.csv", t.id))).unwrap();
        assert_eq!(bulk, single_export(&c.path, &t.id, c.dir.path()), "tab {}", t.id);
    }
    // And every issue with URLs.
    let mut r = Reader::open(&c.path).unwrap();
    let summary = r.issues_summary(false).unwrap();
    let mut issues = 0;
    for i in summary.issues.iter().filter(|i| i.count.unwrap_or(0) > 0) {
        let bulk = std::fs::read_to_string(out.join(format!("issue-{}.csv", i.id))).unwrap();
        assert_eq!(
            bulk,
            single_export(&c.path, &format!("issue:{}", i.id), c.dir.path()),
            "{}",
            i.id
        );
        assert_eq!(bulk.lines().count() as u64, i.count.unwrap() + 1);
        issues += 1;
    }
    assert!(issues > 3);
    assert!(res.files.len() >= shown.len() + issues + 5);

    // US4-4: the links table, filtered by source or target, is the detail
    // pane's Outlinks and Inlinks.
    let links = std::fs::read_to_string(out.join("links.csv")).unwrap();
    let mut csv = csv_rows(&links);
    let header = csv.remove(0);
    assert_eq!(header[0], "Source");
    let home = report::HOST;
    let home_id = {
        let q = ViewQuery {
            tab: "internal".into(),
            filter: "all".into(),
            scope: UrlScope::All,
            search: String::new(),
            sort: None,
            segment: None,
            values: None,
        };
        let page = r.open_view(&q, 100).unwrap();
        let rows = r.rows(page.view, 0, 100).unwrap().rows;
        rows.iter()
            .find(|row| {
                row[1].as_str().unwrap().ends_with(&format!("{home}/"))
                    || row[1].as_str().unwrap().ends_with('/')
            })
            .map(|row| (row[0].as_i64().unwrap(), row[1].as_str().unwrap().to_string()))
            .unwrap()
    };
    let out_links = r.list(home_id.0, ListKind::Outlinks, 0, 1000).unwrap().total;
    assert_eq!(
        csv.iter().filter(|row| row[0] == home_id.1).count() as u64,
        out_links
    );
    let in_links = r.list(home_id.0, ListKind::Inlinks, 0, 1000).unwrap().total;
    assert_eq!(
        csv.iter().filter(|row| row[1] == home_id.1).count() as u64,
        in_links
    );

    // Redirect chains: /r1 -> /r2 -> /a, one row with both hops.
    let chains = csv_rows(&std::fs::read_to_string(out.join("redirect-chains.csv")).unwrap());
    let r1 = chains.iter().find(|row| row[0].ends_with("/r1")).unwrap();
    assert_eq!(r1[2], "2");
    assert!(r1[1].contains("/r1 (301)") && r1[1].contains("/r2 (301)"));
    assert!(r1[3].ends_with("/a"));
    assert!(
        !chains.iter().any(|row| row[0].ends_with("/r2")),
        "a chain is listed once, from its start"
    );

    // FR-025: an existing non-empty folder is refused.
    let again = run_bulk(
        &c.path,
        &ResolvedSettings::default(),
        &opts(&out, BulkFormat::Csv, &["summary"]),
        &mut |_| {},
        &AtomicBool::new(false),
    );
    assert!(matches!(again, Err(ExportError::Exists(_))));
}

fn csv_rows(s: &str) -> Vec<Vec<String>> {
    csv::ReaderBuilder::new()
        .has_headers(false)
        .from_reader(s.trim_start_matches('\u{feff}').as_bytes())
        .records()
        .map(|r| r.unwrap().iter().map(str::to_string).collect())
        .collect()
}

#[tokio::test]
async fn us4_workbook_has_overview_and_sheets() {
    let c = crawl().await;
    let out = c.dir.path().join("audit.xlsx");
    let res = run_bulk(
        &c.path,
        &ResolvedSettings::default(),
        &opts(&out, BulkFormat::Xlsx, &["tabs", "issues", "links"]),
        &mut |_| {},
        &AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!(res.files, vec![out.clone()]);
    let mut wb: Xlsx<_> = open_workbook(&out).unwrap();
    let names = wb.sheet_names();
    assert_eq!(names[0], "Overview");
    assert!(names.iter().any(|n| n == "tab-page-titles"));
    assert!(names.iter().any(|n| n == "links"));
    let overview = wb.worksheet_range("Overview").unwrap();
    // One overview line per table written.
    assert_eq!(overview.height() - 1, res.tables.len());
    // Rows match the CSV export of the same tab.
    let titles = wb.worksheet_range("tab-page-titles").unwrap();
    let single = single_export(&c.path, "page-titles", c.dir.path());
    assert_eq!(titles.height(), single.lines().count());
    // Numbers are numbers, text is stored exactly (no formula prefix).
    let rc = wb.worksheet_range("tab-response-codes").unwrap();
    assert!(
        rc.rows()
            .skip(1)
            .any(|r| matches!(r[1], Data::Float(_) | Data::Int(_)))
    );
    let all_text: Vec<String> = titles
        .rows()
        .flat_map(|r| r.iter().map(|c| c.to_string()).collect::<Vec<_>>())
        .collect();
    assert!(
        all_text.iter().any(|t| t.contains("דף בעברית")),
        "non-Latin text survives"
    );
    assert!(
        all_text.iter().all(|t| !t.starts_with("'")),
        "no CSV quote prefix in Excel"
    );
}

#[tokio::test]
async fn us4_large_tables_continue_on_numbered_sheets() {
    let c = crawl().await;
    let out = c.dir.path().join("split.xlsx");
    let res = run_bulk_with(
        &c.path,
        &ResolvedSettings::default(),
        &opts(&out, BulkFormat::Xlsx, &["links"]),
        &mut |_| {},
        &AtomicBool::new(false),
        Some(5),
    )
    .unwrap();
    assert!(res.split_sheets > 0);
    let total = res.tables[0].rows as usize;
    let mut wb: Xlsx<_> = open_workbook(&out).unwrap();
    let mut rows = 0;
    for n in wb.sheet_names().iter().filter(|n| n.starts_with("links")) {
        let r = wb.worksheet_range(n).unwrap();
        assert_eq!(
            r.get((0, 0)).unwrap().to_string(),
            "Source",
            "{n} repeats the header"
        );
        rows += r.height() - 1;
    }
    assert_eq!(rows, total, "no row is lost");
}

#[tokio::test]
async fn us4_cancel_and_failure_leave_nothing() {
    let c = crawl().await;
    let out = c.dir.path().join("cancelled.xlsx");
    let cancel = AtomicBool::new(true);
    let r = run_bulk(
        &c.path,
        &ResolvedSettings::default(),
        &opts(&out, BulkFormat::Xlsx, &["all"]),
        &mut |_| {},
        &cancel,
    );
    assert!(matches!(r, Err(ExportError::Cancelled)));
    assert!(!out.exists());
    assert!(!c.dir.path().join(".cancelled.xlsx.partial").exists());

    let folder = c.dir.path().join("cancelled-csv");
    let r = run_bulk(
        &c.path,
        &ResolvedSettings::default(),
        &opts(&folder, BulkFormat::Csv, &["all"]),
        &mut |_| {},
        &cancel,
    );
    assert!(matches!(r, Err(ExportError::Cancelled)));
    assert!(!folder.exists());
    assert!(!c.dir.path().join(".cancelled-csv.partial").exists());

    // Unknown tables are a usage error, before anything is written.
    let r = run_bulk(
        &c.path,
        &ResolvedSettings::default(),
        &opts(&folder, BulkFormat::Csv, &["nope"]),
        &mut |_| {},
        &AtomicBool::new(false),
    );
    assert!(matches!(r, Err(ExportError::Results(_))));
    assert!(!folder.exists());

    // An existing file is kept unless overwrite is set.
    std::fs::write(&out, b"keep me").unwrap();
    let r = run_bulk(
        &c.path,
        &ResolvedSettings::default(),
        &opts(&out, BulkFormat::Xlsx, &["summary"]),
        &mut |_| {},
        &AtomicBool::new(false),
    );
    assert!(matches!(r, Err(ExportError::Exists(_))));
    assert_eq!(std::fs::read(&out).unwrap(), b"keep me");
    let mut o = opts(&out, BulkFormat::Xlsx, &["summary"]);
    o.overwrite = true;
    run_bulk(
        &c.path,
        &ResolvedSettings::default(),
        &o,
        &mut |_| {},
        &AtomicBool::new(false),
    )
    .unwrap();
    assert_ne!(std::fs::read(&out).unwrap(), b"keep me");
}
