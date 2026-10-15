//! CLI contract tests for spec 019 (specs/019-crawl-data-management/
//! contracts/cli.md): `files` and `purge-html`.

use std::path::{Path, PathBuf};

use assert_cmd::Command;
use predicates::str::contains;
use serde_json::Value;
use spiderlens_test_fixtures::{Site, SiteBuilder};

const HOST: &str = "127.0.0.1";
const PAGES: usize = 40;
const TABS: [&str; 10] = [
    "internal",
    "external",
    "response-codes",
    "url",
    "page-titles",
    "meta-description",
    "h1",
    "h2",
    "images",
    "canonicals",
];

fn spiderlens() -> Command {
    Command::cargo_bin("spiderlens").expect("binary built")
}

fn site() -> Site {
    let mut b = SiteBuilder::new();
    for i in 0..PAGES {
        let links: Vec<String> = (1..=5)
            .map(|k| i * 5 + k)
            .filter(|&c| c < PAGES)
            .map(|c| format!("/p/{c}"))
            .collect();
        let refs: Vec<&str> = links.iter().map(String::as_str).collect();
        let text: String = (0..800)
            .map(|w| format!("w{}x{} ", i * 7919 + w * 104_729, w % 13))
            .collect();
        b = b.page(
            HOST,
            &format!("/p/{i}"),
            &format!("<h1>Page {i}</h1><p>{text}</p>"),
            &refs,
        );
    }
    b.start()
}

fn crawl(dir: &Path, site: &Site, name: &str) -> PathBuf {
    let path = dir.join(name);
    spiderlens()
        .args(["crawl", &site.url(HOST, "/p/0"), "--quiet", "--out"])
        .arg(&path)
        .assert()
        .success();
    path
}

fn exports(file: &Path) -> Vec<String> {
    TABS.iter()
        .map(|tab| {
            let out = spiderlens()
                .args(["export", "--tab", tab])
                .arg(file)
                .output()
                .unwrap();
            assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
            String::from_utf8(out.stdout).unwrap()
        })
        .collect()
}

fn files_json(dir: &Path) -> Vec<Value> {
    let out = spiderlens().args(["files", "--json"]).arg(dir).output().unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    serde_json::from_slice::<Value>(&out.stdout)
        .unwrap()
        .as_array()
        .unwrap()
        .clone()
}

#[test]
fn files_lists_size_and_stored_html() {
    let dir = tempfile::tempdir().unwrap();
    let site = site();
    crawl(dir.path(), &site, "a.spiderlens");
    std::fs::write(dir.path().join("notes.txt"), b"mine").unwrap();
    std::fs::write(dir.path().join("a (format 2 backup).spiderlens"), b"old").unwrap();
    let rows = files_json(dir.path());
    assert_eq!(rows.len(), 1, "{rows:?}");
    let r = &rows[0];
    assert_eq!(r["html_pages"], PAGES as u64);
    assert!(r["html_bytes"].as_u64().unwrap() > 0);
    assert!(r["size_bytes"].as_u64().unwrap() > r["html_bytes"].as_u64().unwrap());
    assert_eq!(r["urls"], PAGES as u64);
    assert_eq!(r["backups"].as_array().unwrap().len(), 1);

    spiderlens()
        .arg("files")
        .arg(dir.path())
        .assert()
        .success()
        .stdout(contains("a.spiderlens"))
        .stdout(contains("completed"));
}

#[test]
fn files_refuses_a_missing_folder() {
    spiderlens().args(["files", "/no/such/folder"]).assert().code(2);
}

#[test]
fn sc002_purge_html_keeps_every_export_identical_and_shrinks_the_file() {
    let dir = tempfile::tempdir().unwrap();
    let site = site();
    let path = crawl(dir.path(), &site, "a.spiderlens");
    let before = exports(&path);
    let size_before = std::fs::metadata(&path).unwrap().len();

    let out = spiderlens()
        .args(["purge-html", "--yes", "--json"])
        .arg(&path)
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let report: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(report["html_pages"], PAGES as u64);
    assert_eq!(report["compacted"], true);

    assert_eq!(exports(&path), before, "results unchanged (FR-013)");
    let size_after = std::fs::metadata(&path).unwrap().len();
    assert!(size_after < size_before, "{size_after} < {size_before}");
    assert_eq!(files_json(dir.path())[0]["html_pages"], 0);

    // The page HTML is gone: `html` says so instead of printing it.
    spiderlens()
        .args(["html"])
        .arg(&path)
        .arg(site.url(HOST, "/p/3"))
        .assert()
        .failure();
    // The event is in the crawl's history (FR-016).
    spiderlens()
        .arg("info")
        .arg(&path)
        .assert()
        .success()
        .stdout(contains("html_purged"));
}

#[test]
fn purge_html_asks_first_and_changes_nothing_on_no() {
    let dir = tempfile::tempdir().unwrap();
    let site = site();
    let path = crawl(dir.path(), &site, "a.spiderlens");
    spiderlens()
        .arg("purge-html")
        .arg(&path)
        .write_stdin("n\n")
        .assert()
        .code(2)
        .stderr(contains("nothing changed"));
    assert_eq!(files_json(dir.path())[0]["html_pages"], PAGES as u64);
}

#[test]
fn purge_html_without_compaction() {
    let dir = tempfile::tempdir().unwrap();
    let site = site();
    let path = crawl(dir.path(), &site, "a.spiderlens");
    let before = exports(&path);
    spiderlens()
        .args(["purge-html", "--yes", "--no-compact"])
        .arg(&path)
        .assert()
        .success()
        .stdout(contains("not returned to the disk"));
    assert_eq!(exports(&path), before);
}
