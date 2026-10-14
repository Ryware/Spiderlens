//! Times the results queries on a crawl file (T035):
//! `cargo run --release -p spiderlens-results --example bench -- /tmp/1m.spiderlens`

use std::time::Instant;

use spiderlens_results::{Reader, Sort, UrlScope, ViewQuery};

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

fn main() {
    let path = std::env::args().nth(1).expect("crawl file");
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
    time("detail of URL 1 (most inlinks)", || r.detail(1).expect("detail"));
    let cancel = std::sync::atomic::AtomicBool::new(false);
    let out = std::env::temp_dir().join("bench-export.csv");
    let n = time("export Internal to CSV", || {
        spiderlens_results::export_file(
            std::path::Path::new(&path),
            &q("internal", "all", "", Some(("title", false))),
            &[],
            None,
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
