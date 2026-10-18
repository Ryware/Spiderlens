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
fn no_usable_browser_exits_9_before_the_file_exists() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("x.spiderlens");
    let missing = dir.path().join("no-such-browser");
    spiderlens()
        .args(["crawl", "http://127.0.0.1:9/", "--render", "--browser"])
        .arg(&missing)
        .arg("--out")
        .arg(&file)
        .assert()
        .code(9)
        .stderr(contains("cannot be used: No browser at this path."))
        .stderr(contains("crawl without --render"));
    assert!(!file.exists(), "no crawl file without a browser");
}

#[test]
fn browser_status_with_a_missing_path_exits_9() {
    let dir = tempfile::tempdir().unwrap();
    let out = spiderlens()
        .args(["browser", "status", "--json", "--browser"])
        .arg(dir.path().join("no-such-browser"))
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(9));
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["usable"], false);
    assert!(
        v["problem"]
            .as_str()
            .unwrap()
            .contains("No browser at this path.")
    );
}

/// Offline: the download host cannot be reached (a proxy that refuses), so
/// the command says so, names `--browser <path>` and exits 9.
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn browser_install_offline_exits_9_with_a_clear_message() {
    let data = tempfile::tempdir().unwrap();
    spiderlens()
        .args(["browser", "install", "--yes", "--json"])
        .env("XDG_DATA_HOME", data.path())
        .env("HTTPS_PROXY", "http://127.0.0.1:9")
        .env("https_proxy", "http://127.0.0.1:9")
        .env_remove("NO_PROXY")
        .env_remove("no_proxy")
        .assert()
        .code(9)
        .stderr(contains("No internet connection"))
        .stderr(contains("--browser <path>"));
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
    // `url_id` is the row id, given in discovery order, which concurrent
    // fetches can change between two crawls; pages are matched by URL.
    const TIMINGS: [&str; 5] = ["response_ms", "fetched_at", "render_ms", "rendered_at", "url_id"];
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
        // A render that timed out once on a loaded runner is retried; the
        // result is the same, only the attempt count differs.
        for v in [&mut a, &mut b] {
            if let Some(r) = v["render"].as_object_mut() {
                r.remove("attempts");
            }
        }
        assert_eq!(a, b, "{url}");
    }
}

/// `spiderlens page` for a rendered page (T026, contracts/cli.md).
/// T065 and T066: `html --rendered` writes the stored serialised DOM; with
/// no screenshots taken, `screenshot` says so and fails.
#[test]
fn html_rendered_and_screenshot_read_what_the_crawl_stored() {
    let Some(browser) = browser_path() else { return };
    let site = us1_site();
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("stored.spiderlens");
    crawl_with_cli(&site, &browser, &file);
    let url = site.local_url("/title-changed");

    let out = spiderlens()
        .arg("html")
        .arg(&file)
        .args([url.as_str(), "--rendered"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let html = String::from_utf8(out.stdout).unwrap();
    assert!(html.to_ascii_lowercase().starts_with("<!doctype html"), "{html}");
    assert!(html.contains("Red Shoes | Shop"), "{html}");

    spiderlens()
        .arg("screenshot")
        .arg(&file)
        .arg(&url)
        .assert()
        .failure()
        .stderr(contains("has no screenshot"));
}

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

/// Processes (pid and command line) whose command line holds `needle`.
#[cfg(unix)]
fn processes_with(needle: &str) -> Vec<u32> {
    let out = std::process::Command::new("ps")
        .args(["-axww", "-o", "pid=", "-o", "args="])
        .output()
        .expect("ps");
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter(|l| l.contains(needle))
        .filter_map(|l| l.split_whitespace().next()?.parse().ok())
        .collect()
}

/// FR-046, SC-009: killing `spiderlens` (SIGKILL) during a rendered crawl
/// leaves no browser process of it behind after 10 s. The guard process
/// notices and closes the browser. (The Windows leg, `taskkill /F`, runs in
/// CI on Windows through the same guard; `ps` is Unix only.)
#[cfg(unix)]
#[test]
fn killing_spiderlens_during_a_rendered_crawl_leaves_no_browser_behind() {
    let Some(browser) = browser_path() else { return };
    // Pages that take a while, so the crawl is still rendering when killed.
    let page = |links: &str| {
        format!(
            "<!doctype html><html><head><title>Slow</title><script src=\"/slow.js\"></script></head>\
             <body><p>Words.</p>{links}</body></html>"
        )
    };
    let links: String = (1..=8).map(|i| format!("<a href=\"/p{i}\">P{i}</a>")).collect();
    let mut b = JsSiteBuilder::new().page("/", &page(&links));
    for i in 1..=8 {
        b = b.page(&format!("/p{i}"), &page(""));
    }
    let site = b
        .script(
            spiderlens_test_fixtures::js_site::JS_HOST,
            "/slow.js",
            "document.title = 'Done';",
        )
        .delay(
            spiderlens_test_fixtures::js_site::JS_HOST,
            "/slow.js",
            std::time::Duration::from_secs(2),
        )
        .start();
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("killed.spiderlens");
    let mut child = std::process::Command::new(env!("CARGO_BIN_EXE_spiderlens"))
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
        .arg("--quiet")
        .env("SPIDERLENS_ACCEPT_OLD_BROWSER", "1")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    // Each browser of this process has a profile folder named after its pid.
    let marker = format!("spiderlens-render-{}-", child.id());
    let started = std::time::Instant::now();
    while processes_with(&marker).is_empty() {
        assert!(
            started.elapsed().as_secs() < 30,
            "the crawl never started a browser"
        );
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    child.kill().unwrap(); // SIGKILL
    child.wait().unwrap();
    let killed = std::time::Instant::now();
    while !processes_with(&marker).is_empty() {
        assert!(
            killed.elapsed().as_secs() < 10,
            "browser processes remain 10 s after the kill: {:?}",
            processes_with(&marker)
        );
        std::thread::sleep(std::time::Duration::from_millis(200));
    }
}
