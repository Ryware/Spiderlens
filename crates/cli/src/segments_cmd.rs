//! `spiderlens segments`: URLs per segment of a crawl file, using the
//! segments stored with the crawl (spec 027 part B, contracts/cli.md).

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Args, ValueEnum};
use serde::Serialize;
use spiderlens_results::Reader;

use crate::exit;

#[derive(Clone, Copy, ValueEnum)]
pub enum FormatArg {
    Table,
    Json,
}

#[derive(Args)]
pub struct SegmentsArgs {
    /// The crawl file.
    file: PathBuf,
    /// Output format.
    #[arg(long, value_enum, default_value = "table")]
    format: FormatArg,
}

#[derive(Serialize)]
struct Row<'a> {
    segment: Option<&'a str>,
    urls: u64,
}

pub fn run_segments(a: SegmentsArgs) -> ExitCode {
    let counts = match Reader::open(&a.file).and_then(|mut r| r.segment_counts()) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::from(3);
        }
    };
    let mut rows: Vec<Row> = counts
        .segments
        .iter()
        .filter(|s| s.enabled)
        .map(|s| Row {
            segment: Some(s.name.trim()),
            urls: counts.counts.get(&s.id).copied().unwrap_or(0),
        })
        .collect();
    rows.push(Row {
        segment: None,
        urls: counts.no_segment,
    });
    match a.format {
        FormatArg::Json => crate::print_json(&rows),
        FormatArg::Table => {
            if counts.segments.iter().all(|s| !s.enabled) {
                eprintln!("This crawl has no segments.");
            }
            let width = rows
                .iter()
                .filter_map(|r| r.segment)
                .map(|s| s.chars().count())
                .max()
                .unwrap_or(0)
                .max(10);
            for r in &rows {
                println!("{:<width$}  {:>9}", r.segment.unwrap_or("No segment"), r.urls);
            }
        }
    }
    ExitCode::from(exit::FETCHED)
}
