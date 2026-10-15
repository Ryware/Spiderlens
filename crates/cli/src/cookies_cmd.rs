//! `spiderlens cookies`: the Cookies view of a crawl file (spec 025
//! FR-027). Values are never stored, so none are printed.

use std::io::Write;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Args, ValueEnum};
use spiderlens_results::{CookieFilter, CookieRow, Reader};

use crate::exit;

#[derive(Args)]
pub struct CookiesArgs {
    /// Crawl file.
    file: PathBuf,
    /// Only cookies matching this filter.
    #[arg(long, value_enum, default_value = "all")]
    filter: FilterArg,
    /// The cookies one URL's response set, instead of all distinct cookies.
    #[arg(long, value_name = "URL")]
    url: Option<String>,
    /// Print JSON.
    #[arg(long)]
    json: bool,
}

#[derive(Clone, Copy, ValueEnum)]
enum FilterArg {
    All,
    Session,
    Persistent,
    ThirdParty,
    MissingSecure,
    MissingHttponly,
    SamesiteNone,
}

impl From<FilterArg> for CookieFilter {
    fn from(f: FilterArg) -> Self {
        match f {
            FilterArg::All => Self::All,
            FilterArg::Session => Self::Session,
            FilterArg::Persistent => Self::Persistent,
            FilterArg::ThirdParty => Self::ThirdParty,
            FilterArg::MissingSecure => Self::MissingSecure,
            FilterArg::MissingHttponly => Self::MissingHttpOnly,
            FilterArg::SamesiteNone => Self::SameSiteNone,
        }
    }
}

pub fn run_cookies(args: CookiesArgs) -> ExitCode {
    let reader = match Reader::open(&args.file) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::from(exit::USAGE);
        }
    };
    let rows = match &args.url {
        Some(url) => {
            let (store, url) = match crate::crawl_cmd::open_for_page(&args.file, url) {
                Ok(x) => x,
                Err(code) => return code,
            };
            let id = match store.id_of(&url) {
                Ok(Some(id)) => id,
                Ok(None) => return ExitCode::FAILURE,
                Err(e) => {
                    eprintln!("error: {e}");
                    return ExitCode::FAILURE;
                }
            };
            reader.url_cookies(id)
        }
        None => reader
            .cookies(args.filter.into(), None, true, 0, u32::MAX)
            .map(|p| p.rows),
    };
    let rows = match rows {
        Ok(r) => r,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };
    let mut out = std::io::stdout().lock();
    if args.json {
        let _ = writeln!(
            out,
            "{}",
            serde_json::to_string_pretty(&rows).expect("rows serialize")
        );
    } else {
        print_table(&mut out, &rows, args.url.is_none());
    }
    ExitCode::from(exit::FETCHED)
}

fn print_table(out: &mut impl Write, rows: &[CookieRow], with_count: bool) {
    let _ = writeln!(
        out,
        "Name\tDomain\tPath\tLifetime\tSecure\tHttpOnly\tSameSite\tParty{}",
        if with_count { "\tURLs" } else { "" }
    );
    let yes = |b: bool| if b { "yes" } else { "no" };
    for c in rows {
        let lifetime = match c.expires {
            None => "session".to_string(),
            Some(ms) => jiff::Timestamp::from_millisecond(ms)
                .map(|t| t.strftime("%Y-%m-%d").to_string())
                .unwrap_or_else(|_| "persistent".into()),
        };
        let _ = write!(
            out,
            "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
            c.name,
            c.domain,
            c.path,
            lifetime,
            yes(c.secure),
            yes(c.http_only),
            c.same_site.as_deref().unwrap_or("-"),
            if c.third_party { "third" } else { "first" },
        );
        if with_count {
            let _ = write!(out, "\t{}", c.urls);
        }
        let _ = writeln!(out);
    }
}
