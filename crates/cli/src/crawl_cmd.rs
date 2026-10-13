//! `spiderlens crawl`, `report` and `scope test` (spec 003 contracts/cli.md).

use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{Duration, Instant};

use clap::{ArgMatches, Args, parser::ValueSource};
use spiderlens_config::{ModeInput, RobotsMode, normalize_url, validate};
use spiderlens_engine::crawl::{
    Control, CrawlEvent, CrawlStore, CrawlSummary, Crawler, NotCrawledReason, ScopeRules, UrlState,
    default_file_name, fetch_robots, host_key, test_url,
};
use spiderlens_engine::robots::RobotsPolicy;
use tokio::sync::{mpsc, watch};

use crate::options::CrawlArgs;
use crate::{exit, print_issues, print_json, read_input};

#[derive(Args)]
pub struct CrawlCmd {
    /// Start URL (Spider mode).
    #[arg(required_unless_present_any = ["list", "resume"])]
    url: Option<String>,
    /// URL list file, text or CSV (List mode).
    #[arg(long, value_name = "FILE", conflicts_with = "url")]
    list: Option<PathBuf>,
    /// Continue a paused or stopped crawl with its stored settings.
    #[arg(long, value_name = "FILE", conflicts_with_all = ["url", "list", "out"])]
    resume: Option<PathBuf>,
    /// Crawl file to create [default: <host>-<time>.spiderlens].
    #[arg(long, value_name = "FILE")]
    out: Option<PathBuf>,
    /// Print the summary as JSON.
    #[arg(long)]
    json: bool,
    /// No progress on stderr.
    #[arg(long)]
    quiet: bool,
    #[command(flatten)]
    crawl: CrawlArgs,
}

#[derive(Args)]
pub struct ReportArgs {
    /// Crawl file.
    file: PathBuf,
    /// Only URLs that were not crawled.
    #[arg(long, conflicts_with = "crawled")]
    not_crawled: bool,
    /// Only crawled URLs.
    #[arg(long)]
    crawled: bool,
    /// Only this not-crawled reason (for example blocked_by_robots). Repeatable.
    #[arg(long, value_name = "REASON")]
    reason: Vec<String>,
    /// JSON (one object per line for URL listings).
    #[arg(long)]
    json: bool,
}

#[derive(Args)]
pub struct ScopeTestArgs {
    /// URLs to test, absolute or relative to the start URL.
    #[arg(required = true)]
    urls: Vec<String>,
    /// The crawl's start URL.
    #[arg(long, value_name = "URL")]
    start: String,
    /// Fetch each host's robots.txt and apply it.
    #[arg(long)]
    fetch_robots: bool,
    #[arg(long)]
    json: bool,
    #[command(flatten)]
    crawl: CrawlArgs,
}

fn runtime() -> Result<tokio::runtime::Runtime, ExitCode> {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|e| {
            eprintln!("error: could not start: {e}");
            ExitCode::FAILURE
        })
}

fn default_out(input: &ModeInput) -> PathBuf {
    PathBuf::from(default_file_name(input))
}

