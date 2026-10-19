//! Spec 024 slice 6 against a fake Google Sheets service (plan D12): a
//! results view exported to a new spreadsheet with the rows and columns of
//! its CSV export, batches, the capacity check with truncate and split, the
//! Looker Studio link, and the hub path with the drive.file sign-in. Test
//! names carry the scenario id.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use spiderlens_config::{CrawlConfig, ModeInput, SpeedSettings};
use spiderlens_engine::crawl::{Control as CrawlControl, Crawler};
use spiderlens_integrations::credentials::MemoryStore;
use spiderlens_integrations::gsc::Api as GscApi;
use spiderlens_integrations::http::Http;
use spiderlens_integrations::hub::Hub;
use spiderlens_integrations::limits::Limits;
use spiderlens_integrations::oauth::{self, Client, Endpoints, Session, StoredGoogle};
use spiderlens_integrations::run::{Control, Ctx};
use spiderlens_integrations::settings::{Folder, ProviderSettings};
use spiderlens_integrations::sheets::{self, Api, Overflow};
use spiderlens_integrations::{Error, Provider, looker};
use spiderlens_results::{Sort, UrlScope, ViewQuery, export_file};
use spiderlens_test_fixtures::providers::{FakeGoogle, GoogleData};
use spiderlens_test_fixtures::sheets::FakeSheets;
use spiderlens_test_fixtures::{Site, SiteBuilder};
use tokio::sync::watch;
use url::Url;

const HOST: &str = "127.0.0.1";
const PAGES: usize = 30;

fn site() -> Site {
    let mut b = SiteBuilder::new();
    for i in 0..PAGES {
        let next = format!("/p/{}", (i + 1) % PAGES);
        b = b.page(HOST, &format!("/p/{i}"), "", &[&next, "/p/0"]);
    }
    b.start()
}

async fn crawl(site: &Site, dir: &Path) -> PathBuf {
    let path = dir.join("c.spiderlens");
    let crawler = Crawler::create(
        &path,
        CrawlConfig {
            speed: SpeedSettings::FAST,
            ..CrawlConfig::default()
        },
        &ModeInput::Spider {
            start_url: site.url(HOST, "/p/0"),
        },
    )
    .unwrap();
    let (_tx, rx) = watch::channel(CrawlControl::Run);
    let s = crawler.run(rx, None).await.unwrap();
    assert_eq!(s.crawled as usize, PAGES);
    path
}

fn endpoints(g: &FakeGoogle) -> Endpoints {
    Endpoints {
        auth: g.auth_url(),
        token: g.token_url(),
        revoke: g.revoke_url(),
    }
}

fn session(g: &FakeGoogle) -> Session {
    Session::new(
        Http::new(Duration::from_secs(10)),
        endpoints(g),
        StoredGoogle {
            client_id: "test-client".into(),
            client_secret: None,
            refresh_token: g.issue_refresh_token(),
        },
    )
}

fn ctx() -> Ctx {
    let mut limits = Limits::defaults(Provider::Sheets);
    limits.per_minute = 6000;
    Ctx::new(Provider::Sheets, limits, None)
}

fn api(s: &FakeSheets, cell_capacity: u64) -> Api {
    Api {
        base: s.base_url(),
        cell_capacity,
    }
}

/// The CSV export of a view, as the app and `spiderlens export` write it.
fn export_csv(crawl: &Path, query: &ViewQuery, columns: &[String], dest: &Path) -> u64 {
    export_file(
        crawl,
        query,
        columns,
        None,
        None,
        dest,
        &AtomicBool::new(false),
        &mut |_, _| {},
    )
    .unwrap()
}

fn query(tab: &str, filter: &str, sort: Option<(&str, bool)>) -> ViewQuery {
    ViewQuery {
        tab: tab.into(),
        filter: filter.into(),
        scope: UrlScope::All,
        search: String::new(),
        sort: sort.map(|(c, desc)| Sort {
            column: c.into(),
            desc,
        }),
        segment: None,
        values: None,
    }
}

