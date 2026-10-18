//! `spiderlens compare` (spec 012 story 5, contracts/cli.md).

use std::sync::atomic::AtomicBool;

use assert_cmd::Command;
use serde_json::Value;
use spiderlens_compare::Comparison;
use spiderlens_config::CompareSettings;
use spiderlens_results::synth::Synth;

fn pair(dir: &tempfile::TempDir) -> (std::path::PathBuf, std::path::PathBuf) {
    let a = dir.path().join("a.spiderlens");
    let b = dir.path().join("b.spiderlens");
    for (path, seed) in [(&a, 5), (&b, 6)] {
        Synth {
            urls: 600,
            links_per_page: 2,
            seed,
            ..Synth::default()
        }
        .write(path)
        .unwrap();
    }
    (a, b)
}

fn spiderlens() -> Command {
    Command::cargo_bin("spiderlens").unwrap()
}

#[test]
fn us5_json_matches_the_library_and_exports_write_groups() {
    let dir = tempfile::tempdir().unwrap();
    let (a, b) = pair(&dir);
    let out_dir = dir.path().join("out");
    let out = spiderlens()
        .args([
            "compare",
            a.to_str().unwrap(),
            b.to_str().unwrap(),
            "--json",
            "--export",
        ])
        .arg(&out_dir)
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let json: Value = serde_json::from_slice(&out.stdout).unwrap();

    let mut c = Comparison::run(
        &a,
        &b,
        CompareSettings::default(),
        None,
        &AtomicBool::new(false),
        &mut |_| {},
    )
    .unwrap();
    let lib = serde_json::to_value(c.report().unwrap()).unwrap();
    // Paths and every count agree (SC-005).
    assert_eq!(json["summary"]["filters"], lib["summary"]["filters"]);
    assert_eq!(json["summary"]["elements"], lib["summary"]["elements"]);
    assert_eq!(json["elements"], lib["elements"]);
    assert_eq!(json["summary"]["matched"], lib["summary"]["matched"]);

    let changed_titles = lib["summary"]["elements"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["element"] == "title")
        .unwrap()["changed"]
        .as_u64()
        .unwrap();
    assert!(changed_titles > 0, "different seeds change titles");
    let csv = std::fs::read_to_string(out_dir.join("changed-title.csv")).unwrap();
    assert_eq!(
        csv.lines().count() as u64,
        changed_titles + 1,
        "header plus one row per page"
    );
    assert!(csv.starts_with(
        "\u{feff}Address (previous),Address (current),Page title (previous),Page title (current)"
    ));
}

#[test]
fn us5_fail_conditions_set_the_exit_code() {
    let dir = tempfile::tempdir().unwrap();
    let (a, b) = pair(&dir);
    let (a, b) = (a.to_str().unwrap(), b.to_str().unwrap());
    let c = Comparison::run(
        a.as_ref(),
        b.as_ref(),
        CompareSettings::default(),
        None,
        &AtomicBool::new(false),
        &mut |_| {},
    )
    .unwrap();
    let lost = c.summary().lost_indexable;
    drop(c);
    spiderlens()
        .args([
            "compare",
            a,
            b,
            "--fail-on-lost-indexable",
            &(lost + 1).to_string(),
        ])
        .assert()
        .code(0);
    spiderlens()
        .args([
            "compare",
            a,
            b,
            "--fail-on-lost-indexable",
            &lost.max(1).to_string(),
        ])
        .assert()
        .code(if lost >= 1 { 8 } else { 0 });
    spiderlens()
        .args(["compare", a, b, "--fail-on-missing", "0"])
        .assert()
        .code(8)
        .stderr(predicates::str::contains("internal HTML pages are missing"));
    // Usage errors: same file twice, malformed rule.
    spiderlens().args(["compare", a, a]).assert().code(2);
    spiderlens()
        .args(["compare", a, b, "--map", "no-arrow"])
        .assert()
        .code(2);
    spiderlens()
        .args(["compare", a, b, "--map-regex", "(=>x"])
        .assert()
        .code(2);
}

#[test]
fn us5_settings_file_and_command_line_rules_combine() {
    let dir = tempfile::tempdir().unwrap();
    let (a, b) = pair(&dir);
    let settings = dir.path().join("cmp.json");
    std::fs::write(
        &settings,
        r#"{"elements":[{"id":"title","on":false}],"mapping":[{"find":"https://nowhere.test/","replace":"https://nowhere2.test/"}]}"#,
    )
    .unwrap();
    let out = spiderlens()
        .args([
            "compare",
            a.to_str().unwrap(),
            b.to_str().unwrap(),
            "--json",
            "--settings",
        ])
        .arg(&settings)
        .args(["--map", "https://other.test/=>https://other2.test/"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let json: Value = serde_json::from_slice(&out.stdout).unwrap();
    let rules = json["mapping"].as_array().unwrap();
    assert_eq!(rules.len(), 2);
    assert_eq!(rules[0]["find"], "https://nowhere.test/");
    assert_eq!(rules[1]["find"], "https://other.test/");
    let title = json["summary"]["elements"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["element"] == "title")
        .unwrap();
    assert_eq!(title["on"], false);
    assert_eq!(title["changed"], 0);
}
