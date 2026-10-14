//! `spiderlens sitemap generate`: XML sitemaps from a crawl file, byte for
//! byte what the desktop app writes for the same options (spec 011 FR-019,
//! specs/011-sitemaps-orphan-pages/contracts/cli.md).

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Args, Subcommand, ValueEnum};
use spiderlens_config::{Changefreq, ImageEntries, LastmodSource, PrioritySource, SitemapGenerateOptions};
use spiderlens_results::{GenerateError, Reader, SitemapOutput, generate_sitemaps};

use crate::exit;

/// The crawl file cannot be opened.
const UNREADABLE: u8 = 3;
/// No page matches the options.
const NO_PAGES: u8 = 4;

#[derive(Subcommand)]
pub enum SitemapCommand {
    /// Write XML sitemaps (and an index when needed) from a crawl file.
    Generate(Box<GenerateArgs>),
}

#[derive(Clone, Copy, ValueEnum)]
pub enum ImagesArg {
    Off,
    Inline,
    Separate,
}

#[derive(Args)]
pub struct GenerateArgs {
    /// The crawl file.
    file: PathBuf,
    /// Folder to write the files into.
    #[arg(long, required_unless_present = "dry_run")]
    out: Option<PathBuf>,
    /// Address of the folder the files will be published in (default: the
    /// start host's root).
    #[arg(long, value_name = "URL")]
    base_url: Option<String>,
    /// Also include pages with a noindex directive.
    #[arg(long)]
    include_noindex: bool,
    /// Also include pages whose canonical is another URL.
    #[arg(long)]
    include_canonicalised: bool,
    /// Only pages matching this pattern. Repeatable.
    #[arg(long, value_name = "REGEX")]
    include: Vec<String>,
    /// Leave out pages matching this pattern. Repeatable.
    #[arg(long, value_name = "REGEX")]
    exclude: Vec<String>,
    /// none, header (Last-Modified), crawl (fetch time) or a YYYY-MM-DD date.
    #[arg(long, default_value = "header")]
    lastmod: String,
    /// always, hourly, daily, weekly, monthly, yearly or never.
    #[arg(long, value_parser = parse_changefreq)]
    changefreq: Option<Changefreq>,
    /// off, depth, or a number from 0.0 to 1.0.
    #[arg(long, default_value = "off")]
    priority: String,
    #[arg(long, value_enum, default_value = "off")]
    images: ImagesArg,
    /// List images from this other host too. Repeatable.
    #[arg(long = "image-host", value_name = "HOST")]
    image_hosts: Vec<String>,
    /// Add hreflang alternates.
    #[arg(long)]
    hreflang: bool,
    /// URLs per file, at most 50,000.
    #[arg(long, default_value_t = 50_000)]
    max_urls: u32,
    /// Write .xml.gz files.
    #[arg(long)]
    gzip: bool,
    /// File name prefix.
    #[arg(long, default_value = "sitemap")]
    prefix: String,
    /// Replace files that already exist.
    #[arg(long)]
    force: bool,
    /// Only count what would be written.
    #[arg(long)]
    dry_run: bool,
    /// Print the report as JSON.
    #[arg(long)]
    json: bool,
}

fn parse_changefreq(s: &str) -> Result<Changefreq, String> {
    serde_json::from_value(serde_json::Value::String(s.to_ascii_lowercase()))
        .map_err(|_| "expected always, hourly, daily, weekly, monthly, yearly or never".to_string())
}

fn options(a: &GenerateArgs) -> Result<SitemapGenerateOptions, String> {
    let lastmod = match a.lastmod.as_str() {
        "none" => LastmodSource::None,
        "header" => LastmodSource::LastModifiedHeader,
        "crawl" => LastmodSource::CrawlDate,
        date => LastmodSource::Fixed(date.to_string()),
    };
    let priority = match a.priority.as_str() {
        "off" => PrioritySource::Off,
        "depth" => PrioritySource::ByDepth,
        n => PrioritySource::Fixed(
            n.parse()
                .map_err(|_| "--priority takes off, depth or a number from 0.0 to 1.0".to_string())?,
        ),
    };
    Ok(SitemapGenerateOptions {
        include_noindex: a.include_noindex,
        include_canonicalised: a.include_canonicalised,
        include: a.include.clone(),
        exclude: a.exclude.clone(),
        lastmod,
        changefreq: a.changefreq,
        priority,
        images: match a.images {
            ImagesArg::Off => ImageEntries::Off,
            ImagesArg::Inline => ImageEntries::Inline,
            ImagesArg::Separate => ImageEntries::Separate,
        },
        image_hosts: a.image_hosts.clone(),
        hreflang: a.hreflang,
        max_urls_per_file: a.max_urls,
        gzip: a.gzip,
        base_url: a.base_url.clone().unwrap_or_default(),
        file_prefix: a.prefix.clone(),
    })
}

pub fn run(cmd: SitemapCommand) -> ExitCode {
    match cmd {
        SitemapCommand::Generate(a) => run_generate(&a),
    }
}

fn run_generate(a: &GenerateArgs) -> ExitCode {
    let opts = match options(a) {
        Ok(o) => o,
        Err(m) => {
            eprintln!("error: {m}");
            return ExitCode::from(exit::USAGE);
        }
    };
    let reader = match Reader::open(&a.file) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::from(UNREADABLE);
        }
    };
    if let Some(dir) = &a.out
        && !a.dry_run
        && let Err(e) = std::fs::create_dir_all(dir)
    {
        eprintln!("error: could not create {}: {e}", dir.display());
        return ExitCode::from(exit::USAGE);
    }
    let out = if a.dry_run {
        SitemapOutput::DryRun {
            dir: a.out.as_deref(),
        }
    } else {
        SitemapOutput::Folder {
            dir: a.out.as_deref().expect("required by clap"),
            replace: a.force,
        }
    };
    match generate_sitemaps(&reader, &opts, out) {
        Ok(report) => {
            if a.json {
                crate::print_json(&report);
            } else {
                for f in &report.files {
                    println!("{}  {} URLs  {} bytes", f.name, f.urls, f.bytes);
                }
                let verb = if a.dry_run { "would be written" } else { "written" };
                eprintln!(
                    "{} pages, {} images {verb}; {} left out by patterns",
                    report.pages, report.images, report.skipped_by_pattern
                );
            }
            ExitCode::from(exit::FETCHED)
        }
        Err(GenerateError::NoPages) => {
            eprintln!("error: no page matches these options; nothing was written");
            ExitCode::from(NO_PAGES)
        }
        Err(GenerateError::Exists(names)) => {
            eprintln!(
                "error: these files already exist: {}. Use --force to replace them.",
                names.join(", ")
            );
            ExitCode::from(exit::USAGE)
        }
        Err(GenerateError::Invalid(m)) => {
            eprintln!("error: {m}");
            ExitCode::from(exit::USAGE)
        }
        Err(e @ GenerateError::Read(_)) => {
            eprintln!("error: {e}");
            ExitCode::from(UNREADABLE)
        }
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::from(exit::FAILED)
        }
    }
}
