//! `spiderlens report --audit|--html` and `spiderlens export --bulk`
//! (spec 010, specs/010-crawl-report-exports/contracts/cli.md).

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::atomic::AtomicBool;

use spiderlens_results::bulk::{self, BulkFormat, BulkOptions};
use spiderlens_results::issues::{ResolvedSettings, resolve};
use spiderlens_results::report::html::FileOptions;
use spiderlens_results::report::{Block, Report, ReportOptions, SectionState};
use spiderlens_results::{Error, ExportError, Reader};

use crate::exit;

/// The crawl file cannot be opened or read.
const UNREADABLE: u8 = 3;
/// The output could not be written (spec 006 local problem).
const LOCAL: u8 = 6;

/// Issue settings from `--issues`, else defaults (spec 009 plan D9).
pub fn issue_settings(file: Option<&Path>) -> Result<ResolvedSettings, ExitCode> {
    let Some(file) = file else {
        return Ok(ResolvedSettings::default());
    };
    let parsed = std::fs::read_to_string(file)
        .map_err(|e| e.to_string())
        .and_then(|s| serde_json::from_str(&s).map_err(|e| e.to_string()));
    match parsed {
        Ok(s) => {
            let (resolved, notices) = resolve(&s);
            for n in notices {
                eprintln!(
                    "{}: {}: {}",
                    if n.error { "error" } else { "note" },
                    n.rule,
                    n.message
                );
            }
            Ok(resolved)
        }
        Err(e) => {
            eprintln!("error: --issues {}: {e}", file.display());
            Err(ExitCode::from(exit::USAGE))
        }
    }
}

fn open(file: &Path, settings: ResolvedSettings) -> Result<Reader, ExitCode> {
    Reader::open_with(file, settings).map_err(|e| {
        eprintln!("error: {}: {e}", file.display());
        ExitCode::from(UNREADABLE)
    })
}

pub struct HtmlArgs {
    pub out: PathBuf,
    pub title: Option<String>,
    pub note: Option<String>,
    pub urls_per_issue: u32,
    pub force: bool,
}

pub fn run_audit(file: &Path, issues: Option<&Path>, json: bool) -> ExitCode {
    let settings = match issue_settings(issues) {
        Ok(s) => s,
        Err(c) => return c,
    };
    let mut reader = match open(file, settings) {
        Ok(r) => r,
        Err(c) => return c,
    };
    let report = match reader.report(&ReportOptions::default()) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::from(UNREADABLE);
        }
    };
    if json {
        crate::print_json(&report);
    } else {
        print_audit(&report);
    }
    ExitCode::from(exit::FETCHED)
}

fn print_audit(r: &Report) {
    let mut o = std::io::stdout().lock();
    let s = &r.score;
    let _ = writeln!(
        o,
        "Health score: {}{}",
        s.value.map_or("-".into(), |v| v.to_string()),
        if r.provisional { " (provisional)" } else { "" }
    );
    let _ = writeln!(o, "Scored pages: {}", s.pages);
    for p in &s.by_priority {
        let _ = writeln!(
            o,
            "  {:<12} {:>6} pages  -{:.1}",
            p.priority.label(),
            p.pages,
            p.points
        );
    }
    if let Some(inc) = &r.incomplete {
        let _ = writeln!(o, "Incomplete: {} URLs not crawled", inc.not_crawled);
        for c in &inc.reasons {
            let _ = writeln!(o, "  {:<24} {}", c.label, c.count);
        }
    }
    for sec in &r.sections {
        let _ = writeln!(o, "\n## {}", sec.title);
        if let SectionState::Unavailable { reason } = &sec.state {
            let _ = writeln!(o, "  Not available: {reason}");
            continue;
        }
        for b in &sec.blocks {
            match b {
                Block::Figures { items } => {
                    for f in items {
                        let _ = writeln!(o, "  {:<36} {}", f.label, f.value);
                    }
                }
                Block::Table { columns, rows } => {
                    let _ = writeln!(o, "  {}", columns.join("\t"));
                    for row in rows {
                        let cells: Vec<_> = row.iter().map(|c| c.text.as_str()).collect();
                        let _ = writeln!(o, "  {}", cells.join("\t"));
                    }
                }
                Block::Bars { items } => {
                    for bar in items {
                        let _ = writeln!(o, "  {:<12} {}", bar.label, bar.value);
                    }
                }
                Block::Text { text } => {
                    let _ = writeln!(o, "  {text}");
                }
                Block::Issue { issue: i } => {
                    let count = i.count.map_or("-".into(), |c| c.to_string());
                    let _ = writeln!(o, "  [{}] {:<44} {:>6} pages", i.priority.label(), i.name, count);
                }
            }
        }
    }
}

