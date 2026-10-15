//! Spec 025 CLI: cookie flags and `spiderlens cookies`.

use assert_cmd::Command;
use predicates::prelude::*;
use predicates::str::contains;
use serde_json::Value;
use spiderlens_test_fixtures::SiteBuilder;

const HOST: &str = "127.0.0.1";

fn spiderlens() -> Command {
    Command::cargo_bin("spiderlens").expect("binary built")
}

#[test]
fn cookie_flags_send_cookies_and_the_audit_lists_them() {
    let site = SiteBuilder::new()
        .page(HOST, "/", "", &["/b"])
        .header(HOST, "/", "Set-Cookie", "sid=CLI-SESSION-1; Path=/; HttpOnly")
        .page(HOST, "/b", "", &[])
        .start();
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("c.spiderlens");
    spiderlens()
        .arg("crawl")
        .arg(site.url(HOST, "/"))
        .args([
            "--cookie",
            "token@127.0.0.1",
            "--cookie-plain",
            "consent=yes@127.0.0.1",
        ])
        .args(["--cookie-policy", "keep", "--quiet", "--out"])
        .arg(&file)
        .env("SPIDERLENS_COOKIE_TOKEN", "CLI-SECRET-2")
        .assert()
        .success();
    let b = site.page_hits().into_iter().find(|h| h.path == "/b").unwrap();
    let mut sent: Vec<String> = b.cookie.unwrap().split("; ").map(str::to_string).collect();
    sent.sort();
    assert_eq!(sent, ["consent=yes", "sid=CLI-SESSION-1", "token=CLI-SECRET-2"]);

    let bytes = std::fs::read(&file).unwrap();
    for value in ["CLI-SESSION-1", "CLI-SECRET-2"] {
        assert!(
            !bytes.windows(value.len()).any(|w| w == value.as_bytes()),
            "{value} stored"
        );
    }

    let out = spiderlens()
        .args(["cookies", "--json"])
        .arg(&file)
        .output()
        .unwrap();
    let rows: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(rows[0]["name"], "sid");
    assert_eq!(rows[0]["http_only"], true);
    assert_eq!(rows[0]["urls"], 1);
    spiderlens()
        .args(["cookies", "--filter", "missing-httponly"])
        .arg(&file)
        .assert()
        .success()
        .stdout(contains("Name\tDomain").and(contains("sid").not()));
}

#[test]
fn a_secret_cookie_needs_its_environment_variable() {
    spiderlens()
        .args([
            "crawl",
            "http://127.0.0.1:9/",
            "--cookie",
            "sid@127.0.0.1",
            "--quiet",
        ])
        .env_remove("SPIDERLENS_COOKIE_SID")
        .assert()
        .code(2)
        .stderr(contains("SPIDERLENS_COOKIE_SID"));
}
