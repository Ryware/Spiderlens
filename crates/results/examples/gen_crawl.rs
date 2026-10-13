//! Writes a synthetic crawl file for scale work:
//! `cargo run --release -p spiderlens-results --example gen_crawl -- 1000000 /tmp/1m.spiderlens`

fn main() {
    let mut args = std::env::args().skip(1);
    let urls: u64 = args.next().and_then(|a| a.parse().ok()).unwrap_or(1_000_000);
    let out = args.next().unwrap_or_else(|| "synthetic.spiderlens".into());
    let start = std::time::Instant::now();
    spiderlens_results::synth::Synth {
        urls,
        ..Default::default()
    }
    .write(std::path::Path::new(&out))
    .expect("write crawl file");
    eprintln!(
        "{urls} URLs written to {out} in {:.1} s",
        start.elapsed().as_secs_f64()
    );
}
