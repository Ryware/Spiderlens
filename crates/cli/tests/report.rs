//! `spiderlens report --audit|--html` and `spiderlens export --bulk`
//! (spec 010 contracts/cli.md).

use assert_cmd::Command;
use spiderlens_results::Reader;
use spiderlens_results::report::ReportOptions;
use spiderlens_results::synth::Synth;

fn crawl_file(dir: &tempfile::TempDir) -> std::path::PathBuf {
    let path = dir.path().join("c.spiderlens");
    Synth {
        urls: 300,
        links_per_page: 2,
        seed: 7,
        ..Synth::default()
    }
    .write(&path)
    .unwrap();
    path
}

fn cli() -> Command {
    Command::cargo_bin("spiderlens").unwrap()
}

#[test]
fn audit_json_is_the_report_the_app_shows() {
    let dir = tempfile::tempdir().unwrap();
    let path = crawl_file(&dir);
    let out = cli()
        .args(["report", path.to_str().unwrap(), "--audit", "--json"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let app = Reader::open(&path)
        .unwrap()
        .report(&ReportOptions::default())
        .unwrap();
    assert_eq!(v["score"], serde_json::to_value(&app.score).unwrap());
    assert_eq!(v["sections"].as_array().unwrap().len(), app.sections.len());

    let text = cli()
        .args(["report", path.to_str().unwrap(), "--audit"])
        .output()
        .unwrap();
    assert!(text.status.success());
    let text = String::from_utf8(text.stdout).unwrap();
    assert!(text.starts_with("Health score: "), "{text}");
    assert!(text.contains("## "), "{text}");
}

#[test]
fn html_file_is_written_once_unless_forced() {
    let dir = tempfile::tempdir().unwrap();
    let path = crawl_file(&dir);
    let out = dir.path().join("report.html");
    let run = |force: bool| {
        let mut c = cli();
        c.args(["report", path.to_str().unwrap(), "--html", out.to_str().unwrap()])
            .args(["--title", "Acme audit", "--urls-per-issue", "5"]);
        if force {
            c.arg("--force");
        }
        c.output().unwrap()
    };
    let first = run(false);
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    let html = std::fs::read_to_string(&out).unwrap();
    assert!(html.contains("Acme audit"));
    assert!(!html.contains("<script"));

    let again = run(false);
    assert_eq!(again.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&again.stderr).contains("--force"));
    assert!(run(true).status.success());

    let bad = cli()
        .args([
            "report",
            path.to_str().unwrap(),
            "--html",
            "x.html",
            "--urls-per-issue",
            "20000",
        ])
        .output()
        .unwrap();
    assert_eq!(bad.status.code(), Some(2));
    assert!(!std::path::Path::new("x.html").exists());
}

#[test]
fn unreadable_crawl_file_exits_3() {
    let dir = tempfile::tempdir().unwrap();
    let junk = dir.path().join("junk.spiderlens");
    std::fs::write(&junk, b"not a crawl file").unwrap();
    let out = cli()
        .args(["report", junk.to_str().unwrap(), "--audit"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(3));
}

#[test]
fn bulk_csv_and_xlsx() {
    let dir = tempfile::tempdir().unwrap();
    let path = crawl_file(&dir);
    let folder = dir.path().join("bulk");
    let out = cli()
        .args(["export", path.to_str().unwrap(), "--bulk", "tabs,links", "--out"])
        .arg(&folder)
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    assert!(folder.join("index.csv").exists());
    let listed = String::from_utf8(out.stdout).unwrap();
    assert!(listed.lines().count() > 2, "{listed}");
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("rows in"), "{err}");
    assert!(err.contains("[12/12] All links"), "{err}");

    let again = cli()
        .args(["export", path.to_str().unwrap(), "--bulk", "tabs", "--out"])
        .arg(&folder)
        .output()
        .unwrap();
    assert_eq!(again.status.code(), Some(2));

    let unknown = cli()
        .args(["export", path.to_str().unwrap(), "--bulk", "nope", "--out"])
        .arg(dir.path().join("n"))
        .output()
        .unwrap();
    assert_eq!(unknown.status.code(), Some(2));
    assert!(!dir.path().join("n").exists());

    let book = dir.path().join("all.xlsx");
    let out = cli()
        .args([
            "export",
            path.to_str().unwrap(),
            "--bulk",
            "all",
            "--format",
            "xlsx",
            "--out",
        ])
        .arg(&book)
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let bytes = std::fs::read(&book).unwrap();
    assert_eq!(&bytes[..2], b"PK");
}
