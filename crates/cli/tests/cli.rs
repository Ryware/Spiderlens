//! CLI contract tests (contracts/cli.md). Each test name maps to an acceptance
//! scenario in quickstart.md.

use assert_cmd::Command;
use predicates::prelude::*;
use predicates::str::contains;
use serde_json::Value;
use spiderlens_test_fixtures::{Fixture, UNRESOLVABLE_URL, closed_port_url};

fn spiderlens() -> Command {
    Command::cargo_bin("spiderlens").expect("binary built")
}

fn json_of(args: &[&str]) -> (Value, i32) {
    let out = spiderlens().args(args).arg("--json").output().unwrap();
    let value = serde_json::from_slice(&out.stdout).expect("stdout is JSON");
    (value, out.status.code().unwrap())
}

#[test]
fn check_ok_title() {
    let f = Fixture::start();
    spiderlens()
        .args(["check", &f.url("/home")])
        .assert()
        .code(0)
        .stdout(contains("Status       200"))
        .stdout(contains(f.url("/home")))
        .stdout(contains("Title        Home | Example"));
}

#[test]
fn check_two_redirects() {
    let f = Fixture::start();
    spiderlens()
        .args(["check", &f.url("/old")])
        .assert()
        .code(0)
        .stdout(contains(format!("Redirect     308 → {}", f.url("/mid"))))
        .stdout(contains(format!("Redirect     302 → {}", f.url("/home"))))
        .stdout(contains(format!("Final URL    {}", f.url("/home"))))
        .stdout(contains("Title        Home | Example"));
}

#[test]
fn check_404_is_fetched() {
    let f = Fixture::start();
    spiderlens()
        .args(["check", &f.url("/missing")])
        .assert()
        .code(0)
        .stdout(contains("Status       404"))
        .stdout(contains("Title        Not found"));
}

#[test]
fn check_missing_title() {
    let f = Fixture::start();
    spiderlens()
        .args(["check", &f.url("/no-title")])
        .assert()
        .code(0)
        .stdout(contains("Status       200"))
        .stdout(contains("Title        (no title)"));
}

#[test]
fn check_non_html() {
    let f = Fixture::start();
    spiderlens()
        .args(["check", &f.url("/image.png")])
        .assert()
        .code(0)
        .stdout(contains("Type         image/png"))
        .stdout(contains("Title        (not applicable)"));
}

#[test]
fn check_dns_error() {
    spiderlens()
        .args(["check", UNRESOLVABLE_URL])
        .assert()
        .code(4)
        .stdout(contains("Error        dns: "))
        .stdout(contains("does not exist"))
        .stdout(contains("Attempts     1"));
}

#[test]
fn check_connection_error() {
    spiderlens()
        .args(["check", &closed_port_url()])
        .assert()
        .code(4)
        .stdout(contains("Error        connection: "));
}

#[test]
fn check_malformed_url() {
    spiderlens()
        .args(["check", "http://exa mple.com"])
        .assert()
        .code(2)
        .stdout(predicate::str::is_empty())
        .stderr(contains("not a valid web address"));
}

#[test]
fn check_robots_blocked() {
    let f = Fixture::start();
    spiderlens()
        .args(["check", &f.url("/private/page")])
        .assert()
        .code(3)
        .stdout(contains("State        blocked by robots.txt"))
        .stdout(contains("Error").not());
}

#[test]
fn check_timeout_option() {
    let f = Fixture::start();
    spiderlens()
        .args(["check", &f.url("/slow"), "--read-timeout", "0.5"])
        .assert()
        .code(4)
        .stdout(contains("Error        timeout: "));
}

#[test]
fn usage_error_exit_code() {
    spiderlens().args(["check"]).assert().code(2);
    spiderlens()
        .args(["check", "x.test", "--read-timeout", "0"])
        .assert()
        .code(2);
}

#[test]
fn json_matches_schema_for_every_fixture() {
    let schema: Value = serde_json::from_str(include_str!(
        "../../../specs/001-project-foundation/contracts/page-check.schema.json"
    ))
    .unwrap();
    let validator = jsonschema::validator_for(&schema).unwrap();
    let f = Fixture::start();
    let cases: Vec<(String, i32, &str)> = vec![
        (f.url("/home"), 0, "fetched"),
        (f.url("/old"), 0, "fetched"),
        (f.url("/missing"), 0, "fetched"),
        (f.url("/no-title"), 0, "fetched"),
        (f.url("/doc.pdf"), 0, "fetched"),
        (f.url("/private/page"), 3, "blocked_by_robots"),
        (f.url("/loop-a"), 4, "failed"),
        (UNRESOLVABLE_URL.to_string(), 4, "failed"),
        ("http://exa mple.com".to_string(), 2, "failed"),
    ];
    for (url, code, state) in cases {
        let (v, actual) = json_of(&["check", &url]);
        assert_eq!(actual, code, "{url}");
        assert_eq!(v["state"], state, "{url}");
        let errors: Vec<_> = validator.iter_errors(&v).map(|e| e.to_string()).collect();
        assert!(errors.is_empty(), "{url}: {errors:?}");
    }
}

#[test]
fn json_has_every_human_field() {
    let f = Fixture::start();
    let (v, _) = json_of(&["check", &f.url("/old")]);
    assert_eq!(v["requested_url"], f.url("/old"));
    assert_eq!(v["redirects"].as_array().unwrap().len(), 2);
    assert_eq!(v["final_url"], f.url("/home"));
    assert_eq!(v["status"], 200);
    assert_eq!(v["content_type"], "text/html");
    assert!(v["response_time_ms"].is_u64());
    assert_eq!(v["title"]["value"], "Home | Example");
}

#[test]
fn version_reports_build_identity() {
    let out = spiderlens().args(["version", "--json"]).output().unwrap();
    assert!(out.status.success());
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["product"], "SpiderLens");
    assert_eq!(v["product_version"], env!("CARGO_PKG_VERSION"));
    assert!(!v["build_id"].as_str().unwrap().is_empty());

    spiderlens()
        .arg("--version")
        .assert()
        .success()
        .stdout(contains(format!(
            "spiderlens {} (build ",
            env!("CARGO_PKG_VERSION")
        )));
}

/// The binary is also the browser's guard process (spec 014 research R12):
/// `--render-guard` runs the given program and exits with its status.
#[cfg(unix)]
#[test]
fn render_guard_runs_the_browser_command() {
    let status = std::process::Command::new(env!("CARGO_BIN_EXE_spiderlens"))
        .args([
            "--render-guard",
            &std::process::id().to_string(),
            "sh",
            "-c",
            "exit 7",
        ])
        .status()
        .unwrap();
    assert_eq!(status.code(), Some(7));
}
