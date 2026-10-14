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

/// Crawled URLs of a file, from `spiderlens report --crawled --json`.
fn crawled_urls(file: &std::path::Path) -> Vec<String> {
    let out = spiderlens()
        .arg("report")
        .arg(file)
        .args(["--crawled", "--json"])
        .output()
        .unwrap();
    let mut urls: Vec<String> = String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(|l| {
            serde_json::from_str::<Value>(l).unwrap()["url"]
                .as_str()
                .unwrap()
                .to_string()
        })
        .collect();
    urls.sort();
    urls
}

/// Removes what differs between two runs: timings and the `Date` header.
fn without_timings(v: &mut Value) {
    const TIMINGS: [&str; 4] = ["response_ms", "fetched_at", "render_ms", "rendered_at"];
    match v {
        Value::Object(m) => {
            for k in TIMINGS {
                m.remove(k);
            }
            if let Some(Value::Array(h)) = m.get_mut("headers") {
                h.retain(|pair| !pair[0].as_str().is_some_and(|n| n.eq_ignore_ascii_case("date")));
            }
            m.values_mut().for_each(without_timings);
        }
        Value::Array(a) => a.iter_mut().for_each(without_timings),
        _ => {}
    }
}

/// US1 scenario 7 (T025): the CLI and the engine API with the same
/// settings store the same data for every page, and the CLI needs no
/// window.
#[test]
fn cli_and_engine_render_the_same() {
    use spiderlens_config::{CrawlConfig, ModeInput, RenderMode, SpeedSettings};
    use spiderlens_engine::crawl::{Control, Crawler};
    let Some(info) = spiderlens_render::browser_for_tests().unwrap() else {
        return;
    };
    let site = us1_site();
    let dir = tempfile::tempdir().unwrap();
    let by_cli = dir.path().join("cli.spiderlens");
    crawl_with_cli(&site, &info.path.display().to_string(), &by_cli);

    let by_api = dir.path().join("api.spiderlens");
    let mut config = CrawlConfig {
        speed: SpeedSettings::FAST,
        ..CrawlConfig::default()
    };
    config.rendering.mode = RenderMode::Javascript;
    let crawler = Crawler::create(
        &by_api,
        config,
        &ModeInput::Spider {
            start_url: site.local_url("/"),
        },
    )
    .unwrap()
    .with_browser(info)
    .with_render_launch(spiderlens_render::LaunchOptions::default());
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    let (_tx, rx) = tokio::sync::watch::channel(Control::Run);
    let summary = rt.block_on(crawler.run(rx, None)).unwrap();
    assert_eq!(summary.status, "completed");

    let urls = crawled_urls(&by_cli);
    assert_eq!(urls.len(), 10, "{urls:?}");
    assert_eq!(urls, crawled_urls(&by_api));
    for url in &urls {
        let mut a = page_json(&by_cli, url);
        let mut b = page_json(&by_api, url);
        assert!(a["render"].is_object(), "{url} was rendered");
        without_timings(&mut a);
        without_timings(&mut b);
        assert_eq!(a, b, "{url}");
    }
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
