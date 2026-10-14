//! Times the near duplicate analysis on synthetic pages (spec 020 T013):
//! `cargo run --release -p spiderlens-results --example near_dup_bench -- 100000 [dense]`
//!
//! Each page gets 300 words of its own text; one page in ten belongs to a
//! family of five near copies with a few words changed. `dense` adds one
//! template group (pages differing in a few words) of that size.

use std::sync::atomic::AtomicBool;
use std::time::Instant;

use spiderlens_engine::extract::shingle::signature;
use spiderlens_results::near_dup::analyze::{Item, Params, analyze};

/// Small deterministic generator (xorshift).
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
}

fn words(rng: &mut Rng, n: usize) -> Vec<String> {
    (0..n).map(|_| format!("w{}", rng.next() % 20_000)).collect()
}

fn main() {
    let mut args = std::env::args().skip(1);
    let pages: usize = args.next().and_then(|a| a.parse().ok()).unwrap_or(100_000);
    let dense: usize = args.next().and_then(|a| a.parse().ok()).unwrap_or(0);
    let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
    let t = Instant::now();
    let mut texts: Vec<String> = Vec::with_capacity(pages + dense);
    while texts.len() < pages {
        let base = words(&mut rng, 300);
        let family = if rng.next().is_multiple_of(10) { 5 } else { 1 };
        for _ in 0..family {
            let mut w = base.clone();
            for _ in 0..(rng.next() % 6) {
                let i = (rng.next() % 300) as usize;
                w[i] = format!("x{}", rng.next() % 1000);
            }
            texts.push(w.join(" "));
        }
    }
    texts.truncate(pages);
    let template = words(&mut rng, 300);
    for i in 0..dense {
        let mut w = template.clone();
        w[i % 300] = format!("city{i}");
        w[(i * 7) % 300] = format!("name{i}");
        texts.push(w.join(" "));
    }
    eprintln!(
        "{} texts generated in {:.1} s",
        texts.len(),
        t.elapsed().as_secs_f64()
    );

    let t = Instant::now();
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get());
    let chunk = texts.len().div_ceil(threads);
    let sigs: Vec<_> = std::thread::scope(|s| {
        let handles: Vec<_> = texts
            .chunks(chunk)
            .map(|c| {
                s.spawn(move || {
                    c.iter()
                        .map(|t| signature(t).expect("signature"))
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        handles
            .into_iter()
            .flat_map(|h| h.join().expect("thread"))
            .collect()
    });
    let sig_s = t.elapsed().as_secs_f64();
    println!(
        "signatures: {:.1} s on {threads} threads, {:.1} µs per page on one thread",
        sig_s,
        sig_s * 1e6 * threads as f64 / texts.len() as f64
    );

    let items: Vec<Item> = sigs
        .into_iter()
        .enumerate()
        .map(|(i, sig)| Item {
            id: i as i64 + 1,
            sig,
            hash: i as i64,
            inlinks: 0,
            url_len: 30,
        })
        .collect();
    let cancel = AtomicBool::new(false);
    for threshold in [90u8, 80] {
        let t = Instant::now();
        let a = analyze(&items, &Params::new(threshold, 80), &cancel, |_, _| {}).expect("analysis");
        let near = a.pages.iter().filter(|p| p.near_count > 0).count();
        println!(
            "analysis at {threshold}%: {:.2} s, {near} near duplicate pages, {} clusters, largest {}",
            t.elapsed().as_secs_f64(),
            a.clusters.len(),
            a.clusters.first().map_or(0, |c| c.size)
        );
    }
}
