//! `spiderlens integrations` (spec 024 contracts/cli.md) against fake Google
//! services, with a private settings folder and credential file (test builds
//! only), so nothing touches the real keychain.

use std::path::{Path, PathBuf};

use assert_cmd::Command;
use spiderlens_config::{CrawlConfig, ModeInput, SpeedSettings};
use spiderlens_engine::crawl::{Control, Crawler};
use spiderlens_test_fixtures::links::{FakeLinks, LinkKeys};
use spiderlens_test_fixtures::providers::{FakeGoogle, GoogleData, GscRow};
use spiderlens_test_fixtures::sheets::FakeSheets;
use spiderlens_test_fixtures::{Site, SiteBuilder};
use tokio::sync::watch;

const HOST: &str = "127.0.0.1";

fn site() -> Site {
    let mut b = SiteBuilder::new();
    for i in 0..4 {
        b = b.page(HOST, &format!("/p/{i}"), "", &[&format!("/p/{}", (i + 1) % 4)]);
    }
    b.start()
}

fn crawl(site: &Site, dir: &Path) -> PathBuf {
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
    let (_tx, rx) = watch::channel(Control::Run);
    tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(crawler.run(rx, None))
        .unwrap();
    path
}

fn cli(home: &Path, g: &FakeGoogle) -> Command {
    let mut c = Command::cargo_bin("spiderlens").unwrap();
    c.env("SPIDERLENS_TEST_HOME", home)
        .env("SPIDERLENS_TEST_GOOGLE", g.base())
        .env("SPIDERLENS_TEST_BROWSER", "follow")
        .arg("integrations");
    c
}

