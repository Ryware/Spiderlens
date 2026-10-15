//! `spiderlens custom-rules validate|test|run` (spec 013 contracts/cli.md;
//! renamed from `rules`, which lists the spec 009 issue rules).

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use clap::{Args, Subcommand};
use serde::Serialize;
use spiderlens_config::{CustomRules, Mode, RobotsMode, import_rules, validate_config};
use spiderlens_engine::CheckOptions;
use spiderlens_engine::crawl::CrawlStore;
use spiderlens_engine::custom::rerun::{RerunError, RerunState, Which, rerun, test_stored_page};
use spiderlens_engine::custom::{PageRules, compile, flags, tester};

use crate::options::CrawlArgs;
use crate::{exit, print_issues, print_json};

/// Interrupted by Ctrl-C (shell convention 128 + SIGINT).
const CANCELLED: u8 = 130;

#[derive(Subcommand)]
pub enum RulesCommand {
    /// Check a rule file; print each problem with the rule and position.
    Validate { file: PathBuf },
    /// Show what each rule finds on one page.
    Test(Box<TestArgs>),
    /// Run rules on a finished crawl from its stored HTML. Nothing is fetched.
    Run(RunArgs),
}

#[derive(Args)]
pub struct TestArgs {
    /// Rule file.
    file: PathBuf,
    /// Fetch this URL once and test on it.
    #[arg(
        long,
        value_name = "URL",
        required_unless_present = "crawl",
        conflicts_with = "crawl"
    )]
    url: Option<String>,
    /// Test on a page of this crawl file (stored HTML, no request).
    #[arg(long, value_name = "FILE", requires = "page")]
    crawl: Option<PathBuf>,
    /// The page's URL in the crawl file.
    #[arg(long, value_name = "URL")]
    page: Option<String>,
    #[arg(long)]
    json: bool,
    #[command(flatten)]
    crawl_args: CrawlArgs,
}

#[derive(Args)]
pub struct RunArgs {
    /// Crawl file.
    crawl: PathBuf,
    /// Rules to add to the crawl and run; without it, the crawl's own rules run.
    #[arg(long, value_name = "FILE")]
    rules: Option<PathBuf>,
    /// Only the rules with this name. Repeatable.
    #[arg(long, value_name = "NAME")]
    only: Vec<String>,
    /// No progress on stderr.
    #[arg(long)]
    quiet: bool,
    #[arg(long)]
    json: bool,
}

pub fn run(cmd: RulesCommand) -> ExitCode {
    match cmd {
        RulesCommand::Validate { file } => match read_rules(&file) {
            Ok(_) => ExitCode::SUCCESS,
            Err(code) => code,
        },
        RulesCommand::Test(args) => run_test(*args),
        RulesCommand::Run(args) => run_rerun(args),
    }
}

/// Reads and validates a rule file, printing problems.
fn read_rules(path: &Path) -> Result<CustomRules, ExitCode> {
    let text = std::fs::read_to_string(path).map_err(|e| {
        eprintln!("error: {}: {e}", path.display());
        ExitCode::from(exit::USAGE)
    })?;
    let rules = import_rules(&text).map_err(|e| {
        eprintln!("error: {}: {e}", path.display());
        ExitCode::from(exit::USAGE)
    })?;
    let config = spiderlens_config::CrawlConfig {
        custom: rules.clone(),
        ..Default::default()
    };
    if print_issues(&validate_config(&config)) {
        return Err(ExitCode::from(exit::USAGE));
    }
    Ok(rules)
}

#[derive(Serialize)]
struct RuleLine<'a> {
    name: &'a str,
    kind: &'static str,
    count: u32,
    flags: Vec<&'static str>,
    values: &'a [String],
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<&'a str>,
}

fn lines<'a>(rules: &'a CustomRules, out: &'a PageRules) -> Vec<RuleLine<'a>> {
    let ex = rules
        .extractors
        .iter()
        .zip(&out.extractors)
        .map(|(r, o)| RuleLine {
            name: &r.name,
            kind: "extractor",
            count: o.count,
            flags: flags::names(o.flags),
            values: &o.values,
            error: o.error.as_deref(),
        });
    let se = rules.searches.iter().zip(&out.searches).map(|(r, o)| RuleLine {
        name: &r.name,
        kind: "search",
        count: o.count,
        flags: flags::names(o.flags),
        values: &o.values,
        error: o.error.as_deref(),
    });
    ex.chain(se).collect()
}

