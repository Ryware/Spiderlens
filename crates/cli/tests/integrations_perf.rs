//! `spiderlens integrations` for field data and lab tests (spec 024 US3,
//! contracts/cli.md) against fake Chrome UX Report and PageSpeed services,
//! with a private settings folder and credential file (test builds only).

use std::path::{Path, PathBuf};

use assert_cmd::Command;
use spiderlens_config::{CrawlConfig, ModeInput, SpeedSettings};
use spiderlens_engine::crawl::{Control, Crawler};
use spiderlens_test_fixtures::perf_providers::{CruxVitals, FakePerf, PerfData};
use spiderlens_test_fixtures::{Site, SiteBuilder};
use tokio::sync::watch;

const HOST: &str = "127.0.0.1";
const KEY: &str = "k-perf-123";

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

fn cli(home: &Path, fake: &FakePerf) -> Command {
    let mut c = Command::cargo_bin("spiderlens").unwrap();
    c.env("SPIDERLENS_TEST_HOME", home)
        .env("SPIDERLENS_TEST_PERF", fake.base())
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
fn connect_key_enable_fetch_field_data_and_lab_tests() {
    let site = site();
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().join("home");
    let path = crawl(&site, dir.path());
    let file = path.to_str().unwrap();
    let mut data = PerfData {
        api_key: KEY.into(),
        ..PerfData::default()
    };
    for i in 0..4 {
        for device in ["PHONE", "DESKTOP"] {
            data.crux
                .insert((site.url(HOST, &format!("/p/{i}")), device.into()), CruxVitals::GOOD);
        }
    }
    let fake = FakePerf::start(data);

    // No key and nothing turned on: a usage error that says what to do.
    let out = cli(&home, &fake)
        .args(["fetch", file, "--provider", "field-data"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));

    // One key, read from standard input, serves both providers.
    let out = cli(&home, &fake)
        .args(["connect", "google-key", "--label", "main"])
        .write_stdin(format!("{KEY}\n"))
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let listing = std::fs::read_to_string(home.join("integrations/connections.json")).unwrap();
    assert!(listing.contains("field_data") && listing.contains("lab_tests") && !listing.contains(KEY));
    let keys = std::fs::read_to_string(home.join("test-keychain.json")).unwrap();
    assert!(keys.contains(KEY));

    // Still off until turned on.
    let out = cli(&home, &fake)
        .args(["fetch", file, "--provider", "field-data"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    ok(&mut cli(&home, &fake), &["enable", "field-data"]);
    ok(&mut cli(&home, &fake), &["enable", "lab-tests"]);

    // The estimate comes first, on standard error.
    let out = cli(&home, &fake)
        .args(["fetch", file, "--provider", "field-data", "--json"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("8 requests for 4 pages"),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let run: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(run["state"], "finished");
    assert_eq!(run["provider"], "field_data");
    assert_eq!(run["done"], 4);

    // Lab tests: both devices for the first two pages.
    let json = ok(
        &mut cli(&home, &fake),
        &[
            "fetch",
            file,
            "--provider",
            "lab-tests",
            "--strategy",
            "both",
            "--pages",
            "all",
            "--max-pages",
            "2",
            "--json",
        ],
    );
    let run: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(run["state"], "finished");
    assert_eq!(run["done"], 2);
    assert_eq!(fake.psi_calls().len(), 4);

    // The same data through the export command (results parity).
    let csv = Command::cargo_bin("spiderlens")
        .unwrap()
        .args([
            "export",
            file,
            "--tab",
            "page-experience",
            "--filter",
            "all",
            "--columns",
            "address,phone_lcp,phone_level",
        ])
        .output()
        .unwrap();
    assert!(csv.status.success(), "{}", String::from_utf8_lossy(&csv.stderr));
    let csv = String::from_utf8(csv.stdout).unwrap();
    assert_eq!(csv.lines().count(), 5, "{csv}");
    assert!(csv.contains("2100") && csv.contains("Page data"), "{csv}");

    let runs = ok(&mut cli(&home, &fake), &["runs", file]);
    assert!(runs.contains("field_data") && runs.contains("lab_tests"), "{runs}");
    ok(&mut cli(&home, &fake), &["remove", file, "--provider", "field-data"]);
    ok(&mut cli(&home, &fake), &["remove", file, "--provider", "lab-tests"]);
    assert!(ok(&mut cli(&home, &fake), &["runs", file]).trim().is_empty());
}

#[test]
fn a_quota_stop_exits_with_ten_and_resume_finishes_the_run() {
    let site = site();
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().join("home");
    let path = crawl(&site, dir.path());
    let file = path.to_str().unwrap();
    let fake = FakePerf::start(PerfData {
        api_key: KEY.into(),
        ..PerfData::default()
    });
    let out = cli(&home, &fake)
        .args(["connect", "lab-tests", "--label", "main"])
        .write_stdin(format!("{KEY}\n"))
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    ok(&mut cli(&home, &fake), &["enable", "lab-tests"]);

    fake.fail_next(
        "psi",
        429,
        r#"{"error":{"code":429,"message":"Quota exceeded for quota metric 'Queries' and limit 'Queries per day'"}}"#,
    );
    let out = cli(&home, &fake)
        .args(["fetch", file, "--provider", "lab-tests", "--pages", "all", "--max-pages", "3"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(10), "{}", String::from_utf8_lossy(&out.stderr));
    assert!(String::from_utf8_lossy(&out.stderr).contains("resume after"));

    let out = ok(&mut cli(&home, &fake), &["resume", file]);
    assert!(out.contains("finished"), "{out}");
}
