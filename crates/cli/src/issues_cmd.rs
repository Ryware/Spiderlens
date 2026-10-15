//! `spiderlens rules` and `spiderlens issues`: the issue rule catalogue and
//! the issues found in a crawl file (spec 009,
//! specs/009-issue-rules-engine/contracts/cli.md).

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::atomic::AtomicBool;

use clap::Args;
use spiderlens_config::{IssueSettings, Priority, import_profile};
use spiderlens_results::issues::{self, IssueCount, IssuesSummary, ResolvedSettings, RuleInfo};
use spiderlens_results::{Reader, UrlScope, ViewQuery, export_file_with};

use crate::exit;

/// Exit code when the crawl file cannot be opened (as `export`).
const UNREADABLE: u8 = 3;

#[derive(Args)]
pub struct RulesArgs {
    /// Print as JSON.
    #[arg(long)]
    json: bool,
    /// Only this category, by id or name (for example page_titles).
    #[arg(long, value_name = "NAME")]
    category: Option<String>,
}

#[derive(Args)]
pub struct IssuesArgs {
    /// The crawl file.
    file: PathBuf,
    /// Print as JSON.
    #[arg(long)]
    json: bool,
    /// Also list rules with no URLs, that cannot run, or that are turned off.
    #[arg(long)]
    all: bool,
    /// Hide issues below this priority.
    #[arg(long, value_name = "PRIORITY", value_parser = parse_priority)]
    min_priority: Option<Priority>,
    /// Issue settings file (JSON, as saved in a profile's "issues").
    #[arg(long, value_name = "FILE", conflicts_with = "profile")]
    issues: Option<PathBuf>,
    /// Exported profile file whose issue settings apply.
    #[arg(long, value_name = "FILE")]
    profile: Option<PathBuf>,
    /// Exit with code 7 when an issue at or above this priority has URLs.
    #[arg(long, value_name = "PRIORITY", value_parser = parse_priority)]
    fail_on: Option<Priority>,
    /// Write one CSV per issue into this folder.
    #[arg(long, value_name = "DIR")]
    export: Option<PathBuf>,
    /// With --export: only these issues. Repeatable.
    #[arg(long = "issue", value_name = "ID", requires = "export")]
    only: Vec<String>,
}

fn parse_priority(s: &str) -> Result<Priority, String> {
    Priority::parse(s).ok_or_else(|| "expected high, medium, low or opportunity".into())
}

pub fn run_rules(a: RulesArgs) -> ExitCode {
    let mut rules = issues::rule_infos(&ResolvedSettings::default());
    if let Some(c) = &a.category {
        rules.retain(|r| category_matches(r, c));
        if rules.is_empty() {
            eprintln!("error: no category named {c}");
            return ExitCode::from(exit::USAGE);
        }
    }
    if a.json {
        crate::print_json(&rules);
        return ExitCode::from(exit::FETCHED);
    }
    for r in &rules {
        let threshold = r
            .threshold
            .as_ref()
            .map(|t| format!("  threshold {} {}", issues::fmt_num(t.value), t.unit_label))
            .unwrap_or_default();
        println!(
            "{}  {}  [{}, {}]{threshold}",
            r.id,
            r.name,
            r.category_label,
            r.priority.label()
        );
        println!("    why: {}", r.why);
        println!("    fix: {}", r.fix);
    }
    ExitCode::from(exit::FETCHED)
}

fn category_matches(r: &RuleInfo, name: &str) -> bool {
    let id = serde_json::to_value(r.category)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_default();
    let name = name.trim();
    id.eq_ignore_ascii_case(&name.replace(['-', ' '], "_")) || r.category_label.eq_ignore_ascii_case(name)
}

pub fn run_issues(a: IssuesArgs) -> ExitCode {
    let mut reader = match Reader::open(&a.file) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::from(UNREADABLE);
        }
    };
    // Settings: --issues or --profile, else the file's own, else defaults.
    let settings = match (&a.issues, &a.profile) {
        (Some(p), _) => read_settings(p),
        (None, Some(p)) => read_profile(p),
        (None, None) => Ok(reader.stored_issue_settings().ok().flatten().unwrap_or_default()),
    };
    let settings = match settings {
        Ok(s) => s,
        Err(m) => {
            eprintln!("error: {m}");
            return ExitCode::from(exit::USAGE);
        }
    };
    let notices = reader.set_issue_settings(&settings);
    for n in &notices {
        let level = if n.error { "error" } else { "notice" };
        eprintln!("{level}: {}: {}", n.rule, n.message);
    }
    if notices.iter().any(|n| n.error) {
        return ExitCode::from(exit::USAGE);
    }
    let summary = match reader.issues_summary(false) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::from(UNREADABLE);
        }
    };

    if let Some(dir) = &a.export {
        if let Err(code) = export(&a.file, dir, &a.only, &summary, reader.issue_settings()) {
            return code;
        }
    } else {
        let shown = shown(&summary, a.all, a.min_priority);
        if a.json {
            crate::print_json(&IssuesSummary {
                issues: shown.into_iter().cloned().collect(),
                ..summary.clone()
            });
        } else {
            print_human(&shown);
        }
    }

    let failed = a.fail_on.is_some_and(|p| {
        summary
            .issues
            .iter()
            .any(|i| i.enabled && i.priority <= p && i.count.unwrap_or(0) > 0)
    });
    ExitCode::from(if failed { exit::ISSUES_FOUND } else { exit::FETCHED })
}

