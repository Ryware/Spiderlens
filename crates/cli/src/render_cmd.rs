//! `spiderlens render FILE` (spec 014 T072, contracts/cli.md): renders the
//! pages of an existing, finished crawl from their stored raw HTML. The
//! main documents are not requested again; page resources are, politely.
//! Ctrl-C pauses; running the command again continues.

use std::path::PathBuf;
use std::process::ExitCode;
use std::time::{Duration, Instant};

use clap::Args;
use spiderlens_config::{CrawlConfig, RenderMode, RenderSettings, validate_config};
use spiderlens_engine::crawl::{Control, CrawlError, CrawlEvent, Crawler};
use tokio::sync::{mpsc, watch};

use crate::browser::BrowserChoice;
use crate::crawl_cmd::{duration, print_summary, render_line, runtime};
use crate::exit;
use crate::options::RenderingArgs;
use crate::print_issues;

#[derive(Args)]
pub struct RenderArgs {
    /// Crawl file to render. It must be finished and have its page HTML stored.
    file: PathBuf,
    /// The browser to render with [default: detected]. Not stored in the
    /// crawl file.
    #[arg(long, value_name = "PATH", help_heading = "Rendering")]
    browser: Option<PathBuf>,
    /// Download the pinned browser if none is usable, without asking.
    #[arg(long, help_heading = "Rendering")]
    allow_browser_download: bool,
    #[command(flatten)]
    rendering: RenderingArgs,
    /// Print the summary as JSON.
    #[arg(long)]
    json: bool,
    /// No progress on stderr.
    #[arg(long)]
    quiet: bool,
}

/// The rendering settings the flags ask for, over the defaults, checked with
/// the same validation `crawl --render` gets.
fn settings(a: &RenderArgs) -> Result<RenderSettings, String> {
    let mut c = CrawlConfig::default();
    c.rendering.mode = RenderMode::Javascript;
    a.rendering.apply(&mut c.rendering)?;
    let issues: Vec<_> = validate_config(&c)
        .into_iter()
        .filter(|i| i.path.starts_with("rendering."))
        .collect();
    if print_issues(&issues) {
        return Err("the rendering settings are not valid".into());
    }
    Ok(c.rendering)
}

pub fn run_render(args: RenderArgs) -> ExitCode {
    let settings = match settings(&args) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::from(exit::USAGE);
        }
    };
    // Refusals come before a browser is looked for or downloaded.
    if let Err(e) = Crawler::check_renderable(&args.file) {
        eprintln!("error: {e}");
        return ExitCode::from(exit::USAGE);
    }
    let browser = match crate::browser::choose(&BrowserChoice {
        path: args.browser.clone(),
        allow_download: args.allow_browser_download,
        quiet: args.quiet,
    }) {
        Ok(b) => b,
        Err(code) => return code,
    };
    let crawler = match Crawler::open_for_render(&args.file, settings) {
        Ok(c) => c.with_browser(browser),
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::from(exit::USAGE);
        }
    };
    let (quiet, json, file) = (args.quiet, args.json, args.file);
    let rt = match runtime() {
        Ok(rt) => rt,
        Err(code) => return code,
    };
    let result = rt.block_on(async move {
        let (control_tx, control_rx) = watch::channel(Control::Run);
        let (event_tx, mut event_rx) = mpsc::unbounded_channel();
        // Ctrl-C pauses; a second Ctrl-C exits at once. The file stays
        // consistent and the next run continues.
        tokio::spawn(async move {
            if tokio::signal::ctrl_c().await.is_ok() {
                eprintln!("\npausing: waiting for pages being rendered (Ctrl-C again to quit now)...");
                let _ = control_tx.send(Control::Pause);
                if tokio::signal::ctrl_c().await.is_ok() {
                    std::process::exit(130);
                }
            }
            std::future::pending::<()>().await;
        });
        let progress = tokio::spawn(async move {
            let mut last = Instant::now() - Duration::from_secs(1);
            while let Some(e) = event_rx.recv().await {
                match e {
                    CrawlEvent::Warning(w) => eprintln!("warning: {w}"),
                    CrawlEvent::Notice(n) => eprintln!("notice: {}", n.message),
                    CrawlEvent::Progress(p) => {
                        if !quiet
                            && last.elapsed() >= Duration::from_secs(1)
                            && let Some(r) = &p.render
                        {
                            last = Instant::now();
                            eprintln!("{}", render_line(r));
                        }
                    }
                }
            }
        });
        let started = Instant::now();
        let result = crawler.run(control_rx, Some(event_tx)).await;
        let _ = progress.await;
        result.map(|s| (s, started.elapsed().as_secs()))
    });
    let (summary, secs) = match result {
        Ok(r) => r,
        Err(CrawlError::Local(e)) => {
            eprintln!("paused: {e}");
            eprintln!("continue with: spiderlens render {}", e.path.display());
            return ExitCode::from(exit::LOCAL);
        }
        Err(CrawlError::Browser(m)) => {
            eprintln!("error: {m}");
            return ExitCode::from(exit::NO_BROWSER);
        }
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };
    print_summary(&summary, json, &file);
    if !json && !quiet {
        eprintln!("took {}", duration(secs));
    }
    if summary.status == "paused" {
        eprintln!("paused; continue with: spiderlens render {}", file.display());
        return ExitCode::from(exit::PAUSED);
    }
    ExitCode::from(exit::FETCHED)
}
