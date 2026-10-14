//! Issue evaluation figures for spec 009 benchmarks.md: final pass
//! (SC-003), a threshold change (SC-004), the first screen of the largest
//! issue (SC-005) and peak memory (SC-006, Linux only).
//!
//! `cargo run --release -p spiderlens-results --example issue_bench -- <file>`

use std::time::Instant;

use spiderlens_config::{IssueSettings, RuleOverride};
use spiderlens_results::{Reader, ViewQuery};

fn main() {
    let path = std::path::PathBuf::from(std::env::args().nth(1).expect("crawl file"));
    let t = Instant::now();
    let mut r = Reader::open(&path).expect("open");
    let s = r.issues_summary(false).expect("summary");
    println!("final pass: {:?}", t.elapsed());

    let mut s2 = IssueSettings::default();
    s2.rules.insert(
        "title.over".into(),
        RuleOverride {
            threshold: Some(70.0),
            ..Default::default()
        },
    );
    let t = Instant::now();
    assert!(r.set_issue_settings(&s2).is_empty());
    r.issues_summary(false).expect("summary");
    println!("threshold change: {:?}", t.elapsed());

    let top = s
        .issues
        .iter()
        .max_by_key(|i| i.count.unwrap_or(0))
        .expect("an issue");
    let q = ViewQuery {
        tab: format!("issue:{}", top.id),
        filter: "all".into(),
        scope: Default::default(),
        search: String::new(),
        sort: None,
    };
    let t = Instant::now();
    let page = r.open_view(&q, 50).expect("view");
    println!(
        "first screen of {} ({} URLs): {:?}, {} rows",
        top.id,
        top.count.unwrap_or(0),
        t.elapsed(),
        page.rows.len()
    );

    if let Ok(status) = std::fs::read_to_string("/proc/self/status")
        && let Some(line) = status.lines().find(|l| l.starts_with("VmHWM"))
    {
        println!("peak memory: {}", line.trim_start_matches("VmHWM:").trim());
    }
}
