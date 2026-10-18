//! `spiderlens` command-line tool. A thin layer over `spiderlens-engine` and
//! `spiderlens-config`: it parses arguments, calls them and prints results.
//! See specs/001-project-foundation/contracts/cli.md and
//! specs/002-crawl-configuration/contracts/cli.md.

mod accessibility_cmd;
mod aeo_cmd;
mod agents_cmd;
mod browser;
mod compare_cmd;
mod cookies_cmd;
mod crawl_cmd;
mod duplicates_cmd;
mod export_cmd;
mod files_cmd;
mod integrations_cmd;
mod integrations_links_cmd;
mod issues_cmd;
mod links_cmd;
mod options;
mod report_cmd;
mod rules_cmd;
mod schedule_cmd;
mod segments_cmd;
mod serp_cmd;
mod sitemap_cmd;
mod spell_cmd;
mod viz_cmd;

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Args, CommandFactory, FromArgMatches, Parser, Subcommand};
use crawl_cmd::{CrawlCmd, HtmlArgs, InfoArgs, PageArgs, ReportArgs, RetryArgs, ScopeTestArgs};
use options::{CrawlArgs, IdentityArgs, NetworkArgs};
use spiderlens_config::{
    CrawlConfig, Decision, Issue, ListFormat, Mode, ModeInput, Severity, export_profile, parse_list_bytes,
    test_patterns, validate, validate_config,
};
use spiderlens_engine::crawl::sitemap_list::{SitemapListSource, list_from_sitemap};
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
    /// `spiderlens compare` found a regression it was told to fail on
    /// (spec 012).
    pub const COMPARE_FAILED: u8 = 8;
    /// `issues --fail-on`: an issue at or above that priority has URLs
    /// (spec 009 FR-027).
    pub const ISSUES_FOUND: u8 = 7;
    /// A quality gate failed (`aeo run --fail-under`); the same number as
    /// `ISSUES_FOUND`, both being "the checks found what you asked to fail on".
    pub const QUALITY_GATE: u8 = 7;
    /// No usable browser for JavaScript rendering (spec 014). The rendering
    /// contract asked for 6, which spec 006 already uses, and 7 and 8 are
    /// taken by specs 009 and 012.
    pub const NO_BROWSER: u8 = 9;
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
    /// List the URLs in a crawl file, its summary or its log.
    Report(ReportArgs),
    /// Retry the URLs of a crawl that failed for temporary reasons.
    Retry(RetryArgs),
    /// Show a crawl file's configuration, status, versions and history.
    Info(InfoArgs),
    /// Show everything extracted for one URL of a crawl file.
    Page(PageArgs),
    /// Write the stored raw HTML of one page of a crawl file.
    Html(HtmlArgs),
    /// List the cookies the site set during a crawl (no values).
    Cookies(cookies_cmd::CookiesArgs),
    /// Show a crawl file's accessibility results, or its mobile measures.
    Accessibility(accessibility_cmd::AccessibilityArgs),
    /// List crawl files in a folder with their size and stored HTML.
    Files(files_cmd::FilesArgs),
    /// Remove the stored page HTML from a crawl file, keeping its results.
    PurgeHtml(files_cmd::PurgeHtmlArgs),
    /// Export a tab and filter of a crawl file as CSV (same rows as the app).
    Export(Box<export_cmd::ExportArgs>),
    /// Near duplicate content of a crawl file: pages, pairs or clusters.
    Duplicates(Box<duplicates_cmd::DuplicatesArgs>),
    /// Serve crawl files to AI assistants over MCP (standard input and output).
    Mcp(McpCmd),
    /// What changed between two crawl files of a site.
    Compare(Box<compare_cmd::CompareArgs>),
    /// List the issue rules: what each checks, why it matters, how to fix.
    Rules(issues_cmd::RulesArgs),
    /// List the issues found in a crawl file, or export them as CSV.
    Issues(Box<issues_cmd::IssuesArgs>),
    /// Run the crawl analysis passes that have not run on a crawl file yet.
    Analyse(issues_cmd::AnalyseArgs),
    /// External data: Search Console and other providers (spec 024).
    #[command(subcommand)]
    Integrations(Box<integrations_cmd::IntegrationsCommand>),
    /// Write a visualisation of a crawl file as SVG, PNG or CSV (same as the app's export).
    Viz(Box<viz_cmd::VizArgs>),
    /// Link Score and link positions of a crawl file.
    #[command(subcommand)]
    Links(links_cmd::LinksCommand),
    /// AI search checks (AEO and GEO) on a crawl file.
    #[command(subcommand)]
    Aeo(aeo_cmd::AeoCommand),
    /// AI agent readiness of a crawl file: score, parts and entry points.
    #[command(subcommand)]
    Agents(agents_cmd::AgentsCommand),
    /// URLs per segment of a crawl file.
    Segments(segments_cmd::SegmentsArgs),
    /// Check the spelling of a crawl file, or manage dictionaries.
    Spell(Box<spell_cmd::SpellArgs>),
    /// Explain crawl scope decisions.
    #[command(subcommand)]
    Scope(ScopeCommand),
    /// Validate, test and run custom extraction and search rules.
    #[command(subcommand)]
    CustomRules(rules_cmd::RulesCommand),
    /// The browser JavaScript rendering uses: status, download, removal.
    #[command(subcommand)]
    Browser(browser::BrowserCommand),
    /// Generate XML sitemaps from a crawl file.
    #[command(subcommand)]
    Sitemap(sitemap_cmd::SitemapCommand),
    /// List the sitemap files a crawl read, as a tree.
    Sitemaps(sitemap_cmd::SitemapsArgs),
    /// Orphan pages: in a sitemap but linked from no crawled page.
    Orphans(sitemap_cmd::OrphansArgs),
    /// Recurring crawls that run without the app (spec 021).
    #[command(subcommand)]
    Schedule(Box<schedule_cmd::ScheduleCommand>),
    /// Title and description pixel widths for a CSV of drafts (no network).
    Serp(serp_cmd::SerpArgs),
    /// Print product version, engine version and build identifier.
    Version {
        /// Print as JSON.
        #[arg(long)]
        json: bool,
    },
}

