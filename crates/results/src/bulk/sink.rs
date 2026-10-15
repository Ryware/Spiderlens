//! Where bulk tables go: a folder of CSV files or one Excel workbook
//! (spec 010 plan D9, D10).

use std::collections::HashSet;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

use rusqlite::types::Value;
use rust_xlsxwriter::{Format, Workbook, Worksheet};

use crate::ValueKind;
use crate::export::{ExportError, csv_field};

/// One column of a bulk table.
#[derive(Debug, Clone)]
pub struct Col {
    pub label: String,
    pub kind: ValueKind,
}

impl Col {
    pub fn text(label: &str) -> Self {
        Self {
            label: label.into(),
            kind: ValueKind::Text,
        }
    }
    pub fn int(label: &str) -> Self {
        Self {
            label: label.into(),
            kind: ValueKind::Int,
        }
    }
    pub fn bool(label: &str) -> Self {
        Self {
            label: label.into(),
            kind: ValueKind::Bool,
        }
    }
}

/// A line of the workbook's overview, or of `index.csv`.
#[derive(Debug, Clone)]
pub struct OverviewRow {
    pub table: String,
    pub sheet: String,
    pub rows: u64,
    pub priority: String,
    pub why: String,
    pub fix: String,
}

pub trait Sink {
    /// `name` is the file stem or sheet name; `title` what the overview says.
    fn begin(&mut self, name: &str, cols: &[Col]) -> Result<(), ExportError>;
    fn row(&mut self, values: &[Value]) -> Result<(), ExportError>;
    fn end(&mut self, overview: OverviewRow) -> Result<(), ExportError>;
    /// Writes what is left and moves the output into place.
    fn finish(self: Box<Self>) -> Result<Finished, ExportError>;
    /// Removes everything written so far.
    fn abort(self: Box<Self>);
}

#[derive(Debug, Clone, Default)]
pub struct Finished {
    pub files: Vec<PathBuf>,
    pub truncated_cells: u64,
    pub split_sheets: u32,
}

/// `<dir>/.<name>.partial`: written there, renamed at the end.
pub(crate) fn partial_path(dest: &Path) -> PathBuf {
    let mut name = std::ffi::OsString::from(".");
    name.push(dest.file_name().unwrap_or_default());
    name.push(".partial");
    dest.with_file_name(name)
}

/// Refuses an existing file or a non-empty folder unless `overwrite`
/// (FR-025).
pub(crate) fn check_target(dest: &Path, overwrite: bool) -> Result<(), ExportError> {
    let exists = if dest.is_dir() {
        std::fs::read_dir(dest)?.next().is_some()
    } else {
        dest.exists()
    };
    if exists && !overwrite {
        return Err(ExportError::Exists(dest.to_path_buf()));
    }
    Ok(())
}

fn text_of(kind: ValueKind, v: &Value) -> String {
    match v {
        Value::Null | Value::Blob(_) => String::new(),
        Value::Integer(i) if kind == ValueKind::Bool => (if *i != 0 { "Yes" } else { "No" }).into(),
        Value::Integer(i) if kind == ValueKind::Issues => crate::export::issues_text(*i),
        Value::Integer(i) => i.to_string(),
        Value::Real(f) => f.to_string(),
        Value::Text(s) => s.clone(),
    }
}

/// Safe file name part: letters, digits, `-`, `_` and `.`.
pub(crate) fn file_stem(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.') {
                c
            } else {
                '-'
            }
        })
        .collect()
}

pub struct CsvFolderSink {
    dest: PathBuf,
    tmp: PathBuf,
    overwrite: bool,
    cur: Option<(BufWriter<File>, Vec<Col>)>,
    files: Vec<PathBuf>,
    overview: Vec<OverviewRow>,
}

impl CsvFolderSink {
    pub fn create(dest: &Path, overwrite: bool) -> Result<Self, ExportError> {
        check_target(dest, overwrite)?;
        let tmp = partial_path(dest);
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp)?;
        Ok(Self {
            dest: dest.to_path_buf(),
            tmp,
            overwrite,
            cur: None,
            files: Vec::new(),
            overview: Vec::new(),
        })
    }

    fn write_header(out: &mut impl Write, labels: impl Iterator<Item = String>) -> std::io::Result<()> {
        out.write_all(b"\xEF\xBB\xBF")?;
        for (i, l) in labels.enumerate() {
            if i > 0 {
                out.write_all(b",")?;
            }
            csv_field(out, &l)?;
        }
        out.write_all(b"\r\n")
    }
}

