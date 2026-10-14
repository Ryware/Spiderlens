//! `spiderlens aeo`: AI search checks on a crawl file (spec 015
//! contracts/cli.md).

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Args, Subcommand};
use rusqlite::Connection;
use spiderlens_aeo::analyze::{self, AnalyzeOptions, Progress, Summary};
use spiderlens_aeo::rules;

use crate::{exit, print_json};

#[derive(Subcommand)]
pub enum AeoCommand {
    /// Run the AI checks and print the site score and top fixes.
    Run(RunArgs),
    /// Print stored AI results without running anything.
    Summary(FileArgs),
    /// Explain one page's AI result.
    Page(PageArgs),
    /// Write a draft llms.txt from the crawl. Never uploads.
    LlmsTxt(LlmsArgs),
    /// List the known AI crawlers.
    Bots {
        /// Print as JSON.
        #[arg(long)]
        json: bool,
    },
}

#[derive(Args)]
pub struct FileArgs {
    file: PathBuf,
    /// Print as JSON.
    #[arg(long)]
    json: bool,
}

#[derive(Args)]
pub struct RunArgs {
    file: PathBuf,
    /// Exit with the quality-gate code when the site score is below this.
    #[arg(long, value_name = "SCORE")]
    fail_under: Option<u32>,
    /// Print as JSON.
    #[arg(long)]
    json: bool,
    /// No progress on stderr.
    #[arg(long)]
    quiet: bool,
}

#[derive(Args)]
pub struct PageArgs {
    file: PathBuf,
    url: String,
    /// Print as JSON.
    #[arg(long)]
    json: bool,
}

#[derive(Args)]
pub struct LlmsArgs {
    file: PathBuf,
    /// Most links in the draft.
    #[arg(long, default_value_t = 100)]
    max_links: usize,
    /// Where to write the draft [default: stdout].
    #[arg(short = 'o', long, value_name = "FILE")]
    out: Option<PathBuf>,
}

pub fn run(cmd: AeoCommand) -> ExitCode {
    match cmd {
        AeoCommand::Run(a) => run_checks(a),
        AeoCommand::Summary(a) => summary(a),
        AeoCommand::Page(a) => page(a),
        AeoCommand::LlmsTxt(a) => llms_txt(a),
        AeoCommand::Bots { json } => bots(json),
    }
}

fn open(file: &Path) -> Result<Connection, ExitCode> {
    if !file.exists() {
        eprintln!("error: {} does not exist", file.display());
        return Err(ExitCode::from(exit::USAGE));
    }
    Connection::open(file).map_err(|e| {
        eprintln!("error: cannot open {}: {e}", file.display());
        ExitCode::from(exit::USAGE)
    })
}

fn progress(quiet: bool) -> impl Fn(Progress) {
    move |p| {
        if quiet {
            return;
        }
        if let Progress::Pages { done, total } = p
            && (done == total || done % 10_000 == 0)
        {
            eprintln!("AI checks: {done} of {total} pages");
        }
    }
}

/// The crawl's finish step: runs the checks and prints one line.
pub fn run_after_crawl(file: &Path, quiet: bool) {
    match analyze::analyze(file, &AnalyzeOptions::default(), &progress(true)) {
        Ok(s) if !quiet => eprintln!(
            "AI search score {} (pages {}, site {}); details: spiderlens aeo summary {}",
            s.score,
            s.pages_score,
            s.site_checks,
            file.display()
        ),
        Ok(_) => {}
        Err(e) => eprintln!("warning: AI checks did not run: {e}"),
    }
}

fn run_checks(a: RunArgs) -> ExitCode {
    if let Err(code) = open(&a.file) {
        return code;
    }
    let s = match analyze::analyze(&a.file, &AnalyzeOptions::default(), &progress(a.quiet || a.json)) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };
    show(&s, a.json);
    match a.fail_under {
        Some(min) if s.score < min => {
            eprintln!("AI search score {} is below {min}", s.score);
            ExitCode::from(exit::QUALITY_GATE)
        }
        _ => ExitCode::from(exit::FETCHED),
    }
}

