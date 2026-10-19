//! `spiderlens accessibility <file>`: the Accessibility and Mobile views of a
//! crawl file (spec 026 FR-021, FR-022, FR-033). Every output states what
//! automated checks cannot do (FR-025).

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Args, Subcommand, ValueEnum};
use rusqlite::types::ValueRef;
use serde_json::{Value, json};
use spiderlens_config::{A11yLevel, AccessibilitySettings, WcagLevel, WcagTarget, WcagVersion};
use spiderlens_results::{A11Y_DISCLAIMER, Need, Reader};

use crate::exit;

#[derive(Args)]
#[command(args_conflicts_with_subcommands = true, subcommand_negates_reqs = true)]
pub struct AccessibilityArgs {
    #[command(subcommand)]
    command: Option<AccessibilityCommand>,
    /// Crawl file.
    #[arg(required = true)]
    file: Option<PathBuf>,
    /// Count against another target without checking again, such as
    /// 2.1-aa (2.0, 2.1 or 2.2, then a, aa or aaa).
    #[arg(long, value_name = "TARGET", value_parser = parse_wcag)]
    wcag: Option<WcagTarget>,
    /// Show "needs review" results instead of violations.
    #[arg(long)]
    needs_review: bool,
    /// One row per check: pages, elements, impact, priority, example page.
    #[arg(long)]
    summary: bool,
    /// The Mobile tab's rows instead.
    #[arg(long)]
    mobile: bool,
    /// Output format.
    #[arg(long, value_enum, default_value = "text")]
    format: FormatArg,
}

#[derive(Subcommand)]
enum AccessibilityCommand {
    /// Run the accessibility checks on a finished crawl from its stored HTML.
    /// Nothing is fetched.
    Run(RunArgs),
}

#[derive(Args)]
struct RunArgs {
    /// Crawl file.
    file: PathBuf,
    /// Static checks, from the stored HTML (the default).
    #[arg(long = "static", conflicts_with = "full")]
    static_checks: bool,
    /// Full checks in a browser. Needs the render-later path, which is not
    /// built yet.
    #[arg(long)]
    full: bool,
    /// Replace existing results without asking.
    #[arg(long)]
    yes: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, ValueEnum)]
enum FormatArg {
    Text,
    Csv,
    Json,
}

/// `2.2-aa` style WCAG targets, for `--wcag`.
pub fn parse_wcag(s: &str) -> Result<WcagTarget, String> {
    let bad = || {
        format!("{s:?} is not a WCAG target; use 2.0, 2.1 or 2.2, then -a, -aa or -aaa (for example 2.2-aa)")
    };
    let (version, level) = s.split_once('-').ok_or_else(bad)?;
    let version = match version {
        "2.0" => WcagVersion::V2_0,
        "2.1" => WcagVersion::V2_1,
        "2.2" => WcagVersion::V2_2,
        _ => return Err(bad()),
    };
    let level = match level.to_ascii_lowercase().as_str() {
        "a" => WcagLevel::A,
        "aa" => WcagLevel::Aa,
        "aaa" => WcagLevel::Aaa,
        _ => return Err(bad()),
    };
    Ok(WcagTarget { version, level })
}

/// What the three result queries select, with the column names shown.
struct Table {
    headers: &'static [&'static str],
    rows: Vec<Vec<Value>>,
}

const PAGES: &str = "SELECT u.url AS address, \
     COALESCE(SUM(r.impact = 'critical'), 0), COALESCE(SUM(r.impact = 'serious'), 0), \
     COALESCE(SUM(r.impact = 'moderate'), 0), COALESCE(SUM(r.impact = 'minor'), 0), \
     COUNT(r.id), COALESCE(SUM(r.nodes_total), 0) \
     FROM a11y_audits d JOIN urls u ON u.id = d.url_id \
     LEFT JOIN a11y_results r ON r.url_id = u.id AND r.kind = ?1 AND wcag_in_target(r.wcag) \
     GROUP BY u.id ORDER BY 2 DESC, 3 DESC, 4 DESC, 5 DESC, u.url";

const PAGE_HEADERS: &[&str] = &[
    "Address", "Critical", "Serious", "Moderate", "Minor", "Checks", "Elements",
];

const SUMMARY: &str = "SELECT r.check_id, COALESCE(r.impact, ''), COUNT(DISTINCT r.url_id), COALESCE(SUM(r.nodes_total), 0), \
     CASE WHEN r.wcag = 'best-practice' THEN 'Opportunity' WHEN r.impact = 'critical' THEN 'High' \
          WHEN r.impact = 'serious' THEN 'Medium' ELSE 'Low' END, MIN(u.url) \
     FROM a11y_results r JOIN urls u ON u.id = r.url_id \
     WHERE r.kind = ?1 AND wcag_in_target(r.wcag) \
     GROUP BY r.check_id, r.impact, r.wcag \
     ORDER BY CASE r.impact WHEN 'critical' THEN 0 WHEN 'serious' THEN 1 WHEN 'moderate' THEN 2 \
          WHEN 'minor' THEN 3 ELSE 4 END, 3 DESC, r.check_id";

