//! `spiderlens` command-line tool. A thin layer over `spiderlens-engine` and
//! `spiderlens-config`: it parses arguments, calls them and prints results.
//! See specs/001-project-foundation/contracts/cli.md and
//! specs/002-crawl-configuration/contracts/cli.md.

mod crawl_cmd;
mod export_cmd;
mod files_cmd;
mod options;

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Args, CommandFactory, FromArgMatches, Parser, Subcommand};
use crawl_cmd::{CrawlCmd, HtmlArgs, InfoArgs, PageArgs, ReportArgs, ScopeTestArgs};
use options::{CrawlArgs, IdentityArgs, NetworkArgs};
use spiderlens_config::{
    CrawlConfig, Decision, Issue, ListFormat, Mode, ModeInput, Severity, export_profile, parse_list_bytes,
    test_patterns, validate, validate_config,
};
use spiderlens_engine::{
    BuildInfo, CheckOptions, ErrorClass, FinalState, PageCheckResult, Title, check_page,
};

/// Exit codes from the CLI contract.
pub(crate) mod exit {
    pub const FETCHED: u8 = 0;
    pub const USAGE: u8 = 2;
    pub const BLOCKED: u8 = 3;
    pub const FAILED: u8 = 4;
    /// The crawl was paused (Ctrl-C) and can be resumed.
    pub const PAUSED: u8 = 5;
    /// A disk or write problem paused the crawl; it can be resumed (spec 006 FR-040).
    pub const LOCAL: u8 = 6;
}

const BUILD: BuildInfo = BuildInfo::current();

#[derive(Parser)]
#[command(name = "spiderlens", about = "SpiderLens SEO crawler", version)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Check a single page: robots.txt, redirects, status and title.
    Check(CheckArgs),
    /// Show, validate and test crawl configurations.
    // Boxed: the shared crawl options make this variant large, and on Windows
    // (larger path types) clippy's large_enum_variant lint fires.
    #[command(subcommand)]
    Config(Box<ConfigCommand>),
    /// Crawl a site (Spider mode) or a URL list (List mode) into a crawl file.
    Crawl(Box<CrawlCmd>),
    /// List the URLs in a crawl file, or its summary.
    Report(ReportArgs),
    /// Show a crawl file's configuration, status, versions and history.
    Info(InfoArgs),
    /// Show everything extracted for one URL of a crawl file.
    Page(PageArgs),
    /// Write the stored raw HTML of one page of a crawl file.
    Html(HtmlArgs),
    /// List crawl files in a folder with their size and stored HTML.
    Files(files_cmd::FilesArgs),
    /// Remove the stored page HTML from a crawl file, keeping its results.
    PurgeHtml(files_cmd::PurgeHtmlArgs),
    /// Export a tab and filter of a crawl file as CSV (same rows as the app).
    Export(Box<export_cmd::ExportArgs>),
    /// Explain crawl scope decisions.
    #[command(subcommand)]
    Scope(ScopeCommand),
    /// Print product version, engine version and build identifier.
    Version {
        /// Print as JSON.
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand)]
enum ScopeCommand {
    /// Would these URLs be crawled from this start URL, and if not, why?
    Test(Box<ScopeTestArgs>),
}

#[derive(Args)]
struct CheckArgs {
    /// Address to check. https:// is assumed when no scheme is given.
    url: String,
    /// Print the result as one JSON object.
    #[arg(long)]
    json: bool,
    #[command(flatten)]
    identity: IdentityArgs,
    #[command(flatten)]
    network: NetworkArgs,
}

#[derive(Subcommand)]
enum ConfigCommand {
    /// Print the resolved configuration and any issues.
    Show(ConfigArgs),
    /// Print only the issues; exit 2 if any is an error.
    Validate(ConfigArgs),
    /// Print the built-in defaults as a profile file.
    Defaults,
    /// Show which sample URLs the patterns would crawl, and why.
    TestPatterns {
        /// Include pattern (regex). Repeatable.
        #[arg(long, value_name = "REGEX")]
        include: Vec<String>,
        /// Exclude pattern (regex). Repeatable.
        #[arg(long, value_name = "REGEX")]
        exclude: Vec<String>,
        /// Print as JSON.
        #[arg(long)]
        json: bool,
        /// Sample URLs.
        #[arg(required = true)]
        urls: Vec<String>,
    },
}

