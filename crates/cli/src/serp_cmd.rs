//! `spiderlens serp`: title and description pixel widths for a list of
//! drafts, with no network (spec 027 US A3, FR-A07,
//! specs/027-content-checks/contracts/cli.md).

use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Args, ValueEnum};
use serde::Serialize;
use spiderlens_results::serp::{SerpStyle, SnippetMeasure, measure};

use crate::exit;

#[derive(Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Format {
    Table,
    Csv,
    Json,
}

#[derive(Args)]
pub struct SerpArgs {
    /// CSV of url,title,description (header row optional), or - for standard input.
    input: PathBuf,
    /// Output format.
    #[arg(long, value_enum, default_value = "table")]
    format: Format,
    /// Write to this file instead of standard output.
    #[arg(long, short)]
    out: Option<PathBuf>,
}

#[derive(Serialize)]
struct Row {
    url: String,
    title: String,
    description: String,
    title_measure: SnippetMeasure,
    description_measure: SnippetMeasure,
}

pub fn run_serp(a: SerpArgs) -> ExitCode {
    match serp(&a) {
        Ok(()) => ExitCode::from(exit::FETCHED),
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::from(exit::USAGE)
        }
    }
}

fn read_rows(a: &SerpArgs) -> anyhow::Result<Vec<Row>> {
    let mut data = Vec::new();
    if a.input.as_os_str() == "-" {
        std::io::stdin().read_to_end(&mut data)?;
    } else {
        data = std::fs::read(&a.input).map_err(|e| anyhow::anyhow!("{}: {e}", a.input.display()))?;
    }
    let mut rdr = csv::ReaderBuilder::new()
        .has_headers(false)
        .flexible(true)
        .from_reader(data.as_slice());
    let mut rows = Vec::new();
    for (i, rec) in rdr.records().enumerate() {
        let rec = rec.map_err(|e| anyhow::anyhow!("line {}: {e}", i + 1))?;
        let field = |n: usize| rec.get(n).unwrap_or("").to_string();
        if i == 0 && field(0).trim().eq_ignore_ascii_case("url") {
            continue;
        }
        let (url, title, description) = (field(0), field(1), field(2));
        if url.trim().is_empty() && title.trim().is_empty() && description.trim().is_empty() {
            continue;
        }
        rows.push(Row {
            title_measure: measure(&title, SerpStyle::Title),
            description_measure: measure(&description, SerpStyle::Description),
            url,
            title,
            description,
        });
    }
    Ok(rows)
}

fn serp(a: &SerpArgs) -> anyhow::Result<()> {
    let rows = read_rows(a)?;
    let mut out: Box<dyn Write> = match &a.out {
        Some(p) => Box::new(std::io::BufWriter::new(std::fs::File::create(p)?)),
        None => Box::new(std::io::stdout().lock()),
    };
    match a.format {
        Format::Json => serde_json::to_writer_pretty(&mut out, &rows)?,
        Format::Csv => {
            let mut w = csv::Writer::from_writer(&mut out);
            w.write_record([
                "url",
                "title",
                "title_chars",
                "title_px",
                "title_cut_desktop",
                "title_cut_mobile",
                "description",
                "description_chars",
                "description_px",
                "description_cut_desktop",
                "description_cut_mobile",
            ])?;
            for r in &rows {
                let (t, d) = (&r.title_measure, &r.description_measure);
                w.write_record([
                    r.url.clone(),
                    r.title.clone(),
                    t.chars.to_string(),
                    t.px.to_string(),
                    t.cut_desktop.to_string(),
                    t.cut_mobile.to_string(),
                    r.description.clone(),
                    d.chars.to_string(),
                    d.px.to_string(),
                    d.cut_desktop.to_string(),
                    d.cut_mobile.to_string(),
                ])?;
            }
            w.flush()?;
        }
        Format::Table => {
            let cut = |m: &SnippetMeasure| match (m.cut_desktop, m.cut_mobile) {
                (true, _) => "cut",
                (false, true) => "cut on mobile",
                (false, false) => "fits",
            };
            writeln!(out, "{:>9}  {:<13}  {:>9}  {:<13}  URL", "TITLE", "", "DESC", "")?;
            for r in &rows {
                let (t, d) = (&r.title_measure, &r.description_measure);
                writeln!(
                    out,
                    "{:>6} px  {:<13}  {:>6} px  {:<13}  {}",
                    t.px,
                    cut(t),
                    d.px,
                    cut(d),
                    r.url
                )?;
            }
        }
    }
    out.flush()?;
    Ok(())
}
