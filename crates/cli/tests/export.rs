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
        rendered: false,
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
        values: None,
        sort: Some(Sort {
            column: "title".into(),
            desc: true,
        }),
    };
    let dest = dir.path().join("desktop.csv");
    let cols: Vec<String> = ["address", "title", "occurrences"].map(String::from).to_vec();
    let n = export_file(
        &path,
        &q,
        &cols,
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
