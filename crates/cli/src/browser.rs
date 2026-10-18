//! The browser check before a rendering crawl (spec 014 FR-045, plan D12,
//! contracts/cli.md "Browser check"). Runs before the crawl file is
//! created, so a crawl never starts in rendering mode without a browser.

use std::io::Write;
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

use clap::{Args, Subcommand};
use spiderlens_render::{
    BrowserInfo, BrowserSource, CancellationFlag, DownloadError, DownloadOptions, DownloadProgress,
    DownloadStage, FindOptions, NotUsable, data_dir, download_plan, download_with, find_browser,
    remove_download, should_offer_download,
};

use crate::exit;

/// `--browser` and `--allow-browser-download`: machine preferences, never
/// stored in the crawl file.
pub struct BrowserChoice {
    pub path: Option<PathBuf>,
    pub allow_download: bool,
    pub quiet: bool,
}

fn source_label(s: BrowserSource) -> &'static str {
    match s {
        BrowserSource::Downloaded => "downloaded",
        BrowserSource::Installed => "installed",
        BrowserSource::UserPath => "your path",
    }
}

/// The message for "no usable browser". `can_download` names the download
/// when this build has one for the platform.
pub fn not_usable_message(e: &NotUsable, can_download: bool) -> String {
    let fix = if can_download {
        "Run \"spiderlens browser install\" (downloads about 100 MB) or add --allow-browser-download, give an installed Chrome or Edge with --browser PATH, or crawl without --render."
    } else {
        "Install Google Chrome, Chromium or Microsoft Edge, or give a browser with --browser PATH, or crawl without --render."
    };
    match e {
        NotUsable::NotFound => {
            format!("JavaScript rendering needs a Chromium-based browser and none was found. {fix}")
        }
        NotUsable::TooOld { found, min_major } => format!(
            "JavaScript rendering needs a Chromium-based browser version {min_major} or newer; {} {} is too old. {fix}",
            found.name.label(),
            found.version
        ),
        NotUsable::Broken { path, message } => format!(
            "The browser at {} cannot be used: {message} {fix}",
            path.display()
        ),
    }
}

/// Finds the browser rendering uses, downloading the pinned one first when
/// the policy prefers it and `--allow-browser-download` gives consent
/// (`spiderlens_render::should_offer_download`). Prints the renderer notice;
/// on failure prints the error and returns exit 9.
pub fn choose(choice: &BrowserChoice) -> Result<BrowserInfo, ExitCode> {
    let data = data_dir().unwrap_or_else(std::env::temp_dir);
    let found = match find_browser(&FindOptions {
        user_path: choice.path.clone(),
        data_dir: data.clone(),
    }) {
        // Tests pin an older browser with --browser (as
        // `spiderlens_render::browser_for_tests` does); never documented
        // for users.
        Err(NotUsable::TooOld { found, .. })
            if choice.path.is_some()
                && std::env::var("SPIDERLENS_ACCEPT_OLD_BROWSER").as_deref() == Ok("1") =>
        {
            Ok(found)
        }
        other => other,
    };
    let plan = download_plan();
    let mut chosen = found.clone();
    if should_offer_download(&found, plan.is_some())
        && choice.allow_download
        && let Some(plan) = &plan
    {
        let quiet = choice.quiet;
        let progress = progress_line(quiet);
        let downloaded = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| e.to_string())
            .and_then(|rt| {
                rt.block_on(download_with(
                    plan,
                    &data,
                    &cli_download_options(),
                    &progress,
                    &CancellationFlag::new(),
                ))
                .map_err(|e| failure_text(&e))
            });
        if !quiet {
            eprintln!();
        }
        match downloaded {
            Ok(b) => chosen = Ok(b),
            // An installed browser still renders; without one, say why.
            Err(e) if chosen.is_err() => {
                eprintln!("error: {e}");
                return Err(ExitCode::from(exit::NO_BROWSER));
            }
            Err(e) => eprintln!("warning: {e} Using the installed browser."),
        }
    }
    match chosen {
        Ok(b) => {
            eprintln!(
                "notice: rendering with {} {} ({})",
                b.name.label(),
                b.version,
                source_label(b.source)
            );
            Ok(b)
        }
        Err(e) => {
            eprintln!("error: {}", not_usable_message(&e, plan.is_some()));
            Err(ExitCode::from(exit::NO_BROWSER))
        }
    }
}

