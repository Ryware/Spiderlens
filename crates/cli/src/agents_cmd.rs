//! `spiderlens agents`: the site-wide AI agent summary and entry points of a
//! crawl file (spec 016 contracts/cli.md).

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Args, Subcommand};
use spiderlens_results::Reader;
use spiderlens_results::agents::{AgentsSummary, EntryPointRow};

use crate::{exit, print_json};

/// Exit code when the crawl file cannot be opened.
const UNREADABLE: u8 = 3;

#[derive(Subcommand)]
pub enum AgentsCommand {
    /// The crawl's AI agent score, its parts, top rules and checks not run.
    Summary(FileArgs),
    /// What each host offers to AI agents: catalogs, agent cards, service links.
    EntryPoints(FileArgs),
}

#[derive(Args)]
pub struct FileArgs {
    /// The crawl file.
    file: PathBuf,
    /// Print as JSON.
    #[arg(long)]
    json: bool,
}

pub fn run(cmd: AgentsCommand) -> ExitCode {
    match cmd {
        AgentsCommand::Summary(a) => summary(a),
        AgentsCommand::EntryPoints(a) => entry_points(a),
    }
}

fn open(a: &FileArgs) -> Result<Reader, ExitCode> {
    Reader::open(&a.file).map_err(|e| {
        eprintln!("error: cannot open {}: {e}", a.file.display());
        ExitCode::from(UNREADABLE)
    })
}

fn summary(a: FileArgs) -> ExitCode {
    let r = match open(&a) {
        Ok(r) => r,
        Err(code) => return code,
    };
    let s = match r.agents_summary() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };
    if a.json {
        print_json(&s);
    } else {
        print_summary(&s);
    }
    ExitCode::from(exit::FETCHED)
}

fn print_summary(s: &AgentsSummary) {
    let Some(score) = s.score.filter(|_| s.available) else {
        println!(
            "AI agent score: not available. {}",
            s.unavailable_reason
                .as_deref()
                .unwrap_or("No score for this crawl.")
        );
        return;
    };
    println!(
        "AI agent score: {score:.1} / 100 ({} pages scored)",
        s.pages_scored
    );
    println!();
    println!("Parts");
    for p in &s.parts {
        match p.score {
            Some(v) => println!("  {:<10} {v:>5.1}  (weight {})", p.part.label(), p.weight),
            None => println!("  {:<10} {:>5}  (weight {})", p.part.label(), "n/a", p.weight),
        }
    }
    if !s.top_failed.is_empty() {
        println!();
        println!("Top rules by points lost");
        for f in &s.top_failed {
            let scope = match f.pages {
                Some(n) => format!("{n} pages"),
                None => "site-wide".to_string(),
            };
            println!(
                "  -{:>4.1}  {} [{}] ({scope})",
                f.points_lost,
                f.name,
                f.part.label()
            );
        }
    }
    if !s.not_checked.is_empty() {
        println!();
        println!("Not checked");
        for n in &s.not_checked {
            println!("  {}: {}", n.name, n.reason);
        }
    }
}

fn entry_points(a: FileArgs) -> ExitCode {
    let r = match open(&a) {
        Ok(r) => r,
        Err(code) => return code,
    };
    let rows = match r.agent_entry_points() {
        Ok(rows) => rows,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };
    if a.json {
        print_json(&rows);
    } else {
        print_entry_points(&rows);
    }
    ExitCode::from(exit::FETCHED)
}

fn print_entry_points(rows: &[EntryPointRow]) {
    if rows.is_empty() {
        println!(
            "No agent entry points recorded. Run `spiderlens analyse` on the crawl file, or crawl with entry points on."
        );
        return;
    }
    let mut host: Option<&str> = None;
    for r in rows {
        if host != Some(r.host_key.as_str()) {
            if host.is_some() {
                println!();
            }
            println!("{}", r.host_key);
            host = Some(r.host_key.as_str());
        }
        let status = r.status.map_or_else(|| "-".to_string(), |s| s.to_string());
        println!("  {:<13} {:<12} {:>3}  {}", r.kind, r.outcome, status, r.url);
        for p in &r.problems {
            println!("      - {}", p.message);
        }
    }
}
