//! The browser check before a rendering crawl (spec 014 FR-045, plan D12,
//! contracts/cli.md "Browser check"). Runs before the crawl file is
//! created, so a crawl never starts in rendering mode without a browser.

use std::io::Write;
use std::path::PathBuf;
use std::process::ExitCode;

use spiderlens_render::{
    BrowserInfo, BrowserSource, CancellationFlag, DownloadProgress, FindOptions, NotUsable, data_dir,
    download, download_plan, find_browser, should_offer_download,
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
        "Run \"spiderlens browser install\" (downloads about 100 MB) or add --allow-browser-download, or crawl without --render."
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
/// on failure prints the error and returns exit 7.
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
        let progress = move |p: DownloadProgress| {
            if !quiet && p.total_bytes > 0 {
                eprint!(
                    "\rdownloading the browser: {} of {} MB",
                    p.done_bytes / 1_000_000,
                    p.total_bytes / 1_000_000
                );
                let _ = std::io::stderr().flush();
            }
        };
        let downloaded = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| e.to_string())
            .and_then(|rt| {
                rt.block_on(download(plan, &data, &progress, &CancellationFlag::new()))
                    .map_err(|e| format!("{e:?}"))
            });
        if !quiet {
            eprintln!();
        }
        match downloaded {
            Ok(b) => chosen = Ok(b),
            // An installed browser still renders; without one, say why.
            Err(e) if chosen.is_err() => {
                eprintln!("error: the browser download failed: {e}");
                return Err(ExitCode::from(exit::NO_BROWSER));
            }
            Err(e) => eprintln!("warning: the browser download failed ({e}); using the installed browser"),
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