/// The records of a CSV file as text, without the byte order mark.
fn csv_rows(path: &Path) -> Vec<Vec<String>> {
    let mut rd = csv::ReaderBuilder::new()
        .has_headers(false)
        .from_path(path)
        .unwrap();
    rd.records()
        .map(|r| {
            r.unwrap()
                .iter()
                .map(|f| f.trim_start_matches('\u{feff}').to_string())
                .collect()
        })
        .collect()
}

/// A CSV of `rows` data rows and `cols` columns, written directly.
fn plain_csv(dir: &Path, name: &str, rows: usize, cols: usize) -> PathBuf {
    let path = dir.join(name);
    let mut w = csv::Writer::from_path(&path).unwrap();
    w.write_record((0..cols).map(|c| format!("Col {c}"))).unwrap();
    for r in 0..rows {
        w.write_record((0..cols).map(|c| format!("r{r}c{c}"))).unwrap();
    }
    w.flush().unwrap();
    path
}

async fn export(
    g: &FakeGoogle,
    s: &FakeSheets,
    capacity: u64,
    csv: &Path,
    overflow: Overflow,
) -> Result<sheets::SheetExport, Error> {
    let mut seen: Vec<(u64, u64)> = Vec::new();
    let r = sheets::export_csv(
        &ctx(),
        &session(g),
        &api(s, capacity),
        csv,
        "Crawl export",
        "Page titles",
        overflow,
        &Control::default(),
        &mut |a, b| seen.push((a, b)),
    )
    .await;
    if let Ok(x) = &r {
        // Progress ends at the total and never goes backwards.
        assert_eq!(seen.last(), Some(&(x.rows, x.rows)));
        assert!(seen.windows(2).all(|w| w[0].0 <= w[1].0));
    }
    r
}

#[tokio::test]
async fn us6_1_the_sheet_holds_the_rows_and_columns_of_the_csv_export_of_the_view() {
    let site = site();
    let dir = tempfile::tempdir().unwrap();
    let path = crawl(&site, dir.path()).await;
    let g = FakeGoogle::start(GoogleData::default());
    let s = FakeSheets::start(10_000_000);

    // A filter, a sort and chosen columns, as the user sees the table.
    let q = query("response-codes", "all", Some(("address", true)));
    let columns: Vec<String> = ["address", "status_code"].map(String::from).to_vec();
    let csv = dir.path().join("view.csv");
    let n = export_csv(&path, &q, &columns, &csv);
    assert_eq!(n as usize, PAGES);

    let r = export(&g, &s, 10_000_000, &csv, Overflow::Refuse).await.unwrap();
    assert_eq!(
        (r.rows, r.total_rows, r.truncated),
        (PAGES as u64, PAGES as u64, false)
    );
    assert_eq!(r.spreadsheets.len(), 1);

    let sheet = &s.spreadsheets()[0];
    assert_eq!(sheet.title, "Crawl export");
    assert_eq!(sheet.tabs[0].title, "Page titles");
    // Same header, same rows, same order as the CSV.
    let got = sheet.text_rows(0);
    let want = csv_rows(&csv);
    assert_eq!(got, want);
    assert_eq!(got[0], vec!["Address".to_string(), "Status code".to_string()]);
    assert_eq!(got.len(), PAGES + 1);
    // Sorted by address, descending (natural order: /p/10 after /p/9).
    let addresses: Vec<&String> = got[1..].iter().map(|r| &r[0]).collect();
    let n = |a: &str| a.rsplit('/').next().unwrap().parse::<u32>().unwrap();
    assert!(addresses.windows(2).all(|w| n(w[0]) > n(w[1])), "{addresses:?}");
    // Numbers arrive as numbers, so a sheet can sum and sort them.
    assert!(sheet.tabs[0].values[1][1].is_number());
    // The grid was made to fit the data exactly.
    assert_eq!(
        (sheet.tabs[0].grid_rows, sheet.tabs[0].grid_cols),
        (PAGES as u64 + 1, 2)
    );
    // The link the app shows is the one Google returned.
    assert!(r.spreadsheets[0].url.contains(&sheet.id));
    // Only the sign-in's access token was sent.
    assert!(s.tokens().iter().all(|t| t.starts_with("access-")));
}

