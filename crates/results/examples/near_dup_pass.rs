//! Times the near duplicate pass on a crawl file, step by step, without
//! changing the file (results go to a temporary schema):
//! `cargo run --release -p spiderlens-results --example near_dup_pass -- crawl.spiderlens`

use std::sync::atomic::AtomicBool;
use std::time::Instant;

use spiderlens_results::near_dup;

fn main() {
    let path = std::env::args().nth(1).expect("crawl file");
    let path = std::path::Path::new(&path);
    let settings =
        near_dup::stored_settings(&near_dup::open_read_only(path).expect("open")).expect("settings");
    let started = Instant::now();
    let mut last = String::new();
    let (_, summary) = near_dup::analyse_file(
        path,
        &settings,
        false,
        &AtomicBool::new(false),
        |done, total, step| {
            if step != last || std::env::var_os("EVERY").is_some() {
                println!(
                    "{:>8.0} ms  {step} {done}/{total}",
                    started.elapsed().as_secs_f64() * 1000.0
                );
                last = step.to_string();
            }
        },
    )
    .expect("analysis");
    println!(
        "{:>8.0} ms  done: {summary:?}",
        started.elapsed().as_secs_f64() * 1000.0
    );
}
