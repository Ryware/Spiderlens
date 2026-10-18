//! `spiderlens viz`: a visualisation of a crawl file as SVG, PNG or CSV,
//! the same as the desktop export of the view's default state (spec 022
//! FR-025, specs/022-site-visualisations/contracts/cli.md).

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Args, ValueEnum};
use spiderlens_results::Reader;
use spiderlens_results::viz::export::{ExportFormat, ExportOptions, VizExportError, export};
use spiderlens_results::viz::palette::Theme;
use spiderlens_results::viz::png::PngError;
use spiderlens_results::viz::{ColourBy, Layout, SizeBy, Terms, ViewKind, ViewState};

use crate::exit;

/// Exit code when the crawl file cannot be opened (contracts/cli.md).
const UNREADABLE: u8 = 3;
/// Exit code when the PNG would be over 16,000 pixels on a side.
const TOO_LARGE: u8 = 4;

#[derive(Clone, Copy, ValueEnum)]
pub enum ViewArg {
    CrawlTree,
    DirectoryTree,
    LinkGraph,
    Anchors,
}

#[derive(Clone, Copy, ValueEnum)]
pub enum LayoutArg {
    Tree,
    Radial,
    Force,
}

#[derive(Clone, Copy, ValueEnum)]
pub enum ColourArg {
    Status,
    Depth,
}

#[derive(Clone, Copy, ValueEnum)]
pub enum SizeArg {
    Score,
    Inlinks,
    Uniform,
}

#[derive(Clone, Copy, ValueEnum)]
pub enum ThemeArg {
    Light,
    Dark,
}

#[derive(Args)]
pub struct VizArgs {
    /// The crawl file.
    file: PathBuf,
    #[arg(long, value_enum)]
    view: ViewArg,
    /// Trees only (the link graph is always force-directed). Force uses the graph node budget.
    #[arg(long, value_enum, default_value = "tree")]
    layout: LayoutArg,
    #[arg(long, value_enum, default_value = "status")]
    colour: ColourArg,
    /// Link Score falls back to the same size for all when it has not been worked out.
    #[arg(long, value_enum, default_value = "score")]
    size: SizeArg,
    /// Link graph: centre on this URL.
    #[arg(long, value_name = "URL")]
    focus: Option<String>,
    /// Link graph with --focus: neighbours up to this many links away.
    #[arg(long, default_value_t = 1, value_parser = clap::value_parser!(u8).range(1..=2))]
    distance: u8,
    /// Anchor text of the links to this URL (default: the whole site).
    #[arg(long, value_name = "URL")]
    url: Option<String>,
    /// Anchor text: single words (the default).
    #[arg(long, conflicts_with = "anchors")]
    words: bool,
    /// Anchor text: whole anchors as terms.
    #[arg(long)]
    anchors: bool,
    /// Anchor text: followed links only.
    #[arg(long)]
    followed_only: bool,
    /// Anchor text: keep stop words.
    #[arg(long)]
    keep_stop_words: bool,
    /// Anchor text: only links at these positions (navigation, header,
    /// content, sidebar, footer, unknown); repeat or comma-separate.
    /// Needs Crawl Analysis; ignored with a note when it has not run.
    #[arg(long, value_name = "POSITION", value_delimiter = ',')]
    position: Vec<String>,
    #[arg(long, value_enum, default_value = "light")]
    theme: ThemeArg,
    #[arg(long, value_name = "FILE")]
    svg: Option<PathBuf>,
    #[arg(long, value_name = "FILE")]
    png: Option<PathBuf>,
    /// PNG scale.
    #[arg(long, default_value_t = 2, value_parser = clap::builder::TypedValueParser::map(clap::builder::PossibleValuesParser::new(["1", "2", "4"]), |s: String| s.parse::<u8>().unwrap_or(2)))]
    scale: u8,
    /// The nodes; the links go to a second file beside it (NAME-links.csv).
    #[arg(long, value_name = "FILE")]
    csv: Option<PathBuf>,
}

