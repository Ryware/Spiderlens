//! `spiderlens links`: Crawl Analysis on a crawl file and the link position
//! tester (specs/023-link-score/contracts/cli.md).

use std::io::Write;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Args, Subcommand};
use spiderlens_config::{PositionRule, check_selector};
use spiderlens_results::Reader;

use crate::exit;

/// Exit code when the crawl file cannot be opened (contracts/cli.md).
const UNREADABLE: u8 = 3;

#[derive(Subcommand)]
pub enum LinksCommand {
    /// Work out link positions and Link Score and print a summary.
    ///
    /// Export the scores with `spiderlens export --tab links`.
    Analyse(AnalyseArgs),
    /// Print where each link on one page sits, and which rule decided.
    Positions(PositionsArgs),
}

#[derive(Args)]
pub struct AnalyseArgs {
    /// The crawl file.
    file: PathBuf,
    /// Print nothing on success.
    #[arg(long)]
    quiet: bool,
    /// Print the summary as JSON.
    #[arg(long, conflicts_with = "quiet")]
    json: bool,
}

#[derive(Args)]
pub struct PositionsArgs {
    /// The crawl file.
    file: PathBuf,
    /// The page, as crawled.
    #[arg(long)]
    url: String,
    /// JSON file with the rules to try, for example
    /// [{"selector": ".mega-menu", "position": "navigation"}].
    /// Default: the rules stored with the crawl.
    #[arg(long, value_name = "FILE")]
    rules: Option<PathBuf>,
}

pub fn run(cmd: LinksCommand) -> ExitCode {
    match cmd {
        LinksCommand::Analyse(a) => analyse(a),
        LinksCommand::Positions(a) => positions(a),
    }
}

fn open(file: &std::path::Path) -> Result<Reader, ExitCode> {
    Reader::open(file).map_err(|e| {
        eprintln!("error: cannot open {}: {e}", file.display());
        ExitCode::from(UNREADABLE)
    })
}

fn analyse(a: AnalyseArgs) -> ExitCode {
    let mut r = match open(&a.file) {
        Ok(r) => r,
        Err(code) => return code,
    };
    let run = match r.analyse() {
        Ok(run) => run,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };
    if a.json {
        crate::print_json(&run);
    } else if !a.quiet {
        println!(
            "Link Score worked out for {} URLs from {} links in {} rounds ({} ms).",
            run.urls, run.links, run.rounds, run.ms
        );
        if run.weighted {
            println!("Links are weighted by position.");
        }
        if run.pages_without_html > 0 {
            println!(
                "{} pages had no stored HTML; their links count as position unknown.",
                run.pages_without_html
            );
        }
    }
    ExitCode::from(exit::FETCHED)
}

fn read_rules(path: &std::path::Path) -> Result<Vec<PositionRule>, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    let rules: Vec<PositionRule> =
        serde_json::from_slice(&bytes).map_err(|e| format!("{}: {e}", path.display()))?;
    for (i, rule) in rules.iter().enumerate() {
        check_selector(&rule.selector).map_err(|e| format!("rule {}: {e}", i + 1))?;
    }
    Ok(rules)
}

fn positions(a: PositionsArgs) -> ExitCode {
    let rules = match a.rules.as_deref().map(read_rules).transpose() {
        Ok(r) => r,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::from(exit::USAGE);
        }
    };
    let r = match open(&a.file) {
        Ok(r) => r,
        Err(code) => return code,
    };
    let rules = rules.unwrap_or_else(|| r.link_settings().position_rules.clone());
    let links = match r.test_positions_url(&a.url, &rules) {
        Ok(Some(links)) => links,
        Ok(None) => {
            eprintln!(
                "error: {} is not in {}, or its HTML was not stored",
                a.url,
                a.file.display()
            );
            return ExitCode::FAILURE;
        }
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };
    let mut out = std::io::stdout().lock();
    for l in links {
        let rule = l
            .rule
            .map_or("built-in".to_string(), |i| format!("rule {}", i + 1));
        let text = l.text.unwrap_or_default().replace(['\t', '\n'], " ");
        let _ = writeln!(
            out,
            "{}\t{}\t{rule}\t{}\t{text}",
            l.pos,
            l.position.as_str(),
            l.raw
        );
    }
    ExitCode::from(exit::FETCHED)
}