fn summary(a: FileArgs) -> ExitCode {
    let conn = match open(&a.file) {
        Ok(c) => c,
        Err(code) => return code,
    };
    match analyze::summary(&conn) {
        Ok(Some(s)) => {
            show(&s, a.json);
            ExitCode::from(exit::FETCHED)
        }
        Ok(None) => {
            eprintln!(
                "error: AI checks have not run on this crawl; run: spiderlens aeo run {}",
                a.file.display()
            );
            ExitCode::from(exit::USAGE)
        }
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn show(s: &Summary, json: bool) {
    if json {
        print_json(s);
        return;
    }
    println!(
        "AI search score {}  (pages {} × 70%, site checks {} × 30%)",
        s.score, s.pages_score, s.site_checks
    );
    for p in &s.parts {
        let score = p.score.map_or("-".to_string(), |v| v.to_string());
        println!("  {:<12} {score:>3} / {}", format!("{:?}", p.part), p.max);
    }
    println!(
        "pages analysed {}  provisional {}  bands 0-19: {}  20-49: {}  50-79: {}  80-100: {}",
        s.pages_analyzed,
        s.provisional_pages,
        s.distribution[0],
        s.distribution[1],
        s.distribution[2],
        s.distribution[3]
    );
    for h in &s.hosts {
        let blocked: Vec<&str> = h
            .access
            .iter()
            .filter(|a| a.urls_blocked > 0)
            .map(|a| a.bot.as_str())
            .collect();
        println!(
            "{}  robots.txt {}  blocked crawlers: {}",
            h.host_key,
            h.robots,
            if blocked.is_empty() {
                "none".into()
            } else {
                blocked.join(", ")
            }
        );
    }
    if !s.top_fixes.is_empty() {
        println!("top fixes:");
        for f in &s.top_fixes {
            let name = rules::by_id(&f.rule_id).map_or(f.rule_id.as_str(), |r| r.rule.name);
            println!(
                "  +{}.{:02}  {name} ({} pages)",
                f.gain / 100,
                f.gain % 100,
                f.pages
            );
        }
    }
}

fn page(a: PageArgs) -> ExitCode {
    let conn = match open(&a.file) {
        Ok(c) => c,
        Err(code) => return code,
    };
    match analyze::page_result(&conn, analyze::PageKey::Url(&a.url)) {
        Ok(Some(p)) if a.json => {
            print_json(&p);
            ExitCode::from(exit::FETCHED)
        }
        Ok(Some(p)) => {
            let score = p.score.map_or("-".into(), |s| s.to_string());
            println!(
                "{}  score {score}  {} ({})",
                p.url,
                p.page_type,
                p.page_type_reason.unwrap_or_default()
            );
            if !p.blocked_crawlers.is_empty() {
                println!("  blocked for: {}", p.blocked_crawlers.join(", "));
            }
            for c in &p.checks {
                let name = rules::by_id(&c.rule_id).map_or(c.rule_id.as_str(), |r| r.rule.name);
                println!(
                    "  {:<14} -{}.{:02}  {name}",
                    c.outcome,
                    c.points_lost / 100,
                    c.points_lost % 100
                );
            }
            ExitCode::from(exit::FETCHED)
        }
        Ok(None) => {
            eprintln!("error: no AI result for {}", a.url);
            ExitCode::from(exit::USAGE)
        }
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn llms_txt(a: LlmsArgs) -> ExitCode {
    let conn = match open(&a.file) {
        Ok(c) => c,
        Err(code) => return code,
    };
    let text = match analyze::draft_llms_txt(&conn, a.max_links) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };
    match a.out {
        Some(path) => {
            if let Err(e) = std::fs::write(&path, text) {
                eprintln!("error: cannot write {}: {e}", path.display());
                return ExitCode::FAILURE;
            }
            eprintln!("draft written to {}; review it before publishing", path.display());
        }
        None => print!("{text}"),
    }
    ExitCode::from(exit::FETCHED)
}

fn bots(json: bool) -> ExitCode {
    let all = spiderlens_config::ai_bots::all();
    if json {
        print_json(&serde_json::json!({
            "version": spiderlens_config::ai_bots::version(),
            "date": spiderlens_config::ai_bots::date(),
            "bots": all,
        }));
        return ExitCode::from(exit::FETCHED);
    }
    println!(
        "AI crawler list version {} ({})",
        spiderlens_config::ai_bots::version(),
        spiderlens_config::ai_bots::date()
    );
    for b in all.iter().filter(|b| !b.retired) {
        println!("  {:<22} {:<12} {:?}", b.token, b.vendor, b.purpose);
    }
    ExitCode::from(exit::FETCHED)
}
