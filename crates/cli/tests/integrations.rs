//! `spiderlens integrations` (spec 024 contracts/cli.md) against fake Google
//! services, with a private settings folder and credential file (test builds
//! only), so nothing touches the real keychain.

use std::path::{Path, PathBuf};

use assert_cmd::Command;
use spiderlens_config::{CrawlConfig, ModeInput, SpeedSettings};
use spiderlens_engine::crawl::{Control, Crawler};
use spiderlens_test_fixtures::providers::{FakeGoogle, GoogleData, GscRow};
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
    tokio::runtime::Runtime::new().unwrap().block_on(crawler.run(rx, None)).unwrap();
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
    assert!(out.status.success(), "{args:?}: {}", String::from_utf8_lossy(&out.stderr));
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
    let out = cli(&home, &g).args(["fetch", file, "--provider", "search-console"]).output().unwrap();
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("connect"));

    assert!(ok(&mut cli(&home, &g), &["connect", "google", "--no-browser"]).contains("ana@example.com"));
    let props = ok(&mut cli(&home, &g), &["properties", "--crawl", file]);
    assert!(props.contains("(suggested)"), "{props}");

    // Off until turned on.
    let out = cli(&home, &g).args(["fetch", file, "--provider", "search-console"]).output().unwrap();
    assert_eq!(out.status.code(), Some(2));
    ok(&mut cli(&home, &g), &["enable", "search-console"]);

    let json = ok(
        &mut cli(&home, &g),
        &["fetch", file, "--provider", "search-console", "--from", "2026-09-01", "--to", "2026-09-28", "--json"],
    );
    let run: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(run["state"], "finished");
    assert_eq!(run["matched"], 1);

    let list: serde_json::Value = serde_json::from_str(&ok(&mut cli(&home, &g), &["list", "--json"])).unwrap();
    let sc = list["providers"].as_array().unwrap().iter().find(|p| p["provider"] == "search_console").unwrap();
    assert_eq!(sc["settings"]["enabled"], true);

    // The same data through the export command (results parity).
    let csv = Command::cargo_bin("spiderlens")
        .unwrap()
        .args(["export", file, "--tab", "search-console", "--filter", "clicks", "--columns", "address,gsc_clicks"])
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

    ok(&mut cli(&home, &g), &["forget", "google", "--label", "ana@example.com"]);
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
