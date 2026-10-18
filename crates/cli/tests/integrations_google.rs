//! `spiderlens integrations fetch --provider url-inspection|analytics`
//! (spec 024 tasks T031 and T039) against fake Google services, with a
//! private settings folder and credential file (test builds only).

use std::path::{Path, PathBuf};

use assert_cmd::Command;
use spiderlens_config::{CrawlConfig, ModeInput, SpeedSettings};
use spiderlens_engine::crawl::{Control, Crawler};
use spiderlens_test_fixtures::google_apis::{
    AnalyticsData, FakeAnalytics, FakeInspection, GaProperty, GaRow, GaStream,
};
use spiderlens_test_fixtures::providers::{FakeGoogle, GoogleData};
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

struct Fakes {
    google: FakeGoogle,
    inspection: FakeInspection,
    analytics: FakeAnalytics,
}

fn cli(home: &Path, f: &Fakes) -> Command {
    let mut c = Command::cargo_bin("spiderlens").unwrap();
    c.env("SPIDERLENS_TEST_HOME", home)
        .env("SPIDERLENS_TEST_GOOGLE", f.google.base())
        .env("SPIDERLENS_TEST_INSPECTION", f.inspection.base())
        .env("SPIDERLENS_TEST_ANALYTICS", f.analytics.admin_base())
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
fn inspect_and_analytics_from_the_command_line() {
    let site = site();
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().join("home");
    let path = crawl(&site, dir.path());
    let file = path.to_str().unwrap();
    let f = Fakes {
        google: FakeGoogle::start(GoogleData {
            email: "ana@example.com".into(),
            sites: vec![(site.url(HOST, "/"), "siteOwner".into())],
            ..GoogleData::default()
        }),
        // Google's own allowance: two inspections a day.
        inspection: FakeInspection::start(vec![site.url(HOST, "/")], 2),
        analytics: FakeAnalytics::start(AnalyticsData {
            properties: vec![GaProperty {
                id: "123".into(),
                name: "Shop".into(),
                streams: vec![GaStream::web("456", "Website", &format!("http://{HOST}"))],
            }],
            rows: vec![
                GaRow::new("456", &format!("{HOST}:{}", site.port()), "/p/1", 9, 5),
                GaRow::new("456", &format!("{HOST}:{}", site.port()), "/p/2", 4, 4),
                GaRow::new("456", &format!("{HOST}:{}", site.port()), "/elsewhere", 3, 1),
            ],
            ..AnalyticsData::default()
        }),
    };

    ok(
        &mut cli(&home, &f),
        &[
            "connect",
            "google",
            "--no-browser",
            "--scope",
            "search-console",
            "--scope",
            "analytics",
        ],
    );

    // Off until turned on: a usage error.
    let out = cli(&home, &f)
        .args(["fetch", file, "--provider", "url-inspection"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("off"));
    ok(&mut cli(&home, &f), &["enable", "url-inspection"]);
    ok(&mut cli(&home, &f), &["enable", "analytics"]);

    // URL Inspection: the count against the allowance is printed first.
    let out = cli(&home, &f)
        .args([
            "fetch",
            file,
            "--provider",
            "url-inspection",
            "--max",
            "2",
            "--json",
        ])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("Today's allowance: 2000 left of 2000"),
        "{stderr}"
    );
    let run: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(run["state"], "finished");
    assert_eq!(run["done"], 2);
    assert_eq!(f.inspection.inspected().len(), 2);

    // Google's allowance is used up: the run pauses with the quota exit code.
    let out = cli(&home, &f)
        .args([
            "fetch",
            file,
            "--provider",
            "url-inspection",
            "--max",
            "3",
            "--fresh",
            "--json",
        ])
        .output()
        .unwrap();
    assert_eq!(
        out.status.code(),
        Some(10),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let run: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(run["state"], "paused_quota");
    assert!(run["resume_at"].is_string());
    assert!(String::from_utf8_lossy(&out.stderr).contains("resume after"));

    // The same results through the export command (results parity).
    let csv = Command::cargo_bin("spiderlens")
        .unwrap()
        .args([
            "export",
            file,
            "--tab",
            "index-status",
            "--filter",
            "inspected",
            "--columns",
            "address,index_verdict",
        ])
        .output()
        .unwrap();
    assert!(csv.status.success(), "{}", String::from_utf8_lossy(&csv.stderr));
    let csv = String::from_utf8(csv.stdout).unwrap();
    assert_eq!(csv.lines().count(), 3, "{csv}");
    assert!(csv.contains("Pass"), "{csv}");

    // Analytics: the stream for the crawled site is listed and used.
    let streams = ok(
        &mut cli(&home, &f),
        &["properties", "--provider", "analytics", "--crawl", file],
    );
    assert!(
        streams.contains("(suggested)") && streams.contains("--stream 456"),
        "{streams}"
    );
    let json = ok(
        &mut cli(&home, &f),
        &[
            "fetch",
            file,
            "--provider",
            "analytics",
            "--last",
            "28d",
            "--json",
        ],
    );
    let run: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(run["state"], "finished");
    assert_eq!(run["matched"], 2);
    assert_eq!(run["unmatched"], 1);
    let csv = Command::cargo_bin("spiderlens")
        .unwrap()
        .args([
            "export",
            file,
            "--tab",
            "analytics",
            "--filter",
            "with_sessions",
            "--columns",
            "address,ga_sessions",
        ])
        .output()
        .unwrap();
    assert!(csv.status.success(), "{}", String::from_utf8_lossy(&csv.stderr));
    let csv = String::from_utf8(csv.stdout).unwrap();
    assert_eq!(csv.lines().count(), 3, "{csv}");
    assert!(csv.contains(",9") && csv.contains(",4"), "{csv}");

    let runs = ok(&mut cli(&home, &f), &["runs", file]);
    assert!(
        runs.contains("url_inspection") && runs.contains("analytics"),
        "{runs}"
    );
    ok(
        &mut cli(&home, &f),
        &["remove", file, "--provider", "url-inspection"],
    );
    ok(&mut cli(&home, &f), &["remove", file, "--provider", "analytics"]);
    assert!(ok(&mut cli(&home, &f), &["runs", file]).trim().is_empty());
}
