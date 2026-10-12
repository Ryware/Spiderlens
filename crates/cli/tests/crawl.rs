//! CLI contract tests for spec 003 (specs/003-frontier-scope/contracts/cli.md).
//! The fixture site answers on 127.0.0.1, so no name overrides are needed.

use assert_cmd::Command;
use predicates::prelude::*;
use predicates::str::contains;
use serde_json::Value;
use spiderlens_test_fixtures::{Site, SiteBuilder};

const HOST: &str = "127.0.0.1";

fn spiderlens() -> Command {
    Command::cargo_bin("spiderlens").expect("binary built")
}

/// Home links to two pages, one of them blocked by robots.txt, and an
/// excluded page.
fn small_site() -> Site {
    SiteBuilder::new()
        .robots(HOST, "User-agent: *\nDisallow: /private\n")
        .page(HOST, "/", "", &["/a", "/private/x", "/skip/me"])
        .page(HOST, "/a", "", &["/"])
        .start()
}

fn crawl_json(args: &[&str]) -> (Value, i32) {
    let out = spiderlens()
        .arg("crawl")
        .args(args)
        .args(["--json", "--quiet"])
        .output()
        .unwrap();
    let value = serde_json::from_slice(&out.stdout).unwrap_or_else(|e| {
        panic!(
            "stdout is JSON ({e}); stderr: {}",
            String::from_utf8_lossy(&out.stderr)
        )
    });
    (value, out.status.code().unwrap())
}

#[test]
fn crawl_summary_matches_schema_and_counts_reasons() {
    let site = small_site();
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("site.spiderlens");
    let (summary, code) = crawl_json(&[
        &site.url(HOST, "/"),
        "--exclude",
        "/skip/",
        "--out",
        file.to_str().unwrap(),
    ]);
    assert_eq!(code, 0);

    let schema: Value = serde_json::from_str(include_str!(
        "../../../specs/003-frontier-scope/contracts/crawl-summary.schema.json"
    ))
    .unwrap();
    let validator = jsonschema::validator_for(&schema).unwrap();
    let errors: Vec<_> = validator.iter_errors(&summary).map(|e| e.to_string()).collect();
    assert!(errors.is_empty(), "schema errors: {errors:?}\n{summary:#}");

    assert_eq!(summary["status"], "completed");
    assert_eq!(summary["crawled"], 2);
    assert_eq!(summary["by_reason"]["blocked_by_robots"], 1);
    assert_eq!(summary["by_reason"]["excluded"], 1);
    assert!(file.exists());
}

#[test]
fn existing_out_file_is_never_overwritten() {
    let site = small_site();
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("taken.spiderlens");
    std::fs::write(&file, b"keep me").unwrap();
    spiderlens()
        .args([
            "crawl",
            &site.url(HOST, "/"),
            "--quiet",
            "--out",
            file.to_str().unwrap(),
        ])
        .assert()
        .code(2);
    assert_eq!(std::fs::read(&file).unwrap(), b"keep me");
    assert!(
        site.page_hits().is_empty(),
        "no request before the file is created"
    );
}

#[test]
fn excluded_start_url_is_a_usage_error_before_any_request() {
    let site = small_site();
    let dir = tempfile::tempdir().unwrap();
    spiderlens()
        .current_dir(dir.path())
        .args(["crawl", &site.url(HOST, "/a"), "--exclude", "/a$", "--quiet"])
        .assert()
        .code(2)
        .stderr(contains("start_url"));
    assert!(site.hits().is_empty());
}

#[test]
fn default_out_file_is_named_after_the_host() {
    let site = small_site();
    let dir = tempfile::tempdir().unwrap();
    spiderlens()
        .current_dir(dir.path())
        .args(["crawl", &site.url(HOST, "/"), "--quiet"])
        .assert()
        .code(0)
        .stdout(contains("Crawled          3"));
    let names: Vec<String> = std::fs::read_dir(dir.path())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".spiderlens"))
        .collect();
    assert_eq!(names.len(), 1, "{names:?}");
    assert!(names[0].starts_with("127.0.0.1"), "{names:?}");
}