impl Sink for CsvFolderSink {
    fn begin(&mut self, name: &str, cols: &[Col]) -> Result<(), ExportError> {
        let file = format!("{}.csv", file_stem(name));
        let path = self.tmp.join(&file);
        let mut out = BufWriter::with_capacity(1 << 16, File::create(&path)?);
        Self::write_header(&mut out, cols.iter().map(|c| c.label.clone()))?;
        self.files.push(self.dest.join(file));
        self.cur = Some((out, cols.to_vec()));
        Ok(())
    }

    fn row(&mut self, values: &[Value]) -> Result<(), ExportError> {
        let (out, cols) = self.cur.as_mut().expect("begin before row");
        for (i, (v, c)) in values.iter().zip(cols.iter()).enumerate() {
            if i > 0 {
                out.write_all(b",")?;
            }
            match v {
                Value::Text(s) => csv_field(out, s)?,
                other => out.write_all(text_of(c.kind, other).as_bytes())?,
            }
        }
        out.write_all(b"\r\n")?;
        Ok(())
    }

    fn end(&mut self, overview: OverviewRow) -> Result<(), ExportError> {
        if let Some((mut out, _)) = self.cur.take() {
            out.flush()?;
            out.into_inner().map_err(|e| e.into_error())?.sync_all()?;
        }
        self.overview.push(overview);
        Ok(())
    }

    fn finish(self: Box<Self>) -> Result<Finished, ExportError> {
        let mut me = *self;
        // index.csv: what each file holds (the CSV twin of the Overview sheet).
        {
            let path = me.tmp.join("index.csv");
            let mut out = BufWriter::new(File::create(&path)?);
            Self::write_header(
                &mut out,
                [
                    "File",
                    "Table",
                    "Rows",
                    "Priority",
                    "Why it matters",
                    "How to fix",
                ]
                .iter()
                .map(|s| s.to_string()),
            )?;
            for o in &me.overview {
                for (i, f) in [
                    format!("{}.csv", file_stem(&o.sheet)),
                    o.table.clone(),
                    o.rows.to_string(),
                    o.priority.clone(),
                    o.why.clone(),
                    o.fix.clone(),
                ]
                .iter()
                .enumerate()
                {
                    if i > 0 {
                        out.write_all(b",")?;
                    }
                    csv_field(&mut out, f)?;
                }
                out.write_all(b"\r\n")?;
            }
            out.flush()?;
            me.files.insert(0, me.dest.join("index.csv"));
        }
        if me.dest.exists() {
            if !me.overwrite {
                return Err(ExportError::Exists(me.dest.clone()));
            }
            if me.dest.is_dir() {
                std::fs::remove_dir_all(&me.dest)?;
            } else {
                std::fs::remove_file(&me.dest)?;
            }
        }
        std::fs::rename(&me.tmp, &me.dest)?;
        Ok(Finished {
            files: me.files,
            truncated_cells: 0,
            split_sheets: 0,
        })
    }

    fn abort(self: Box<Self>) {
        let _ = std::fs::remove_dir_all(&self.tmp);
    }
}

/// Excel limits.
pub const XLSX_MAX_CELL: usize = 32_767;
const XLSX_SHEET_NAME: usize = 31;
const TRUNCATED: &str = " [cut: the full value is in the CSV export]";

pub struct XlsxSink {
    dest: PathBuf,
    tmp: PathBuf,
    overwrite: bool,
    book: Workbook,
    /// Data rows per sheet before a continuation sheet (Excel's limit
    /// minus the header row; smaller in tests).
    max_rows: u32,
    names: HashSet<String>,
    cur: Option<Cur>,
    overview: Vec<OverviewRow>,
    truncated: u64,
    split: u32,
    header: Format,
}

