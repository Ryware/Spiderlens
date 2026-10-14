//! Live refresh with anchors (US1-2, US2-6, plan D5) and reopen (US1-4).

use rusqlite::{Connection, params};
use spiderlens_results::synth::Synth;
use spiderlens_results::{Reader, Sort, UrlScope, ViewQuery};

fn view(sort: Option<&str>) -> ViewQuery {
    ViewQuery {
        tab: "internal".into(),
        filter: "all".into(),
        scope: UrlScope::All,
        search: String::new(),
        sort: sort.map(|c| Sort {
            column: c.into(),
            desc: false,
        }),
    }
}

#[test]
fn live_refresh_keeps_anchor_and_respects_view() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("live.spiderlens");
    Synth {
        urls: 2_000,
        links_per_page: 3,
        seed: 3,
        html: false,
    }
    .write(&path)
    .unwrap();
    let mut r = Reader::open(&path).unwrap();
    let page = r.open_view(&view(Some("title")), 50).unwrap();
    let before = r.materialize(page.view).unwrap();
    let anchor = page.rows[10][0].as_i64().unwrap();
    assert_eq!(r.locate(page.view, &[anchor]).unwrap(), [Some(10)]);
    assert!(
        r.refresh(page.view, &[anchor]).unwrap().is_none(),
        "nothing written yet"
    );

    // The crawl commits new pages whose titles sort first (digits sort before letters).
    let w = Connection::open(&path).unwrap();
    for i in 0..5 {
        w.execute(
            "INSERT INTO urls (url, host_key, internal, depth, state, source, status, content_type, title, fetched_at)
             VALUES (?1, 'https://example.test:443', 1, 1, 'crawled', 'link', 200, 'text/html', ?2, 1)",
            params![format!("https://example.test/new/{i}"), format!("0 new {i}")],
        )
        .unwrap();
    }
    let refresh = r.refresh(page.view, &[anchor]).unwrap().expect("data changed");
    assert_eq!(refresh.total, before + 5);
    assert_eq!(
        refresh.positions,
        [Some(15)],
        "the anchor row moved down by the 5 new rows"
    );
    let first = r.rows(page.view, 0, 5).unwrap();
    assert!(
        first
            .rows
            .iter()
            .all(|row| row[7].as_str().unwrap().starts_with("0 new"))
    );
}

#[test]
fn reopen_gives_identical_views() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("reopen.spiderlens");
    Synth {
        urls: 1_500,
        links_per_page: 2,
        seed: 11,
        html: false,
    }
    .write(&path)
    .unwrap();
    let read = || {
        let mut r = Reader::open(&path).unwrap();
        let page = r.open_view(&view(Some("status_code")), 1000).unwrap();
        let counts = r.counts("page-titles", UrlScope::All).unwrap();
        (page.rows, counts.counts)
    };
    assert_eq!(read(), read());
}

#[test]
fn tab_counts_follow_the_crawl_and_stay_cheap() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("counts.spiderlens");
    Synth {
        urls: 100_000,
        links_per_page: 3,
        seed: 5,
    }
    .write(&path)
    .unwrap();
    let mut r = Reader::open(&path).unwrap();
    let t = std::time::Instant::now();
    let first = r.tab_counts().unwrap();
    let cold = t.elapsed();
    eprintln!("tab_counts over 100k URLs: {} ms", cold.as_millis());
    let internal = r.counts("internal", UrlScope::All).unwrap();
    assert_eq!(Some(first.counts["internal"]), internal.counts["all"]);
    let t = std::time::Instant::now();
    assert_eq!(
        r.tab_counts().unwrap(),
        first,
        "unchanged data is answered from memory"
    );
    assert!(t.elapsed() < std::time::Duration::from_millis(50));

    let w = Connection::open(&path).unwrap();
    w.execute(
        "INSERT INTO urls (url, host_key, internal, depth, state, source, status, content_type, title, fetched_at)
         VALUES ('https://example.test/fresh', 'https://example.test:443', 1, 1, 'crawled', 'link', 200, 'text/html', 'Fresh', 1)",
        [],
    )
    .unwrap();
    let after = r.tab_counts().unwrap();
    assert!(after.version > first.version);
    assert_eq!(after.counts["internal"], first.counts["internal"] + 1);
    assert_eq!(after.counts["page-titles"], first.counts["page-titles"] + 1);
    assert_eq!(after.counts["external"], first.counts["external"]);
}
