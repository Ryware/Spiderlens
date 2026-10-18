//! `spiderlens integrations fetch --provider ahrefs|majestic|moz` and
//! `integrations export-sheets` (spec 024 US5 and US6,
//! specs/024-external-integrations/contracts/cli.md).

use std::io::{IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::atomic::AtomicBool;

use clap::Args;
use spiderlens_integrations::hub::Hub;
use spiderlens_integrations::links::{LinkRequest, UrlSet};
use spiderlens_integrations::sheets::Overflow;
use spiderlens_integrations::store::RunState;
use spiderlens_integrations::{Error, Provider};
use spiderlens_results::{Sort, UrlScope, ViewQuery, export_file};

use crate::exit;
use crate::export_cmd::ScopeArg;
use crate::integrations_cmd::{QUOTA, finish, pause_on_ctrl_c, print_json, progress};

/// Options of `fetch` for link-data providers.
#[derive(Args)]
pub struct LinkArgs {
    /// Link metrics: metric ids, comma separated (default: all the provider offers).
    #[arg(long, value_delimiter = ',')]
    metrics: Vec<String>,
    /// Link metrics: stop after this many of the provider's units (rows for Majestic and Moz).
    #[arg(long)]
    cap: Option<u64>,
    /// Link metrics: start without asking after showing the estimate.
    #[arg(long)]
    yes: bool,
}

#[allow(clippy::too_many_arguments)]
pub fn fetch_links(
    h: &Hub,
    p: Provider,
    file: &Path,
    key: Option<&str>,
    urls: &str,
    a: &LinkArgs,
    refresh: bool,
    json: bool,
) -> Result<ExitCode, Error> {
    let req = LinkRequest {
        provider: p,
        url_set: match urls {
            "indexable" => UrlSet::Indexable,
            "all" => UrlSet::All,
            other => {
                return Err(Error::Invalid(format!("--urls {other}: link metrics take indexable or all")));
            }
        },
        metrics: a.metrics.clone(),
        cap: a.cap,
        refresh,
    };
    let e = h.estimate_links(file, key, &req)?;
    eprintln!(
        "{}: {} pages ({} cached), about {} {} in {} requests. Cap: {} {}.",
        p.label(),
        e.urls,
        e.cached,
        e.units,
        e.unit,
        e.requests,
        e.cap,
        e.unit
    );
    eprintln!("{}", p.sends());
    if e.units > 0 && !a.yes {
        if !std::io::stdin().is_terminal() {
            return Err(Error::Invalid(
                "this run uses the provider's units; add --yes to start it".into(),
            ));
        }
        eprint!("Start the run? [y/N] ");
        std::io::stderr().flush().ok();
        let mut line = String::new();
        std::io::stdin().read_line(&mut line)?;
        if !matches!(line.trim().to_ascii_lowercase().as_str(), "y" | "yes") {
            eprintln!("Nothing was sent.");
            return Ok(ExitCode::from(exit::FETCHED));
        }
    }
    let control = pause_on_ctrl_c();
    let r = h.fetch_links(file, key, &req, &control, &mut progress())?;
    let code = finish(&r, json);
    // Stopped at the cap (or the re-estimate): fewer pages than asked for.
    if r.state == RunState::Finished && r.done < r.requested {
        return Ok(ExitCode::from(QUOTA));
    }
    Ok(code)
}

#[derive(Args)]
pub struct SheetsArgs {
    /// The crawl file.
    file: PathBuf,
    /// Tab id, for example page-titles.
    #[arg(long, default_value = "internal")]
    tab: String,
    /// Filter id within the tab.
    #[arg(long, default_value = "all")]
    filter: String,
    /// Internal or external URLs only.
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
    /// The Google account to use when several are connected.
    #[arg(long)]
    account: Option<String>,
    /// The spreadsheet's title (default: the crawl file's name and the tab).
    #[arg(long)]
    title: Option<String>,
    /// Too many cells for one spreadsheet: export the first rows that fit.
    #[arg(long, conflicts_with = "split")]
    truncate: bool,
    /// Too many cells for one spreadsheet: use several.
    #[arg(long)]
    split: bool,
    /// Print the Looker Studio link that opens a report on the new sheet.
    #[arg(long)]
    looker: bool,
    /// Also open that link in the browser.
    #[arg(long, requires = "looker")]
    open: bool,
    /// Print the result as JSON.
    #[arg(long)]
    json: bool,
}

pub fn export_sheets(h: &Hub, a: SheetsArgs) -> Result<ExitCode, Error> {
    let sort = a.sort.as_ref().map(|s| match s.strip_suffix(":desc") {
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
        tab: a.tab.clone(),
        filter: a.filter.clone(),
        scope: match a.scope {
            ScopeArg::All => UrlScope::All,
            ScopeArg::Internal => UrlScope::Internal,
            ScopeArg::External => UrlScope::External,
        },
        search: a.search.clone(),
        values: None,
        sort,
        segment: None,
    };
    let csv = std::env::temp_dir().join(format!("spiderlens-sheets-{}.csv", std::process::id()));
    let never = AtomicBool::new(false);
    let exported = export_file(&a.file, &q, &a.columns, None, None, &csv, &never, &mut |_, _| {});
    let result = exported
        .map_err(|e| Error::Invalid(e.to_string()))
        .and_then(|_| send(h, &a, &csv));
    let _ = std::fs::remove_file(&csv);
    result
}

fn send(h: &Hub, a: &SheetsArgs, csv: &Path) -> Result<ExitCode, Error> {
    let plan = h.sheets_plan(csv)?;
    let overflow = match (a.truncate, a.split) {
        (true, _) => Overflow::Truncate,
        (_, true) => Overflow::Split,
        _ => Overflow::Refuse,
    };
    if let Some(w) = plan.warning() {
        eprintln!("{w}");
        if overflow == Overflow::Refuse {
            eprintln!("Nothing was sent. Run again with --truncate or --split.");
            return Ok(ExitCode::from(exit::USAGE));
        }
    }
    let stem = a.file.file_stem().and_then(|s| s.to_str()).unwrap_or("crawl");
    let title = a.title.clone().unwrap_or_else(|| format!("{stem} - {}", a.tab));
    let control = pause_on_ctrl_c();
    let out = h.export_sheets(
        csv,
        &title,
        &a.tab,
        a.account.as_deref(),
        overflow,
        &control,
        &mut |_, _| {},
    )?;
    let link = match (a.looker, out.spreadsheets.first()) {
        (true, Some(first)) => Some(h.looker_link(first)?),
        _ => None,
    };
    if a.json {
        print_json(&serde_json::json!({ "export": out, "looker": link }));
    } else {
        for s in &out.spreadsheets {
            println!("{}  ({} rows)", s.url, s.rows);
        }
        if out.truncated {
            println!("Only {} of {} rows were exported.", out.rows, out.total_rows);
        }
        if let Some(l) = &link {
            println!("Looker Studio: {l}");
        }
    }
    if let (Some(l), true) = (&link, a.open) {
        crate::integrations_cmd::open_browser(l);
    }
    Ok(ExitCode::from(exit::FETCHED))
}