const SUMMARY_HEADERS: &[&str] = &["Check", "Impact", "Pages", "Elements", "Priority", "Example page"];

const MOBILE: &str = "SELECT u.url, m.viewport, m.viewport_state, m.plugins, m.measured, \
     CAST(ROUND(m.content_px) AS INTEGER), CAST(ROUND(m.window_px) AS INTEGER), m.widest, \
     m.small_text_share, m.small_targets \
     FROM mobile_measures m JOIN urls u ON u.id = m.url_id ORDER BY u.url";

const MOBILE_HEADERS: &[&str] = &[
    "Address",
    "Viewport",
    "Viewport state",
    "Plugins",
    "Measured",
    "Content width",
    "Window width",
    "Widest element",
    "Small text share",
    "Small targets",
];

fn json_of(v: ValueRef<'_>) -> Value {
    match v {
        ValueRef::Null => Value::Null,
        ValueRef::Integer(i) => json!(i),
        ValueRef::Real(f) => json!((f * 10.0).round() / 10.0),
        ValueRef::Text(t) => Value::String(String::from_utf8_lossy(t).into_owned()),
        ValueRef::Blob(_) => Value::Null,
    }
}

fn query(
    reader: &Reader,
    sql: &str,
    kind: Option<&str>,
    headers: &'static [&'static str],
) -> rusqlite::Result<Table> {
    let mut stmt = reader.connection().prepare(sql)?;
    let n = headers.len();
    let map = |r: &rusqlite::Row<'_>| -> rusqlite::Result<Vec<Value>> {
        (0..n).map(|i| Ok(json_of(r.get_ref(i)?))).collect()
    };
    let rows = match kind {
        Some(k) => stmt.query_map([k], map)?.collect::<rusqlite::Result<Vec<_>>>()?,
        None => stmt.query_map([], map)?.collect::<rusqlite::Result<Vec<_>>>()?,
    };
    Ok(Table { headers, rows })
}

