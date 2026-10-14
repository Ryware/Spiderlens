//! Spec 014 command line: the browser check, `--render` crawls and their
//! summary (T023). Browser tests are skipped when no browser is found,
//! unless `SPIDERLENS_REQUIRE_BROWSER=1`.

use assert_cmd::Command;
use predicates::str::contains;
use serde_json::Value;
use spiderlens_test_fixtures::js_site::{JsSite, JsSiteBuilder};

fn spiderlens() -> Command {
    let mut c = Command::cargo_bin("spiderlens").expect("binary built");
    // The test browser may be older than the pinned version.
    c.env("SPIDERLENS_ACCEPT_OLD_BROWSER", "1");
    c
}

/// The browser the tests render with, as a path for `--browser`.
fn browser_path() -> Option<String> {
    spiderlens_render::browser_for_tests()
        .expect("browser for tests")
        .map(|b| b.path.display().to_string())
}

fn us1_site() -> JsSite {
    JsSiteBuilder::new().us1_pages().start()
}

#[test]
fn browser_options_need_render() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("x.spiderlens");
    spiderlens()
        .args(["crawl", "http://127.0.0.1:9/", "--browser", "/nope", "--out"])
        .arg(&file)
        .assert()
        .code(2)
        .stderr(contains("need --render"));
    assert!(!file.exists());
}

#[test]
fn no_usable_browser_exits_7_before_the_file_exists() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("x.spiderlens");
    let missing = dir.path().join("no-such-browser");
    spiderlens()
        .args(["crawl", "http://127.0.0.1:9/", "--render", "--browser"])
        .arg(&missing)
        .arg("--out")
        .arg(&file)
        .assert()
        .code(7)
        .stderr(contains("cannot be used: No browser at this path."))
        .stderr(contains("crawl without --render"));
    assert!(!file.exists(), "no crawl file without a browser");
}

#[test]
fn render_crawl_prints_the_renderer_and_the_rendering_block() {
    let Some(browser) = browser_path() else { return };
    let site = us1_site();
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("us1.spiderlens");
    let out = spiderlens()
        .args([
            "crawl",
            &site.local_url("/"),
            "--render",
            "--speed",
            "fast",
            "--browser",
            &browser,
        ])
        .arg("--out")
        .arg(&file)
        .args(["--json", "--quiet"])
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(0), "{stderr}");
    assert!(stderr.contains("notice: rendering with "), "{stderr}");
    let summary: Value = serde_json::from_slice(&out.stdout).unwrap();
    let render = &summary["render"];
    // Home, the six US1 pages, the two script-only pages and the script
    // redirect target: ten eligible pages, all rendered.
    assert_eq!(render["by_state"]["rendered"], 10, "{render}");
    assert_eq!(summary["crawled"], 10);
    assert!(render["renderer"]["name"].is_string(), "{render}");
    assert!(render["avg_render_ms"].is_u64());

    // The text summary has the Rendering block; a raw crawl has none.
    spiderlens()
        .arg("report")
        .arg(&file)
        .assert()
        .success()
        .stdout(contains("Rendering\n  Rendered"))
        .stdout(contains("Renderer"));
    let raw = dir.path().join("raw.spiderlens");
    let out = spiderlens()
        .args(["crawl", &site.local_url("/"), "--speed", "fast", "--out"])
        .arg(&raw)
        .args(["--json", "--quiet"])
        .output()
        .unwrap();
    let summary: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(summary.get("render").is_none(), "{summary}");
    assert_eq!(summary["crawled"], 7, "script-only pages are not found raw");
}

fn crawl_with_cli(site: &JsSite, browser: &str, file: &std::path::Path) {
    let out = spiderlens()
        .args([
            "crawl",
            &site.local_url("/"),
            "--render",
            "--speed",
            "fast",
            "--browser",
            browser,
        ])
        .arg("--out")
        .arg(file)
        .arg("--quiet")
        .output()
        .unwrap();
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

fn page_json(file: &std::path::Path, url: &str) -> Value {
    let out = spiderlens()
        .arg("page")
        .arg(file)
        .args([url, "--json"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    serde_json::from_slice(&out.stdout).unwrap()
}

/// `spiderlens page` for a rendered page (T026, contracts/cli.md).
#[test]
fn page_shows_the_rendered_version_with_origins() {
    let Some(browser) = browser_path() else { return };
    let site = us1_site();
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("page.spiderlens");
    crawl_with_cli(&site, &browser, &file);

    let p = page_json(&file, &site.local_url("/title-changed"));
    assert_eq!(p["title"], "Loading", "the raw values stay");
    assert_eq!(p["render"]["state"], "rendered");
    assert_eq!(p["render"]["attempts"], 1);
    assert_eq!(p["rendered"]["page"]["title"], "Red Shoes | Shop");
    let titles = p["rendered"]["titles"].as_array().unwrap();
    assert_eq!(titles[0]["text"], "Red Shoes | Shop");
    assert_eq!(titles[0]["origin"], "rendered");
    assert_eq!(titles[1]["text"], "Loading");
    assert_eq!(titles[1]["origin"], "raw", "what JavaScript removed");
    assert_eq!(p["comparison"]["title_diff"], "changed");
    assert!(p["dependence"].is_object());
    assert!(p["resources"].is_array() && p["console"].is_array());

    let nav = page_json(&file, &site.local_url("/js-nav"));
    let links = nav["rendered"]["links"].as_array().unwrap();
    let only: Vec<&str> = links
        .iter()
        .filter(|l| l["origin"] == "rendered")
        .map(|l| l["url"].as_str().unwrap())
        .collect();
    assert_eq!(only, [site.local_url("/only-js-1"), site.local_url("/only-js-2")]);
    assert_eq!(nav["dependence"]["internal_links"], 1.0);

    let jr = page_json(&file, &site.local_url("/js-redirect"));
    assert_eq!(
        jr["render"]["js_redirect_url"],
        site.local_url("/redirect-target")
    );
    let ld = page_json(&file, &site.local_url("/jsonld-added"));
    assert_eq!(ld["rendered"]["structured_data"][0]["origin"], "rendered");

    // The text output has a short Rendering section.
    spiderlens()
        .arg("page")
        .arg(&file)
        .arg(site.local_url("/title-changed"))
        .assert()
        .success()
        .stdout(contains("  Rendering: rendered"))
        .stdout(contains("Rendered title: Red Shoes | Shop"));

    // A raw crawl's page report has no rendering keys.
    let raw = dir.path().join("raw.spiderlens");
    spiderlens()
        .args([
            "crawl",
            &site.local_url("/"),
            "--speed",
            "fast",
            "--quiet",
            "--out",
        ])
        .arg(&raw)
        .assert()
        .success();
    let p = page_json(&raw, &site.local_url("/title-changed"));
    for k in [
        "render",
        "rendered",
        "comparison",
        "dependence",
        "resources",
        "console",
    ] {
        assert!(p.get(k).is_none(), "{k}");
    }
}
