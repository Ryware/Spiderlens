//! Export a results view to a new Google spreadsheet (spec 024 US6, plan D10,
//! FR-030).
//!
//! The rows come from the results CSV export (`spiderlens_results::export_file_in`
//! writes the file; the caller passes its path), so a sheet holds exactly the
//! rows and columns the CSV export of the same view holds: the visible
//! columns, the filter and the sort as the user sees them.
//!
//! The spreadsheet is created with a grid that fits the data
//! (`spreadsheets.create`) and filled with `spreadsheets.values:batchUpdate`
//! in batches. A spreadsheet holds at most 10,000,000 cells, so the size is
//! checked before anything is sent and the user may export the first rows
//! that fit or split the rows across several spreadsheets. Only the
//! `drive.file` scope is used: SpiderLens sees the files it created.

use std::path::Path;

use reqwest::Method;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use ts_rs::TS;

use crate::http::HttpError;
use crate::limits::with_retry;
use crate::oauth::Session;
use crate::run::{Control, Ctx};
use crate::{Error, Result};

/// Cells one spreadsheet holds (Google's published limit).
pub const CELL_CAPACITY: u64 = 10_000_000;

/// Most rows in one `values:batchUpdate` request (plan D10).
pub const BATCH_ROWS: usize = 5_000;

/// Most cells in one request, to keep a payload around 2 MB (research.md).
const BATCH_CELLS: usize = 200_000;

#[derive(Debug, Clone)]
pub struct Api {
    pub base: String,
    /// Cells one spreadsheet holds; tests lower it.
    pub cell_capacity: u64,
}

impl Default for Api {
    fn default() -> Self {
        Self {
            base: "https://sheets.googleapis.com/v4".into(),
            cell_capacity: CELL_CAPACITY,
        }
    }
}

/// What to do when the rows do not fit one spreadsheet (US6.2).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum Overflow {
    /// Send nothing and say why.
    #[default]
    Refuse,
    /// Export the first rows that fit.
    Truncate,
    /// Several spreadsheets, each within the limit.
    Split,
}

/// What an export would send, known before anything is sent (FR-030).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SheetsPlan {
    /// Data rows, without the header.
    #[ts(type = "number")]
    pub rows: u64,
    #[ts(type = "number")]
    pub columns: u64,
    /// Header and data.
    #[ts(type = "number")]
    pub cells: u64,
    #[ts(type = "number")]
    pub capacity: u64,
    pub fits: bool,
    /// Data rows one spreadsheet holds next to its header.
    #[ts(type = "number")]
    pub rows_per_spreadsheet: u64,
    /// Spreadsheets a split needs.
    #[ts(type = "number")]
    pub spreadsheets: u64,
}

pub fn plan(rows: u64, columns: u64, capacity: u64) -> SheetsPlan {
    let columns = columns.max(1);
    let rows_per = (capacity / columns).saturating_sub(1);
    SheetsPlan {
        rows,
        columns,
        cells: (rows + 1) * columns,
        capacity,
        fits: (rows + 1) * columns <= capacity,
        rows_per_spreadsheet: rows_per,
        spreadsheets: if rows_per == 0 {
            0
        } else {
            rows.div_ceil(rows_per).max(1)
        },
    }
}

impl SheetsPlan {
    /// The sentence shown before anything is sent when the rows do not fit.
    pub fn warning(&self) -> Option<String> {
        if self.fits {
            return None;
        }
        Some(format!(
            "This export has {} cells ({} rows by {} columns). A spreadsheet holds {}. \
             Export the first {} rows, or split it across {} spreadsheets.",
            group(self.cells),
            group(self.rows),
            group(self.columns),
            group(self.capacity),
            group(self.rows_per_spreadsheet),
            self.spreadsheets
        ))
    }
}

fn group(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// A spreadsheet an export created.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SpreadsheetRef {
    pub spreadsheet_id: String,
    pub url: String,
    #[ts(type = "number")]
    pub worksheet_id: i64,
    pub title: String,
    /// Data rows in it.
    #[ts(type = "number")]
    pub rows: u64,
}

/// The result of an export (data-model.md `SheetExport`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SheetExport {
    pub spreadsheets: Vec<SpreadsheetRef>,
    /// Data rows sent.
    #[ts(type = "number")]
    pub rows: u64,
    /// Data rows the view has.
    #[ts(type = "number")]
    pub total_rows: u64,
    /// Fewer rows were sent than the view has.
    pub truncated: bool,
}

fn csv_error(e: csv::Error) -> Error {
    Error::Invalid(format!("could not read the exported rows: {e}"))
}