fn print_lines(lines: &[RuleLine<'_>], json: bool) {
    if json {
        print_json(&lines);
        return;
    }
    for l in lines {
        let flags = if l.flags.is_empty() {
            String::new()
        } else {
            format!("  [{}]", l.flags.join(", "))
        };
        println!("{} ({}): {}{flags}", l.name, l.kind, l.count);
        for v in l.values.iter().take(10) {
            println!("    {v}");
        }
        if let Some(e) = l.error {
            println!("    error: {e}");
        }
    }
}

fn run_test(args: TestArgs) -> ExitCode {
    let rules = match read_rules(&args.file) {
        Ok(r) => r,
        Err(code) => return code,
    };
    let compiled = match compile(&rules) {
        Ok(Some(c)) => c,
        Ok(None) => {
            eprintln!("error: the rule file has no rules");
            return ExitCode::from(exit::USAGE);
        }
        Err(errors) => {
            for e in errors {
                eprintln!("error: {e}");
            }
            return ExitCode::from(exit::USAGE);
        }
    };
    let out = if let Some(crawl) = &args.crawl {
        let page = args.page.as_deref().expect("clap requires --page");
        let store = match CrawlStore::open(crawl) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("error: {e}");
                return ExitCode::from(exit::USAGE);
            }
        };
        let id = match store.id_of(page) {
            Ok(Some(id)) => id,
            _ => {
                eprintln!("error: {page} is not in {}", crawl.display());
                return ExitCode::from(exit::FAILED);
            }
        };
        match test_stored_page(&store, id, &compiled) {
            Ok(Some(out)) => out,
            Ok(None) => {
                eprintln!("error: {page} has no stored HTML in this crawl");
                return ExitCode::from(exit::FAILED);
            }
            Err(e) => {
                eprintln!("error: {e}");
                return ExitCode::from(exit::FAILED);
            }
        }
    } else {
        let resolved = match args.crawl_args.resolve(Mode::Spider) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("error: {e}");
                return ExitCode::from(exit::USAGE);
            }
        };
        let opts = CheckOptions::from(&resolved.config);
        let respect = resolved.config.robots.mode == RobotsMode::Respect;
        let url = args.url.as_deref().expect("clap requires --url or --crawl");
        let rt = match tokio::runtime::Builder::new_current_thread().enable_all().build() {
            Ok(rt) => rt,
            Err(e) => {
                eprintln!("error: could not start: {e}");
                return ExitCode::FAILURE;
            }
        };
        match rt.block_on(tester::test_url(url, &opts, respect, &compiled)) {
            Ok(t) => {
                if !args.json {
                    println!("{} ({})", t.final_url, t.status);
                }
                t.rules
            }
            Err(e) => {
                eprintln!("error: {}", e.message);
                return ExitCode::from(exit::FAILED);
            }
        }
    };
    print_lines(&lines(&rules, &out), args.json);
    ExitCode::SUCCESS
}

fn run_rerun(args: RunArgs) -> ExitCode {
    let extra = match &args.rules {
        Some(path) => match read_rules(path) {
            Ok(r) => Some(r),
            Err(code) => return code,
        },
        None => None,
    };
    let cancel = Arc::new(AtomicBool::new(false));
    {
        let cancel = Arc::clone(&cancel);
        // Ctrl-C finishes the current batch and records a partial run.
        let _ = ctrlc_hook(move || cancel.store(true, Ordering::Relaxed));
    }
    let which = match &extra {
        Some(r) => Which::These(r),
        None => Which::Stored,
    };
    let quiet = args.quiet;
    let result = rerun(&args.crawl, which, &args.only, &cancel, |p| {
        if !quiet {
            eprint!("\r{} of {} pages, {}/s   ", p.done, p.total, p.per_sec);
        }
    });
    if !quiet {
        eprintln!();
    }
    match result {
        Ok(report) => {
            if args.json {
                print_json(&report);
            } else {
                println!(
                    "{} rules on {} pages{}",
                    report.rules,
                    report.pages,
                    if report.not_available > 0 {
                        format!(" ({} without stored HTML)", report.not_available)
                    } else {
                        String::new()
                    }
                );
            }
            match report.state {
                RerunState::Complete => ExitCode::SUCCESS,
                RerunState::Partial => ExitCode::from(CANCELLED),
            }
        }
        Err(RerunError::NotPossible(m)) => {
            eprintln!("error: {m}");
            ExitCode::from(exit::FAILED)
        }
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::from(exit::FAILED)
        }
    }
}

/// Runs `f` on Ctrl-C, from a small Tokio runtime on its own thread.
fn ctrlc_hook(f: impl FnOnce() + Send + 'static) -> std::io::Result<()> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    std::thread::spawn(move || {
        rt.block_on(async {
            if tokio::signal::ctrl_c().await.is_ok() {
                f();
            }
        });
    });
    Ok(())
}