fn cell(v: &Value) -> String {
    match v {
        Value::Null => "-".into(),
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

pub fn run_accessibility(args: AccessibilityArgs) -> ExitCode {
    if let Some(AccessibilityCommand::Run(run)) = &args.command {
        return run_checks(run);
    }
    let Some(file) = args.file.clone() else {
        eprintln!("error: give a crawl file");
        return ExitCode::from(exit::USAGE);
    };
    let mut reader = match Reader::open(&file) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };
    let features = reader.features();
    let has_a11y = features.has(Need::AccessibilityStatic) || features.has(Need::AccessibilityFull);
    let has_mobile = features.has(Need::AccessibilityStatic) || features.has(Need::MobileLayout);
    if args.mobile && !has_mobile || !args.mobile && !has_a11y {
        eprintln!("error: {}", Need::AccessibilityStatic.unavailable_reason());
        return ExitCode::from(exit::USAGE);
    }
    if let Some(target) = args.wcag
        && let Err(e) = reader.set_a11y_target(target)
    {
        eprintln!("error: {e}");
        return ExitCode::FAILURE;
    }
    let kind = if args.needs_review {
        "needs_review"
    } else {
        "violation"
    };
    let table = if args.mobile {
        query(&reader, MOBILE, None, MOBILE_HEADERS)
    } else if args.summary {
        query(&reader, SUMMARY, Some(kind), SUMMARY_HEADERS)
    } else {
        query(&reader, PAGES, Some(kind), PAGE_HEADERS)
    };
    let table = match table {
        Ok(t) => t,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };
    let mut out = std::io::stdout().lock();
    match args.format {
        FormatArg::Text => print_text(&mut out, &table, &args),
        FormatArg::Csv => print_csv(&mut out, &table),
        FormatArg::Json => print_json_table(&mut out, &table),
    }
    ExitCode::from(exit::FETCHED)
}

fn print_text(out: &mut impl Write, t: &Table, args: &AccessibilityArgs) {
    let _ = writeln!(out, "{A11Y_DISCLAIMER}");
    let _ = writeln!(out);
    let what = if args.mobile {
        "Mobile measures"
    } else if args.needs_review {
        "Results that need review"
    } else {
        "Violations"
    };
    let _ = writeln!(
        out,
        "{what}: {} {}",
        t.rows.len(),
        if args.summary { "checks" } else { "pages" }
    );
    let _ = writeln!(out, "{}", t.headers.join("\t"));
    for row in &t.rows {
        let line: Vec<String> = row.iter().map(cell).collect();
        let _ = writeln!(out, "{}", line.join("\t"));
    }
}

fn print_csv(out: &mut impl Write, t: &Table) {
    let _ = writeln!(out, "# {A11Y_DISCLAIMER}");
    let line = |mut out: &mut dyn Write, cells: &[String]| {
        for (i, c) in cells.iter().enumerate() {
            if i > 0 {
                let _ = out.write_all(b",");
            }
            let _ = spiderlens_results::csv_field(&mut out, c);
        }
        let _ = out.write_all(b"\r\n");
    };
    line(
        &mut *out,
        &t.headers.iter().map(|h| (*h).to_string()).collect::<Vec<_>>(),
    );
    for row in &t.rows {
        let cells: Vec<String> = row
            .iter()
            .map(|v| if v.is_null() { String::new() } else { cell(v) })
            .collect();
        line(&mut *out, &cells);
    }
}

fn print_json_table(out: &mut impl Write, t: &Table) {
    let rows: Vec<Value> = t
        .rows
        .iter()
        .map(|row| {
            Value::Object(
                t.headers
                    .iter()
                    .map(|h| (*h).to_string())
                    .zip(row.iter().cloned())
                    .collect(),
            )
        })
        .collect();
    let v = json!({ "disclaimer": A11Y_DISCLAIMER, "rows": rows });
    let _ = writeln!(
        out,
        "{}",
        serde_json::to_string_pretty(&v).expect("rows serialize")
    );
}

/// `spiderlens accessibility run <file> [--static | --full]` (spec 026 FR-031).
fn run_checks(args: &RunArgs) -> ExitCode {
    use std::io::{BufRead, IsTerminal};
    if args.full {
        eprintln!(
            "error: Full checks on an existing crawl need the render-later path, which is not built yet"
        );
        return ExitCode::from(exit::USAGE);
    }
    let has_results = Reader::open(&args.file)
        .map(|r| r.features().has(Need::AccessibilityStatic) || r.features().has(Need::AccessibilityFull))
        .unwrap_or(false);
    if has_results && !args.yes {
        if !std::io::stdin().is_terminal() {
            eprintln!("error: this crawl already has accessibility results; add --yes to replace them");
            return ExitCode::from(exit::USAGE);
        }
        eprint!("This crawl already has accessibility results. Replace the static ones? [y/N] ");
        let mut answer = String::new();
        let _ = std::io::stdin().lock().read_line(&mut answer);
        if !matches!(answer.trim(), "y" | "Y" | "yes") {
            eprintln!("nothing changed");
            return ExitCode::from(exit::USAGE);
        }
    }
    let result = spiderlens_engine::accessibility::rerun_static(
        &args.file,
        &AccessibilitySettings::default(),
        |done, total| {
            eprint!("\r{done} of {total} pages   ");
        },
    );
    eprintln!();
    match result {
        Ok(s) => {
            println!(
                "accessibility: {} pages checked, {} with findings, {} without stored HTML",
                s.pages, s.with_findings, s.skipped_no_html
            );
            println!("{A11Y_DISCLAIMER}");
            ExitCode::from(exit::FETCHED)
        }
        Err(spiderlens_engine::accessibility::AccessibilityError::NotPossible(m)) => {
            eprintln!("error: {m}");
            ExitCode::from(exit::USAGE)
        }
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

/// The short block after a crawl (spec 026 FR-033): pages checked, pages with
/// violations and the disclaimer. Silent when the crawl did not check
/// accessibility.
pub fn print_crawl_summary(file: &Path, level: &A11yLevel) {
    if matches!(level, A11yLevel::Off) {
        return;
    }
    let Ok(reader) = Reader::open(file) else { return };
    if !reader.features().has(Need::AccessibilityStatic) && !reader.features().has(Need::AccessibilityFull) {
        return;
    }
    let counts = reader.connection().query_row(
        "SELECT COUNT(*), COALESCE(SUM(EXISTS (SELECT 1 FROM a11y_results r WHERE r.url_id = d.url_id \
         AND r.kind = 'violation' AND wcag_in_target(r.wcag))), 0), \
         (SELECT COUNT(*) FROM a11y_results r WHERE r.kind = 'violation' AND wcag_in_target(r.wcag)) \
         FROM a11y_audits d",
        [],
        |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?, r.get::<_, i64>(2)?)),
    );
    if let Ok((checked, with, violations)) = counts {
        eprintln!("accessibility: {checked} pages checked, {with} with violations ({violations} in total)");
        eprintln!("accessibility: {A11Y_DISCLAIMER}");
    }
}
