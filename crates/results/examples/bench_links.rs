//! Times Link Score on a synthetic crawl file (spec 023 SC-002 to SC-004):
//! `cargo run --release -p spiderlens-results --example bench_links -- 100000 50 /tmp/100k.spiderlens`
//! (URLs, links per page, file; the file is written first when missing).
//! With `SYNTH_HTML=1` the pages carry HTML and link positions are timed
//! too (SC-009).

use std::path::Path;
use std::time::Instant;

use spiderlens_results::{Reader, Sort, UrlScope, ViewQuery};

fn main() {
    let mut args = std::env::args().skip(1);
    let urls: u64 = args.next().and_then(|a| a.parse().ok()).unwrap_or(100_000);
    let per_page: u32 = args.next().and_then(|a| a.parse().ok()).unwrap_or(50);
    let out = args
        .next()
        .unwrap_or_else(|| format!("synthetic-{urls}-{per_page}.spiderlens"));
    let path = Path::new(&out);
    if !path.exists() {
        let t = Instant::now();
        spiderlens_results::synth::Synth {
            urls,
            links_per_page: per_page,
            html: std::env::var_os("SYNTH_HTML").is_some(),
            ..Default::default()
        }
        .write(path)
        .expect("write crawl file");
        eprintln!("wrote {out} in {:.1} s", t.elapsed().as_secs_f64());
    }
    let mut r = Reader::open(path).expect("open");
    let t = Instant::now();
    let q = ViewQuery {
        tab: "internal".into(),
        filter: "all".into(),
        scope: UrlScope::All,
        search: String::new(),
        sort: Some(Sort {
            column: "link_score".into(),
            desc: true,
        }),
        segment: None,
    };
    let page = r.open_view(&q, 50).expect("view");
    println!(
        "{:>8.0} ms  Link Score for {urls} URLs, ~{} links, first sorted screen",
        t.elapsed().as_secs_f64() * 1000.0,
        urls * u64::from(per_page.saturating_sub(1))
    );
    let first = page.rows.first().and_then(|row| row.get(1)).cloned();
    println!("top URL: {first:?}");
    if std::env::var_os("SYNTH_HTML").is_some() {
        let t = Instant::now();
        let run = r.analyse().expect("analyse");
        println!(
            "{:>8.0} ms  link positions and Link Score, {} links classified",
            t.elapsed().as_secs_f64() * 1000.0,
            run.links
        );
    }
    if let Ok(status) = std::fs::read_to_string("/proc/self/status") {
        for l in status.lines().filter(|l| l.starts_with("VmHWM")) {
            println!("peak memory: {}", l.trim_start_matches("VmHWM:").trim());
        }
    }
}
