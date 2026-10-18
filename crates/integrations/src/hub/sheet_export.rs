//! The hub's Google Sheets and Looker Studio methods (spec 024 US6): export
//! a results CSV to a new spreadsheet, and the link that opens a report on it.

use std::path::Path;

use super::Hub;
use crate::run::{self, Control};
use crate::sheets::{self, Overflow, SheetExport, SheetsPlan, SpreadsheetRef};
use crate::{Provider, Result, looker};

impl Hub {
    /// Points Google Sheets elsewhere, or lowers its cell limit (tests).
    pub fn with_sheets(mut self, api: sheets::Api) -> Self {
        self.sheets = api;
        self
    }

    /// The size of an export, known before anything is sent, so the app can
    /// warn when it will not fit one spreadsheet (FR-030).
    pub fn sheets_plan(&self, csv: &Path) -> Result<SheetsPlan> {
        let (rows, cols) = sheets::scan_csv(csv)?;
        Ok(sheets::plan(rows, cols, self.sheets.cell_capacity))
    }

    /// Creates a new spreadsheet in the signed-in account's Drive with the
    /// rows of `csv` (a results CSV export). `on_progress` gets data rows
    /// sent and rows to send.
    #[allow(clippy::too_many_arguments)]
    pub fn export_sheets(
        &self,
        csv: &Path,
        title: &str,
        worksheet: &str,
        account: Option<&str>,
        overflow: Overflow,
        control: &Control,
        on_progress: &mut dyn FnMut(u64, u64),
    ) -> Result<SheetExport> {
        self.require_enabled(Provider::Sheets)?;
        let (session, _) = self.google_session(Provider::Sheets, account)?;
        let ctx = self.ctx(Provider::Sheets, false);
        run::block_on(sheets::export_csv(
            &ctx,
            &session,
            &self.sheets,
            csv,
            title,
            worksheet,
            overflow,
            control,
            on_progress,
        ))
    }

    /// The link that opens a new Looker Studio report on an exported sheet.
    pub fn looker_link(&self, sheet: &SpreadsheetRef) -> Result<String> {
        looker::link_for(sheet)
    }
}