#[tokio::test]
async fn us6_1_a_filtered_view_exports_only_its_rows() {
    let site = site();
    let dir = tempfile::tempdir().unwrap();
    let path = crawl(&site, dir.path()).await;
    let g = FakeGoogle::start(GoogleData::default());
    let s = FakeSheets::start(10_000_000);
    let q = query("response-codes", "success", None);
    let csv = dir.path().join("view.csv");
    let n = export_csv(&path, &q, &[], &csv);
    assert_eq!(n as usize, PAGES);
    let q404 = query("response-codes", "client_error", None);
    let none = dir.path().join("none.csv");
    assert_eq!(export_csv(&path, &q404, &[], &none), 0);

    let r = export(&g, &s, 10_000_000, &none, Overflow::Refuse).await.unwrap();
    assert_eq!(r.rows, 0);
    // An empty view still gets its header row.
    assert_eq!(s.spreadsheets()[0].text_rows(0).len(), 1);
    let r = export(&g, &s, 10_000_000, &csv, Overflow::Refuse).await.unwrap();
    assert_eq!(r.rows as usize, PAGES);
}

#[tokio::test]
async fn fr_030_rows_go_in_batches_of_at_most_5000() {
    let dir = tempfile::tempdir().unwrap();
    let g = FakeGoogle::start(GoogleData::default());
    let s = FakeSheets::start(10_000_000);
    let csv = plain_csv(dir.path(), "big.csv", 12_000, 3);
    let r = export(&g, &s, 10_000_000, &csv, Overflow::Refuse).await.unwrap();
    assert_eq!(r.rows, 12_000);
    // The header rides with the first batch: 5,001 + 5,000 + 2,000 rows.
    assert_eq!(s.requests("values:batchUpdate"), 3);
    assert_eq!(s.requests("/spreadsheets"), 1);
    let values = &s.spreadsheets()[0].tabs[0].values;
    assert_eq!(values.len(), 12_001);
    assert_eq!(values[0][0], "Col 0");
    assert_eq!(values[7_777][1], "r7776c1");
    assert_eq!(values[12_000][2], "r11999c2");
    assert!(values.iter().all(|r| r.len() == 3));
}

#[tokio::test]
async fn us6_2_an_export_that_does_not_fit_is_warned_about_before_anything_is_sent() {
    let dir = tempfile::tempdir().unwrap();
    let g = FakeGoogle::start(GoogleData::default());
    let s = FakeSheets::start(100);
    // 51 rows with the header, 3 columns: 153 cells in a 100-cell spreadsheet.
    let csv = plain_csv(dir.path(), "many.csv", 50, 3);
    let (rows, cols) = sheets::scan_csv(&csv).unwrap();
    assert_eq!((rows, cols), (50, 3));
    let plan = sheets::plan(rows, cols, 100);
    assert!(!plan.fits);
    assert_eq!(
        (plan.cells, plan.rows_per_spreadsheet, plan.spreadsheets),
        (153, 32, 2)
    );
    let warning = plan.warning().unwrap();
    assert!(
        warning.contains("153 cells") && warning.contains("split it across 2"),
        "{warning}"
    );

    let e = export(&g, &s, 100, &csv, Overflow::Refuse).await.unwrap_err();
    assert!(e.to_string().contains("153 cells"), "{e}");
    assert!(s.log().is_empty(), "nothing was sent");
}

