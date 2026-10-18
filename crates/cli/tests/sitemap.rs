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

#[test]
fn sitemaps_lists_files_as_a_tree() {
    let dir = tempfile::tempdir().unwrap();
    let path = crawl_file(&dir);
    {
        let conn = rusqlite::Connection::open(&path).unwrap();
        conn.execute_batch(
            "UPDATE meta SET value = 'sitemap_entries' WHERE key = 'data_features';
             INSERT INTO sitemap_files (id, location, source, level, kind, http_status, entry_count)
                 VALUES (1, 'https://example.com/index.xml', 'robots', 0, 'index', 200, 2);
             INSERT INTO sitemap_files (id, location, source, parent_id, level, kind, http_status, entry_count)
                 VALUES (2, 'https://example.com/a.xml', 'robots', 1, 1, 'urlset', 200, 1);
             INSERT INTO sitemap_files (id, location, source, parent_id, level, http_status, error_class, error)
                 VALUES (3, 'https://example.com/gone.xml', 'robots', 1, 1, 404, 'http_4xx', 'not found');
             INSERT INTO sitemap_entries (file_id, pos, kind, loc, invalid) VALUES (2, 0, 'url', 'mailto:x', 'mailto: address');",
        )
        .unwrap();
    }
    let p = path.to_str().unwrap();
    let out = cli().args(["sitemaps", p]).output().unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(
        String::from_utf8(out.stdout).unwrap(),
        "https://example.com/index.xml  robots  200  2 entries\n  \
         https://example.com/a.xml  robots  200  1 entries  1 invalid\n  \
         https://example.com/gone.xml  robots  http_4xx  0 entries  not found\n"
    );
    let out = cli().args(["sitemaps", p, "--json"]).output().unwrap();
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["available"], true);
    assert_eq!(v["files"][0]["children"], 2);
    cli()
        .args(["sitemaps", dir.path().join("nope.spiderlens").to_str().unwrap()])
        .assert()
        .code(2);
}

#[test]
fn orphans_lists_what_the_analysis_found() {
    let dir = tempfile::tempdir().unwrap();
    let path = crawl_file(&dir);
    let p = path.to_str().unwrap();
    // Before the pass has run, the list is empty and a note says why.
    let out = cli().args(["orphans", p]).output().unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    assert!(out.stdout.is_empty());
    assert!(String::from_utf8_lossy(&out.stderr).contains("spiderlens analyse"));
    let url: String = {
        let conn = rusqlite::Connection::open(&path).unwrap();
        conn.execute_batch(
            "UPDATE meta SET value = 'sitemap_entries' WHERE key = 'data_features';
             CREATE TABLE IF NOT EXISTS orphans (url_id INTEGER PRIMARY KEY, class TEXT NOT NULL, sources TEXT NOT NULL);
             DELETE FROM orphans;
             INSERT INTO orphans VALUES (2, 'orphan', 'sitemap'), (3, 'nofollow_only', 'sitemap');
             INSERT OR REPLACE INTO meta (key, value) VALUES ('analysis', '{\"orphans\":{\"version\":1}}');",
        )
        .unwrap();
        conn.query_row("SELECT url FROM urls WHERE id = 2", [], |r| r.get(0))
            .unwrap()
    };
    let out = cli().args(["orphans", p]).output().unwrap();
    let text = String::from_utf8(out.stdout).unwrap();
    assert_eq!(text.lines().count(), 1, "{text}");
    assert!(text.starts_with(&format!("{url}  ")), "{text}");
    let out = cli()
        .args(["orphans", p, "--csv", "--nofollow-only"])
        .output()
        .unwrap();
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(
        text.starts_with("url,class,sources,status,state,sitemap\n"),
        "{text}"
    );
    assert!(text.contains(",nofollow_only,sitemap,"), "{text}");
    let out = cli().args(["orphans", p, "--json"]).output().unwrap();
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["summary"]["orphans"], 1);
    assert_eq!(v["summary"]["nofollow_only"], 1);
    assert_eq!(v["orphans"][0]["url_id"], 2);
}
