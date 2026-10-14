//! `spiderlens duplicates`: near duplicate content of a crawl file
//! (spec 020, specs/020-near-duplicate-content/contracts/cli.md).

use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::atomic::AtomicBool;

use clap::{Args, ValueEnum};
use spiderlens_results::Error;
use spiderlens_results::near_dup::{self, Listing, NearDupState};

use crate::exit;

/// Exit codes shared with `spiderlens export`.
const UNREADABLE: u8 = 3;
const UNAVAILABLE: u8 = 4;

#[derive(Clone, Copy, ValueEnum)]
pub enum Format {
    Csv,
    Json,
}

#[derive(Args)]
pub struct DuplicatesArgs {
    /// The crawl file.
    file: PathBuf,
    /// One row per page (default).
    #[arg(long, conflicts_with_all = ["pairs", "clusters"])]
    pages: bool,
    /// One row per near duplicate pair.
    #[arg(long, conflicts_with = "clusters")]
    pairs: bool,
    /// One row per cluster.
    #[arg(long)]
    clusters: bool,
    /// With --pairs: only this cluster.
    #[arg(long, requires = "pairs")]
    cluster: Option<u32>,
    /// Similarity threshold in percent (50 to 100). Not saved to the file.
    #[arg(long, value_parser = clap::value_parser!(u8).range(50..=100))]
    threshold: Option<u8>,
    /// Minimum main content words to compare. Not saved to the file.
    #[arg(long, value_parser = clap::value_parser!(u32).range(1..=10_000))]
    min_words: Option<u32>,
    /// Compare noindex and canonicalised pages too. Not saved to the file.
    #[arg(long)]
    include_non_indexable: bool,
    #[arg(long, value_enum, default_value = "csv")]
    format: Format,
    /// Write to this file instead of standard output.
    #[arg(long, short)]
    out: Option<PathBuf>,
}

pub fn run_duplicates(a: DuplicatesArgs) -> ExitCode {
    let probe = match spiderlens_results::Reader::open(&a.file) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::from(UNREADABLE);
        }
    };
    drop(probe);
    let ro = match rusqlite_open(&a.file) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::from(UNREADABLE);
        }
    };
    let stored = near_dup::stored_settings(&ro).unwrap_or_default();
    let overrides = a.threshold.is_some() || a.min_words.is_some() || a.include_non_indexable;
    let mut settings = stored.clone();
    if let Some(t) = a.threshold {
        settings.threshold = t;
        settings.report_floor = settings.report_floor.min(t);
    }
    if let Some(w) = a.min_words {
        settings.min_words = w;
    }
    if a.include_non_indexable {
        settings.indexable_only = false;
    }
    let status = match near_dup::status(&ro, &settings) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::from(UNREADABLE);
        }
    };
    if status.state == NearDupState::Unavailable {
        eprintln!("error: {}", status.reason.unwrap_or_default());
        return ExitCode::from(UNAVAILABLE);
    }
    let cancel = AtomicBool::new(false);
    let conn = if overrides || status.state != NearDupState::Done {
        if !overrides {
            eprintln!(
                "analysing near duplicates ({})",
                status.reason.as_deref().unwrap_or("not analysed yet")
            );
        }
        drop(ro);
        match near_dup::analyse_file(&a.file, &settings, !overrides, &cancel, |_, _, _| {}) {
            Ok((c, _)) => c,
            Err(Error::Unavailable(m)) => {
                eprintln!("error: {m}");
                return ExitCode::from(UNAVAILABLE);
            }
            Err(e) => {
                eprintln!("error: {e}");
                return ExitCode::FAILURE;
            }
        }
    } else {
        ro
    };
    let listing = if a.pairs {
        Listing::Pairs { cluster: a.cluster }
    } else if a.clusters {
        Listing::Clusters
    } else {
        Listing::Pages
    };
    let written = match &a.out {
        Some(path) => std::fs::File::create(path)
            .map_err(|e| Error::Invalid(format!("{}: {e}", path.display())))
            .and_then(|f| write(&conn, listing, a.format, f)),
        None => write(&conn, listing, a.format, std::io::stdout().lock()),
    };
    match written {
        Ok(_) => ExitCode::from(exit::FETCHED),
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn write(
    conn: &spiderlens_results::near_dup::Connection,
    listing: Listing,
    format: Format,
    out: impl std::io::Write,
) -> Result<u64, Error> {
    match format {
        Format::Csv => near_dup::export::write_csv(conn, listing, out),
        Format::Json => near_dup::export::write_json(conn, listing, out),
    }
}

fn rusqlite_open(path: &std::path::Path) -> Result<spiderlens_results::near_dup::Connection, Error> {
    near_dup::open_read_only(path)
}