#[tokio::test]
async fn us6_2_truncate_sends_the_first_rows_that_fit() {
    let dir = tempfile::tempdir().unwrap();
    let g = FakeGoogle::start(GoogleData::default());
    let s = FakeSheets::start(100);
    let csv = plain_csv(dir.path(), "many.csv", 50, 3);
    let r = export(&g, &s, 100, &csv, Overflow::Truncate).await.unwrap();
    assert_eq!((r.rows, r.total_rows, r.truncated), (32, 50, true));
    assert_eq!(r.spreadsheets.len(), 1);
    let sheet = &s.spreadsheets()[0];
    assert_eq!(sheet.text_rows(0).len(), 33);
    assert_eq!(sheet.text_rows(0)[32][0], "r31c0");
}

#[tokio::test]
async fn us6_2_split_makes_several_spreadsheets_each_within_the_limit() {
    let dir = tempfile::tempdir().unwrap();
    let g = FakeGoogle::start(GoogleData::default());
    let s = FakeSheets::start(100);
    let csv = plain_csv(dir.path(), "many.csv", 50, 3);
    let r = export(&g, &s, 100, &csv, Overflow::Split).await.unwrap();
    assert_eq!((r.rows, r.total_rows, r.truncated), (50, 50, false));
    assert_eq!(r.spreadsheets.len(), 2);
    assert_eq!(r.spreadsheets[0].title, "Crawl export (part 1 of 2)");
    assert_eq!(r.spreadsheets[1].title, "Crawl export (part 2 of 2)");
    assert_eq!(r.spreadsheets.iter().map(|x| x.rows).sum::<u64>(), 50);
    let sheets = s.spreadsheets();
    // Each part has its own header and the next rows, none lost or repeated.
    let a = sheets[0].text_rows(0);
    let b = sheets[1].text_rows(0);
    assert_eq!((a.len(), b.len()), (33, 19));
    assert_eq!(a[0], b[0]);
    assert_eq!(a[32][0], "r31c0");
    assert_eq!(b[1][0], "r32c0");
    assert_eq!(b[18][0], "r49c0");
}

#[tokio::test]
async fn too_many_columns_for_any_spreadsheet_are_refused() {
    let dir = tempfile::tempdir().unwrap();
    let g = FakeGoogle::start(GoogleData::default());
    let s = FakeSheets::start(5);
    let csv = plain_csv(dir.path(), "wide.csv", 2, 6);
    let e = export(&g, &s, 5, &csv, Overflow::Split).await.unwrap_err();
    assert!(e.to_string().contains("columns"), "{e}");
    assert!(s.log().is_empty());
}

