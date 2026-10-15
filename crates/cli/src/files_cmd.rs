//! `spiderlens files` and `spiderlens purge-html` (spec 019,
//! contracts/cli.md): what crawl files cost on disk, and dropping their
//! stored HTML. No automatic cleanup here (FR-026).

use std::io::{BufRead, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use clap::Args;
use serde::Serialize;
use spiderlens_engine::crawl::file;
use spiderlens_engine::crawl::manage::{self, CrawlRow};

use crate::{exit, print_json};

#[derive(Args)]
pub struct FilesArgs {
    /// Folder to list [default: the current folder].
    dir: Option<PathBuf>,
    /// Print as JSON.
    #[arg(long)]
    json: bool,
}

#[derive(Args)]
pub struct PurgeHtmlArgs {
    /// Crawl file.
    file: PathBuf,
    /// Remove the HTML but do not shrink the file (needs no extra space).
    #[arg(long)]
    no_compact: bool,
    /// Do not ask for confirmation.
    #[arg(long)]
    yes: bool,
    /// Print the result as JSON.
    #[arg(long)]
    json: bool,
}

#[derive(Serialize)]
struct FileRow {
    #[serde(flatten)]
    row: CrawlRow,
    html_bytes: Option<u64>,
    html_pages: Option<u64>,
}

pub fn run_files(args: FilesArgs) -> ExitCode {
    let dir = args.dir.unwrap_or_else(|| PathBuf::from("."));
    if !dir.is_dir() {
        eprintln!("error: {} is not a folder", dir.display());
        return ExitCode::from(exit::USAGE);
    }
    let now = jiff::Timestamp::now();
    let mut rows: Vec<FileRow> = manage::crawls_in(&dir)
        .into_iter()
        .map(|p| {
            let html = manage::html_size(&p).ok();
            FileRow {
                row: manage::describe(&p, None, None, now),
                html_bytes: html.map(|h| h.bytes),
                html_pages: html.map(|h| h.pages),
            }
        })
        .collect();
    rows.sort_by(|a, b| b.row.last_used.cmp(&a.row.last_used));
    if args.json {
        print_json(&rows);
        return ExitCode::from(exit::FETCHED);
    }
    let mut out = std::io::stdout().lock();
    let _ = writeln!(
        out,
        "{:<32} {:>10} {:>10} {:>8} {:<10} LAST USED",
        "NAME", "SIZE", "HTML", "URLS", "STATUS"
    );
    for r in &rows {
        let name = r.row.path.file_name().unwrap_or_default().to_string_lossy();
        let _ = writeln!(
            out,
            "{:<32} {:>10} {:>10} {:>8} {:<10} {}",
            name,
            file::format_bytes(r.row.size_bytes),
            r.html_bytes.map(file::format_bytes).unwrap_or_else(|| "-".into()),
            r.row.urls.map(|n| n.to_string()).unwrap_or_else(|| "-".into()),
            r.row.status.as_deref().unwrap_or("-"),
            r.row.last_used.as_deref().map(short_time).unwrap_or("-"),
        );
    }
    ExitCode::from(exit::FETCHED)
}

/// `2026-10-13T14:02:11.5Z` → `2026-10-13 14:02`.
fn short_time(t: &str) -> &str {
    t.get(..16).unwrap_or(t)
}

pub fn run_purge_html(args: PurgeHtmlArgs) -> ExitCode {
    let check = match manage::purge_check(&args.file) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::from(exit::USAGE);
        }
    };
    if check.html_pages == 0 {
        eprintln!("{} has no stored HTML", args.file.display());
        return ExitCode::from(exit::FETCHED);
    }
    let compact = !args.no_compact;
    if compact && !check.can_compact {
        eprintln!(
            "error: not enough free space to shrink the file: {} free, about {} needed. \
             Free some space, or use --no-compact to remove the HTML without shrinking the file.",
            check
                .free_bytes
                .map(file::format_bytes)
                .unwrap_or_else(|| "unknown".into()),
            file::format_bytes(check.new_size + manage::COMPACT_MARGIN),
        );
        return ExitCode::from(exit::LOCAL);
    }
    if !args.yes {
        eprint!(
            "Remove the stored HTML of {} pages ({}) from {}? Results stay as they are, \
             but the page HTML can no longer be viewed. [y/N] ",
            check.html_pages,
            file::format_bytes(check.html_bytes),
            args.file.display()
        );
        let mut answer = String::new();
        let _ = std::io::stdin().lock().read_line(&mut answer);
        if !matches!(answer.trim(), "y" | "Y" | "yes") {
            eprintln!("nothing changed");
            return ExitCode::from(exit::USAGE);
        }
    }
    match manage::purge_html(&args.file, compact) {
        Ok(report) => {
            if args.json {
                print_json(&report);
            } else {
                println!(
                    "Removed the stored HTML of {} pages; the file is {} smaller{}.",
                    report.html_pages,
                    file::format_bytes(report.freed_bytes),
                    if report.compacted {
                        ""
                    } else {
                        " (space reusable by this crawl, not returned to the disk)"
                    }
                );
            }
            ExitCode::from(exit::FETCHED)
        }
        // A lock held elsewhere, a write failure: both are this computer's
        // problem, not the file's (006 exit code 6).
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::from(exit::LOCAL)
        }
    }
}