/// Data rows and columns of a CSV export, read without loading it.
pub fn scan_csv(path: &Path) -> Result<(u64, u64)> {
    let mut rd = csv::ReaderBuilder::new()
        .has_headers(false)
        .flexible(true)
        .from_path(path)
        .map_err(csv_error)?;
    let mut rec = csv::StringRecord::new();
    let mut rows = 0u64;
    let mut cols = 0u64;
    let mut first = true;
    while rd.read_record(&mut rec).map_err(csv_error)? {
        if first {
            cols = rec.len() as u64;
            first = false;
        } else {
            rows += 1;
        }
    }
    Ok((rows, cols))
}

/// Undoes the CSV export's formula guard: a value that would run as a
/// formula in a spreadsheet gets a leading apostrophe there, which a RAW
/// cell would show.
fn unguard(s: &str) -> &str {
    match s.strip_prefix('\'') {
        Some(rest) if rest.starts_with(['=', '+', '-', '@', '\t', '\r']) => rest,
        _ => s,
    }
}

/// A cell: numbers as numbers (they sum and sort), everything else as text.
fn cell(field: &str) -> Value {
    let s = unguard(field);
    if let Ok(i) = s.parse::<i64>()
        && i.to_string() == s
    {
        return json!(i);
    }
    if let Ok(f) = s.parse::<f64>()
        && f.is_finite()
        && f.to_string() == s
    {
        return json!(f);
    }
    Value::String(s.to_string())
}

/// A worksheet name Sheets accepts.
pub fn worksheet_title(s: &str) -> String {
    let t: String = s
        .chars()
        .filter(|c| !matches!(c, '[' | ']' | '*' | '?' | ':' | '/' | '\\'))
        .take(100)
        .collect();
    let t = t.trim();
    if t.is_empty() { "Results".into() } else { t.into() }
}

fn a1(sheet: &str, row: usize) -> String {
    format!("'{}'!A{row}", sheet.replace('\'', "''"))
}

/// Calls the API with the session's token: refreshes once on 401 and
/// applies spec 004's retry rules to transient failures.
async fn call(ctx: &Ctx, session: &Session, url: &str, body: &Value) -> Result<Value> {
    let mut refreshed = false;
    loop {
        ctx.bucket.acquire().await;
        let (r, _) = with_retry(|| async {
            let token = session.bearer().await?;
            let req = ctx
                .http
                .request(Method::POST, url)
                .bearer_auth(token)
                .header(reqwest::header::CONTENT_TYPE, "application/json")
                .body(body.to_string());
            ctx.http.json("Google Sheets", req).await
        })
        .await;
        match r {
            Err(HttpError::Unauthorized { .. }) if !refreshed => {
                session.invalidate();
                refreshed = true;
            }
            other => return other.map_err(Error::from),
        }
    }
}