/// `spiderlens mcp` (spec 016 contracts/cli.md). Without flags, access
/// comes from the desktop app's Settings › AI assistants.
#[derive(Args)]
struct McpCmd {
    /// Allow reading crawls for this process only.
    #[arg(long)]
    allow_read: bool,
    /// Also allow crawl files in this folder, for this process only.
    #[arg(long, value_name = "DIR")]
    folder: Vec<PathBuf>,
    /// Print the server name, access and tool list as JSON, then exit.
    #[arg(long)]
    self_test: bool,
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
    /// Sitemap address or file whose page URLs are the list (List mode);
    /// a file may be followed by =BASE-URL for relative addresses.
    #[arg(long, value_name = "URL-OR-PATH[=BASE-URL]", conflicts_with_all = ["url", "list"])]
    list_sitemap: Option<String>,
    /// Print as JSON.
    #[arg(long)]
    json: bool,
    #[command(flatten)]
    crawl: CrawlArgs,
}

fn main() -> ExitCode {
    spiderlens_engine::run_render_guard_if_asked();
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
        Command::Retry(args) => crawl_cmd::run_retry(args),
        Command::Info(args) => crawl_cmd::run_info(args),
        Command::Page(args) => crawl_cmd::run_page(args),
        Command::Html(args) => crawl_cmd::run_html(args),
        Command::Cookies(args) => cookies_cmd::run_cookies(args),
        Command::Accessibility(args) => accessibility_cmd::run_accessibility(args),
        Command::Files(args) => files_cmd::run_files(args),
        Command::PurgeHtml(args) => files_cmd::run_purge_html(args),
        Command::Export(args) => export_cmd::run_export(*args),
        Command::Duplicates(args) => duplicates_cmd::run_duplicates(*args),
        Command::Mcp(args) => ExitCode::from(spiderlens_mcp::run(spiderlens_mcp::McpArgs {
            allow_read: args.allow_read,
            folders: args.folder,
            self_test: args.self_test,
        }) as u8),
        Command::Compare(args) => compare_cmd::run_compare(*args),
        Command::Rules(args) => issues_cmd::run_rules(args),
        Command::Issues(args) => issues_cmd::run_issues(*args),
        Command::Analyse(args) => issues_cmd::run_analyse(args),
        Command::Integrations(cmd) => integrations_cmd::run(*cmd),
        Command::Viz(args) => viz_cmd::run_viz(*args),
        Command::Links(cmd) => links_cmd::run(cmd),
        Command::Aeo(cmd) => aeo_cmd::run(cmd),
        Command::Agents(cmd) => agents_cmd::run(cmd),
        Command::Segments(args) => segments_cmd::run_segments(args),
        Command::Spell(args) => spell_cmd::run_spell(*args),
        Command::Scope(ScopeCommand::Test(args)) => crawl_cmd::run_scope_test(*args),
        Command::CustomRules(cmd) => rules_cmd::run(cmd),
        Command::Browser(cmd) => browser::run(cmd),
        Command::Sitemap(cmd) => sitemap_cmd::run(cmd),
        Command::Sitemaps(args) => sitemap_cmd::run_sitemaps(&args),
        Command::Orphans(args) => sitemap_cmd::run_orphans(&args),
        Command::Schedule(cmd) => schedule_cmd::run(*cmd, &matches),
        Command::Serp(args) => serp_cmd::run_serp(args),
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
    let mode = if args.list.is_some() || args.list_sitemap.is_some() {
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
    let input = match read_input(
        args.url.as_deref(),
        args.list.as_deref(),
        args.list_sitemap.as_deref(),
        &resolved.config,
    ) {
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

/// Reads the Spider start URL, the List file or the List sitemap into a
/// mode input. A sitemap is read here, before the crawl, so the crawl is an
/// ordinary list crawl (spec 011 FR-005).
pub(crate) fn read_input(
    url: Option<&str>,
    list: Option<&Path>,
    list_sitemap: Option<&str>,
    config: &CrawlConfig,
) -> Result<Option<ModeInput>, ExitCode> {
    match (url, list, list_sitemap) {
        (Some(url), _, _) => Ok(Some(ModeInput::Spider {
            start_url: url.to_string(),
        })),
        (None, Some(path), _) => match std::fs::read(path) {
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
        (None, None, Some(arg)) => {
            let source = SitemapListSource::parse_arg(arg).map_err(|e| {
                eprintln!("error: --list-sitemap: {e}");
                ExitCode::from(exit::USAGE)
            })?;
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|e| {
                    eprintln!("error: could not start: {e}");
                    ExitCode::FAILURE
                })?;
            let list = runtime.block_on(list_from_sitemap(source, config)).map_err(|e| {
                eprintln!("error: --list-sitemap: {e}");
                ExitCode::from(exit::USAGE)
            })?;
            for n in &list.notices {
                eprintln!("notice: {n}");
            }
            eprintln!(
                "list: {} URLs to crawl, {} duplicates removed, {} invalid",
                list.urls.len(),
                list.duplicates,
                list.invalid_count
            );
            Ok(Some(ModeInput::List(list)))
        }
        (None, None, None) => Ok(None),
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