struct Cur {
    base: String,
    cols: Vec<Col>,
    sheet: usize,
    row: u32,
    part: u32,
}

impl XlsxSink {
    pub fn create(dest: &Path, overwrite: bool) -> Result<Self, ExportError> {
        Self::with_max_rows(dest, overwrite, 1_048_575)
    }

    pub fn with_max_rows(dest: &Path, overwrite: bool, max_rows: u32) -> Result<Self, ExportError> {
        check_target(dest, overwrite)?;
        let mut book = Workbook::new();
        let tmp = partial_path(dest);
        let ov = book.add_worksheet();
        ov.set_name("Overview").map_err(xl)?;
        let mut names = HashSet::new();
        names.insert("overview".to_string());
        Ok(Self {
            dest: dest.to_path_buf(),
            tmp,
            overwrite,
            book,
            max_rows,
            names,
            cur: None,
            overview: Vec::new(),
            truncated: 0,
            split: 0,
            header: Format::new().set_bold(),
        })
    }

    /// A valid, unique sheet name (31 characters, no `[]:*?/\`).
    fn sheet_name(&mut self, want: &str) -> String {
        let clean: String = want
            .chars()
            .map(|c| {
                if matches!(c, '[' | ']' | ':' | '*' | '?' | '/' | '\\') {
                    '-'
                } else {
                    c
                }
            })
            .collect();
        let clean = clean.trim_matches('\'').to_string();
        let mut n = 1;
        loop {
            let suffix = if n == 1 { String::new() } else { format!(" ({n})") };
            let room = XLSX_SHEET_NAME - suffix.chars().count();
            let name: String = clean.chars().take(room).collect::<String>() + &suffix;
            if self.names.insert(name.to_lowercase()) {
                return name;
            }
            n += 1;
        }
    }

    fn new_sheet(&mut self, name: &str, cols: &[Col]) -> Result<usize, ExportError> {
        let name = self.sheet_name(name);
        let header = self.header.clone();
        let ws: &mut Worksheet = self.book.add_worksheet_with_constant_memory();
        ws.set_name(&name).map_err(xl)?;
        for (i, c) in cols.iter().enumerate() {
            ws.write_string_with_format(0, i as u16, &c.label, &header)
                .map_err(xl)?;
            ws.set_column_width(i as u16, if c.kind.is_text() { 40 } else { 12 })
                .map_err(xl)?;
        }
        ws.set_freeze_panes(1, 0).map_err(xl)?;
        Ok(self.book.worksheets().len() - 1)
    }
}

fn xl(e: rust_xlsxwriter::XlsxError) -> ExportError {
    match e {
        rust_xlsxwriter::XlsxError::IoError(io) => ExportError::Io(io),
        other => ExportError::Io(std::io::Error::other(other.to_string())),
    }
}

impl Sink for XlsxSink {
    fn begin(&mut self, name: &str, cols: &[Col]) -> Result<(), ExportError> {
        let sheet = self.new_sheet(name, cols)?;
        self.cur = Some(Cur {
            base: name.to_string(),
            cols: cols.to_vec(),
            sheet,
            row: 0,
            part: 1,
        });
        Ok(())
    }

