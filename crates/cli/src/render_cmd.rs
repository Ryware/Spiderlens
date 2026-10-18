//! `spiderlens render FILE` (spec 014 T072, contracts/cli.md): renders the
//! pages of an existing, finished crawl from their stored raw HTML. The
//! main documents are not requested again; page resources are, politely.
//! Ctrl-C pauses; running the command again continues.

use std::path::PathBuf;
use std::process::ExitCode;
use std::time::{Duration, Instant};

use clap::Args;
use spiderlens_config::{RenderSettings, UrlFilter, WindowPreset};
use spiderlens_engine::crawl::{Control, CrawlError, CrawlEvent, Crawler};
use tokio::sync::{mpsc, watch};

use crate::browser::BrowserChoice;
use crate::crawl_cmd::{duration, print_summary, render_line, runtime};
use crate::exit;

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
    /// The longest one page may take, 1 to 60 seconds [default: 10].
    #[arg(long, value_name = "SECONDS", help_heading = "Rendering")]
    render_timeout: Option<f64>,
    /// Seconds waited before the page counts as settled [default: 0].
    #[arg(long, value_name = "SECONDS", help_heading = "Rendering")]
    render_min_wait: Option<f64>,
    /// auto, mobile, desktop or WIDTHxHEIGHT [default: auto].
    #[arg(long, value_name = "WINDOW", help_heading = "Rendering")]
    render_window: Option<String>,
    /// Resource types not loaded: a comma list of images, media, fonts,
    /// stylesheets, third-party, or none [default: images,media,fonts].
    #[arg(long, value_name = "LIST", help_heading = "Rendering")]
    render_block: Option<String>,
    /// Only render URLs matching this regex. Repeatable.
    #[arg(long, value_name = "REGEX", help_heading = "Rendering")]
    render_include: Vec<String>,
    /// Do not render URLs matching this regex. Repeatable.
    #[arg(long, value_name = "REGEX", help_heading = "Rendering")]
    render_exclude: Vec<String>,
    /// Also render internal 4xx and 5xx HTML pages.
    #[arg(long, help_heading = "Rendering")]
    render_error_pages: bool,
    /// Pages rendered at once, 1 to 16, or auto [default: auto].
    #[arg(long, value_name = "N", help_heading = "Rendering")]
    render_tabs: Option<String>,
    /// Browser memory budget in MB, or auto [default: auto].
    #[arg(long, value_name = "MB", help_heading = "Rendering")]
    render_memory: Option<String>,
    /// Print the summary as JSON.
    #[arg(long)]
    json: bool,
    /// No progress on stderr.
    #[arg(long)]
    quiet: bool,
}

/// The rendering settings the flags ask for, over the defaults.
fn settings(a: &RenderArgs) -> Result<RenderSettings, String> {
    let mut s = spiderlens_config::CrawlConfig::default().rendering;
    s.mode = spiderlens_config::RenderMode::Javascript;
    if let Some(t) = a.render_timeout {
        if !(1.0..=60.0).contains(&t) {
            return Err("--render-timeout takes 1 to 60 seconds".into());
        }
        s.max_render_secs = t;
    }
    if let Some(w) = a.render_min_wait {
        if !(0.0..=s.max_render_secs).contains(&w) {
            return Err("--render-min-wait takes 0 up to --render-timeout seconds".into());
        }
        s.min_wait_secs = w;
    }
    if let Some(w) = &a.render_window {
        match w.to_ascii_lowercase().as_str() {
            "auto" => s.window.preset = WindowPreset::Auto,
            "mobile" => s.window.preset = WindowPreset::Mobile,
            "desktop" => s.window.preset = WindowPreset::Desktop,
            other => {
                let size = other
                    .split_once('x')
                    .and_then(|(w, h)| Some((w.trim().parse::<u32>().ok()?, h.trim().parse::<u32>().ok()?)));
                match size {
                    Some((w, h)) if (320..=3840).contains(&w) && (480..=4320).contains(&h) => {
                        s.window.preset = WindowPreset::Custom;
                        s.window.width = Some(w);
                        s.window.height = Some(h);
                    }
                    _ => {
                        return Err(
                            "--render-window takes auto, mobile, desktop or WIDTHxHEIGHT (320 to 3840 by 480 to 4320)"
                                .into(),
                        );
                    }
                }
            }
        }
    }
    if let Some(list) = &a.render_block {
        s.block.images = false;
        s.block.media = false;
        s.block.fonts = false;
        s.block.stylesheets = false;
        s.block.third_party = false;
        for item in list.split(',').map(|i| i.trim().to_ascii_lowercase()) {
            match item.as_str() {
                "images" => s.block.images = true,
                "media" => s.block.media = true,
                "fonts" => s.block.fonts = true,
                "stylesheets" => s.block.stylesheets = true,
                "third-party" => s.block.third_party = true,
                "none" | "" => {}
                other => return Err(format!("--render-block does not know \"{other}\"")),
            }
        }
    }
    s.include = a.render_include.clone();
    s.exclude = a.render_exclude.clone();
    if UrlFilter::new(&s.include, &s.exclude).is_err() {
        return Err("--render-include or --render-exclude is not a valid regex".into());
    }
    s.render_error_pages = a.render_error_pages;
    if let Some(t) = &a.render_tabs
        && !t.eq_ignore_ascii_case("auto")
    {
        match t.parse::<u32>() {
            Ok(n) if (1..=16).contains(&n) => s.concurrency = Some(n),
            _ => return Err("--render-tabs takes 1 to 16, or auto".into()),
        }
    }
    if let Some(m) = &a.render_memory
        && !m.eq_ignore_ascii_case("auto")
    {
        match m.parse::<u32>() {
            Ok(n) if (256..=65_536).contains(&n) => s.memory_budget_mb = Some(n),
            _ => return Err("--render-memory takes 256 to 65536 MB, or auto".into()),
        }
    }
    Ok(s)
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
