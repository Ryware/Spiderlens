//! Writes a synthetic crawl file for scale work:
//! `cargo run --release -p spiderlens-results --example gen_crawl -- 1000000 /tmp/1m.spiderlens [rendered]`
//!
//! `--pages` adds page content (titles' tabs need it); `--result-indexes`
//! adds the indexes a finished run leaves, which a live crawl lacks.

fn main() {
    let flags: Vec<String> = std::env::args().skip(1).filter(|a| a.starts_with("--")).collect();
    let mut args = std::env::args().skip(1).filter(|a| !a.starts_with("--"));
    let urls: u64 = args.next().and_then(|a| a.parse().ok()).unwrap_or(1_000_000);
    let out = args.next().unwrap_or_else(|| "synthetic.spiderlens".into());
    let rendered = args.next().as_deref() == Some("rendered");
    let start = std::time::Instant::now();
    spiderlens_results::synth::Synth {
        urls,
        rendered,
        pages: flags.iter().any(|f| f == "--pages"),
        result_indexes: flags.iter().any(|f| f == "--result-indexes"),
        ..Default::default()
    }
    .write(std::path::Path::new(&out))
    .expect("write crawl file");
    eprintln!(
        "{urls} URLs written to {out} in {:.1} s",
        start.elapsed().as_secs_f64()
    );
}
