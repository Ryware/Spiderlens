//! `spiderlens rules` and `spiderlens issues` (spec 009,
//! specs/009-issue-rules-engine/contracts/cli.md).

use assert_cmd::Command;
use spiderlens_results::synth::Synth;

fn crawl_file(dir: &tempfile::TempDir) -> std::path::PathBuf {
    let path = dir.path().join("c.spiderlens");
    Synth {
        urls: 600,
        links_per_page: 2,
        seed: 9,
        ..Synth::default()
    }
    .write(&path)
    .unwrap();
    path
}

fn run(args: &[&str]) -> std::process::Output {
    Command::cargo_bin("spiderlens")
        .unwrap()
        .args(args)
        .output()
        .unwrap()
}

fn json(out: &std::process::Output) -> serde_json::Value {
    serde_json::from_slice(&out.stdout)
        .unwrap_or_else(|e| panic!("{e}: {}", String::from_utf8_lossy(&out.stderr)))
}

#[test]
fn rules_lists_the_catalogue_and_filters_by_category() {
    let all = json(&run(&["rules", "--json"]));
    assert_eq!(
        all.as_array().unwrap().len(),
        spiderlens_results::issues::catalogue().len()
    );
    let titles = json(&run(&["rules", "--json", "--category", "Page titles"]));
    let titles = titles.as_array().unwrap();
    assert!(!titles.is_empty());
    assert!(titles.iter().all(|r| r["category"] == "page_titles"));
    assert_eq!(run(&["rules", "--category", "nope"]).status.code(), Some(2));
}

#[test]
fn issues_match_the_reader_and_fail_on_sets_the_exit_code() {
    let dir = tempfile::tempdir().unwrap();
    let path = crawl_file(&dir);
    let file = path.to_str().unwrap();

    let out = run(&["issues", file, "--json"]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let shown = json(&out);
    let mut reader = spiderlens_results::Reader::open(&path).unwrap();
    let summary = reader.issues_summary(false).unwrap();
    let expected: Vec<_> = summary
        .issues
        .iter()
        .filter(|i| i.enabled && i.count.unwrap_or(0) > 0)
        .collect();
    assert!(!expected.is_empty(), "the synthetic crawl has issues");
    let shown = shown["issues"].as_array().unwrap();
    assert_eq!(shown.len(), expected.len());
    for i in &expected {
        let s = shown.iter().find(|s| s["id"] == i.id.as_str()).unwrap();
        assert_eq!(s["count"].as_u64(), i.count);
    }

    let lowest = expected.iter().map(|i| i.priority).max().unwrap();
    let flag = lowest.label().to_lowercase();
    assert_eq!(run(&["issues", file, "--fail-on", &flag]).status.code(), Some(7));
}

#[test]
fn settings_file_turns_rules_off_and_bad_values_are_usage_errors() {
    let dir = tempfile::tempdir().unwrap();
    let path = crawl_file(&dir);
    let file = path.to_str().unwrap();
    let before = json(&run(&["issues", file, "--json"]));
    let first = before["issues"][0]["id"].as_str().unwrap().to_string();

    let off = dir.path().join("off.json");
    std::fs::write(&off, format!(r#"{{"rules":{{"{first}":{{"enabled":false}}}}}}"#)).unwrap();
    let after = json(&run(&[
        "issues",
        file,
        "--json",
        "--issues",
        off.to_str().unwrap(),
    ]));
    assert!(
        after["issues"]
            .as_array()
            .unwrap()
            .iter()
            .all(|i| i["id"] != first.as_str())
    );

    let bad = dir.path().join("bad.json");
    std::fs::write(&bad, r#"{"rules":{"title.over":{"threshold":-5}}}"#).unwrap();
    assert_eq!(
        run(&["issues", file, "--issues", bad.to_str().unwrap()])
            .status
            .code(),
        Some(2)
    );
}

#[test]
fn export_writes_one_csv_per_issue_with_the_desktop_rows() {
    let dir = tempfile::tempdir().unwrap();
    let path = crawl_file(&dir);
    let out_dir = dir.path().join("issues");
    let out = run(&[
        "issues",
        path.to_str().unwrap(),
        "--export",
        out_dir.to_str().unwrap(),
    ]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let summary = spiderlens_results::Reader::open(&path)
        .unwrap()
        .issues_summary(false)
        .unwrap();
    for i in summary
        .issues
        .iter()
        .filter(|i| i.enabled && i.count.unwrap_or(0) > 0)
    {
        let csv = std::fs::read_to_string(out_dir.join(format!("{}.csv", i.id))).unwrap();
        // Header plus one line per URL.
        assert_eq!(csv.lines().count() as u64, i.count.unwrap() + 1, "{}", i.id);
    }
}

#[test]
fn analyse_runs_the_phase_and_reports_what_ran() {
    let dir = tempfile::tempdir().unwrap();
    let path = crawl_file(&dir);
    let out = run(&["analyse", path.to_str().unwrap(), "--json"]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let report = json(&out);
    assert_eq!(report["saved"], true);
    // Each pass either ran or says why it was skipped.
    assert_eq!(
        report["ran"].as_array().unwrap().len() + report["skipped"].as_array().unwrap().len(),
        spiderlens_results::analysis::PASSES.len()
    );
    let missing = run(&["analyse", "/nonexistent.spiderlens"]);
    assert_eq!(missing.status.code(), Some(3));
}

#[test]
fn crawl_refuses_a_profile_with_invalid_issue_settings() {
    // FR-028: the same validation as the app, before anything is crawled.
    let dir = tempfile::tempdir().unwrap();
    let defaults = run(&["config", "defaults"]);
    let mut profile: serde_json::Value = serde_json::from_slice(&defaults.stdout).unwrap();
    profile["config"]["issues"] = serde_json::json!({ "rules": { "title.over": { "threshold": -1 } } });
    let path = dir.path().join("p.json");
    std::fs::write(&path, profile.to_string()).unwrap();
    let out_file = dir.path().join("c.spiderlens");
    let out = run(&[
        "crawl",
        "http://127.0.0.1:9/",
        "--profile",
        path.to_str().unwrap(),
        "--out",
        out_file.to_str().unwrap(),
    ]);
    assert_eq!(
        out.status.code(),
        Some(2),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(String::from_utf8_lossy(&out.stderr).contains("title.over"));
    assert!(!out_file.exists());
}
