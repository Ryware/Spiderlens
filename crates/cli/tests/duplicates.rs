//! `spiderlens duplicates` (spec 020, contracts/cli.md).

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use assert_cmd::Command;
use spiderlens_config::{ContentSettings, CrawlConfig, ModeInput, SpeedSettings};
use spiderlens_engine::crawl::{Control, Crawler};
use spiderlens_results::near_dup::{self, Listing, Schema};
use spiderlens_test_fixtures::near_dup::{HOST, site};

fn crawl(dir: &tempfile::TempDir) -> PathBuf {
    let site = site();
    let path = dir.path().join("d.spiderlens");
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let c = CrawlConfig {
            speed: SpeedSettings::FAST,
            ..CrawlConfig::default()
        };
        let input = ModeInput::Spider {
            start_url: site.url(HOST, "/"),
        };
        let crawler = Crawler::create(&path, c, &input)
            .unwrap()
            .with_resolve(site.resolve());
        let (_tx, rx) = tokio::sync::watch::channel(Control::Run);
        tokio::time::timeout(Duration::from_secs(60), crawler.run(rx, None))
            .await
            .unwrap()
            .unwrap();
    });
    path
}

fn run(args: &[&str]) -> (bool, String, String, Option<i32>) {
    let out = Command::cargo_bin("spiderlens")
        .unwrap()
        .args(args)
        .output()
        .unwrap();
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
        out.status.code(),
    )
}

fn analysis_meta(path: &Path) -> Option<String> {
    let c = rusqlite::Connection::open(path).unwrap();
    c.query_row("SELECT value FROM meta WHERE key = 'analysis'", [], |r| r.get(0))
        .ok()
}

#[test]
fn views_formats_and_parity_with_the_library() {
    let dir = tempfile::tempdir().unwrap();
    let path = crawl(&dir);
    let file = path.to_str().unwrap();

    // First call analyses into the file, then lists clusters.
    let (ok, out, err, _) = run(&["duplicates", file, "--clusters", "--format", "json"]);
    assert!(ok, "{err}");
    assert!(err.contains("analysing near duplicates"));
    assert!(analysis_meta(&path).is_some(), "stored results");
    let clusters: Vec<serde_json::Value> = serde_json::from_str(&out).unwrap();
    assert_eq!(clusters[0]["size"], 12);

    // SC-008: the CLI equals the library on the same file and settings.
    let conn = near_dup::open_read_only(&path).unwrap();
    let mut lib = Vec::new();
    near_dup::export::write_json(&conn, Listing::Clusters, &mut lib).unwrap();
    assert_eq!(out, String::from_utf8(lib).unwrap());

    // CSV pages, with the header from the contract.
    let (ok, out, _, _) = run(&["duplicates", file]);
    assert!(ok);
    assert!(out.trim_start_matches('\u{feff}').starts_with(
        "address,status,main_words,closest_similarity,closest_address,near_duplicates,near_duplicates_is_minimum,cluster,exact_duplicate"
    ));
    let (ok, out, _, _) = run(&["duplicates", file, "--pairs", "--format", "json"]);
    assert!(ok);
    let pairs: Vec<serde_json::Value> = serde_json::from_str(&out).unwrap();
    assert!(pairs.iter().all(|p| p["similarity"].as_u64().unwrap() >= 90));
}

#[test]
fn overrides_never_change_the_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = crawl(&dir);
    let conn = rusqlite::Connection::open(&path).unwrap();
    near_dup::run_pass(
        &conn,
        Schema::Main,
        &ContentSettings::default(),
        &AtomicBool::new(false),
        |_, _, _| {},
    )
    .unwrap();
    drop(conn);
    let before = analysis_meta(&path);
    let (ok, out, err, _) = run(&[
        "duplicates",
        path.to_str().unwrap(),
        "--threshold",
        "80",
        "--pairs",
    ]);
    assert!(ok, "{err}");
    assert!(out.lines().count() > 1);
    assert_eq!(analysis_meta(&path), before);
    let (_, _, _, code) = run(&["duplicates", path.to_str().unwrap(), "--threshold", "40"]);
    assert_eq!(code, Some(2), "out of range is a usage error");
}

#[test]
fn files_without_signatures_are_unavailable() {
    let dir = tempfile::tempdir().unwrap();
    let path = crawl(&dir);
    rusqlite::Connection::open(&path)
        .unwrap()
        .execute_batch("DROP TABLE content_sig; DELETE FROM bodies;")
        .unwrap();
    let (_, _, err, code) = run(&["duplicates", path.to_str().unwrap()]);
    assert_eq!(code, Some(4), "{err}");
    assert!(err.contains("crawl the site again"), "{err}");
    let (_, _, _, code) = run(&["duplicates", dir.path().join("nope").to_str().unwrap()]);
    assert_eq!(code, Some(3));
}