pub fn run_crawl(cmd: CrawlCmd, matches: &ArgMatches) -> ExitCode {
    let (crawler, file) = if let Some(path) = &cmd.resume {
        // The stored configuration decides; other options would be ignored,
        // so they are refused instead (contracts/cli.md).
        let extra: Vec<String> = matches
            .ids()
            .map(|id| id.as_str().to_string())
            .filter(|id| !["resume", "json", "quiet"].contains(&id.as_str()))
            .filter(|id| matches.value_source(id) == Some(ValueSource::CommandLine))
            .collect();
        if !extra.is_empty() {
            eprintln!(
                "error: --resume uses the settings stored in the crawl file; remove: --{}",
                extra.join(", --").replace('_', "-")
            );
            return ExitCode::from(exit::USAGE);
        }
        match Crawler::open(path) {
            Ok(c) => (c, path.clone()),
            Err(e) => {
                eprintln!("error: {e}");
                return ExitCode::from(exit::USAGE);
            }
        }
    } else {
        let mode = if cmd.list.is_some() {
            spiderlens_config::Mode::List
        } else {
            spiderlens_config::Mode::Spider
        };
        let resolved = match cmd.crawl.resolve(mode) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("error: {e}");
                return ExitCode::from(exit::USAGE);
            }
        };
        for n in &resolved.notices {
            eprintln!("notice: {n}");
        }
        let input = match read_input(cmd.url.as_deref(), cmd.list.as_deref()) {
            Ok(Some(i)) => i,
            Ok(None) => {
                eprintln!("error: give a start URL or --list FILE");
                return ExitCode::from(exit::USAGE);
            }
            Err(code) => return code,
        };
        if print_issues(&validate(&resolved.config, &input)) {
            return ExitCode::from(exit::USAGE);
        }
        let out = cmd.out.clone().unwrap_or_else(|| default_out(&input));
        match Crawler::create(&out, resolved.config, &input) {
            Ok(c) => {
                if !cmd.quiet {
                    eprintln!("crawl file: {}", out.display());
                }
                (c, out)
            }
            Err(e) => {
                eprintln!("error: {e}");
                return ExitCode::from(exit::USAGE);
            }
        }
    };
    let rt = match runtime() {
        Ok(rt) => rt,
        Err(code) => return code,
    };
    let quiet = cmd.quiet;
    let result = rt.block_on(async move {
        let (control_tx, control_rx) = watch::channel(Control::Run);
        let (event_tx, mut event_rx) = mpsc::unbounded_channel();
        // Ctrl-C pauses; a second Ctrl-C exits at once (the file stays
        // consistent: results are committed in transactions a fraction of a
        // second apart, so at most that much work is lost).
        tokio::spawn(async move {
            if tokio::signal::ctrl_c().await.is_ok() {
                eprintln!("\npausing: waiting for requests in flight (Ctrl-C again to quit now)...");
                let _ = control_tx.send(Control::Pause);
                if tokio::signal::ctrl_c().await.is_ok() {
                    std::process::exit(130);
                }
            }
            // Keep the sender alive so the crawl does not see a closed channel.
            std::future::pending::<()>().await;
        });
        let speed = tokio::spawn(async move {
            let mut last = Instant::now() - Duration::from_secs(1);
            let mut speed = SpeedSummary::default();
            while let Some(e) = event_rx.recv().await {
                match e {
                    CrawlEvent::Warning(w) => eprintln!("warning: {w}"),
                    CrawlEvent::Progress(p) => {
                        speed.peak = speed.peak.max(p.urls_per_sec);
                        speed.average = p.avg_urls_per_sec;
                        if !quiet && last.elapsed() >= Duration::from_secs(1) {
                            last = Instant::now();
                            let eta = p
                                .eta_secs
                                .map(|s| format!("  ~{} left for the URLs found so far", duration(s)))
                                .unwrap_or_default();
                            eprintln!(
                                "crawled {}  queued {}  in flight {}  {:.0} URLs/s{eta}",
                                p.crawled, p.queued, p.in_flight, p.urls_per_sec
                            );
                        }
                    }
                }
            }
            speed
        });
        let started = Instant::now();
        let result = crawler.run(control_rx, Some(event_tx)).await;
        let mut speed = speed.await.unwrap_or_default();
        speed.secs = started.elapsed().as_secs();
        result.map(|s| (s, speed))
    });
    let (summary, speed) = match result {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };
    print_summary(&summary, cmd.json, &file);
    if !cmd.json && !quiet {
        eprintln!(
            "took {} at {:.0} URLs/s on average, {:.0} at peak",
            duration(speed.secs),
            speed.average,
            speed.peak
        );
    }
    if summary.status == "paused" {
        eprintln!(
            "paused; resume with: spiderlens crawl --resume {}",
            file.display()
        );
        return ExitCode::from(exit::PAUSED);
    }
    if summary.crawled == 0 && summary.discovered > 0 {
        eprintln!(
            "error: nothing was crawled; the start URL is excluded, blocked or unreachable (see above)"
        );
        return ExitCode::from(exit::USAGE);
    }
    ExitCode::from(exit::FETCHED)
}

/// End-of-crawl speed (spec 008 FR-030, FR-031).
#[derive(Debug, Default)]
struct SpeedSummary {
    secs: u64,
    average: f64,
    peak: f64,
}

/// `42s`, `3m 05s`, `2h 10m`.
fn duration(secs: u64) -> String {
    match secs {
        s if s < 60 => format!("{s}s"),
        s if s < 3600 => format!("{}m {:02}s", s / 60, s % 60),
        s => format!("{}h {:02}m", s / 3600, (s % 3600) / 60),
    }
}

