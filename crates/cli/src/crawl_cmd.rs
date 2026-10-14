//! `spiderlens crawl`, `report` and `scope test` (spec 003 contracts/cli.md).

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{Duration, Instant};

use clap::{ArgMatches, Args, parser::ValueSource};
use spiderlens_config::Change;
use spiderlens_config::{ModeInput, normalize_url, validate};
use spiderlens_engine::crawl::file::{self, DEFAULT_DISK_RESERVE};
use spiderlens_engine::crawl::{
    Control, CrawlError, CrawlEvent, CrawlStore, CrawlSummary, Crawler, NotCrawledReason, ResumeOptions,
    Upgrade, UrlState, default_file_name,
};
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
    /// Pause when less than this much space (MB) is free on the crawl
    /// file's drive.
    #[arg(long, value_name = "MB", default_value_t = DEFAULT_DISK_RESERVE / (1024 * 1024))]
    disk_reserve: u64,
    /// With --resume: upgrade an older crawl file without keeping a copy of
    /// the original.
    #[arg(long, requires = "resume")]
    no_backup: bool,
    /// With --resume: the value of a secret header, which crawl files never
    /// store. Repeatable.
    #[arg(long, value_name = "NAME=VALUE", requires = "resume")]
    header_secret: Vec<String>,
    #[command(flatten)]
    crawl: CrawlArgs,
}

