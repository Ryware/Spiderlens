//! `spiderlens export` (spec 007 FR-031, contracts/cli.md).

use std::sync::atomic::AtomicBool;

use assert_cmd::Command;
use spiderlens_results::synth::Synth;
use spiderlens_results::{Sort, UrlScope, ViewQuery, export_file};

fn crawl_file(dir: &tempfile::TempDir) -> std::path::PathBuf {
    let path = dir.path().join("c.spiderlens");
    Synth {
        urls: 800,
        links_per_page: 2,
        seed: 5,
        ..Default::default()
    }
    .write(&path)
    .unwrap();
    path
}

#[test]
fn cli_csv_equals_desktop_export_for_the_same_view() {
    let dir = tempfile::tempdir().unwrap();
    let path = crawl_file(&dir);
    let out = Command::cargo_bin("spiderlens")
        .unwrap()
        .args([
            "export",
            path.to_str().unwrap(),
            "--tab",
            "page-titles",
            "--filter",
            "duplicate",
        ])
        .args(["--sort", "title:desc", "--columns", "address,title,occurrences"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let q = ViewQuery {
        tab: "page-titles".into(),
        filter: "duplicate".into(),
        scope: UrlScope::All,
        search: String::new(),
        sort: Some(Sort {
            column: "title".into(),
            desc: true,
        }),
        segment: None,
    };
    let dest = dir.path().join("desktop.csv");
    let cols: Vec<String> = ["address", "title", "occurrences"].map(String::from).to_vec();
    let n = export_file(
        &path,
        &q,
        &cols,
        None,
        None,
        &dest,
        &AtomicBool::new(false),
        &mut |_, _| {},
    )
    .unwrap();
    assert!(n > 10);
    assert_eq!(out.stdout, std::fs::read(&dest).unwrap());
}

#[test]
fn unknown_ids_and_unavailable_filters_have_their_own_exit_codes() {
    let dir = tempfile::tempdir().unwrap();
    let path = crawl_file(&dir);
    let p = path.to_str().unwrap();
    Command::cargo_bin("spiderlens")
        .unwrap()
        .args(["export", p, "--tab", "nope"])
        .assert()
        .code(2)
        .stderr(predicates::str::contains("--list-tabs"));
    Command::cargo_bin("spiderlens")
        .unwrap()
        .args(["export", p, "--tab", "h1", "--filter", "missing"])
        .assert()
        .code(4)
        .stderr(predicates::str::contains("not collected"));
    Command::cargo_bin("spiderlens")
        .unwrap()
        .args(["export", "/no/such/file.spiderlens"])
        .assert()
        .code(3);
    Command::cargo_bin("spiderlens")
        .unwrap()
        .args(["export", "--list-tabs"])
        .assert()
        .success()
        .stdout(predicates::str::contains("page-titles"));
}

#[test]
fn segment_filter_and_segments_command() {
    use spiderlens_config::{Clause, CrawlConfig, Segment, SegmentField, SegmentOp};
    let dir = tempfile::tempdir().unwrap();
    let path = crawl_file(&dir);
    let seg = |id: u32, name: &str, word: &str| Segment {
        id,
        name: name.into(),
        clauses: vec![Clause {
            field: SegmentField::Path,
            op: SegmentOp::StartsWith,
            value: format!("/{word}/"),
        }],
        ..Segment::default()
    };
    spiderlens_results::synth::store_config(
        &path,
        CrawlConfig {
            segments: vec![seg(1, "Blog", "blog"), seg(2, "Shoes", "shoes")],
            ..CrawlConfig::default()
        },
    )
    .unwrap();
    let run = |args: &[&str]| {
        let out = Command::cargo_bin("spiderlens")
            .unwrap()
            .args(args)
            .output()
            .unwrap();
        (out.status.code(), String::from_utf8(out.stdout).unwrap())
    };
    let p = path.to_str().unwrap();

    let (code, csv) = run(&["export", p, "--segment", "blog"]);
    assert_eq!(code, Some(0));
    let mut lines = csv.lines();
    let header = lines.next().unwrap();
    assert!(header.ends_with(",Segment"), "{header}");
    let rows: Vec<_> = lines.collect();
    assert!(!rows.is_empty());
    assert!(rows.iter().all(|l| l.contains("/blog/") && l.ends_with(",Blog")));

    let (_, json) = run(&["segments", p, "--format", "json"]);
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    let rows = v.as_array().unwrap();
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[0]["segment"], "Blog");
    let total: u64 = rows.iter().map(|r| r["urls"].as_u64().unwrap()).sum();
    assert_eq!(total, 800);
    let (_, none) = run(&["export", p, "--segment", "none", "--tab", "external"]);
    assert!(
        none.lines()
            .skip(1)
            .all(|l| !l.contains(",Blog") && !l.contains(",Shoes"))
    );

    let (code, _) = run(&["export", p, "--segment", "nope"]);
    assert_eq!(code, Some(2));
}
