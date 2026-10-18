//! `spiderlens links` (specs/023-link-score/contracts/cli.md): the CLI
//! prints what the reader works out.

use assert_cmd::Command;
use spiderlens_results::Reader;
use spiderlens_results::synth::Synth;

fn crawl_file(dir: &tempfile::TempDir, html: bool) -> std::path::PathBuf {
    let path = dir.path().join("c.spiderlens");
    Synth {
        urls: 300,
        links_per_page: 12,
        seed: 9,
        html,
        ..Default::default()
    }
    .write(&path)
    .unwrap();
    path
}

fn cli() -> Command {
    Command::cargo_bin("spiderlens").unwrap()
}

#[test]
fn analyse_prints_the_reader_summary() {
    let dir = tempfile::tempdir().unwrap();
    let path = crawl_file(&dir, true);
    // First run: the analysis is worked out, with progress. Later runs read
    // the stored results.
    cli()
        .args(["links", "analyse", path.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicates::str::contains("Link Score worked out for"))
        .stderr(predicates::str::contains("Analysing: Link positions 0%"))
        .stderr(predicates::str::contains("Analysing: Link Score 100% ("));

    let out = cli()
        .args(["links", "analyse", path.to_str().unwrap(), "--json"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let got: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let want = Reader::open(&path).unwrap().analyse().unwrap();
    assert_eq!(got["urls"], want.urls);
    assert_eq!(got["links"], want.links);
    assert_eq!(got["rounds"], want.rounds);
    assert_eq!(got["pages_without_html"], 0);
    cli()
        .args(["links", "analyse", path.to_str().unwrap(), "--quiet"])
        .assert()
        .success()
        .stdout("")
        .stderr("");
    cli()
        .args([
            "links",
            "analyse",
            dir.path().join("none.spiderlens").to_str().unwrap(),
        ])
        .assert()
        .code(3);
}

#[test]
fn analyse_counts_pages_without_html() {
    let dir = tempfile::tempdir().unwrap();
    let path = crawl_file(&dir, false);
    cli()
        .args(["links", "analyse", path.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicates::str::contains("pages had no stored HTML"));
}

#[test]
fn positions_match_the_tester_and_take_a_rules_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = crawl_file(&dir, true);
    let mut r = Reader::open(&path).unwrap();
    let id = (1..=300)
        .find(|&id| r.test_positions(id, &[]).unwrap().is_some_and(|l| !l.is_empty()))
        .expect("a page with stored HTML and links");
    let url = r.detail(id).unwrap().url;
    let want = r.test_positions(id, &[]).unwrap().unwrap();

    let out = cli()
        .args(["links", "positions", path.to_str().unwrap(), "--url", &url])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let stdout = String::from_utf8(out.stdout).unwrap();
    let lines: Vec<Vec<&str>> = stdout.lines().map(|l| l.split('\t').collect()).collect();
    assert_eq!(lines.len(), want.len());
    for (line, l) in lines.iter().zip(&want) {
        assert_eq!(line[0], l.pos.to_string());
        assert_eq!(line[1], l.position.as_str());
        assert_eq!(line[2], "built-in");
        assert_eq!(line[3], l.raw);
    }
    assert!(lines.iter().any(|l| l[1] == "navigation"));

    // A rules file wins over the stored rules: everything in <main> is a sidebar.
    let rules = dir.path().join("rules.json");
    std::fs::write(&rules, r#"[{"selector": "main", "position": "sidebar"}]"#).unwrap();
    let out = cli()
        .args(["links", "positions", path.to_str().unwrap(), "--url", &url])
        .args(["--rules", rules.to_str().unwrap()])
        .output()
        .unwrap();
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.lines().any(|l| l.contains("\tsidebar\trule 1\t")));

    std::fs::write(&rules, r#"[{"selector": "div[", "position": "sidebar"}]"#).unwrap();
    cli()
        .args(["links", "positions", path.to_str().unwrap(), "--url", &url])
        .args(["--rules", rules.to_str().unwrap()])
        .assert()
        .code(2)
        .stderr(predicates::str::contains("rule 1: Not a valid CSS selector"));
    cli()
        .args([
            "links",
            "positions",
            path.to_str().unwrap(),
            "--url",
            "https://nowhere.test/",
        ])
        .assert()
        .failure()
        .stderr(predicates::str::contains("is not in"));
}

#[test]
fn links_tab_exports_like_any_tab() {
    let dir = tempfile::tempdir().unwrap();
    let path = crawl_file(&dir, false);
    let out = cli()
        .args(["export", path.to_str().unwrap(), "--tab", "links"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let csv = String::from_utf8(out.stdout).unwrap();
    assert!(csv.lines().next().unwrap().contains("Link Score"), "{csv:.200}");
    assert!(csv.lines().count() > 100);
}
