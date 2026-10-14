//! CLI contract tests for spec 006 (specs/006-crawl-storage/contracts/cli.md),
//! including the forced-kill recovery test (SC-001).

use std::path::Path;
use std::process::{Command as Process, Stdio};
use std::time::{Duration, Instant};

use assert_cmd::Command;
use assert_cmd::cargo::cargo_bin;
use predicates::str::contains;
use serde_json::Value;
use spiderlens_test_fixtures::{Site, SiteBuilder};

const HOST: &str = "127.0.0.1";

fn spiderlens() -> Command {
    Command::cargo_bin("spiderlens").expect("binary built")
}

fn tree_site(pages: usize) -> Site {
    let mut b = SiteBuilder::new();
    for i in 0..pages {
        let links: Vec<String> = (1..=10)
            .map(|k| i * 10 + k)
            .filter(|&c| c < pages)
            .map(|c| format!("/p/{c}"))
            .collect();
        let refs: Vec<&str> = links.iter().map(String::as_str).collect();
        b = b.page(HOST, &format!("/p/{i}"), "", &refs);
    }
    b.start()
}

fn info(file: &Path) -> Value {
    let out = spiderlens().args(["info", "--json"]).arg(file).output().unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    serde_json::from_slice(&out.stdout).unwrap()
}

/// Starts a crawl (or a resume), waits until `at` URLs are crawled, then
/// kills the process without warning.
fn crawl_and_kill(args: &[&str], file: &Path, at: u64) {
    let mut child = Process::new(cargo_bin("spiderlens"))
        .args(args)
        .arg("--quiet")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        assert!(Instant::now() < deadline, "the crawl never got to {at} URLs");
        std::thread::sleep(Duration::from_millis(100));
        if !file.exists() {
            continue;
        }
        let out = spiderlens().args(["info", "--json"]).arg(file).output().unwrap();
        if let Ok(v) = serde_json::from_slice::<Value>(&out.stdout)
            && v["counts"]["crawled"].as_u64().unwrap_or(0) >= at
        {
            break;
        }
    }
    child.kill().unwrap();
    child.wait().unwrap();
}

#[test]
fn sc_001_killed_crawls_recover_and_finish_like_an_uninterrupted_one() {
    const PAGES: usize = 1500;
    let site = tree_site(PAGES);
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("killed.spiderlens");
    let f = file.to_str().unwrap();
    let start = site.url(HOST, "/p/0");

    crawl_and_kill(&["crawl", &start, "--rate", "300", "--out", f], &file, 200);
    let v = info(&file);
    assert_eq!(v["status"], "running", "a killed crawl was not closed cleanly");
    // Two crashes in a row (edge case), each recovered on resume.
    crawl_and_kill(&["crawl", "--resume", f, "--rate", "300"], &file, 500);
    crawl_and_kill(&["crawl", "--resume", f, "--rate", "300"], &file, 800);

    let out = spiderlens()
        .args(["crawl", "--resume", f, "--speed", "fast", "--json"])
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "{stderr}");
    assert!(stderr.contains("recovered:"), "{stderr}");
    let summary: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(summary["status"], "completed");
    assert_eq!(summary["crawled"], PAGES as u64, "every page once, none lost");
    assert_eq!(summary["discovered"], PAGES as u64, "no URL recorded twice");

    let v = info(&file);
    let kinds: Vec<&str> = v["events"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["kind"].as_str().unwrap())
        .collect();
    assert_eq!(
        kinds.iter().filter(|k| **k == "recovered").count(),
        3,
        "{kinds:?}"
    );
    assert!(!file.with_file_name("killed.spiderlens.lock").exists());
}

#[test]
fn fr_040_disk_reserve_pauses_with_exit_code_6() {
    let site = tree_site(30);
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("low.spiderlens");
    spiderlens()
        .args([
            "crawl",
            &site.url(HOST, "/p/0"),
            "--quiet",
            "--disk-reserve",
            "999999999999",
        ])
        .arg("--out")
        .arg(&file)
        .assert()
        .code(6)
        .stderr(contains("The disk is nearly full"))
        .stderr(contains("resume with: spiderlens crawl --resume"));
    let v = info(&file);
    assert_eq!(v["status"], "paused");
    assert_eq!(v["pause_reason"], "disk_low");
    // Space freed: resume finishes.
    spiderlens()
        .args(["crawl", "--quiet", "--resume"])
        .arg(&file)
        .args(["--disk-reserve", "1"])
        .assert()
        .success();
    assert_eq!(info(&file)["status"], "completed");
}

#[test]
fn fr_011_resume_changes_speed_and_limits_but_not_scope() {
    let site = tree_site(30);
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("r.spiderlens");
    spiderlens()
        .args(["crawl", &site.url(HOST, "/p/0"), "--quiet", "--max-urls", "5"])
        .arg("--out")
        .arg(&file)
        .assert()
        .success();
    spiderlens()
        .args(["crawl", "--resume"])
        .arg(&file)
        .args(["--exclude", "/p/2"])
        .assert()
        .code(2)
        .stderr(contains("only speed and --max-urls can change"));
    spiderlens()
        .args(["crawl", "--quiet", "--resume"])
        .arg(&file)
        .args(["--max-urls", "30", "--speed", "fast"])
        .assert()
        .success();
    let v = info(&file);
    let changes = v["config"]["changes"].as_array().unwrap();
    assert_eq!(changes.len(), 2, "{changes:?}");
}

#[test]
fn info_describes_the_file_and_refuses_other_files() {
    let site = tree_site(12);
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("i.spiderlens");
    spiderlens()
        .args(["crawl", &site.url(HOST, "/p/0"), "--quiet"])
        .arg("--out")
        .arg(&file)
        .assert()
        .success();
    let before = std::fs::read(&file).unwrap();
    spiderlens()
        .arg("info")
        .arg(&file)
        .assert()
        .success()
        .stdout(contains("Status           completed"))
        .stdout(contains("12 discovered, 12 crawled"))
        .stdout(contains("Version (created)"))
        .stdout(contains("can be resumed"));
    assert_eq!(std::fs::read(&file).unwrap(), before, "info never writes");

    let other = dir.path().join("notes.spiderlens");
    std::fs::write(&other, "not a crawl").unwrap();
    spiderlens().arg("info").arg(&other).assert().code(2);
}

#[test]
fn a_crawl_in_use_cannot_be_resumed_twice() {
    let site = tree_site(400);
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("busy.spiderlens");
    let mut child = Process::new(cargo_bin("spiderlens"))
        .args(["crawl", &site.url(HOST, "/p/0"), "--quiet", "--rate", "20"])
        .arg("--out")
        .arg(&file)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(30);
    while !file.with_file_name("busy.spiderlens.lock").exists() {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(50));
    }
    std::thread::sleep(Duration::from_millis(300));
    spiderlens()
        .args(["crawl", "--resume"])
        .arg(&file)
        .assert()
        .code(2)
        .stderr(contains("in use by"));
    child.kill().unwrap();
    child.wait().unwrap();
}