pub fn run_html(file: &Path, issues: Option<&Path>, a: HtmlArgs) -> ExitCode {
    let opts = FileOptions {
        title: a.title.unwrap_or_default(),
        note: a.note.unwrap_or_default(),
        urls_per_issue: a.urls_per_issue,
    };
    if let Err(m) = opts.validate() {
        eprintln!("error: {m}");
        return ExitCode::from(exit::USAGE);
    }
    if a.out.exists() && !a.force {
        eprintln!(
            "error: {} already exists; use --force to replace it",
            a.out.display()
        );
        return ExitCode::from(exit::USAGE);
    }
    let settings = match issue_settings(issues) {
        Ok(s) => s,
        Err(c) => return c,
    };
    let mut reader = match open(file, settings) {
        Ok(r) => r,
        Err(c) => return c,
    };
    let html = match reader.report_html(&ReportOptions::default(), &opts) {
        Ok(h) => h,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::from(UNREADABLE);
        }
    };
    if let Err(e) = write_atomic(&a.out, html.as_bytes()) {
        eprintln!("error: could not write {}: {e}", a.out.display());
        return ExitCode::from(LOCAL);
    }
    println!("{}", a.out.display());
    ExitCode::from(exit::FETCHED)
}

/// Writes next to the target and renames, so no partial file is left.
fn write_atomic(out: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let name = out
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let tmp = out.with_file_name(format!(".{name}.partial"));
    let r = std::fs::write(&tmp, bytes).and_then(|()| std::fs::rename(&tmp, out));
    if r.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    r
}

pub struct BulkArgs {
    pub tables: Vec<String>,
    pub xlsx: bool,
    pub out: PathBuf,
    pub force: bool,
}

pub fn run_bulk(file: &Path, issues: Option<&Path>, a: BulkArgs) -> ExitCode {
    let settings = match issue_settings(issues) {
        Ok(s) => s,
        Err(c) => return c,
    };
    let opts = BulkOptions {
        tables: a.tables,
        format: if a.xlsx { BulkFormat::Xlsx } else { BulkFormat::Csv },
        out: a.out,
        overwrite: a.force,
    };
    let never = AtomicBool::new(false);
    let result = bulk::run_bulk(
        file,
        &settings,
        &opts,
        &mut |p| {
            eprintln!(
                "[{}/{}] {}: {} rows",
                p.tables_done, p.tables_total, p.table, p.rows
            )
        },
        &never,
    );
    match result {
        Ok(r) => {
            for f in &r.files {
                println!("{}", f.display());
            }
            let rows: u64 = r.tables.iter().map(|t| t.rows).sum();
            eprintln!("{rows} rows in {} tables", r.tables.len());
            if r.truncated_cells > 0 {
                eprintln!("{} cells cut at the Excel cell limit", r.truncated_cells);
            }
            if r.split_sheets > 0 {
                eprintln!("{} tables continued on extra sheets", r.split_sheets);
            }
            ExitCode::from(exit::FETCHED)
        }
        Err(ExportError::Exists(p)) => {
            eprintln!("error: {} already exists; use --force to replace it", p.display());
            ExitCode::from(exit::USAGE)
        }
        Err(ExportError::Results(Error::Invalid(m))) => {
            eprintln!("error: {m}");
            ExitCode::from(exit::USAGE)
        }
        Err(ExportError::Io(e)) => {
            eprintln!("error: could not write the export: {e}");
            ExitCode::from(LOCAL)
        }
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::from(UNREADABLE)
        }
    }
}
