//! `spiderlens spell`: check the spelling of a crawl file, and manage the
//! dictionaries it uses (spec 027 FR-D11 to FR-D13).

use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::atomic::AtomicBool;

use clap::{Args, Subcommand};
use spiderlens_config::{SpellingSettings, validate_config};
use spiderlens_spell::{Store, default_dir, manifest};

use crate::exit;

#[derive(Args)]
#[command(args_conflicts_with_subcommands = true)]
pub struct SpellArgs {
    #[command(subcommand)]
    command: Option<DictCommand>,
    /// The crawl file to check. Results replace an earlier check.
    file: Option<PathBuf>,
    /// Language for pages that do not declare one.
    #[arg(long, value_name = "LANG", default_value = "en")]
    default_lang: String,
    /// A word never to report. Repeatable.
    #[arg(long, value_name = "WORD")]
    ignore: Vec<String>,
    /// Also check words written in capitals only.
    #[arg(long)]
    check_all_caps: bool,
    /// Dictionaries folder [default: the app's].
    #[arg(long, value_name = "DIR", global = true)]
    dictionaries: Option<PathBuf>,
    /// Print the summary as JSON.
    #[arg(long, global = true)]
    json: bool,
    /// No progress on stderr.
    #[arg(long)]
    quiet: bool,
}

#[derive(Subcommand)]
enum DictCommand {
    /// Installed dictionaries, and those that can be downloaded.
    Dictionaries,
    /// Download a dictionary, checked against its published checksum.
    Download {
        /// Language code from `spell dictionaries`, for example he.
        lang: String,
        /// Accept its licence (shown by `spell dictionaries`).
        #[arg(long)]
        accept_licence: bool,
    },
    /// Install a Hunspell dictionary from a .aff and .dic file.
    Add {
        lang: String,
        aff: PathBuf,
        dic: PathBuf,
    },
    /// Remove an installed dictionary.
    Remove { lang: String },
}

fn store(dir: Option<PathBuf>) -> Option<Store> {
    match dir.or_else(default_dir) {
        Some(d) => Some(Store::new(d)),
        None => {
            eprintln!("error: no folder for dictionaries; pass --dictionaries DIR");
            None
        }
    }
}

fn mb(bytes: u64) -> String {
    format!("{:.1} MB", bytes as f64 / 1_048_576.0)
}

pub fn run_spell(args: SpellArgs) -> ExitCode {
    let Some(store) = store(args.dictionaries) else {
        return ExitCode::from(exit::USAGE);
    };
    match args.command {
        Some(cmd) => run_dict(cmd, &store, args.json),
        None => {
            let Some(file) = args.file else {
                eprintln!("error: give a crawl file, or a command such as `spell dictionaries`");
                return ExitCode::from(exit::USAGE);
            };
            let settings = SpellingSettings {
                check: true,
                default_lang: args.default_lang,
                skip_all_caps: !args.check_all_caps,
                ignore: args.ignore,
            };
            let config = spiderlens_config::CrawlConfig {
                spelling: settings.clone(),
                ..Default::default()
            };
            if crate::print_issues(
                &validate_config(&config)
                    .into_iter()
                    .filter(|i| i.path.starts_with("spelling"))
                    .collect::<Vec<_>>(),
            ) {
                return ExitCode::from(exit::USAGE);
            }
            let quiet = args.quiet || args.json;
            let result = spiderlens_spell::run(&file, &settings, &store, &AtomicBool::new(false), |p| {
                if !quiet {
                    eprint!("\rChecked {} of {} pages", p.checked, p.total);
                }
            });
            if !quiet {
                eprintln!();
            }
            match result {
                Ok(s) => {
                    if args.json {
                        crate::print_json(&s);
                    } else {
                        println!(
                            "{} pages checked, {} spelling errors ({} different words), {} site-wide blocks.",
                            s.pages - s.no_dictionary,
                            s.errors,
                            s.distinct_words,
                            s.site_wide_blocks
                        );
                        if s.partly_checked > 0 {
                            println!(
                                "{} pages had no stored HTML, so only titles, descriptions and headings were checked.",
                                s.partly_checked
                            );
                        }
                        for (lang, n) in &s.missing {
                            println!(
                                "{n} pages in {lang} were not checked: no dictionary. Run `spiderlens spell download {lang}` if it is listed."
                            );
                        }
                    }
                    ExitCode::from(exit::FETCHED)
                }
                Err(e) => {
                    eprintln!("error: {e}");
                    ExitCode::FAILURE
                }
            }
        }
    }
}

fn run_dict(cmd: DictCommand, store: &Store, json: bool) -> ExitCode {
    match cmd {
        DictCommand::Dictionaries => {
            let installed = store.list();
            let available: Vec<_> = manifest()
                .dictionaries
                .iter()
                .filter(|d| !store.is_installed(&d.lang))
                .collect();
            if json {
                crate::print_json(&serde_json::json!({
                    "folder": store.dir(),
                    "installed": installed,
                    "available": available.iter().map(|d| serde_json::json!({
                        "lang": d.lang, "name": d.name, "licence": d.licence, "size": d.size(),
                    })).collect::<Vec<_>>(),
                }));
            } else {
                println!("Installed ({}):", store.dir().display());
                for d in &installed {
                    println!("  {:<6} {:<28} {:>8}  {}", d.lang, d.name, mb(d.size), d.licence);
                }
                println!("Available to download:");
                for d in available {
                    println!(
                        "  {:<6} {:<28} {:>8}  {}",
                        d.lang,
                        d.name,
                        mb(d.size()),
                        d.licence
                    );
                }
            }
            ExitCode::from(exit::FETCHED)
        }
        DictCommand::Download { lang, accept_licence } => {
            let Some(entry) = manifest().dictionaries.iter().find(|d| d.lang == lang) else {
                eprintln!("error: {lang} is not in the list; see `spiderlens spell dictionaries`");
                return ExitCode::from(exit::USAGE);
            };
            if !accept_licence {
                eprintln!(
                    "{} ({}) is licensed under {}. It is downloaded as a separate data file. Run again with --accept-licence to download it.",
                    entry.name,
                    mb(entry.size()),
                    entry.licence
                );
                return ExitCode::from(exit::USAGE);
            }
            let rt = match tokio::runtime::Builder::new_current_thread().enable_all().build() {
                Ok(rt) => rt,
                Err(e) => {
                    eprintln!("error: could not start: {e}");
                    return ExitCode::FAILURE;
                }
            };
            let client = reqwest::Client::new();
            match rt.block_on(store.download_entry(entry, &client)) {
                Ok(d) => {
                    println!("Installed {} ({}).", d.name, mb(d.size));
                    ExitCode::from(exit::FETCHED)
                }
                Err(e) => {
                    eprintln!("error: {e}");
                    ExitCode::from(exit::FAILED)
                }
            }
        }
        DictCommand::Add { lang, aff, dic } => match store.add_files(&lang, &aff, &dic) {
            Ok(d) => {
                println!("Installed {} ({}).", d.lang, mb(d.size));
                ExitCode::from(exit::FETCHED)
            }
            Err(e) => {
                eprintln!("error: {e}");
                ExitCode::from(exit::USAGE)
            }
        },
        DictCommand::Remove { lang } => match store.remove(&lang) {
            Ok(()) => {
                println!("Removed {lang}.");
                ExitCode::from(exit::FETCHED)
            }
            Err(e) => {
                eprintln!("error: {e}");
                ExitCode::from(exit::USAGE)
            }
        },
    }
}
