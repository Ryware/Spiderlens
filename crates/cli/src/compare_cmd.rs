//! `spiderlens compare`: what changed between two crawl files, with the
//! same numbers as the desktop app (spec 012 FR-029..FR-032,
//! specs/012-crawl-comparison/contracts/cli.md).

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::atomic::AtomicBool;

use clap::Args;
use spiderlens_compare::{Comparison, FilterChange, Group, UrlGroup};
use spiderlens_config::{CompareSettings, MapRule};

use crate::exit;

#[derive(Args)]
pub struct CompareArgs {
    /// The previous crawl file.
    previous: PathBuf,
    /// The current crawl file.
    current: PathBuf,
    /// Plain-text mapping rule for previous URLs, FIND=>REPLACE (repeatable, in order).
    #[arg(long = "map", value_name = "FIND=>REPLACE")]
    map: Vec<String>,
    /// Regular-expression mapping rule, PATTERN=>REPLACE with $1 groups (repeatable).
    #[arg(long = "map-regex", value_name = "PATTERN=>REPLACE")]
    map_regex: Vec<String>,
    /// Comparison settings file saved from the app.
    #[arg(long, value_name = "FILE")]
    settings: Option<PathBuf>,
    /// Print the full comparison report as JSON.
    #[arg(long)]
    json: bool,
    /// Write one CSV per non-empty group into this folder.
    #[arg(long, value_name = "DIR")]
    export: Option<PathBuf>,
    /// Fail when this many internal HTML pages or more are missing.
    #[arg(long, value_name = "N")]
    fail_on_missing: Option<u64>,
    /// Fail when this many pages or more went from indexable to non-indexable.
    #[arg(long, value_name = "N")]
    fail_on_lost_indexable: Option<u64>,
}

fn parse_rule(s: &str, regex: bool) -> Result<MapRule, String> {
    let (find, replace) = s
        .split_once("=>")
        .ok_or_else(|| format!("mapping rule {s:?} needs FIND=>REPLACE"))?;
    Ok(MapRule {
        find: find.into(),
        replace: replace.into(),
        regex,
    })
}

fn load_settings(a: &CompareArgs) -> Result<CompareSettings, String> {
    let mut s = match &a.settings {
        Some(p) => {
            let text = std::fs::read_to_string(p).map_err(|e| format!("{}: {e}", p.display()))?;
            CompareSettings::from_json(&text).map_err(|e| format!("{}: {e}", p.display()))?
        }
        None => CompareSettings::default(),
    };
    for m in &a.map {
        s.mapping.push(parse_rule(m, false)?);
    }
    for m in &a.map_regex {
        s.mapping.push(parse_rule(m, true)?);
    }
    s.validate().map_err(|e| e.to_string())?;
    Ok(s)
}

pub fn run_compare(a: CompareArgs) -> ExitCode {
    let settings = match load_settings(&a) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::from(exit::USAGE);
        }
    };
    let cancel = AtomicBool::new(false);
    let mut c = match Comparison::run(&a.previous, &a.current, settings, &cancel, &mut |_| {}) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::from(exit::USAGE);
        }
    };
    let report = match c.report() {
        Ok(r) => r,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::from(exit::USAGE);
        }
    };
    if a.json {
        crate::print_json(&report);
    } else {
        print_summary(&report);
    }
    if let Some(dir) = &a.export
        && let Err(e) = export_all(&mut c, dir)
    {
        eprintln!("error: export: {e}");
        return ExitCode::from(exit::USAGE);
    }

    let s = &report.summary;
    let mut failed = Vec::new();
    if let Some(n) = a.fail_on_missing
        && s.missing_pages >= n
    {
        failed.push(format!(
            "{} internal HTML pages are missing (limit {n})",
            s.missing_pages
        ));
    }
    if let Some(n) = a.fail_on_lost_indexable
        && s.lost_indexable >= n
    {
        failed.push(format!(
            "{} pages went from indexable to non-indexable (limit {n})",
            s.lost_indexable
        ));
    }
    if failed.is_empty() {
        ExitCode::from(exit::FETCHED)
    } else {
        for f in failed {
            eprintln!("failed: {f}");
        }
        ExitCode::from(exit::COMPARE_FAILED)
    }
}