#[test]
fn report_lists_not_crawled_urls_with_reasons() {
    let site = small_site();
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("site.spiderlens");
    let file = file.to_str().unwrap();
    spiderlens()
        .args([
            "crawl",
            &site.url(HOST, "/"),
            "--exclude",
            "/skip/",
            "--quiet",
            "--out",
            file,
        ])
        .assert()
        .code(0);

    // Tab-separated: state, reason, detail, depth, source, found on, URL.
    let out = spiderlens()
        .args(["report", file, "--not-crawled"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0));
    let text = String::from_utf8(out.stdout).unwrap();
    let lines: Vec<Vec<&str>> = text.lines().map(|l| l.split('\t').collect()).collect();
    assert_eq!(lines.len(), 2, "{text}");
    let blocked = lines
        .iter()
        .find(|l| l[1] == "blocked_by_robots")
        .expect("robots row");
    assert_eq!(blocked[2], "Disallow: /private");
    assert_eq!(blocked[6], site.url(HOST, "/private/x"));

    // JSON Lines, filtered by reason.
    let out = spiderlens()
        .args(["report", file, "--reason", "excluded", "--json"])
        .output()
        .unwrap();
    let rows: Vec<Value> = String::from_utf8(out.stdout)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["url"], site.url(HOST, "/skip/me"));

    // No filter: the summary.
    spiderlens()
        .args(["report", file])
        .assert()
        .code(0)
        .stdout(contains("Status           completed"));

    spiderlens()
        .args(["report", file, "--reason", "no_such_reason"])
        .assert()
        .code(2)
        .stderr(contains("blocked_by_robots"));
}

#[test]
fn resume_refuses_other_options() {
    let site = small_site();
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("site.spiderlens");
    let file = file.to_str().unwrap();
    spiderlens()
        .args(["crawl", &site.url(HOST, "/"), "--quiet", "--out", file])
        .assert()
        .code(0);
    spiderlens()
        .args(["crawl", "--resume", file, "--max-depth", "3"])
        .assert()
        .code(2)
        .stderr(contains("--max-depth"));
    // A finished crawl resumes to the same result without new page requests.
    let before = site.page_hits().len();
    let (summary, code) = crawl_json(&["--resume", file]);
    assert_eq!(code, 0);
    assert_eq!(summary["crawled"], 3);
    assert_eq!(site.page_hits().len(), before);
}

#[test]
fn scope_test_explains_each_url() {
    let site = small_site();
    let start = site.url(HOST, "/");
    spiderlens()
        .args([
            "scope",
            "test",
            "--start",
            &start,
            "--exclude",
            "/skip/",
            &site.url(HOST, "/a"),
            &site.url(HOST, "/skip/me"),
            "mailto:someone@example.com",
        ])
        .assert()
        .code(0)
        .stdout(contains("/a  would crawl (robots.txt not checked)"))
        .stdout(contains(
            "/skip/me  would not be crawled: Excluded by rule (/skip/)",
        ))
        .stdout(contains("would not be crawled: Unsupported scheme"));
    assert!(site.hits().is_empty(), "no requests without --fetch-robots");

    let out = spiderlens()
        .args([
            "scope",
            "test",
            "--start",
            &start,
            "--fetch-robots",
            "--json",
            &site.url(HOST, "/private/x"),
        ])
        .output()
        .unwrap();
    let rows: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(rows[0]["reason"], "blocked_by_robots");
    assert_eq!(rows[0]["detail"], "Disallow: /private");
    assert_eq!(site.count(HOST, "/robots.txt"), 1);
}

#[test]
fn help_lists_the_new_commands() {
    spiderlens()
        .arg("--help")
        .assert()
        .success()
        .stdout(contains("crawl").and(contains("report")).and(contains("scope")));
}