pub fn run_viz(a: VizArgs) -> ExitCode {
    if a.svg.is_none() && a.png.is_none() && a.csv.is_none() {
        eprintln!("error: give at least one of --svg, --png or --csv");
        return ExitCode::from(exit::USAGE);
    }
    let view = match a.view {
        ViewArg::CrawlTree => ViewKind::CrawlTree,
        ViewArg::DirectoryTree => ViewKind::DirectoryTree,
        ViewArg::LinkGraph => ViewKind::LinkGraph,
        ViewArg::Anchors => ViewKind::Anchors,
    };
    if a.focus.is_some() && view != ViewKind::LinkGraph {
        eprintln!("error: --focus is for --view link-graph");
        return ExitCode::from(exit::USAGE);
    }
    if a.url.is_some() && view != ViewKind::Anchors {
        eprintln!("error: --url is for --view anchors");
        return ExitCode::from(exit::USAGE);
    }
    if !a.position.is_empty() && view != ViewKind::Anchors {
        eprintln!("error: --position is for --view anchors");
        return ExitCode::from(exit::USAGE);
    }
    let mut r = match Reader::open(&a.file) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::from(UNREADABLE);
        }
    };
    // The page a focus or cloud is about.
    let key = match a.focus.as_deref().or(a.url.as_deref()) {
        None => None,
        Some(u) => match r.viz_url_key(u) {
            Ok(Some(k)) => Some(k),
            Ok(None) => {
                eprintln!("error: {u} is not in this crawl");
                return ExitCode::from(exit::USAGE);
            }
            Err(e) => {
                eprintln!("error: {e}");
                return ExitCode::from(UNREADABLE);
            }
        },
    };
    let state = ViewState {
        view,
        layout: match (view, a.layout) {
            (ViewKind::LinkGraph, _) => Layout::Force,
            (ViewKind::Anchors, _) => Layout::Cloud,
            (_, LayoutArg::Tree) => Layout::Tree,
            (_, LayoutArg::Radial) => Layout::Radial,
            (_, LayoutArg::Force) => Layout::Force,
        },
        colour: match a.colour {
            ColourArg::Status => ColourBy::Status,
            ColourArg::Depth => ColourBy::Depth,
        },
        size: match a.size {
            SizeArg::Score => SizeBy::Score,
            SizeArg::Inlinks => SizeBy::Inlinks,
            SizeArg::Uniform => SizeBy::Uniform,
        },
        focus: key,
        distance: a.distance,
        terms: if a.anchors { Terms::Whole } else { Terms::Words },
        stop_words: !a.keep_stop_words,
        followed_only: a.followed_only,
        positions: a.position.iter().map(|p| p.trim().to_lowercase()).collect(),
        ..ViewState::default()
    };
    let scene = match r.viz_scene(&state).and_then(|s| Ok((s, r.viz_header()?))) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::from(UNREADABLE);
        }
    };
    let (scene, header) = scene;
    let theme = match a.theme {
        ThemeArg::Light => Theme::Light,
        ThemeArg::Dark => Theme::Dark,
    };
    let outputs = [
        (ExportFormat::Svg, &a.svg),
        (ExportFormat::Png, &a.png),
        (ExportFormat::Csv, &a.csv),
    ];
    for (format, path) in outputs {
        let Some(path) = path else { continue };
        let opts = ExportOptions {
            format,
            scale: a.scale,
            theme,
            ..Default::default()
        };
        match export(&scene, state.colour, &header, &opts, path) {
            Ok(done) => {
                for f in done.files {
                    eprintln!("wrote {f}");
                }
            }
            Err(e @ VizExportError::Png(PngError::TooLarge(..))) => {
                eprintln!("error: {e}");
                return ExitCode::from(TOO_LARGE);
            }
            Err(e) => {
                eprintln!("error: {e}");
                return ExitCode::from(exit::FAILED);
            }
        }
    }
    eprintln!(
        "{}: {} nodes of {} {}",
        spiderlens_results::viz::svg::view_title(view),
        scene.nodes.len(),
        scene.total,
        if view == ViewKind::Anchors {
            "links"
        } else {
            "URLs"
        }
    );
    ExitCode::from(exit::FETCHED)
}