/// The download waits for a lost connection and resumes, but a terminal
/// does not wait forever: after ten minutes it stops and keeps the part.
fn cli_download_options() -> DownloadOptions {
    DownloadOptions {
        retry_every: Duration::from_secs(5),
        give_up_after: Some(Duration::from_secs(600)),
    }
}

fn progress_line(quiet: bool) -> impl Fn(DownloadProgress) + Send + Sync {
    move |p: DownloadProgress| {
        if quiet {
            return;
        }
        let text = if p.waiting {
            "Connection lost, waiting to resume".to_owned()
        } else {
            match p.stage {
                DownloadStage::Download if p.total_bytes > 0 => format!(
                    "downloading the browser: {} of {} MB",
                    p.done_bytes / 1_000_000,
                    p.total_bytes / 1_000_000
                ),
                DownloadStage::Download => return,
                DownloadStage::Verify => "checking the download".to_owned(),
                DownloadStage::Unpack => "unpacking".to_owned(),
            }
        };
        eprint!("\r{text:<60}");
        let _ = std::io::stderr().flush();
    }
}

/// The error text of a failed download, with the way out when the cause is
/// the missing connection.
fn failure_text(e: &DownloadError) -> String {
    match e {
        DownloadError::Network(_) | DownloadError::ConnectionLost => format!(
            "{e} Give an installed Chrome or Edge with --browser <path>, or crawl without --render."
        ),
        other => other.to_string(),
    }
}

/// Exit code of a failed download: a checksum mismatch is 4, everything
/// else (no internet, no pin for this platform) is "no browser".
fn failure_code(e: &DownloadError) -> u8 {
    match e {
        DownloadError::Checksum => exit::FAILED,
        _ => exit::NO_BROWSER,
    }
}

#[derive(Subcommand)]
pub enum BrowserCommand {
    /// Show the browser rendering would use, or why none is usable.
    Status(StatusArgs),
    /// Download the pinned chrome-headless-shell into the SpiderLens data folder.
    Install(InstallArgs),
    /// Delete the downloaded browser. Never touches an installed Chrome or Edge.
    Remove,
}

#[derive(Args)]
pub struct StatusArgs {
    /// An installed Chrome or Edge to check instead of the downloaded browser.
    #[arg(long, value_name = "PATH")]
    browser: Option<PathBuf>,
    /// Print as JSON.
    #[arg(long)]
    json: bool,
}

#[derive(Args)]
pub struct InstallArgs {
    /// Do not ask before downloading.
    #[arg(long)]
    yes: bool,
    /// Print the result as JSON.
    #[arg(long)]
    json: bool,
}

pub fn run(cmd: BrowserCommand) -> ExitCode {
    match cmd {
        BrowserCommand::Status(a) => status(a),
        BrowserCommand::Install(a) => install(a),
        BrowserCommand::Remove => remove(),
    }
}

