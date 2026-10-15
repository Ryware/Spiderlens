//! CLI contract tests for `spiderlens aeo` (specs/015-aeo-geo-checks/contracts/cli.md).

use assert_cmd::Command;
use predicates::str::contains;
use serde_json::Value;
use spiderlens_test_fixtures::SiteBuilder;

const HOST: &str = "127.0.0.1";

fn spiderlens() -> Command {
    Command::cargo_bin("spiderlens").expect("binary built")
}

#[test]
fn crawl_runs_the_checks_and_aeo_commands_read_them() {
    let site = SiteBuilder::new()
        .robots(HOST, "User-agent: GPTBot\nDisallow: /\n")
        .page(HOST, "/", "<h1>Home</h1>", &["/a", "/about"])
        .page(HOST, "/a", "<h1>A page</h1>", &["/"])
        .page(HOST, "/about", "<h1>About</h1>", &["/"])
        .start();
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("site.spiderlens");
    let f = file.to_str().unwrap();

    // Before any crawl: summary refuses with the usage code.
    spiderlens().args(["aeo", "summary", f]).assert().code(2);

    spiderlens()
        .args(["crawl", &site.url(HOST, "/"), "--out", f, "--speed", "fast"])
        .assert()
        .success()
        .stderr(contains("AI search score"));

    let out = spiderlens()
        .args(["aeo", "summary", f, "--json"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0));
    let s: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(s["pages_analyzed"], 3);
    let gpt = s["hosts"][0]["access"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["bot"] == "gptbot")
        .unwrap()
        .clone();
    assert_eq!(gpt["home_verdict"], "blocked");

    // The quality gate.
    spiderlens()
        .args(["aeo", "run", f, "--quiet", "--fail-under", "101"])
        .assert()
        .code(7);
    spiderlens()
        .args(["aeo", "run", f, "--quiet", "--fail-under", "0"])
        .assert()
        .success()
        .stdout(contains("top fixes"));

    let out = spiderlens()
        .args(["aeo", "page", f, &site.url(HOST, "/a"), "--json"])
        .output()
        .unwrap();
    let p: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(p["blocked_crawlers"], serde_json::json!(["GPTBot"]));

    spiderlens()
        .args(["aeo", "llms-txt", f])
        .assert()
        .success()
        .stdout(contains("# 127.0.0.1"));
}

#[test]
fn no_ai_checks_skips_the_run() {
    let site = SiteBuilder::new().page(HOST, "/", "", &[]).start();
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("site.spiderlens");
    let f = file.to_str().unwrap();
    spiderlens()
        .args([
            "crawl",
            &site.url(HOST, "/"),
            "--out",
            f,
            "--no-ai-checks",
            "--no-site-files",
        ])
        .assert()
        .success();
    spiderlens().args(["aeo", "summary", f]).assert().code(2);
    assert_eq!(site.count(HOST, "/llms.txt"), 0);
}

#[test]
fn bots_lists_the_crawler_list() {
    let out = spiderlens().args(["aeo", "bots", "--json"]).output().unwrap();
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(v["version"].as_u64().unwrap() >= 1);
    assert!(
        v["bots"]
            .as_array()
            .unwrap()
            .iter()
            .any(|b| b["token"] == "claudebot")
    );
}