#[derive(Args)]
struct ConfigArgs {
    /// Start URL (Spider mode).
    url: Option<String>,
    /// URL list file, text or CSV (List mode).
    #[arg(long, value_name = "FILE", conflicts_with = "url")]
    list: Option<PathBuf>,
    /// Print as JSON.
    #[arg(long)]
    json: bool,
    #[command(flatten)]
    crawl: CrawlArgs,
}

fn main() -> ExitCode {
    let matches = Cli::command().version(BuildInfo::VERSION_LINE).get_matches();
    let cli = Cli::from_arg_matches(&matches).unwrap_or_else(|e| e.exit());
    match cli.command {
        Command::Version { json } => {
            if json {
                print_json(&BUILD);
            } else {
                println!("{BUILD}");
            }
            ExitCode::from(exit::FETCHED)
        }
        Command::Check(args) => run_check(args),
        Command::Config(cmd) => run_config(*cmd),
        Command::Crawl(cmd) => {
            let sub = matches.subcommand_matches("crawl").expect("crawl matches");
            crawl_cmd::run_crawl(*cmd, sub)
        }
        Command::Report(args) => crawl_cmd::run_report(args),
        Command::Info(args) => crawl_cmd::run_info(args),
        Command::Page(args) => crawl_cmd::run_page(args),
        Command::Html(args) => crawl_cmd::run_html(args),
        Command::Files(args) => files_cmd::run_files(args),
        Command::PurgeHtml(args) => files_cmd::run_purge_html(args),
        Command::Export(args) => export_cmd::run_export(*args),
        Command::Scope(ScopeCommand::Test(args)) => crawl_cmd::run_scope_test(*args),
    }
}

fn run_check(args: CheckArgs) -> ExitCode {
    let mut config = CrawlConfig::default();
    if let Err(e) = args.identity.apply(&mut config) {
        eprintln!("error: {e}");
        return ExitCode::from(exit::USAGE);
    }
    args.network.apply(&mut config);
    // Only identity and network settings apply to a single-page check.
    let issues: Vec<_> = validate_config(&config)
        .into_iter()
        .filter(|i| i.path.starts_with("identity") || i.path.starts_with("network"))
        .collect();
    if print_issues(&issues) {
        return ExitCode::from(exit::USAGE);
    }
    let opts = CheckOptions::from(&config);

    let runtime = match tokio::runtime::Builder::new_current_thread().enable_all().build() {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("error: could not start: {e}");
            return ExitCode::FAILURE;
        }
    };
    let result = runtime.block_on(check_page(&args.url, &opts));

    if args.json {
        print_json(&result);
    } else if let Some(e) = result
        .error
        .as_ref()
        .filter(|e| e.class == ErrorClass::MalformedUrl)
    {
        eprintln!("error: {}", e.message);
    } else {
        print_human(&result);
    }
    ExitCode::from(exit_code(&result))
}

/// Prints issues to stderr. Returns true if any blocks the run.
pub(crate) fn print_issues(issues: &[Issue]) -> bool {
    for i in issues {
        let level = match i.severity {
            Severity::Error => "error",
            Severity::Warning => "warning",
        };
        eprintln!("{level}: {}: {}", i.path, i.message);
    }
    issues.iter().any(|i| i.severity == Severity::Error)
}

fn run_config(cmd: ConfigCommand) -> ExitCode {
    match cmd {
        ConfigCommand::Defaults => {
            println!("{}", export_profile("Defaults", &CrawlConfig::default(), None));
            ExitCode::from(exit::FETCHED)
        }
        ConfigCommand::TestPatterns {
            include,
            exclude,
            json,
            urls,
        } => {
            let rows = test_patterns(&include, &exclude, &urls);
            if json {
                print_json(&rows);
            } else {
                for r in &rows {
                    let pattern = r.pattern.as_deref().unwrap_or("");
                    let verdict = match (&r.error, r.decision) {
                        (Some(e), _) => format!("invalid: {e}"),
                        (None, Some(Decision::Crawl { include: Some(_) })) => {
                            format!("crawled (include {pattern})")
                        }
                        (None, Some(Decision::Crawl { include: None })) => "crawled".into(),
                        (None, Some(Decision::Excluded { .. })) => format!("excluded (exclude {pattern})"),
                        (None, _) => "not included (matches no include pattern)".into(),
                    };
                    println!("{}  {verdict}", r.url.as_deref().unwrap_or(&r.input));
                }
            }
            ExitCode::from(exit::FETCHED)
        }
        ConfigCommand::Show(args) => show_config(args, true),
        ConfigCommand::Validate(args) => show_config(args, false),
    }
}

