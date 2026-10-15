//! Crawl files move between operating systems unchanged (spec 006 SC-008).
//!
//! CI runs `write_crawl_files` on Windows, macOS and Linux with
//! `SPIDERLENS_PORTABLE_OUT` set, then `read_crawl_files` on each OS with
//! `SPIDERLENS_PORTABLE_IN` pointing at the files from all three. A reader must
//! see exactly the data the writer saw. Without the variables both tests only
//! check themselves against a file written here.

use std::path::{Path, PathBuf};
use std::time::Duration;

use rusqlite::types::ValueRef;
use serde_json::{Map, Value, json};
use spiderlens_config::{CrawlConfig, ModeInput, SpeedSettings};
use spiderlens_engine::crawl::file;
use spiderlens_engine::crawl::store::connect_read_only;
use spiderlens_engine::crawl::{Control, CrawlEvent, Crawler};
use spiderlens_test_fixtures::{Site, SiteBuilder};
use tokio::sync::{mpsc, watch};

const HOST: &str = "127.0.0.1";
const PAGES: usize = 120;
const EXPECTED: &str = "expected.json";

/// Pages with titles, descriptions, headings, non-ASCII text and links.
fn site() -> Site {
    let mut b = SiteBuilder::new();
    for i in 0..PAGES {
        let links: Vec<String> = (1..=6)
            .map(|k| i * 6 + k)
            .filter(|&c| c < PAGES)
            .map(|c| format!("/p/{c}"))
            .collect();
        let refs: Vec<&str> = links.iter().map(String::as_str).collect();
        let head = format!(
            "<title>Page {i} – café ✓ שלום</title><meta name=\"description\" content=\"About page {i}\">"
        );
        b = b.page(HOST, &format!("/p/{i}"), &head, &refs);
    }
    b.start()
}

fn config(max_urls: Option<u64>) -> CrawlConfig {
    let mut c = CrawlConfig {
        speed: SpeedSettings::FAST,
        ..CrawlConfig::default()
    };
    c.limits.max_urls = max_urls;
    c
}

async fn crawl(site: &Site, path: &Path, max_urls: Option<u64>) {
    let input = ModeInput::Spider {
        start_url: site.url(HOST, "/p/0"),
    };
    let crawler = Crawler::create(path, config(max_urls), &input).unwrap();
    let (_tx, rx) = watch::channel(Control::Run);
    let (etx, mut erx) = mpsc::unbounded_channel::<CrawlEvent>();
    let drain = tokio::spawn(async move { while erx.recv().await.is_some() {} });
    tokio::time::timeout(Duration::from_secs(60), crawler.run(rx, Some(etx)))
        .await
        .expect("crawl ends")
        .unwrap();
    drain.await.unwrap();
}

/// Every row of every table, in rowid order, exactly as stored. Reals keep
/// their bits and blobs their bytes, so nothing is lost to formatting.
fn dump(path: &Path) -> Value {
    let conn = connect_read_only(path).unwrap();
    let mut tables: Vec<String> = conn
        .prepare(
            "SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%' ORDER BY name",
        )
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    tables.sort();
    let mut out = Map::new();
    for table in tables {
        let mut stmt = conn
            .prepare(&format!("SELECT * FROM \"{table}\" ORDER BY rowid"))
            .unwrap();
        let columns = stmt.column_count();
        let rows: Vec<Value> = stmt
            .query_map([], |r| {
                Ok(Value::Array(
                    (0..columns)
                        .map(|i| match r.get_ref(i).unwrap() {
                            ValueRef::Null => Value::Null,
                            ValueRef::Integer(n) => json!(n),
                            ValueRef::Real(f) => json!({ "real": f.to_bits() }),
                            ValueRef::Text(t) => json!(String::from_utf8_lossy(t)),
                            ValueRef::Blob(b) => {
                                json!({ "blob": b.iter().map(|x| format!("{x:02x}")).collect::<String>() })
                            }
                        })
                        .collect(),
                ))
            })
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        out.insert(table, Value::Array(rows));
    }
    Value::Object(out)
}

