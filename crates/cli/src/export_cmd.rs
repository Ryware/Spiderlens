//! `spiderlens export`: any tab and filter of a crawl file as CSV, with the
//! same rows and columns as the desktop app (spec 007 FR-031,
//! specs/007-results-ui/contracts/cli.md).

use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::atomic::AtomicBool;

use clap::{Args, ValueEnum};
use spiderlens_results::{
    Error, ExportError, Reader, Sort, TabDef, UrlScope, ViewQuery, export_file, tabs, write_csv,
};

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
    /// Only URLs in this segment of the crawl's settings ("none": in no segment).
    #[arg(long, value_name = "NAME")]
    segment: Option<String>,
    /// Sort column id, with :desc for descending.
    #[arg(long, value_name = "COLUMN[:desc]")]
    sort: Option<String>,
    /// Column ids, comma separated (default: the tab's default columns).
    #[arg(long, value_delimiter = ',')]
    columns: Vec<String>,
    /// Custom extraction: every stored value of each extractor, joined by
    /// SEP, instead of the first (spec 013 FR-025).
    #[arg(long, value_name = "SEP")]
    all_values: Option<String>,
    /// Write to this file instead of standard output.
    #[arg(long, short)]
    out: Option<PathBuf>,
    /// List tabs, filters and column ids.
    #[arg(long)]
    list_tabs: bool,
    /// With --list-tabs: print as JSON.
    #[arg(long)]
    json: bool,
    /// Bulk export these tables, comma separated, or all: summary, tabs,
    /// issues, links, images, redirect_chains, headers (spec 010).
    #[arg(long, value_delimiter = ',', value_name = "TABLES", requires = "out")]
    bulk: Vec<String>,
    /// Bulk export format: a CSV folder or one Excel workbook.
    #[arg(long, value_enum, default_value = "csv", requires = "bulk")]
    format: FormatArg,
    /// With --bulk: replace an existing folder or workbook.
    #[arg(long, requires = "bulk")]
    force: bool,
    /// With --bulk: issue settings JSON (rule overrides).
    #[arg(long, value_name = "FILE", requires = "bulk")]
    issues: Option<PathBuf>,
}

#[derive(Clone, Copy, ValueEnum)]
pub enum FormatArg {
    Csv,
    Xlsx,
}

pub fn run_export(a: ExportArgs) -> ExitCode {
    if a.list_tabs {
        // With a crawl file, its custom tabs too (spec 013).
        let all: Vec<TabDef> = match a.file.as_deref().map(Reader::open) {
            Some(Ok(mut r)) => r.tabs().unwrap_or_else(|_| tabs().to_vec()),
            Some(Err(e)) => {
                eprintln!("error: {e}");
                return ExitCode::from(UNREADABLE);
            }
            None => tabs().to_vec(),
        };
        if a.json {
            crate::print_json(&all);
        } else {
            for t in &all {
                println!("{}  ({})", t.id, t.title);
                let filters: Vec<_> = t.filters.iter().map(|f| f.id.as_ref()).collect();
                let columns: Vec<_> = t.columns.iter().map(|c| c.id.as_ref()).collect();
                println!("  filters: {}", filters.join(", "));
                println!("  columns: {}", columns.join(", "));
            }
        }
        return ExitCode::from(exit::FETCHED);
    }
    let file = a.file.expect("required by clap");
    if !a.bulk.is_empty() {
        let b = crate::report_cmd::BulkArgs {
            tables: a.bulk,
            xlsx: matches!(a.format, FormatArg::Xlsx),
            out: a.out.expect("required by clap"),
            force: a.force,
        };
        return crate::report_cmd::run_bulk(&file, a.issues.as_deref(), b);
    }
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
    let segment = match &a.segment {
        None => None,
        Some(name) => match segment_id(&file, name) {
            Ok(id) => Some(id),
            Err(code) => return code,
        },
    };
    let q = ViewQuery {
        tab: a.tab,
        filter: a.filter,
        scope: match a.scope {
            ScopeArg::All => UrlScope::All,
            ScopeArg::Internal => UrlScope::Internal,
            ScopeArg::External => UrlScope::External,
        },
        search: a.search,
        sort,
        segment,
    };
    let never = AtomicBool::new(false);
    let result = match &a.out {
        Some(out) => export_file(
            &file,
            &q,
            &a.columns,
            None,
            a.all_values.as_deref(),
            out,
            &never,
            &mut |_, _| {},
        ),
        None => write_csv(
            &file,
            &q,
            &a.columns,
            None,
            a.all_values.as_deref(),
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

/// Resolves `--segment NAME` against the segments stored with the crawl.
fn segment_id(file: &std::path::Path, name: &str) -> Result<u32, ExitCode> {
    if name.trim().eq_ignore_ascii_case("none") {
        return Ok(0);
    }
    let reader = Reader::open(file).map_err(|e| {
        eprintln!("error: {e}");
        ExitCode::from(UNREADABLE)
    })?;
    reader.segment_id(name).ok_or_else(|| {
        let names: Vec<_> = reader
            .segments()
            .iter()
            .filter(|s| s.enabled)
            .map(|s| s.name.trim())
            .collect();
        if names.is_empty() {
            eprintln!("error: this crawl has no segments");
        } else {
            eprintln!("error: no segment named {name:?}; segments: {}", names.join(", "));
        }
        ExitCode::from(exit::USAGE)
    })
}
