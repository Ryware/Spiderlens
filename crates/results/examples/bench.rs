//! Times the results queries on a crawl file (T035):
//! `cargo run --release -p spiderlens-results --example bench -- /tmp/1m.spiderlens`
//!
//! `-- <file> page-titles` times only the Page titles tab (T039): opening it,
//! its counts, each sort and filter, then the same after the crawl commits
//! (a live crawl's refresh). Make the file with `gen_crawl --pages`.

use std::time::Instant;

use spiderlens_results::{Reader, Sort, UrlScope, ViewQuery, tab};

fn time<T>(label: &str, f: impl FnOnce() -> T) -> T {
    let t = Instant::now();
    let v = f();
    println!("{:>8.0} ms  {label}", t.elapsed().as_secs_f64() * 1000.0);
    v
}

fn q(tab: &str, filter: &str, search: &str, sort: Option<(&str, bool)>) -> ViewQuery {
    ViewQuery {
        tab: tab.into(),
        filter: filter.into(),
        scope: UrlScope::All,
        search: search.into(),
        sort: sort.map(|(c, d)| Sort {
            column: c.into(),
            desc: d,
        }),
    }
}

/// Opens a view, builds its full order and reads its counts, as the
/// results table does when a tab or filter is chosen.
fn open_tab(r: &mut Reader, label: &str, query: &ViewQuery) -> f64 {
    let t = Instant::now();
    let page = r.open_view(query, 60).expect("open");
    let first = t.elapsed().as_secs_f64() * 1000.0;
    let total = r.materialize(page.view).expect("materialize");
    let full = t.elapsed().as_secs_f64() * 1000.0;
    r.counts(&query.tab, UrlScope::All).expect("counts");
    let all = t.elapsed().as_secs_f64() * 1000.0;
    r.close_view(page.view).unwrap();
    println!(
        "{first:>8.0} ms first screen {full:>6.0} ms full order {all:>6.0} ms with counts  {label} ({total} rows)"
    );
    all
}

fn page_titles(path: &str) {
    let mut r = Reader::open(std::path::Path::new(path)).expect("open");
    let w = rusqlite::Connection::open(path).expect("writer");
    // A commit that changes nothing the tabs read (an unchanged UPDATE is
    // not a commit to SQLite, so the value moves on).
    let tick = std::cell::Cell::new(0);
    let commit = || {
        tick.set(tick.get() + 1);
        w.execute(
            "INSERT OR REPLACE INTO meta (key, value) VALUES ('bench_tick', ?1)",
            [tick.get()],
        )
        .unwrap();
    };
    let mut cases = vec![
        ("all".to_string(), q("page-titles", "all", "", None)),
        ("search 'shoes'".into(), q("page-titles", "all", "shoes", None)),
    ];
    for (c, d) in [
        ("title", false),
        ("title", true),
        ("title_length", true),
        ("occurrences", true),
    ] {
        cases.push((
            format!("sort by {c}{}", if d { " desc" } else { "" }),
            q("page-titles", "all", "", Some((c, d))),
        ));
    }
    for f in tab("page-titles").unwrap().filters.iter().skip(1) {
        cases.push((format!("filter {}", f.id), q("page-titles", f.id, "", None)));
    }
    for round in ["cold", "warm", "after a commit"] {
        if round == "after a commit" {
            // What a live crawl does between two refreshes.
            commit();
        }
        println!("-- {round} (data version {})", r.version().unwrap());
        let mut worst: f64 = 0.0;
        for (label, query) in &cases {
            worst = worst.max(open_tab(&mut r, label, query));
            if round == "after a commit" {
                commit();
            }
        }
        println!("   worst {worst:.0} ms");
    }
    let page = r.open_view(&cases[2].1, 60).unwrap();
    r.materialize(page.view).unwrap();
    commit();
    time("live refresh, sorted by title, + counts", || {
        r.refresh(page.view, &[1]).unwrap();
        r.counts("page-titles", UrlScope::All).unwrap()
    });
}

fn main() {
    let path = std::env::args().nth(1).expect("crawl file");
    if std::env::args().nth(2).as_deref() == Some("page-titles") {
        return page_titles(&path);
    }
    let mut r = Reader::open(std::path::Path::new(&path)).expect("open");
    let cases = [
        ("Internal, default order", q("internal", "all", "", None)),
        (
            "Internal, sort by title",
            q("internal", "all", "", Some(("title", false))),
        ),
        (
            "Internal, sort by title desc",
            q("internal", "all", "", Some(("title", true))),
        ),
        (
            "Internal, sort by status",
            q("internal", "all", "", Some(("status_code", true))),
        ),
        (
            "Internal, sort by inlinks",
            q("internal", "all", "", Some(("inlinks", true))),
        ),
        ("Internal, search 'shoes'", q("internal", "all", "shoes", None)),
        (
            "Internal, search 'חנות' sorted by title",
            q("internal", "all", "חנות", Some(("title", false))),
        ),
        ("Page titles, duplicate", q("page-titles", "duplicate", "", None)),
        (
            "Page titles, sort by occurrences",
            q("page-titles", "all", "", Some(("occurrences", true))),
        ),
        (
            "Response codes, 4xx",
            q("response-codes", "client_error", "", None),
        ),
        (
            "Response codes, redirect chains",
            q("response-codes", "redirect_chain", "", None),
        ),
        ("URL, non-ASCII", q("url", "non_ascii", "", None)),
    ];
    for (label, query) in cases {
        let page = time(&format!("{label}: first screen"), || {
            r.open_view(&query, 60).expect("open")
        });
        let total = time(&format!("{label}: full order"), || {
            r.materialize(page.view).expect("materialize")
        });
        time(&format!("{label}: page at 90% of {total}"), || {
            r.rows(page.view, total * 9 / 10, 60).expect("rows")
        });
        r.close_view(page.view).unwrap();
    }
    for tab in [
        "internal",
        "external",
        "response-codes",
        "url",
        "page-titles",
        "images",
    ] {
        time(&format!("counts {tab}"), || {
            r.counts(tab, UrlScope::All).expect("counts")
        });
    }
    time("tab counts (sidebar, every tab)", || {
        r.tab_counts().expect("tab counts")
    });
    time("detail of URL 1 (most inlinks)", || r.detail(1).expect("detail"));
    let cancel = std::sync::atomic::AtomicBool::new(false);
    let out = std::env::temp_dir().join("bench-export.csv");
    let n = time("export Internal to CSV", || {
        spiderlens_results::export_file(
            std::path::Path::new(&path),
            &q("internal", "all", "", Some(("title", false))),
            &[],
            None,
            &out,
            &cancel,
            &mut |_, _| {},
        )
        .expect("export")
    });
    println!(
        "exported {n} rows, {} MB",
        std::fs::metadata(&out).unwrap().len() / 1_000_000
    );
}
