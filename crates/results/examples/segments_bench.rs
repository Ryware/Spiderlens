//! Times segment rebuilds with 100 segments (spec 027 SC-B1, T012):
//! `cargo run --release -p spiderlens-results --example segments_bench -- /tmp/1m.spiderlens`

use std::time::Instant;

use spiderlens_config::{Clause, Segment, SegmentField, SegmentMatch, SegmentOp};
use spiderlens_results::{Reader, UrlScope, ViewQuery};

fn main() {
    let path = std::env::args().nth(1).expect("crawl file");
    let mut r = Reader::open(std::path::Path::new(&path)).expect("open");
    // 99 segments that rarely match (worst case: every URL tests most of
    // them), with patterns and page facts mixed in, then a catch-all.
    let mut segments: Vec<Segment> = (1..=99u32)
        .map(|i| {
            let (field, op, value) = match i % 4 {
                0 => (
                    SegmentField::Path,
                    SegmentOp::StartsWith,
                    format!("/section-{i}/"),
                ),
                1 => (
                    SegmentField::Url,
                    SegmentOp::Matches,
                    format!(r"/(cat|tag)-{i}/\d+$"),
                ),
                2 => (SegmentField::Path, SegmentOp::Contains, format!("-{i}-")),
                _ => (SegmentField::Depth, SegmentOp::Gt, format!("{}", 100 + i)),
            };
            Segment {
                id: i,
                name: format!("Segment {i}"),
                mode: SegmentMatch::Any,
                clauses: vec![Clause { field, op, value }],
                ..Segment::default()
            }
        })
        .collect();
    segments.push(Segment {
        id: 100,
        name: "Blog".into(),
        clauses: vec![Clause {
            field: SegmentField::Path,
            op: SegmentOp::Contains,
            value: "/blog/".into(),
        }],
        ..Segment::default()
    });
    let t = Instant::now();
    let counts = r.set_segments(segments.clone()).expect("set");
    println!(
        "{:>8.0} ms  rebuild with 100 segments ({} in Blog)",
        t.elapsed().as_secs_f64() * 1000.0,
        counts.counts[&100]
    );
    let q = ViewQuery {
        tab: "internal".into(),
        filter: "all".into(),
        scope: UrlScope::All,
        search: String::new(),
        sort: None,
        segment: Some(100),
        values: None,
    };
    let t = Instant::now();
    let page = r.open_view(&q, 60).expect("view");
    let total = r.materialize(page.view).expect("materialize");
    println!(
        "{:>8.0} ms  Internal tab in Blog, full order ({total} rows)",
        t.elapsed().as_secs_f64() * 1000.0
    );
    let t = Instant::now();
    for tab in spiderlens_results::tabs() {
        r.counts_in(&tab.id, UrlScope::All, Some(100)).expect("counts");
    }
    println!(
        "{:>8.0} ms  counts for every tab in Blog",
        t.elapsed().as_secs_f64() * 1000.0
    );
}
