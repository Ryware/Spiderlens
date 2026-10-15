//! CLI contract tests for spec 004 (specs/004-fetch-errors-retries/contracts/cli.md):
//! `spiderlens retry`, the extended summary and `report --log`.

use assert_cmd::Command;
use predicates::str::contains;
use serde_json::Value;
use spiderlens_test_fixtures::SiteBuilder;

const HOST: &str = "127.0.0.1";

fn spiderlens() -> Command {
    Command::cargo_bin("spiderlens").expect("binary built")
}

fn json_lines(stdout: &[u8]) -> Vec<Value> {
    let text = String::from_utf8_lossy(stdout);
    serde_json::Deserializer::from_str(&text)
        .into_iter::<Value>()
        .map(|v| v.expect("stdout is JSON"))
        .collect()
}

#[test]
fn retry_runs_only_the_temporary_failures() {
    let site = SiteBuilder::new()
        .page(HOST, "/", "", &["/flaky", "/missing"])
        .flaky(HOST, "/flaky", 503, 1, &[])
        .start();
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("site.spiderlens");
    let out = spiderlens()
        .args([
            "crawl",
            &site.url(HOST, "/"),
            "--attempts",
            "1",
            "--json",
            "--quiet",
            "--out",
        ])
        .arg(&file)
        .output()
        .unwrap();
    let summary = &json_lines(&out.stdout)[0];
    assert_eq!(summary["by_class"]["http_5xx"], 1, "{summary:#}");
    // Links on the 503 page are not followed: /missing is the only 404.
    assert_eq!(summary["by_class"]["http_4xx"], 1);
    assert_eq!(summary["transient_errors"], 1);
    assert_eq!(summary["permanent_errors"], 1);

    // Without --yes it asks, and "n" changes nothing.
    spiderlens()
        .arg("retry")
        .arg(&file)
        .write_stdin("n\n")
        .assert()
        .success()
        .stdout(contains("1 URLs failed for temporary reasons"))
        .stdout(contains("Server error (5xx)"))
        .stderr(contains("Nothing was changed."));
    assert_eq!(site.count(HOST, "/flaky"), 1);

    let out = spiderlens()
        .arg("retry")
        .arg(&file)
        .args(["--yes", "--json", "--quiet"])
        .output()
        .unwrap();
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let values = json_lines(&out.stdout);
    assert_eq!(values[0]["total"], 1);
    assert_eq!(values[0]["by_class"]["http_5xx"], 1);
    let after = &values[1];
    assert_eq!(after["manual_retries"], 1);
    assert_eq!(after["recovered"], 1);
    assert!(after["by_class"].get("http_5xx").is_none(), "{after:#}");
    assert_eq!(site.count(HOST, "/flaky"), 2);
    assert_eq!(site.count(HOST, "/missing"), 1, "a 404 is never retried");

    spiderlens()
        .arg("retry")
        .arg(&file)
        .assert()
        .success()
        .stdout(contains(
            "Nothing to retry: no URL failed for a temporary reason.",
        ));

    spiderlens()
        .args(["report"])
        .arg(&file)
        .arg("--log")
        .assert()
        .success()
        .stdout(contains("manual_retry"));
}
