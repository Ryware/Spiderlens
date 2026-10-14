//! The Issues column every tab offers (spec 009 FR-018).

use std::sync::atomic::AtomicBool;

use spiderlens_results::synth::Synth;
use spiderlens_results::{Reader, UrlScope, ViewQuery, write_csv};

#[test]
fn issues_column_shows_highest_priority_and_count_per_url() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("c.spiderlens");
    Synth {
        urls: 300,
        links_per_page: 2,
        seed: 4,
    }
    .write(&path)
    .unwrap();
    let q = ViewQuery {
        tab: "internal".into(),
        filter: "all".into(),
        scope: UrlScope::All,
        search: String::new(),
        sort: None,
    };
    let mut out = Vec::new();
    let never = AtomicBool::new(false);
    let cols = ["address".to_string(), "issues".to_string()];
    write_csv(&path, &q, &cols, None, &mut out, &never, &mut |_, _| {}).unwrap();
    let csv = String::from_utf8(out).unwrap();
    let mut reader = Reader::open(&path).unwrap();
    let db = rusqlite::Connection::open(&path).unwrap();
    let mut flagged = 0;
    for line in csv.lines().skip(1).take(50) {
        let (url, cell) = line.split_once(',').unwrap();
        let id: i64 = db
            .query_row(
                "SELECT id FROM urls WHERE url = ?1",
                [url.trim_matches('"')],
                |r| r.get(0),
            )
            .unwrap();
        let issues = reader.url_issues(id).unwrap();
        match issues.first() {
            None => assert_eq!(cell, "", "{url}"),
            Some(top) => {
                flagged += 1;
                assert_eq!(
                    cell,
                    format!("{} · {}", top.priority.label(), issues.len()),
                    "{url}"
                );
            }
        }
    }
    assert!(flagged > 0, "the synthetic crawl has issues");
}
