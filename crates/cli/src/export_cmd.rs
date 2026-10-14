//! `spiderlens export`: any tab and filter of a crawl file as CSV, with the
//! same rows and columns as the desktop app (spec 007 FR-031,
//! specs/007-results-ui/contracts/cli.md).

use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::atomic::AtomicBool;

use clap::{Args, ValueEnum};
use spiderlens_results::{Error, ExportError, Sort, UrlScope, ViewQuery, export_file, tabs, write_csv};

use crate::exit;

/// Exit code when the crawl file cannot be opened (contracts/cli.md).
const UNREADABLE: u8 = 3;
/// Exit code when the filter needs data this crawl file does not have.
const UNAVAILABLE: u8 = 4;

#[derive(Clone, Copy, ValueEnum)]
pub enum ScopeArg {
    All,
    Internal,
    External,
}

#[derive(Args)]
pub struct ExportArgs {
    /// The crawl file.
    #[arg(required_unless_present = "list_tabs")]
    file: Option<PathBuf>,
    /// Tab id, for example page-titles (see --list-tabs).
    #[arg(long, default_value = "internal")]
    tab: String,
    /// Filter id within the tab.
    #[arg(long, default_value = "all")]
    filter: String,
    /// Internal or external URLs only (Response codes tab).
    #[arg(long, value_enum, default_value = "all")]
    scope: ScopeArg,
    /// Keep rows whose URL or main text contains this, ignoring case.
    #[arg(long, default_value = "")]
    search: String,
    /// Sort column id, with :desc for descending.
    #[arg(long, value_name = "COLUMN[:desc]")]
    sort: Option<String>,
    /// Column ids, comma separated (default: the tab's default columns).
    #[arg(long, value_delimiter = ',')]
    columns: Vec<String>,
    /// Write to this file instead of standard output.
    #[arg(long, short)]
    out: Option<PathBuf>,
    /// List tabs, filters and column ids.
    #[arg(long)]
    list_tabs: bool,
    /// With --list-tabs: print as JSON.
    #[arg(long)]
    json: bool,
}

pub fn run_export(a: ExportArgs) -> ExitCode {
    if a.list_tabs {
        if a.json {
            crate::print_json(&tabs());
        } else {
            for t in tabs() {
                println!("{}  ({})", t.id, t.title);
                let filters: Vec<_> = t.filters.iter().map(|f| f.id).collect();
                let columns: Vec<_> = t.columns.iter().map(|c| c.id).collect();
                println!("  filters: {}", filters.join(", "));
                println!("  columns: {}", columns.join(", "));
            }
        }
        return ExitCode::from(exit::FETCHED);
    }
    let file = a.file.expect("required by clap");
    let sort = a.sort.map(|s| match s.strip_suffix(":desc") {
        Some(c) => Sort {
            column: c.into(),
            desc: true,
        },
        None => Sort {
            column: s.trim_end_matches(":asc").into(),
            desc: false,
        },
    });
    let q = ViewQuery {
        tab: a.tab,
        filter: a.filter,
        scope: match a.scope {
            ScopeArg::All => UrlScope::All,
            ScopeArg::Internal => UrlScope::Internal,
            ScopeArg::External => UrlScope::External,
        },
        search: a.search,
        values: None,
        sort,
    };
    let never = AtomicBool::new(false);
    let result = match &a.out {
        Some(out) => export_file(&file, &q, &a.columns, None, out, &never, &mut |_, _| {}),
        None => write_csv(
            &file,
            &q,
            &a.columns,
            None,
            std::io::stdout().lock(),
            &never,
            &mut |_, _| {},
        ),
    };
    match result {
        Ok(n) => {
            if a.out.is_some() {
                eprintln!("{n} rows exported");
            }
            ExitCode::from(exit::FETCHED)
        }
        Err(ExportError::Results(Error::Invalid(m))) if m.starts_with("unknown") => {
            eprintln!("error: {m}. Run `spiderlens export --list-tabs` for the valid ids.");
            ExitCode::from(exit::USAGE)
        }
        Err(ExportError::Results(Error::Unavailable(m))) => {
            eprintln!("error: {m}");
            ExitCode::from(UNAVAILABLE)
        }
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::from(UNREADABLE)
        }
    }
}
