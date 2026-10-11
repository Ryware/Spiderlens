//! `spiderlens` command-line tool. A thin layer over `spiderlens-engine`:
//! it parses arguments, calls the engine and prints the result.
//! See specs/001-project-foundation/contracts/cli.md.

use std::io::Write;
use std::process::ExitCode;
use std::time::Duration;

use clap::{Args, CommandFactory, FromArgMatches, Parser, Subcommand};
use spiderlens_engine::{
    BuildInfo, CheckOptions, ErrorClass, FinalState, PageCheckResult, Title, check_page,
};

/// Exit codes from the CLI contract.
mod exit {
    pub const FETCHED: u8 = 0;
    pub const USAGE: u8 = 2;
    pub const BLOCKED: u8 = 3;
    pub const FAILED: u8 = 4;
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
    /// Print product version, engine version and build identifier.
    Version {
        /// Print as JSON.
        #[arg(long)]
        json: bool,
    },
}

#[derive(Args)]
struct CheckArgs {
    /// Address to check. https:// is assumed when no scheme is given.
    url: String,
    /// Print the result as one JSON object.
    #[arg(long)]
    json: bool,
    /// Connect time limit in seconds.
    #[arg(long, value_name = "SECONDS", default_value_t = 10.0, value_parser = positive_seconds)]
    connect_timeout: f64,
    /// Time limit for each read, in seconds.
    #[arg(long, value_name = "SECONDS", default_value_t = 30.0, value_parser = positive_seconds)]
    read_timeout: f64,
    /// Maximum response size in megabytes.
    #[arg(long, value_name = "MB", default_value_t = 50, value_parser = clap::value_parser!(u64).range(1..=4096))]
    max_size: u64,
}

fn positive_seconds(s: &str) -> Result<f64, String> {
    match s.parse::<f64>() {
        Ok(v) if v > 0.0 && v.is_finite() && v <= 3600.0 => Ok(v),
        _ => Err("must be a number of seconds between 0 and 3600".into()),
    }
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
    }
}

fn run_check(args: CheckArgs) -> ExitCode {
    let opts = CheckOptions {
        connect_timeout: Duration::from_secs_f64(args.connect_timeout),
        read_timeout: Duration::from_secs_f64(args.read_timeout),
        max_body_bytes: args.max_size * 1024 * 1024,
        ..CheckOptions::default()
    };
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

fn print_json<T: serde::Serialize>(value: &T) {
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
