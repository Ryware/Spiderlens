//! `spiderlens sitemap generate` (spec 011 FR-019, contracts/cli.md).

use assert_cmd::Command;
use spiderlens_results::synth::Synth;
use spiderlens_results::{Reader, SitemapOutput, generate_sitemaps};

fn crawl_file(dir: &tempfile::TempDir) -> std::path::PathBuf {
    let path = dir.path().join("c.spiderlens");
    Synth {
        urls: 300,
        links_per_page: 2,
        seed: 7,
    }
    .write(&path)
    .unwrap();
    path
}

fn cli() -> Command {
    Command::cargo_bin("spiderlens").unwrap()
}

#[test]
fn cli_writes_the_same_bytes_as_the_library() {
    let dir = tempfile::tempdir().unwrap();
    let path = crawl_file(&dir);
    let out = dir.path().join("cli");
    cli()
        .args([
            "sitemap",
            "generate",
            path.to_str().unwrap(),
            "--out",
            out.to_str().unwrap(),
        ])
        .args(["--max-urls", "100", "--lastmod", "crawl"])
        .assert()
        .success();

    let lib = dir.path().join("lib");
    std::fs::create_dir_all(&lib).unwrap();
    let opts = spiderlens_config::SitemapGenerateOptions {
        max_urls_per_file: 100,
        lastmod: spiderlens_config::LastmodSource::CrawlDate,
        ..Default::default()
    };
    let report = generate_sitemaps(
        &Reader::open(&path).unwrap(),
        &opts,
        SitemapOutput::Folder {
            dir: &lib,
            replace: false,
        },
    )
    .unwrap();
    assert!(report.files.len() > 1, "expected a split with an index");
    for f in &report.files {
        assert_eq!(
            std::fs::read(out.join(&f.name)).unwrap(),
            std::fs::read(lib.join(&f.name)).unwrap(),
            "{}",
            f.name
        );
    }
}

#[test]
fn dry_run_writes_nothing_and_reports_json() {
    let dir = tempfile::tempdir().unwrap();
    let path = crawl_file(&dir);
    let out = dir.path().join("out");
    let o = cli()
        .args([
            "sitemap",
            "generate",
            path.to_str().unwrap(),
            "--dry-run",
            "--json",
        ])
        .args(["--out", out.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let v: serde_json::Value = serde_json::from_slice(&o.stdout).unwrap();
    assert!(v["pages"].as_u64().unwrap() > 0);
    assert!(!out.exists());
}

#[test]
fn existing_files_need_force_and_bad_input_has_its_own_codes() {
    let dir = tempfile::tempdir().unwrap();
    let path = crawl_file(&dir);
    let p = path.to_str().unwrap();
    let out = dir.path().join("out");
    let o = out.to_str().unwrap();
    cli()
        .args(["sitemap", "generate", p, "--out", o])
        .assert()
        .success();
    cli()
        .args(["sitemap", "generate", p, "--out", o])
        .assert()
        .code(2)
        .stderr(predicates::str::contains("--force"));
    cli()
        .args(["sitemap", "generate", p, "--out", o, "--force"])
        .assert()
        .success();
    cli()
        .args([
            "sitemap",
            "generate",
            p,
            "--dry-run",
            "--include",
            "^nothing-matches$",
        ])
        .assert()
        .code(4);
    cli()
        .args(["sitemap", "generate", p, "--dry-run", "--priority", "loud"])
        .assert()
        .code(2);
    let missing = dir.path().join("missing.spiderlens");
    cli()
        .args(["sitemap", "generate", missing.to_str().unwrap(), "--dry-run"])
        .assert()
        .code(3);
}
