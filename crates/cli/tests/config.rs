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
fn list_sitemap_file_with_base_url() {
    // Spec 011 FR-005: relative and absolute addresses of a local sitemap.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("sitemap.xml");
    std::fs::write(
        &path,
        "<urlset><url><loc>/one</loc></url><url><loc>https://example.com/two</loc></url>\
         <url><loc>https://example.com/two</loc></url></urlset>",
    )
    .unwrap();
    let arg = format!("{}=https://example.com/", path.display());
    spiderlens()
        .args(["config", "validate", "--list-sitemap", &arg])
        .assert()
        .code(0)
        .stderr(contains("Read 1 sitemap file."))
        .stderr(contains("list: 2 URLs to crawl, 1 duplicates removed, 0 invalid"));
}

#[test]
fn list_sitemap_conflicts_with_list_file_and_missing_file_is_refused() {
    spiderlens()
        .args(["config", "validate", "--list-sitemap", "s.xml", "--list", "urls.txt"])
        .assert()
        .code(2);
    spiderlens()
        .args(["config", "validate", "--list-sitemap", "/no/such/sitemap.xml"])
        .assert()
        .code(2)
        .stderr(contains("--list-sitemap"));
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

#[test]
fn sitemap_flags() {
    // Spec 011 contracts/cli.md.
    let (c, code) = json(&["config", "show", "--json"]);
    assert_eq!(code, 0);
    assert_eq!(c["config"]["scope"]["sitemaps"]["read"], true);
    assert_eq!(c["config"]["scope"]["sitemaps"]["crawl_sitemap_only"], true);

    let (c, _) = json(&["config", "show", "--json", "--no-sitemaps"]);
    assert_eq!(c["config"]["scope"]["sitemaps"]["read"], false);
    assert_eq!(c["config"]["scope"]["sitemaps"]["crawl_sitemap_only"], false);

    let (c, _) = json(&[
        "config",
        "show",
        "--json",
        "--no-crawl-sitemap-only",
        "--sitemap-url",
        "https://a.test/extra.xml",
        "--sitemap-file",
        "dir/a=b.xml=https://a.test/sitemap.xml",
    ]);
    let s = &c["config"]["scope"]["sitemaps"];
    assert_eq!(s["read"], true);
    assert_eq!(s["crawl_sitemap_only"], false);
    assert_eq!(s["urls"][0], "https://a.test/extra.xml");
    assert_eq!(s["files"][0]["path"], "dir/a=b.xml");
    assert_eq!(s["files"][0]["base_url"], "https://a.test/sitemap.xml");

    spiderlens()
        .args(["config", "show", "--sitemap-file", "nobase.xml"])
        .assert()
        .code(2)
        .stderr(contains("PATH=BASE-URL"));
    spiderlens()
        .args(["config", "show", "--use-sitemaps", "--no-sitemaps"])
        .assert()
        .code(2);
}
