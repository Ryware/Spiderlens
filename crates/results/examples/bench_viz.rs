//! Times the crawl and directory tree scenes on a synthetic crawl file
//! (spec 022 SC-002): `cargo run --release -p spiderlens-results --example
//! bench_viz -- 100000 20 /tmp/100k.spiderlens` (URLs, links per page,
//! file; the file is written first when missing).

use std::path::Path;
use std::time::Instant;

use spiderlens_results::Reader;
use spiderlens_results::viz::{Layout, ViewKind, ViewState};

fn main() {
    let mut args = std::env::args().skip(1);
    let urls: u64 = args.next().and_then(|a| a.parse().ok()).unwrap_or(100_000);
    let per_page: u32 = args.next().and_then(|a| a.parse().ok()).unwrap_or(20);
    let out = args
        .next()
        .unwrap_or_else(|| format!("synthetic-{urls}-{per_page}.spiderlens"));
    let path = Path::new(&out);
    if !path.exists() {
        let t = Instant::now();
        spiderlens_results::synth::Synth {
            urls,
            links_per_page: per_page,
            ..Default::default()
        }
        .write(path)
        .expect("write crawl file");
        eprintln!("wrote {out} in {:.1} s", t.elapsed().as_secs_f64());
    }
    let mut r = Reader::open(path).expect("open");
    for view in [ViewKind::CrawlTree, ViewKind::DirectoryTree] {
        for layout in [Layout::Tree, Layout::Radial] {
            let t = Instant::now();
            let s = r
                .viz_scene(&ViewState {
                    view,
                    layout,
                    ..ViewState::default()
                })
                .expect("scene");
            print(t, &s);
        }
    }
    let t = Instant::now();
    let s = r
        .viz_scene(&ViewState {
            view: ViewKind::LinkGraph,
            ..ViewState::default()
        })
        .expect("graph");
    print(t, &s);
    eprintln!("          {} edges, settled: {}", s.edges.len(), s.settled);
    let t = Instant::now();
    let s = r
        .viz_scene(&ViewState {
            view: ViewKind::Anchors,
            ..ViewState::default()
        })
        .expect("anchors");
    print(t, &s);
}

fn print(t: Instant, s: &spiderlens_results::viz::Scene) {
    let json = serde_json::to_vec(s).expect("json").len();
    eprintln!(
        "{:>8.0} ms  {:?} {:?}: {} nodes of {} URLs, {} KB JSON",
        t.elapsed().as_secs_f64() * 1000.0,
        s.view,
        s.layout,
        s.nodes.len(),
        s.total,
        json / 1024
    );
}