#[derive(Args)]
pub struct InfoArgs {
    /// Crawl file.
    file: PathBuf,
    /// Print as JSON.
    #[arg(long)]
    json: bool,
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
        // The stored configuration decides; only speed and the URL limit may
        // change (spec 006 FR-011). Other options are refused, not ignored.
        const ALLOWED: [&str; 12] = [
            "resume",
            "json",
            "quiet",
            "disk_reserve",
            "no_backup",
            "header_secret",
            "speed",
            "concurrency",
            "per_host",
            "rate",
            "no_adaptive",
            "max_urls",
        ];
        let extra: Vec<String> = matches
            .ids()
            .map(|id| id.as_str().to_string())
            .filter(|id| !ALLOWED.contains(&id.as_str()))
            .filter(|id| matches.value_source(id) == Some(ValueSource::CommandLine))
            .collect();
        if !extra.is_empty() {
            eprintln!(
                "error: --resume uses the settings stored in the crawl file (only speed and --max-urls can change); remove: --{}",
                extra.join(", --").replace('_', "-")
            );
            return ExitCode::from(exit::USAGE);
        }
        let mut secrets = Vec::new();
        for s in &cmd.header_secret {
            match s.split_once('=') {
                Some((n, v)) if !n.trim().is_empty() => secrets.push((n.trim().to_string(), v.to_string())),
                _ => {
                    eprintln!("error: --header-secret takes NAME=VALUE");
                    return ExitCode::from(exit::USAGE);
                }
            }
        }
        let mut changes = Vec::new();
        if let Ok(info) = file::info(path)
            && let Some(stored) = &info.config
        {
            let current = stored.effective();
            if let Some(speed) = cmd.crawl.speed_change(current.speed)
                && speed != current.speed
            {
                changes.push(Change::Speed(speed));
            }
            if let Some(max) = cmd.crawl.max_urls
                && Some(max) != current.limits.max_urls
            {
                changes.push(Change::MaxUrls(Some(max)));
            }
        }
        let options = ResumeOptions {
            upgrade: Some(if cmd.no_backup {
                Upgrade::NoCopy
            } else {
                Upgrade::KeepCopy
            }),
            changes,
            secrets,
        };
        match Crawler::open_with(path, &options) {
            Ok(c) => {
                if !cmd.quiet {
                    if let Some(n) = c.opened().recovered {
                        eprintln!(
                            "recovered: the crawl was not closed cleanly; {n} URLs returned to the queue"
                        );
                    }
                    if let Some((from, backup)) = &c.opened().upgraded {
                        match backup {
                            Some(b) => eprintln!(
                                "upgraded the crawl file from format {from}; original kept as {}",
                                b.display()
                            ),
                            None => {
                                eprintln!("upgraded the crawl file from format {from}; original not kept")
                            }
                        }
                    }
                }
                (c, path.clone())
            }
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
    let crawler = crawler.with_disk_reserve(Some(cmd.disk_reserve.saturating_mul(1024 * 1024)));
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
        // A disk or write problem: the file is consistent and resumable
        // (spec 006 FR-040).
        Err(CrawlError::Local(e)) => {
            eprintln!("paused: {e}");
            eprintln!("resume with: spiderlens crawl --resume {}", e.path.display());
            return ExitCode::from(exit::LOCAL);
        }
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };
    print_summary(&summary, cmd.json, &file);
    record_issue_settings(&file);
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

/// Records the rule set version and issue settings in the file at run end
/// (spec 009 FR-013). The issues themselves are evaluated when read.
fn record_issue_settings(file: &Path) {
    let stored = spiderlens_results::Reader::open(file).and_then(|r| r.stored_issue_settings());
    if let Ok(Some(s)) = stored
        && let Err(e) = spiderlens_results::issues::record_issue_settings(file, &s)
    {
        eprintln!("warning: could not record the issue settings: {e}");
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
    let store = match CrawlStore::open_read_only(&args.file) {
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
    let rt = match runtime() {
        Ok(rt) => rt,
        Err(code) => return code,
    };
    let results = match rt.block_on(spiderlens_engine::crawl::scope_test(
        &config,
        &start,
        &args.urls,
        args.fetch_robots,
    )) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::from(exit::USAGE);
        }
    };
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

#[derive(Args)]
pub struct PageArgs {
    /// Crawl file.
    file: PathBuf,
    /// URL as crawled.
    url: String,
    /// Print one JSON object.
    #[arg(long)]
    json: bool,
}

#[derive(Args)]
pub struct HtmlArgs {
    /// Crawl file.
    file: PathBuf,
    /// URL as crawled.
    url: String,
    /// Write to this file instead of standard output.
    #[arg(long, value_name = "PATH")]
    out: Option<PathBuf>,
}

/// Finds the URL as given, then in its normalized form.
fn find_url(store: &CrawlStore, url: &str) -> Option<String> {
    let mut candidates = vec![url.trim().to_string()];
    if let Ok(n) = normalize_url(url) {
        candidates.push(n.to_string());
    }
    candidates
        .into_iter()
        .find(|c| matches!(store.id_of(c), Ok(Some(_))))
}

fn open_for_page(file: &Path, url: &str) -> Result<(CrawlStore, String), ExitCode> {
    let store = CrawlStore::open_read_only(file).map_err(|e| {
        eprintln!("error: {e}");
        ExitCode::from(exit::USAGE)
    })?;
    match find_url(&store, url) {
        Some(u) => Ok((store, u)),
        None => {
            eprintln!("error: {url} is not in {}", file.display());
            Err(ExitCode::FAILURE)
        }
    }
}

pub fn run_page(args: PageArgs) -> ExitCode {
    let (store, url) = match open_for_page(&args.file, &args.url) {
        Ok(x) => x,
        Err(code) => return code,
    };
    let report = match store.page_report(&url) {
        Ok(Some(r)) => r,
        Ok(None) => return ExitCode::FAILURE,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };
    if args.json {
        print_json(&report);
    } else {
        print_page(&report);
    }
    ExitCode::from(exit::FETCHED)
}

fn print_page(r: &serde_json::Value) {
    use serde_json::Value;
    let s = |v: &Value| match v {
        Value::Null => "-".to_string(),
        Value::String(s) => s.clone(),
        other => other.to_string(),
    };
    let mut out = std::io::stdout().lock();
    let _ = writeln!(out, "{}", s(&r["url"]));
    for (label, key) in [
        ("Status", "status"),
        ("Content type", "content_type"),
        ("Response time (ms)", "response_ms"),
        ("Size (bytes)", "size_bytes"),
        ("Redirects to", "redirect_to"),
        ("Error", "error_message"),
        ("Title", "title"),
        ("Canonical", "canonical"),
        ("Noindex", "noindex"),
    ] {
        if !r[key].is_null() {
            let _ = writeln!(out, "  {label}: {}", s(&r[key]));
        }
    }
    let p = &r["page"];
    if !p.is_null() {
        let _ = writeln!(out, "  Meta description: {}", s(&p["meta_description"]));
        let _ = writeln!(out, "  H1: {}  ({} total)", s(&p["h1_1"]), s(&p["h1_count"]));
        let _ = writeln!(out, "  Word count: {}", s(&p["word_count"]));
        for flag in ["partial", "encoding_errors", "no_content"] {
            if p[flag] == 1 {
                let _ = writeln!(out, "  Problem: {flag}");
            }
        }
        if !p["extract_error"].is_null() {
            let _ = writeln!(out, "  Problem: {}", s(&p["extract_error"]));
        }
    }
    for h in r["headings"].as_array().into_iter().flatten() {
        let _ = writeln!(out, "  H{}: {}", s(&h["level"]), s(&h["text"]));
    }
    for d in r["directives"].as_array().into_iter().flatten() {
        let _ = writeln!(
            out,
            "  Robots ({}, {}): {}{}",
            s(&d["source"]),
            s(&d["crawler"]),
            s(&d["directive"]),
            d["value"].as_str().map(|v| format!(": {v}")).unwrap_or_default()
        );
    }
    let links = r["links"].as_array().map_or(0, Vec::len);
    let images = r["links"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|l| l["kind"] == "image")
        .count();
    let _ = writeln!(out, "  References: {links} ({images} images)");
    let _ = writeln!(
        out,
        "  Structured data blocks: {}",
        r["structured_data"].as_array().map_or(0, Vec::len)
    );
    let _ = writeln!(out, "  Raw HTML stored: {}", s(&r["html_stored"]));
}

pub fn run_html(args: HtmlArgs) -> ExitCode {
    let (store, url) = match open_for_page(&args.file, &args.url) {
        Ok(x) => x,
        Err(code) => return code,
    };
    let id = match store.id_of(&url) {
        Ok(Some(id)) => id,
        _ => return ExitCode::FAILURE,
    };
    let html = match store.page_html(id) {
        Ok(Some(h)) => h,
        Ok(None) => {
            eprintln!("error: this crawl did not store the HTML of {url}");
            return ExitCode::FAILURE;
        }
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };
    let written = match &args.out {
        Some(path) => std::fs::write(path, &html),
        None => std::io::stdout().lock().write_all(&html),
    };
    match written {
        Ok(()) => ExitCode::from(exit::FETCHED),
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

/// `spiderlens info` (spec 006 contracts/cli.md): never changes the file.
pub fn run_info(args: InfoArgs) -> ExitCode {
    let info = match file::info(&args.file) {
        Ok(i) => i,
        Err(e) => {
            eprintln!("error: {e}");
            if file::salvageable(&args.file)
                && matches!(e, spiderlens_engine::crawl::store::StoreError::Damaged(_))
            {
                eprintln!("part of the file is readable; the desktop app can recover what can be read");
            }
            return ExitCode::from(exit::USAGE);
        }
    };
    if args.json {
        print_json(&serde_json::to_value(&info).expect("info serializes"));
        return ExitCode::from(exit::FETCHED);
    }
    let mut out = std::io::stdout().lock();
    let row = |out: &mut std::io::StdoutLock, k: &str, v: &str| {
        let _ = writeln!(out, "{k:<16} {v}");
    };
    row(&mut out, "Name", &info.name);
    row(&mut out, "File", &info.path.display().to_string());
    row(&mut out, "Size", &file::format_bytes(info.size_bytes));
    row(&mut out, "Format", &info.format_version.to_string());
    let mut status = info.status.clone();
    if let Some(r) = &info.pause_reason {
        status.push_str(&format!(" ({r})"));
    }
    if info.recovered {
        status.push_str(", recovered after a crash");
    }
    row(&mut out, "Status", &status);
    row(&mut out, "Mode", &format!("{:?}", info.mode).to_lowercase());
    if let Some(u) = &info.start_url {
        row(&mut out, "Start URL", u);
    }
    if let Some(t) = &info.started_at {
        row(&mut out, "Started", t);
    }
    if let Some(t) = &info.ended_at {
        row(&mut out, "Ended", t);
    }
    let c = &info.counts;
    row(
        &mut out,
        "URLs",
        &format!(
            "{} discovered, {} crawled ({} failed), {} not crawled, {} queued",
            c.discovered, c.crawled, c.failed, c.not_crawled, c.queued
        ),
    );
    for v in &info.versions {
        row(
            &mut out,
            &format!("Version ({})", v.kind),
            &format!("{} {} {}", v.app_version, v.os, v.at),
        );
    }
    let access = match &info.access {
        spiderlens_engine::crawl::Access::Writable => "can be resumed".to_string(),
        spiderlens_engine::crawl::Access::InUse { by } => format!("in use by {}", by.describe()),
        spiderlens_engine::crawl::Access::ReadOnlyMedia => "read-only here; save a copy to resume".into(),
        spiderlens_engine::crawl::Access::NewerFormat { needs } => {
            format!("view only; resuming needs {needs}")
        }
        spiderlens_engine::crawl::Access::NeedsUpgrade { from } => {
            format!("format {from}; resuming upgrades it (a copy of the original is kept)")
        }
        spiderlens_engine::crawl::Access::ViewOnly { reason } => reason.clone(),
    };
    row(&mut out, "Access", &access);
    if !info.missing_secrets.is_empty() {
        row(&mut out, "Secrets needed", &info.missing_secrets.join(", "));
    }
    if let Some(cfg) = &info.config {
        let json = serde_json::to_string_pretty(&cfg.effective()).expect("config serializes");
        let _ = writeln!(out, "Configuration");
        for line in json.lines() {
            let _ = writeln!(out, "  {line}");
        }
    }
    if !info.events.is_empty() {
        let _ = writeln!(out, "History");
        for e in &info.events {
            let _ = writeln!(
                out,
                "  {}  {:<14} {} {}{}",
                e.at,
                e.kind,
                e.app_version,
                e.os,
                e.detail.as_deref().map(|d| format!("  {d}")).unwrap_or_default()
            );
        }
    }
    ExitCode::from(exit::FETCHED)
}