/// What `spiderlens info` shows, without what depends on where the file is.
fn info(path: &Path) -> Value {
    let mut v = serde_json::to_value(file::info(path).unwrap()).unwrap();
    let o = v.as_object_mut().unwrap();
    for k in ["path", "name", "size_bytes", "access", "unsaved"] {
        o.remove(k);
    }
    v
}

fn describe(path: &Path) -> Value {
    json!({ "info": info(path), "data": dump(path) })
}

/// Writes a completed and a stopped-early crawl, and what they contain.
async fn write_files(dir: &Path) {
    std::fs::create_dir_all(dir).unwrap();
    let site = site();
    let completed = dir.join("completed.spiderlens");
    let partial = dir.join("limited.spiderlens");
    crawl(&site, &completed, None).await;
    crawl(&site, &partial, Some(40)).await;
    // A finished crawl is one file: no log or lock is left beside it.
    // (Checked before the read-only views below, which may add an index.)
    for name in ["completed.spiderlens", "limited.spiderlens"] {
        for suffix in ["-wal", "-shm", ".lock"] {
            let side = dir.join(format!("{name}{suffix}"));
            assert!(
                !side.exists() || std::fs::metadata(&side).unwrap().len() == 0,
                "{} was left beside the crawl",
                side.display()
            );
        }
    }
    let expected = json!({
        "os": std::env::consts::OS,
        "files": {
            "completed.spiderlens": describe(&completed),
            "limited.spiderlens": describe(&partial),
        }
    });
    std::fs::write(dir.join(EXPECTED), serde_json::to_vec_pretty(&expected).unwrap()).unwrap();
}

/// Opens every file in `dir` (one folder per writing OS) and checks it holds
/// what its writer recorded. Returns how many files were checked.
fn read_files(dir: &Path) -> usize {
    let mut checked = 0;
    let mut writers: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.join(EXPECTED).exists())
        .collect();
    if dir.join(EXPECTED).exists() {
        writers.push(dir.to_path_buf());
    }
    assert!(!writers.is_empty(), "no crawl files in {}", dir.display());
    for writer in writers {
        let expected: Value = serde_json::from_slice(&std::fs::read(writer.join(EXPECTED)).unwrap()).unwrap();
        let from = expected["os"].as_str().unwrap_or("?").to_string();
        for (name, want) in expected["files"].as_object().unwrap() {
            // Work on a copy: the downloaded folder may be read-only, and
            // opening for writing must not change what the next check sees.
            let scratch = tempfile::tempdir().unwrap();
            let path = scratch.path().join(name);
            std::fs::copy(writer.join(name), &path).unwrap();
            let got = describe(&path);
            assert!(
                got == *want,
                "{name} written on {from} reads differently on {}",
                std::env::consts::OS
            );
            // It can also be opened for writing (lock, format checks), and
            // nothing needed recovering.
            let crawler =
                Crawler::open(&path).unwrap_or_else(|e| panic!("{name} from {from} does not open: {e}"));
            assert!(crawler.opened().recovered.is_none(), "{name} from {from}");
            assert!(crawler.opened().upgraded.is_none(), "{name} from {from}");
            drop(crawler);
            assert_eq!(dump(&path), want["data"], "{name} from {from} after opening");
            checked += 1;
        }
    }
    checked
}

#[tokio::test]
async fn write_crawl_files() {
    match std::env::var_os("SPIDERLENS_PORTABLE_OUT") {
        Some(out) => write_files(Path::new(&out)).await,
        None => {
            let dir = tempfile::tempdir().unwrap();
            write_files(dir.path()).await;
            assert_eq!(read_files(dir.path()), 2);
        }
    }
}

#[test]
fn read_crawl_files() {
    let Some(input) = std::env::var_os("SPIDERLENS_PORTABLE_IN") else {
        return;
    };
    let checked = read_files(Path::new(&input));
    assert!(
        checked >= 6,
        "expected files from three systems, checked {checked}"
    );
}