fn read_settings(path: &Path) -> Result<IssueSettings, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))
}

fn read_profile(path: &Path) -> Result<IssueSettings, String> {
    let text =
        std::fs::read_to_string(path).map_err(|e| format!("cannot read profile {}: {e}", path.display()))?;
    let p = import_profile(&text).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(p.config.issues)
}

/// Issues to list: with URLs and turned on, unless `all`; at or above `min`.
fn shown(s: &IssuesSummary, all: bool, min: Option<Priority>) -> Vec<&IssueCount> {
    let mut out: Vec<&IssueCount> = s
        .issues
        .iter()
        .filter(|i| all || (i.enabled && i.count.unwrap_or(0) > 0))
        .filter(|i| min.is_none_or(|m| i.priority <= m))
        .collect();
    out.sort_by(|x, y| x.priority.cmp(&y.priority).then(y.count.cmp(&x.count)));
    out
}

fn print_human(shown: &[&IssueCount]) {
    if shown.is_empty() {
        println!("No issues found.");
        return;
    }
    for p in Priority::ALL {
        let group: Vec<_> = shown.iter().filter(|i| i.priority == p).collect();
        if group.is_empty() {
            continue;
        }
        let urls: u64 = group.iter().filter_map(|i| i.count).sum();
        println!("{} ({} issues, {urls} URLs)", p.label(), group.len());
        for i in group {
            let count = match (i.enabled, i.count, &i.unavailable) {
                (false, _, _) => "off".to_string(),
                (_, _, Some(_)) => "n/a".to_string(),
                (_, Some(n), _) => n.to_string(),
                (_, None, _) => "-".to_string(),
            };
            let note = i
                .unavailable
                .as_deref()
                .map(|u| format!("  ({u})"))
                .unwrap_or_default();
            println!("  {count:>7}  {}  {}{note}", i.id, i.name);
        }
    }
}

/// One CSV per issue, `<id>.csv`, same rows and columns as the app's export.
fn export(
    file: &Path,
    dir: &Path,
    only: &[String],
    summary: &IssuesSummary,
    settings: &ResolvedSettings,
) -> Result<(), ExitCode> {
    for id in only {
        if issues::get(id).is_none() {
            eprintln!("error: unknown issue {id}. Run `spiderlens rules` for the ids.");
            return Err(ExitCode::from(exit::USAGE));
        }
    }
    let ids: Vec<&str> = if only.is_empty() {
        summary
            .issues
            .iter()
            .filter(|i| i.enabled && i.count.unwrap_or(0) > 0)
            .map(|i| i.id.as_str())
            .collect()
    } else {
        only.iter().map(String::as_str).collect()
    };
    if let Err(e) = std::fs::create_dir_all(dir) {
        eprintln!("error: cannot create {}: {e}", dir.display());
        return Err(ExitCode::from(UNREADABLE));
    }
    let never = AtomicBool::new(false);
    for id in ids {
        let q = ViewQuery {
            tab: format!("issue:{id}"),
            filter: "all".into(),
            scope: UrlScope::All,
            search: String::new(),
            sort: None,
            values: None,
            segment: None,
        };
        let dest = dir.join(format!("{id}.csv"));
        match export_file_with(file, &q, &[], None, None, settings, &dest, &never, &mut |_, _| {}) {
            Ok(n) => eprintln!("{n} rows  {}", dest.display()),
            Err(e) => {
                eprintln!("error: {id}: {e}");
                return Err(ExitCode::from(UNREADABLE));
            }
        }
    }
    Ok(())
}

#[derive(Args)]
pub struct AnalyseArgs {
    /// The crawl file.
    file: PathBuf,
    /// Print what ran as JSON.
    #[arg(long)]
    json: bool,
}

/// `spiderlens analyse`: runs the Crawl Analysis passes that have not run on
/// this crawl's data (spec 009 plan D12).
pub fn run_analyse(a: AnalyseArgs) -> ExitCode {
    match analyse(&a.file, !a.json) {
        Ok(report) => {
            if a.json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&report).expect("report serializes")
                );
            } else {
                for (id, reason) in &report.skipped {
                    println!("skipped {id}: {reason}");
                }
                for (id, ms) in &report.ran {
                    println!("analysed {id} in {ms} ms");
                }
                if report.ran.is_empty() && report.skipped.is_empty() {
                    println!("nothing to analyse");
                }
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::from(UNREADABLE)
        }
    }
}

/// Runs the stale analysis passes on `file`, printing "Analysing: Link
/// Score 40%" lines to stderr when `show`.
pub(crate) fn analyse(
    file: &Path,
    show: bool,
) -> spiderlens_results::Result<spiderlens_results::analysis::AnalysisReport> {
    let mut last = (String::new(), u64::MAX);
    let mut progress = |p: spiderlens_results::analysis::PassProgress| {
        let pct = (p.done * 100).checked_div(p.total).unwrap_or(0);
        if show && (p.label != last.0 || pct != last.1) {
            eprintln!("Analysing: {} {pct}%", p.label);
            last = (p.label, pct);
        }
    };
    spiderlens_results::analysis::analyse_file(file, &mut progress, &AtomicBool::new(false))
}