/// Creates the spreadsheet(s) for the CSV at `csv` and fills them. Nothing
/// is sent when the rows do not fit and `overflow` is [`Overflow::Refuse`].
/// `on_progress` gets data rows sent and rows to send.
#[allow(clippy::too_many_arguments)]
pub async fn export_csv(
    ctx: &Ctx,
    session: &Session,
    api: &Api,
    csv: &Path,
    title: &str,
    worksheet: &str,
    overflow: Overflow,
    control: &Control,
    on_progress: &mut dyn FnMut(u64, u64),
) -> Result<SheetExport> {
    let (rows, cols) = scan_csv(csv)?;
    if cols == 0 {
        return Err(Error::Invalid("there is nothing to export".into()));
    }
    let pl = plan(rows, cols, api.cell_capacity);
    if pl.rows_per_spreadsheet == 0 {
        return Err(Error::Invalid(format!(
            "{} columns do not fit a spreadsheet; choose fewer columns",
            group(cols)
        )));
    }
    if !pl.fits && overflow == Overflow::Refuse {
        return Err(Error::Invalid(pl.warning().unwrap_or_default()));
    }
    let per = pl.rows_per_spreadsheet;
    let chunks: Vec<u64> = if pl.fits {
        vec![rows]
    } else if overflow == Overflow::Truncate {
        vec![per]
    } else {
        let mut v = vec![per; (rows / per) as usize];
        if rows % per > 0 {
            v.push(rows % per);
        }
        v
    };
    let total: u64 = chunks.iter().sum();
    let sheet = worksheet_title(worksheet);
    let per_batch = BATCH_ROWS.min((BATCH_CELLS / cols as usize).max(1));

    let mut rd = csv::ReaderBuilder::new()
        .has_headers(false)
        .flexible(true)
        .from_path(csv)
        .map_err(csv_error)?;
    let mut rec = csv::StringRecord::new();
    rd.read_record(&mut rec).map_err(csv_error)?;
    let header: Vec<Value> = rec
        .iter()
        .enumerate()
        .map(|(i, f)| {
            let f = if i == 0 {
                f.trim_start_matches('\u{feff}')
            } else {
                f
            };
            Value::String(unguard(f).to_string())
        })
        .collect();
    let width = header.len();

    let mut made: Vec<SpreadsheetRef> = Vec::new();
    let mut sent = 0u64;
    on_progress(0, total);
    for (i, chunk_rows) in chunks.iter().enumerate() {
        let name = if chunks.len() > 1 {
            format!("{title} (part {} of {})", i + 1, chunks.len())
        } else {
            title.to_string()
        };
        let created = call(
            ctx,
            session,
            &format!("{}/spreadsheets", api.base),
            &json!({
                "properties": {"title": name},
                "sheets": [{"properties": {
                    "title": sheet,
                    "gridProperties": {"rowCount": chunk_rows + 1, "columnCount": cols},
                }}],
            }),
        )
        .await?;
        let id = created["spreadsheetId"]
            .as_str()
            .ok_or_else(|| Error::Invalid("Google did not return a spreadsheet id".into()))?
            .to_string();
        let url = created["spreadsheetUrl"].as_str().map_or_else(
            || format!("https://docs.google.com/spreadsheets/d/{id}/edit"),
            str::to_string,
        );
        let worksheet_id = created
            .pointer("/sheets/0/properties/sheetId")
            .and_then(Value::as_i64)
            .unwrap_or(0);
        made.push(SpreadsheetRef {
            spreadsheet_id: id.clone(),
            url: url.clone(),
            worksheet_id,
            title: name,
            rows: *chunk_rows,
        });

        let mut remaining = *chunk_rows;
        let mut next_row = 1usize;
        let mut first = true;
        while first || remaining > 0 {
            if control.cancelled() {
                return Err(Error::Invalid(format!(
                    "export cancelled; the partly filled spreadsheet {url} stays in your Drive"
                )));
            }
            let mut values: Vec<Vec<Value>> = Vec::new();
            if first {
                values.push(header.clone());
            }
            let take = remaining.min(per_batch as u64);
            let mut got = 0u64;
            while got < take {
                if !rd.read_record(&mut rec).map_err(csv_error)? {
                    break;
                }
                let mut row: Vec<Value> = rec.iter().take(width).map(cell).collect();
                row.resize(width, Value::String(String::new()));
                values.push(row);
                got += 1;
            }
            let count = values.len();
            call(
                ctx,
                session,
                &format!("{}/spreadsheets/{id}/values:batchUpdate", api.base),
                &json!({
                    "valueInputOption": "RAW",
                    "data": [{"range": a1(&sheet, next_row), "majorDimension": "ROWS", "values": values}],
                }),
            )
            .await?;
            next_row += count;
            sent += got;
            remaining = if got < take { 0 } else { remaining - got };
            first = false;
            on_progress(sent, total);
        }
    }
    Ok(SheetExport {
        spreadsheets: made,
        rows: sent,
        total_rows: rows,
        truncated: sent < rows,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plan_counts_the_header_and_splits_by_capacity() {
        // 10 columns: a spreadsheet holds 1,000,000 rows of which one is the header.
        let p = plan(2_500_000, 10, CELL_CAPACITY);
        assert!(!p.fits);
        assert_eq!(p.rows_per_spreadsheet, 999_999);
        assert_eq!(p.spreadsheets, 3);
        assert!(p.warning().unwrap().contains("2,500,000 rows"));
        let ok = plan(999_999, 10, CELL_CAPACITY);
        assert!(ok.fits && ok.warning().is_none());
        assert!(!plan(1_000_000, 10, CELL_CAPACITY).fits);
    }

    #[test]
    fn cells_keep_numbers_and_drop_the_formula_guard() {
        assert_eq!(cell("13"), json!(13));
        assert_eq!(cell("4.5"), json!(4.5));
        assert_eq!(cell("007"), json!("007"));
        assert_eq!(cell("1e5"), json!("1e5"));
        assert_eq!(cell("'=SUM(A1)"), json!("=SUM(A1)"));
        assert_eq!(cell("it's"), json!("it's"));
        assert_eq!(cell("https://a.test/"), json!("https://a.test/"));
    }

    #[test]
    fn worksheet_names_are_valid() {
        assert_eq!(
            worksheet_title("Page Titles: Missing/Short"),
            "Page Titles MissingShort"
        );
        assert_eq!(worksheet_title("[]"), "Results");
        assert_eq!(a1("Bob's", 5), "'Bob''s'!A5");
    }
}