fn status(args: StatusArgs) -> ExitCode {
    let data = data_dir().unwrap_or_else(std::env::temp_dir);
    let found = find_browser(&FindOptions {
        user_path: args.browser,
        data_dir: data,
    });
    let plan = download_plan();
    let download = plan.as_ref().map(|p| serde_json::json!({ "version": p.version, "bytes": p.bytes }));
    match found {
        Ok(b) => {
            if args.json {
                println!(
                    "{}",
                    serde_json::json!({
                        "usable": true,
                        "name": b.name.label(),
                        "version": b.version,
                        "source": source_label(b.source),
                        "path": b.path,
                        "download": download,
                    })
                );
            } else {
                println!(
                    "{} {} ({}) at {}",
                    b.name.label(),
                    b.version,
                    source_label(b.source),
                    b.path.display()
                );
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            let message = not_usable_message(&e, plan.is_some());
            if args.json {
                println!(
                    "{}",
                    serde_json::json!({ "usable": false, "problem": message, "download": download })
                );
            } else {
                eprintln!("{message}");
            }
            ExitCode::from(exit::NO_BROWSER)
        }
    }
}

fn install(args: InstallArgs) -> ExitCode {
    let data = data_dir().unwrap_or_else(std::env::temp_dir);
    let Some(plan) = download_plan() else {
        eprintln!(
            "error: no browser download is available for this platform. Install Google Chrome, Chromium or Microsoft Edge and give it with --browser PATH."
        );
        return ExitCode::from(exit::NO_BROWSER);
    };
    if let Ok(b) = spiderlens_render::find_browser(&FindOptions {
        user_path: None,
        data_dir: data.clone(),
    }) && b.source == BrowserSource::Downloaded
    {
        report_installed(&b, args.json);
        return ExitCode::SUCCESS;
    }
    if !args.yes {
        eprint!(
            "Download {} {} (about {} MB) from storage.googleapis.com? [y/N] ",
            "chrome-headless-shell",
            plan.version,
            plan.bytes / 1_000_000
        );
        let _ = std::io::stderr().flush();
        let mut answer = String::new();
        let _ = std::io::stdin().read_line(&mut answer);
        if !matches!(answer.trim().to_ascii_lowercase().as_str(), "y" | "yes") {
            eprintln!("Nothing was downloaded.");
            return ExitCode::from(1);
        }
    }
    let progress = progress_line(args.json);
    let result = match tokio::runtime::Builder::new_current_thread().enable_all().build() {
        Ok(rt) => rt
            .block_on(download_with(
                &plan,
                &data,
                &cli_download_options(),
                &progress,
                &CancellationFlag::new(),
            ))
            .map_err(|e| (failure_code(&e), failure_text(&e))),
        Err(e) => Err((exit::FAILED, e.to_string())),
    };
    if !args.json {
        eprintln!();
    }
    match result {
        Ok(b) => {
            report_installed(&b, args.json);
            ExitCode::SUCCESS
        }
        Err((code, message)) => {
            eprintln!("error: {message}");
            ExitCode::from(code)
        }
    }
}

fn report_installed(b: &BrowserInfo, json: bool) {
    if json {
        println!(
            "{}",
            serde_json::json!({
                "installed": true,
                "name": b.name.label(),
                "version": b.version,
                "path": b.path,
            })
        );
    } else {
        println!("{} {} is installed at {}", b.name.label(), b.version, b.path.display());
    }
}

fn remove() -> ExitCode {
    let data = data_dir().unwrap_or_else(std::env::temp_dir);
    match remove_download(&data) {
        Ok(()) => {
            println!("The downloaded browser was removed.");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("error: could not remove the downloaded browser: {e}");
            ExitCode::from(exit::FAILED)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn messages_name_the_way_out() {
        let none = not_usable_message(&NotUsable::NotFound, true);
        assert!(none.starts_with("JavaScript rendering needs a Chromium-based browser and none was found."));
        assert!(none.contains("spiderlens browser install") && none.contains("--allow-browser-download"));
        let offline = not_usable_message(&NotUsable::NotFound, false);
        assert!(offline.contains("--browser PATH") && !offline.contains("browser install"));
        let broken = not_usable_message(
            &NotUsable::Broken {
                path: PathBuf::from("/nope"),
                message: "No browser at this path.".into(),
            },
            false,
        );
        assert!(broken.contains("/nope") && broken.contains("No browser at this path."));
    }
}