#[tokio::test]
async fn an_expired_token_is_refreshed_once_and_the_export_goes_on() {
    let dir = tempfile::tempdir().unwrap();
    let g = FakeGoogle::start(GoogleData::default());
    let s = FakeSheets::start(1_000_000);
    let csv = plain_csv(dir.path(), "small.csv", 10, 2);
    s.fail_next(401, r#"{"error":{"message":"Invalid Credentials"}}"#);
    let r = export(&g, &s, 1_000_000, &csv, Overflow::Refuse).await.unwrap();
    assert_eq!(r.rows, 10);
    assert_eq!(s.spreadsheets().len(), 1);
}

#[tokio::test]
async fn a_cancelled_export_stops_between_batches_and_says_where_the_sheet_is() {
    let dir = tempfile::tempdir().unwrap();
    let g = FakeGoogle::start(GoogleData::default());
    let s = FakeSheets::start(10_000_000);
    let csv = plain_csv(dir.path(), "big.csv", 12_000, 3);
    let control = Control::default();
    let c = control.clone();
    let mut batches = 0;
    let e = sheets::export_csv(
        &ctx(),
        &session(&g),
        &api(&s, 10_000_000),
        &csv,
        "Crawl export",
        "Results",
        Overflow::Refuse,
        &control,
        &mut |_, _| {
            batches += 1;
            if batches == 2 {
                c.cancel();
            }
        },
    )
    .await
    .unwrap_err();
    assert!(
        e.to_string().contains("cancelled") && e.to_string().contains("docs.google.com"),
        "{e}"
    );
    assert!(s.requests("values:batchUpdate") < 3);
}

#[tokio::test]
async fn us6_3_the_looker_studio_link_points_at_the_exported_sheet_and_worksheet() {
    let dir = tempfile::tempdir().unwrap();
    let g = FakeGoogle::start(GoogleData::default());
    let s = FakeSheets::start(10_000_000);
    let csv = plain_csv(dir.path(), "small.csv", 5, 2);
    let r = export(&g, &s, 10_000_000, &csv, Overflow::Refuse).await.unwrap();
    let made = &r.spreadsheets[0];
    let link = looker::link_for(made).unwrap();

    let u = Url::parse(&link).unwrap();
    assert_eq!(u.host_str(), Some("lookerstudio.google.com"));
    let q: std::collections::HashMap<_, _> = u.query_pairs().into_owned().collect();
    let sheet = &s.spreadsheets()[0];
    assert_eq!(q["ds.ds0.connector"], "googleSheets");
    assert_eq!(q["ds.ds0.spreadsheetId"], sheet.id);
    assert_eq!(q["ds.ds0.worksheetId"], sheet.tabs[0].id.to_string());
    assert_eq!(q["r.reportName"], "Crawl export");
    // Building the link sends nothing anywhere.
    let calls = s.log().len();
    let _ = looker::link_for(made).unwrap();
    assert_eq!(s.log().len(), calls);
}

fn hub(dir: &Path, g: &FakeGoogle, s: &FakeSheets) -> Hub {
    Hub::new(Folder::new(dir), Arc::new(MemoryStore::default()), None)
        .with_google(
            endpoints(g),
            GscApi {
                base: g.webmasters_url(),
            },
            Some(Client {
                id: "test-client".into(),
                secret: None,
            }),
        )
        .with_sheets(api(s, 10_000_000))
}

fn follow(url: &str) {
    // Stands in for the system browser.
    let url = url.to_string();
    tokio::spawn(async move {
        reqwest::get(url).await.unwrap();
    });
}

#[test]
fn fr_015_the_hub_asks_google_only_for_files_it_creates_and_needs_the_source_on() {
    assert_eq!(
        oauth::scope_for("sheets"),
        Some("https://www.googleapis.com/auth/drive.file")
    );
    let dir = tempfile::tempdir().unwrap();
    let g = FakeGoogle::start(GoogleData {
        email: "ana@example.com".into(),
        ..GoogleData::default()
    });
    let s = FakeSheets::start(10_000_000);
    let h = hub(dir.path(), &g, &s);
    let csv = plain_csv(dir.path(), "small.csv", 20, 4);
    let go = |h: &Hub| {
        h.export_sheets(
            &csv,
            "Crawl export",
            "Results",
            None,
            Overflow::Refuse,
            &Control::default(),
            &mut |_, _| {},
        )
    };

    // Off until turned on (FR-001), then it needs a Google account with the scope.
    let e = go(&h).unwrap_err();
    assert!(e.to_string().contains("is off"), "{e}");
    h.save_settings(
        Provider::Sheets,
        ProviderSettings {
            enabled: true,
            ..ProviderSettings::default()
        },
    )
    .unwrap();
    assert!(matches!(go(&h), Err(Error::NoConnection(_))));
    assert!(s.log().is_empty());

    let c = h
        .connect_google(&["sheets"], Duration::from_secs(20), &Control::default(), follow)
        .unwrap();
    assert_eq!(c.scopes, vec!["sheets".to_string()]);
    let plan = h.sheets_plan(&csv).unwrap();
    assert!(plan.fits);
    assert_eq!((plan.rows, plan.columns), (20, 4));
    let r = go(&h).unwrap();
    assert_eq!(r.rows, 20);
    let link = h.looker_link(&r.spreadsheets[0]).unwrap();
    assert!(link.contains(&r.spreadsheets[0].spreadsheet_id));
    assert_eq!(s.spreadsheets()[0].text_rows(0), csv_rows(&csv));
}