fn ok(c: &mut Command, args: &[&str]) -> String {
    let out = c.args(args).output().unwrap();
    assert!(
        out.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap()
}

#[test]
fn connect_enable_fetch_runs_remove_and_forget() {
    let site = site();
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().join("home");
    let path = crawl(&site, dir.path());
    let file = path.to_str().unwrap();
    let g = FakeGoogle::start(GoogleData {
        email: "ana@example.com".into(),
        sites: vec![(site.url(HOST, "/"), "siteOwner".into())],
        rows: vec![GscRow {
            page: site.url(HOST, "/p/1"),
            clicks: 7,
            impressions: 70,
            position: 2.0,
        }],
        ..GoogleData::default()
    });

    // Not connected: a usage error that says what to do.
    let out = cli(&home, &g)
        .args(["fetch", file, "--provider", "search-console"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("connect"));

    assert!(ok(&mut cli(&home, &g), &["connect", "google", "--no-browser"]).contains("ana@example.com"));
    let props = ok(&mut cli(&home, &g), &["properties", "--crawl", file]);
    assert!(props.contains("(suggested)"), "{props}");

    // Off until turned on.
    let out = cli(&home, &g)
        .args(["fetch", file, "--provider", "search-console"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    ok(&mut cli(&home, &g), &["enable", "search-console"]);

    let json = ok(
        &mut cli(&home, &g),
        &[
            "fetch",
            file,
            "--provider",
            "search-console",
            "--from",
            "2026-09-01",
            "--to",
            "2026-09-28",
            "--json",
        ],
    );
    let run: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(run["state"], "finished");
    assert_eq!(run["matched"], 1);

    let list: serde_json::Value =
        serde_json::from_str(&ok(&mut cli(&home, &g), &["list", "--json"])).unwrap();
    let sc = list["providers"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["provider"] == "search_console")
        .unwrap();
    assert_eq!(sc["settings"]["enabled"], true);

    // The same data through the export command (results parity).
    let csv = Command::cargo_bin("spiderlens")
        .unwrap()
        .args([
            "export",
            file,
            "--tab",
            "search-console",
            "--filter",
            "clicks",
            "--columns",
            "address,gsc_clicks",
        ])
        .output()
        .unwrap();
    assert!(csv.status.success(), "{}", String::from_utf8_lossy(&csv.stderr));
    let csv = String::from_utf8(csv.stdout).unwrap();
    assert_eq!(csv.lines().count(), 2, "{csv}");
    assert!(csv.contains("/p/1") && csv.contains(",7"), "{csv}");

    let runs = ok(&mut cli(&home, &g), &["runs", file]);
    assert!(runs.contains("finished"), "{runs}");
    ok(&mut cli(&home, &g), &["remove", file, "--all"]);
    assert!(ok(&mut cli(&home, &g), &["runs", file]).trim().is_empty());

    ok(
        &mut cli(&home, &g),
        &["forget", "google", "--label", "ana@example.com"],
    );
    assert_eq!(g.revoked().len(), 1);
    let keys = std::fs::read_to_string(home.join("test-keychain.json")).unwrap();
    assert!(!keys.contains("ana@example.com"));
}

#[test]
fn keys_are_read_from_standard_input_never_arguments() {
    let dir = tempfile::tempdir().unwrap();
    let g = FakeGoogle::start(GoogleData::default());
    let out = cli(dir.path(), &g)
        .args(["connect", "ahrefs", "--label", "Agency"])
        .write_stdin("k-secret-1\n")
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let listing = std::fs::read_to_string(dir.path().join("integrations/connections.json")).unwrap();
    assert!(listing.contains("Agency") && !listing.contains("k-secret-1"));
    let keys = std::fs::read_to_string(dir.path().join("test-keychain.json")).unwrap();
    assert!(keys.contains("k-secret-1"));
}

fn with_fakes(c: &mut Command, links: &FakeLinks, sheets: &FakeSheets) {
    c.env("SPIDERLENS_TEST_LINKS", links.base())
        .env("SPIDERLENS_TEST_SHEETS", sheets.base_url());
}

#[test]
fn link_metrics_show_the_estimate_need_a_yes_and_stop_at_the_cap() {
    let site = site();
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().join("home");
    let path = crawl(&site, dir.path());
    let file = path.to_str().unwrap();
    let g = FakeGoogle::start(GoogleData::default());
    let links = FakeLinks::start(LinkKeys::default());
    let sheets = FakeSheets::start(10_000_000);
    let run = |args: &[&str], stdin: &str| {
        let mut c = cli(&home, &g);
        with_fakes(&mut c, &links, &sheets);
        c.args(args).write_stdin(stdin.to_string()).output().unwrap()
    };

    let out = run(&["connect", "majestic", "--label", "Agency"], "majestic-key\n");
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    assert!(run(&["enable", "majestic"], "").status.success());

    // The estimate is shown in the provider's units; without --yes nothing is sent.
    let out = run(&["fetch", file, "--provider", "majestic"], "");
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("rows"));
    assert_eq!(links.requests("majestic"), 0);

    // A cap below the estimate stops the run: exit 10, and no more units used.
    let out = run(
        &[
            "fetch",
            file,
            "--provider",
            "majestic",
            "--yes",
            "--cap",
            "2",
            "--json",
        ],
        "",
    );
    assert_eq!(
        out.status.code(),
        Some(10),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let r: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(r["done"], 2);
    assert!(links.used("majestic") <= 2);

    // The rest of the pages; the first two come from the cache.
    let out = run(&["fetch", file, "--provider", "majestic", "--yes"], "");
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(links.used("majestic") <= 4);

    let csv = Command::cargo_bin("spiderlens")
        .unwrap()
        .args([
            "export",
            file,
            "--tab",
            "link-metrics",
            "--filter",
            "with_data",
            "--columns",
            "address,majestic_trust_flow",
        ])
        .output()
        .unwrap();
    assert!(csv.status.success(), "{}", String::from_utf8_lossy(&csv.stderr));
    assert_eq!(String::from_utf8(csv.stdout).unwrap().lines().count(), 5);
}

#[test]
fn export_sheets_creates_a_sheet_and_prints_the_looker_link() {
    let site = site();
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().join("home");
    let path = crawl(&site, dir.path());
    let file = path.to_str().unwrap();
    let g = FakeGoogle::start(GoogleData {
        email: "ana@example.com".into(),
        ..GoogleData::default()
    });
    let links = FakeLinks::start(LinkKeys::default());
    let sheets = FakeSheets::start(10_000_000);
    let run = |args: &[&str]| {
        let mut c = cli(&home, &g);
        with_fakes(&mut c, &links, &sheets);
        c.args(args).output().unwrap()
    };
    let out = run(&["connect", "google", "--scope", "sheets", "--no-browser"]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    assert!(run(&["enable", "sheets"]).status.success());

    let out = run(&[
        "export-sheets",
        file,
        "--tab",
        "internal",
        "--columns",
        "address,status_code",
        "--looker",
    ]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(text.contains("docs.google.com/spreadsheets/d/"), "{text}");
    assert!(
        text.contains("lookerstudio.google.com/reporting/create"),
        "{text}"
    );
    let made = sheets.spreadsheets();
    assert_eq!(made.len(), 1);
    // Header plus one row per crawled page.
    assert_eq!(made[0].text_rows(0).len(), 5);

    // Too many cells for one spreadsheet: refused with the choices, nothing sent.
    let mut c = cli(&home, &g);
    with_fakes(&mut c, &links, &sheets);
    c.env("SPIDERLENS_TEST_SHEETS_CAPACITY", "6");
    let out = c
        .args(["export-sheets", file, "--columns", "address,status_code"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("--split"));
    assert_eq!(sheets.spreadsheets().len(), 1);
}