    fn row(&mut self, values: &[Value]) -> Result<(), ExportError> {
        let full = {
            let cur = self.cur.as_ref().expect("begin before row");
            cur.row >= self.max_rows
        };
        if full {
            let (base, cols, part) = {
                let c = self.cur.as_ref().expect("cur");
                (c.base.clone(), c.cols.clone(), c.part + 1)
            };
            let sheet = self.new_sheet(&format!("{base} ({part})"), &cols)?;
            let c = self.cur.as_mut().expect("cur");
            c.sheet = sheet;
            c.row = 0;
            c.part = part;
            self.split += 1;
        }
        let cur = self.cur.as_mut().expect("cur");
        cur.row += 1;
        let r = cur.row;
        let ws = self.book.worksheet_from_index(cur.sheet).map_err(xl)?;
        for (i, (v, c)) in values.iter().zip(cur.cols.iter()).enumerate() {
            let col = i as u16;
            match v {
                Value::Null | Value::Blob(_) => {}
                Value::Integer(n) if c.kind == ValueKind::Bool => {
                    ws.write_string(r, col, if *n != 0 { "Yes" } else { "No" })
                        .map_err(xl)?;
                }
                Value::Integer(n) if c.kind == ValueKind::Issues => {
                    ws.write_string(r, col, crate::export::issues_text(*n))
                        .map_err(xl)?;
                }
                Value::Integer(n) => {
                    ws.write_number(r, col, *n as f64).map_err(xl)?;
                }
                Value::Real(f) => {
                    ws.write_number(r, col, *f).map_err(xl)?;
                }
                // String cells are never evaluated as formulas, so text is
                // stored exactly as the app shows it (plan D9).
                Value::Text(s) => {
                    if s.chars().count() > XLSX_MAX_CELL {
                        let keep = XLSX_MAX_CELL - TRUNCATED.chars().count();
                        let cut: String = s.chars().take(keep).collect::<String>() + TRUNCATED;
                        ws.write_string(r, col, &cut).map_err(xl)?;
                        self.truncated += 1;
                    } else {
                        ws.write_string(r, col, s).map_err(xl)?;
                    }
                }
            }
        }
        Ok(())
    }

    fn end(&mut self, mut overview: OverviewRow) -> Result<(), ExportError> {
        if let Some(c) = self.cur.take() {
            let first = self.book.worksheets().len() - c.part as usize;
            overview.sheet = self.book.worksheets()[first].name();
        }
        self.overview.push(overview);
        Ok(())
    }

    fn finish(self: Box<Self>) -> Result<Finished, ExportError> {
        let mut me = *self;
        let bold = me.header.clone();
        let overview = std::mem::take(&mut me.overview);
        {
            let ws = me.book.worksheet_from_index(0).map_err(xl)?;
            for (i, h) in [
                "Table",
                "Sheet",
                "Rows",
                "Priority",
                "Why it matters",
                "How to fix",
            ]
            .iter()
            .enumerate()
            {
                ws.write_string_with_format(0, i as u16, *h, &bold).map_err(xl)?;
            }
            ws.set_column_width(0, 48).map_err(xl)?;
            ws.set_column_width(1, 32).map_err(xl)?;
            ws.set_column_width(4, 60).map_err(xl)?;
            ws.set_column_width(5, 60).map_err(xl)?;
            ws.set_freeze_panes(1, 0).map_err(xl)?;
            for (n, o) in overview.iter().enumerate() {
                let r = n as u32 + 1;
                ws.write_string(r, 0, &o.table).map_err(xl)?;
                let link =
                    rust_xlsxwriter::Url::new(format!("internal:'{}'!A1", o.sheet.replace('\'', "''")))
                        .set_text(&o.sheet);
                ws.write_url(r, 1, link).map_err(xl)?;
                ws.write_number(r, 2, o.rows as f64).map_err(xl)?;
                ws.write_string(r, 3, &o.priority).map_err(xl)?;
                ws.write_string(r, 4, &o.why).map_err(xl)?;
                ws.write_string(r, 5, &o.fix).map_err(xl)?;
            }
            if me.split > 0 || me.truncated > 0 {
                let r = overview.len() as u32 + 2;
                ws.write_string(
                    r,
                    0,
                    format!(
                        "{} table(s) continue on numbered sheets; {} cell(s) were cut at {} characters.",
                        me.split, me.truncated, XLSX_MAX_CELL
                    ),
                )
                .map_err(xl)?;
            }
        }
        me.book.save(&me.tmp).map_err(xl)?;
        // Windows refuses to flush a handle opened for reading only.
        std::fs::OpenOptions::new()
            .write(true)
            .open(&me.tmp)?
            .sync_all()?;
        if me.dest.exists() && !me.overwrite {
            let _ = std::fs::remove_file(&me.tmp);
            return Err(ExportError::Exists(me.dest.clone()));
        }
        std::fs::rename(&me.tmp, &me.dest)?;
        Ok(Finished {
            files: vec![me.dest.clone()],
            truncated_cells: me.truncated,
            split_sheets: me.split,
        })
    }

    fn abort(self: Box<Self>) {
        let _ = std::fs::remove_file(&self.tmp);
    }
}