fn show_config(args: ConfigArgs, print_config: bool) -> ExitCode {
    let mode = if args.list.is_some() {
        Mode::List
    } else {
        Mode::Spider
    };
    let resolved = match args.crawl.resolve(mode) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::from(exit::USAGE);
        }
    };
    for n in &resolved.notices {
        eprintln!("notice: {n}");
    }
    let input = match read_input(args.url.as_deref(), args.list.as_deref()) {
        Ok(i) => i,
        Err(code) => return code,
    };
    let issues = match &input {
        Some(i) => validate(&resolved.config, i),
        None => validate_config(&resolved.config),
    };
    if print_config {
        if args.json {
            print_json(&serde_json::json!({ "config": resolved.config, "issues": issues }));
        } else {
            print_json(&resolved.config);
        }
    } else if args.json {
        print_json(&issues);
    }
    if print_issues(&issues) {
        ExitCode::from(exit::USAGE)
    } else {
        ExitCode::from(exit::FETCHED)
    }
}

/// Reads the Spider start URL or the List file into a mode input.
pub(crate) fn read_input(url: Option<&str>, list: Option<&Path>) -> Result<Option<ModeInput>, ExitCode> {
    match (url, list) {
        (Some(url), _) => Ok(Some(ModeInput::Spider {
            start_url: url.to_string(),
        })),
        (None, Some(path)) => match std::fs::read(path) {
            Ok(bytes) => {
                let format = ListFormat::from_file_name(&path.to_string_lossy());
                let list = parse_list_bytes(&bytes, format, &mut |_| {});
                eprintln!(
                    "list: {} URLs to crawl, {} duplicates removed, {} invalid",
                    list.urls.len(),
                    list.duplicates,
                    list.invalid_count
                );
                Ok(Some(ModeInput::List(list)))
            }
            Err(e) => {
                eprintln!("error: cannot read {}: {e}", path.display());
                Err(ExitCode::from(exit::USAGE))
            }
        },
        (None, None) => Ok(None),
    }
}

fn exit_code(r: &PageCheckResult) -> u8 {
    match r.state {
        FinalState::Fetched => exit::FETCHED,
        FinalState::BlockedByRobots => exit::BLOCKED,
        FinalState::Failed => match r.error.as_ref().map(|e| e.class) {
            Some(ErrorClass::MalformedUrl) => exit::USAGE,
            _ => exit::FAILED,
        },
    }
}

pub(crate) fn print_json<T: serde::Serialize>(value: &T) {
    let mut out = std::io::stdout().lock();
    // Serializing our own types cannot fail; a closed pipe is not worth a panic.
    let _ = serde_json::to_writer_pretty(&mut out, value);
    let _ = writeln!(out);
}

fn print_human(r: &PageCheckResult) {
    let mut lines: Vec<(&str, String)> = vec![("URL", r.requested_url.clone())];
    for hop in &r.redirects {
        lines.push(("Redirect", format!("{} → {}", hop.status, hop.location)));
    }
    if let Some(final_url) = &r.final_url {
        lines.push(("Final URL", final_url.clone()));
    }
    if let Some(status) = r.status {
        lines.push(("Status", status.to_string()));
    }
    if let Some(ct) = &r.content_type {
        lines.push(("Type", ct.clone()));
    }
    if let Some(ms) = r.response_time_ms {
        lines.push(("Time", format!("{ms} ms")));
    }
    if let Some(title) = &r.title {
        lines.push((
            "Title",
            match title {
                Title::Text(t) => t.clone(),
                Title::Missing => "(no title)".into(),
                Title::NotApplicable => "(not applicable)".into(),
            },
        ));
    }
    lines.push((
        "State",
        match r.state {
            FinalState::Fetched => "fetched",
            FinalState::BlockedByRobots => "blocked by robots.txt",
            FinalState::Failed => "failed",
        }
        .into(),
    ));
    if let Some(e) = &r.error {
        lines.push(("Error", format!("{}: {}", e.class, e.message)));
        lines.push(("Attempts", e.attempts.to_string()));
    }

    let mut out = std::io::stdout().lock();
    for (label, value) in lines {
        let _ = writeln!(out, "{label:<12} {value}");
    }
}
