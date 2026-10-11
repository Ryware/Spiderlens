//! CLI contract tests for spec 002 (contracts/cli.md in that spec).

use assert_cmd::Command;
use predicates::prelude::*;
use predicates::str::contains;
use serde_json::Value;
use spiderlens_test_fixtures::Fixture;

fn spiderlens() -> Command {
    Command::cargo_bin("spiderlens").expect("binary built")
}

fn json(args: &[&str]) -> (Value, i32) {
    let out = spiderlens().args(args).output().unwrap();
    (
        serde_json::from_slice(&out.stdout).expect("JSON on stdout"),
        out.status.code().unwrap(),
    )
}

#[test]
fn defaults_round_trip_through_a_profile_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("defaults.json");
    let out = spiderlens().args(["config", "defaults"]).output().unwrap();
    std::fs::write(&path, &out.stdout).unwrap();
    let (from_profile, code) = json(&["config", "show", "--json", "--profile", path.to_str().unwrap()]);
    let (from_defaults, _) = json(&["config", "show", "--json"]);
    assert_eq!(code, 0);
    assert_eq!(from_profile["config"], from_defaults["config"]);
}

#[test]
fn profile_matches_flags() {
    // SC-007: a profile file and the same settings as flags give identical configs.
    let flags = [
        "--max-urls",
        "100",
        "--include",
        "/blog/",
        "--exclude",
        "/tag/",
        "--speed",
        "fast",
        "--per-host",
        "8",
        "--user-agent",
        "googlebot-smartphone",
        "--attempts",
        "1",
    ];
    let mut args = vec!["config", "show", "--json"];
    args.extend(flags);
    let (by_flags, _) = json(&args);

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("p.json");
    let profile = serde_json::json!({
        "format": "spiderlens-profile", "format_version": 1, "app_version": "0.1.0", "name": "p",
        "config": by_flags["config"].clone()
    });
    std::fs::write(&path, profile.to_string()).unwrap();
    let (by_profile, _) = json(&["config", "show", "--json", "--profile", path.to_str().unwrap()]);
    assert_eq!(by_profile["config"], by_flags["config"]);
    assert_eq!(by_flags["config"]["speed"]["preset"], "custom");
}

#[test]
fn invalid_settings_exit_2_and_name_the_setting() {
    spiderlens()
        .args([
            "config",
            "validate",
            "example.com",
            "--include",
            "(bad",
            "--header",
            "Host: x",
        ])
        .assert()
        .code(2)
        .stderr(contains("error: scope.include[0]:"))
        .stderr(contains(
            "error: identity.headers[0].name: Host cannot be set here",
        ));
}

#[test]
fn warnings_do_not_block() {
    spiderlens()
        .args([
            "config",
            "validate",
            "example.com",
            "--speed",
            "fast",
            "--robots",
            "ignore",
        ])
        .assert()
        .code(0)
        .stderr(contains(
            "warning: speed: Faster settings are intended for sites you control.",
        ))
        .stderr(contains("warning: robots.mode:"));
}

#[test]
fn list_file_counts() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("urls.txt");
    std::fs::write(
        &path,
        "https://a.test/1\na.test/1\nhttps://a.test/2\nht!tp//bad\n",
    )
    .unwrap();
    spiderlens()
        .args(["config", "validate", "--list", path.to_str().unwrap()])
        .assert()
        .code(0)
        .stderr(contains("list: 2 URLs to crawl, 1 duplicates removed, 1 invalid"));
}

#[test]
fn empty_list_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("urls.csv");
    std::fs::write(&path, "title,notes\nHome,Yes\n").unwrap();
    spiderlens()
        .args(["config", "validate", "--list", path.to_str().unwrap()])
        .assert()
        .code(2)
        .stderr(contains("input.urls"));
}

#[test]
fn test_patterns() {
    spiderlens()
        .args([
            "config",
            "test-patterns",
            "--include",
            "/blog/",
            "--exclude",
            "/blog/tag/",
            "a.test/blog/post-1",
            "https://a.test/blog/tag/news",
            "https://a.test/about",
        ])
        .assert()
        .success()
        .stdout(contains("https://a.test/blog/post-1  crawled (include /blog/)"))
        .stdout(contains(
            "https://a.test/blog/tag/news  excluded (exclude /blog/tag/)",
        ))
        .stdout(contains("https://a.test/about  not included"));
}

#[test]
fn newer_profile_settings_are_reported() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("new.json");
    std::fs::write(
        &path,
        r#"{"format":"spiderlens-profile","format_version":2,"app_version":"9.0.0","name":"n","config":{"ai":{}}}"#,
    )
    .unwrap();
    spiderlens()
        .args(["config", "validate", "--profile", path.to_str().unwrap()])
        .assert()
        .code(0)
        .stderr(contains("ignored: config.ai"));
}

#[test]
fn check_sends_configured_user_agent_and_headers() {
    let f = Fixture::start();
    spiderlens()
        .args([
            "check",
            &f.url("/echo"),
            "--user-agent",
            "bingbot",
            "--secret-header",
            "X-Key: s3cret",
        ])
        .assert()
        .code(0)
        .stdout(contains(
            "UA=Mozilla/5.0 AppleWebKit/537.36 (KHTML, like Gecko; compatible; bingbot/2.0",
        ))
        .stdout(contains("X-Key=s3cret"))
        .stderr(contains("warning: identity.user_agent.preset"));
}

#[test]
fn check_uses_robots_token_of_the_preset() {
    // The fixture blocks /private/ for SpiderLens only; Googlebot falls under `*`.
    let f = Fixture::start();
    spiderlens()
        .args(["check", &f.url("/private/page")])
        .assert()
        .code(3);
    spiderlens()
        .args([
            "check",
            &f.url("/private/page"),
            "--user-agent",
            "googlebot-desktop",
        ])
        .assert()
        .code(0);
}

#[test]
fn check_rejects_bad_network_values() {
    spiderlens()
        .args(["check", "x.test", "--read-timeout", "0"])
        .assert()
        .code(2)
        .stderr(contains("network.read_timeout_secs"))
        .stdout(predicate::str::is_empty());
}