fn print_summary(r: &spiderlens_compare::ComparisonReport) {
    let s = &r.summary;
    let name = |i: &spiderlens_compare::CrawlInfo| {
        format!(
            "{} ({}, {} URLs{})",
            i.path,
            i.start_url.as_deref().unwrap_or("?"),
            i.urls,
            if i.complete { "" } else { ", incomplete" }
        )
    };
    println!("Previous: {}", name(&s.previous));
    println!("Current:  {}", name(&s.current));
    for d in &s.config_diffs {
        println!("Setting changed: {}: {} -> {}", d.label, d.previous, d.current);
    }
    if s.low_overlap {
        println!("Warning: few URLs match; the crawls may be of different sites or need URL mapping.");
    }
    println!();
    println!(
        "Matched {}  New {}  Missing {}  Not reached {}  Collisions {}",
        s.matched, s.new, s.missing, s.not_reached, s.collisions
    );
    println!(
        "Missing HTML pages {}  Pages no longer indexable {}",
        s.missing_pages, s.lost_indexable
    );
    if !r.top_filters.is_empty() {
        println!();
        println!("Largest filter changes:");
        for f in &r.top_filters {
            println!(
                "  {} › {}: {} -> {} ({:+})  new {}  added {}  removed {}  missing {}",
                f.tab_title,
                f.filter_label,
                f.previous.unwrap_or(0),
                f.current.unwrap_or(0),
                f.change(),
                f.new,
                f.added,
                f.removed,
                f.missing
            );
        }
    }
    println!();
    println!("Changed pages:");
    for e in &r.elements {
        if e.comparable {
            println!("  {}: {}", e.label, e.changed);
        } else {
            println!("  {}: not comparable", e.label);
        }
    }
}

fn slug(s: &str) -> String {
    s.replace('_', "-")
}

/// One CSV per non-empty group, named as in contracts/cli.md.
fn export_all(c: &mut Comparison, dir: &Path) -> spiderlens_compare::Result<()> {
    std::fs::create_dir_all(dir)?;
    let s = c.summary().clone();
    let mut groups: Vec<(String, Group)> = Vec::new();
    for f in s.filters.iter().filter(|f| f.comparable) {
        for (n, change, word) in [
            (f.new, FilterChange::New, "new"),
            (f.added, FilterChange::Added, "added"),
            (f.removed, FilterChange::Removed, "removed"),
            (f.missing, FilterChange::Missing, "missing"),
            (f.not_reached, FilterChange::NotReached, "not-reached"),
        ] {
            if n > 0 {
                groups.push((
                    format!("filter-{}-{}-{word}.csv", f.tab, slug(&f.filter)),
                    Group::Filter {
                        tab: f.tab.clone(),
                        filter: f.filter.clone(),
                        change,
                    },
                ));
            }
        }
    }
    for e in s.elements.iter().filter(|e| e.on && e.changed > 0) {
        groups.push((
            format!("changed-{}.csv", slug(&e.element)),
            Group::Element {
                element: e.element.clone(),
            },
        ));
    }
    for (n, which, word) in [
        (s.new, UrlGroup::New, "new"),
        (s.missing, UrlGroup::Missing, "missing"),
        (s.not_reached, UrlGroup::NotReached, "not-reached"),
        (s.collisions, UrlGroup::Collisions, "collisions"),
    ] {
        if n > 0 {
            groups.push((format!("urls-{word}.csv"), Group::Urls { which }));
        }
    }
    let cancel = AtomicBool::new(false);
    for (name, g) in groups {
        let page = c.open_group(&g, "", None, 0)?;
        c.export_group(page.view, &dir.join(name), &cancel, &mut |_, _| {})?;
        c.close_group(page.view)?;
    }
    Ok(())
}