fn print_summary(s: &CrawlSummary, json: bool, file: &Path) {
    if json {
        let mut v = serde_json::to_value(s).expect("summary serializes");
        v["file"] = serde_json::Value::String(file.display().to_string());
        print_json(&v);
        return;
    }
    let mut out = std::io::stdout().lock();
    let _ = writeln!(out, "{:<16} {}", "Status", s.status);
    let _ = writeln!(out, "{:<16} {}", "Discovered", s.discovered);
    let _ = writeln!(out, "{:<16} {}", "Crawled", s.crawled);
    let _ = writeln!(out, "{:<16} {}", "Not crawled", s.not_crawled);
    for (reason, n) in &s.by_reason {
        let _ = writeln!(out, "  {:<32} {n}", reason.label());
    }
    if s.queued > 0 {
        let _ = writeln!(out, "{:<16} {}", "Still queued", s.queued);
    }
    let _ = writeln!(out, "{:<16} {}", "External checks", s.external_checked);
    if s.external_blocked > 0 {
        let _ = writeln!(out, "  {:<32} {}", "Blocked by robots.txt", s.external_blocked);
    }
    if s.url_limit_reached {
        let _ = writeln!(
            out,
            "URL limit reached: the remaining URLs are listed as \"Over URL limit\"."
        );
    }
    for h in &s.hosts {
        let delay = h
            .crawl_delay_secs
            .map(|d| format!(", crawl-delay {d} s"))
            .unwrap_or_default();
        let err = h.error.as_deref().map(|e| format!(" ({e})")).unwrap_or_default();
        let _ = writeln!(out, "robots.txt       {}  {}{delay}{err}", h.host, h.robots);
    }
    for m in &s.sitemaps {
        let err = m
            .error
            .as_deref()
            .map(|e| format!("  error: {e}"))
            .unwrap_or_default();
        let _ = writeln!(out, "sitemap          {}  {} URLs{err}", m.url, m.urls);
    }
    let _ = writeln!(out, "{:<16} {}", "File", file.display());
}

pub fn run_report(args: ReportArgs) -> ExitCode {
    let mut reasons = Vec::new();
    for r in &args.reason {
        match NotCrawledReason::parse(r) {
            Some(r) => reasons.push(r),
            None => {
                let all: Vec<_> = NotCrawledReason::ALL.iter().map(|r| r.as_str()).collect();
                eprintln!("error: unknown reason \"{r}\"; use one of: {}", all.join(", "));
                return ExitCode::from(exit::USAGE);
            }
        }
    }
    let store = match CrawlStore::open(&args.file) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::from(exit::USAGE);
        }
    };
    let state = if args.not_crawled || !reasons.is_empty() {
        Some(UrlState::NotCrawled)
    } else if args.crawled {
        Some(UrlState::Crawled)
    } else {
        match CrawlSummary::read(&args.file) {
            Ok(s) => print_summary(&s, args.json, &args.file),
            Err(e) => {
                eprintln!("error: {e}");
                return ExitCode::FAILURE;
            }
        }
        return ExitCode::from(exit::FETCHED);
    };
    let mut out = std::io::stdout().lock();
    let result = store.rows(state, &reasons, |row| {
        if args.json {
            let _ = serde_json::to_writer(&mut out, &row);
            let _ = writeln!(out);
        } else {
            let _ = writeln!(
                out,
                "{}\t{}\t{}\t{}\t{}\t{}\t{}",
                row.state,
                row.reason.map(|r| r.as_str()).unwrap_or("-"),
                row.reason_detail.as_deref().unwrap_or("-"),
                row.depth,
                row.source,
                row.found_on.as_deref().unwrap_or("-"),
                row.url
            );
        }
        Ok(())
    });
    if let Err(e) = result {
        eprintln!("error: {e}");
        return ExitCode::FAILURE;
    }
    ExitCode::from(exit::FETCHED)
}

pub fn run_scope_test(args: ScopeTestArgs) -> ExitCode {
    let resolved = match args.crawl.resolve(spiderlens_config::Mode::Spider) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::from(exit::USAGE);
        }
    };
    let config = resolved.config;
    let start = match normalize_url(&args.start) {
        Ok(u) => u,
        Err(e) => {
            eprintln!("error: --start: {e}");
            return ExitCode::from(exit::USAGE);
        }
    };
    let rules = match ScopeRules::new(&config, std::slice::from_ref(&start)) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::from(exit::USAGE);
        }
    };
    let rt = match runtime() {
        Ok(rt) => rt,
        Err(code) => return code,
    };
    let mut policies: HashMap<String, RobotsPolicy> = HashMap::new();
    let mut results = Vec::new();
    for input in &args.urls {
        let policy = match rules.normalize(&start, input) {
            Ok(url) if args.fetch_robots && config.robots.mode != RobotsMode::Ignore => {
                let key = host_key(&url);
                if !policies.contains_key(&key) {
                    let p = rt
                        .block_on(fetch_robots(&config, &url))
                        .unwrap_or_else(|_| RobotsPolicy::disallow_all());
                    policies.insert(key.clone(), p);
                }
                policies.get(&key)
            }
            _ => None,
        };
        results.push(test_url(&rules, &start, input, policy, config.robots.mode));
    }
    if args.json {
        print_json(&results);
        return ExitCode::from(exit::FETCHED);
    }
    for t in &results {
        let url = t.url.as_deref().unwrap_or(&t.input);
        let verdict = match t.reason {
            None if t.robots_checked => "would crawl".to_string(),
            None => "would crawl (robots.txt not checked)".to_string(),
            Some(r) => match &t.detail {
                Some(d) => format!("would not be crawled: {} ({d})", r.label()),
                None => format!("would not be crawled: {}", r.label()),
            },
        };
        println!("{url}  {verdict}");
    }
    ExitCode::from(exit::FETCHED)
}
