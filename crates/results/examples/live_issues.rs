//! Evaluates issues on a crawl file every few seconds while a crawl writes
//! it, as the app does (spec 009 SC-002 benchmark). Prints how long each
//! evaluation took. Stops after `secs`.
//!
//! `cargo run --release -p spiderlens-results --example live_issues -- <file> [interval-ms] [secs]`

use std::time::{Duration, Instant};

fn main() {
    let mut args = std::env::args().skip(1);
    let path = std::path::PathBuf::from(args.next().expect("crawl file"));
    let every = Duration::from_millis(args.next().map_or(3000, |s| s.parse().unwrap()));
    let stop = Instant::now() + Duration::from_secs(args.next().map_or(600, |s| s.parse().unwrap()));
    let mut reader = loop {
        match spiderlens_results::Reader::open(&path) {
            Ok(r) => break r,
            Err(_) => std::thread::sleep(Duration::from_millis(200)),
        }
    };
    let (mut runs, mut total) = (0u32, Duration::ZERO);
    while Instant::now() < stop {
        let t = Instant::now();
        let s = reader.issues_summary(true).expect("summary");
        let took = t.elapsed();
        runs += 1;
        total += took;
        let urls: u64 = s.by_priority.iter().map(|p| p.urls).sum();
        eprintln!("eval {runs}: {took:?}, {urls} issue URLs");
        std::thread::sleep(every.saturating_sub(took));
    }
    eprintln!("{runs} evaluations, {total:?} in total");
}
