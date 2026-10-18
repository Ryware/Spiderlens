//! A Looker Studio report on an exported sheet (spec 024 US6.3, FR-031).
//!
//! The Linking API needs no server and no credentials of ours: the link is
//! opened in the user's browser and Looker Studio asks for their own Google
//! account. Nothing is sent from here.

use url::Url;

use crate::sheets::SpreadsheetRef;
use crate::{Error, Result};

const CREATE: &str = "https://lookerstudio.google.com/reporting/create";

/// The link that opens a new Looker Studio report connected to worksheet
/// `worksheet_id` of spreadsheet `spreadsheet_id`.
pub fn link(spreadsheet_id: &str, worksheet_id: i64, report_name: &str) -> Result<String> {
    if spreadsheet_id.is_empty()
        || !spreadsheet_id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err(Error::Invalid(format!(
            "{spreadsheet_id:?} is not a spreadsheet id"
        )));
    }
    let mut u = Url::parse(CREATE).expect("static URL");
    u.query_pairs_mut()
        .append_pair("r.reportName", report_name)
        .append_pair("ds.ds0.connector", "googleSheets")
        .append_pair("ds.ds0.spreadsheetId", spreadsheet_id)
        .append_pair("ds.ds0.worksheetId", &worksheet_id.to_string());
    Ok(u.into())
}

/// The link for an exported spreadsheet.
pub fn link_for(sheet: &SpreadsheetRef) -> Result<String> {
    link(&sheet.spreadsheet_id, sheet.worksheet_id, &sheet.title)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_link_names_the_sheet_and_worksheet() {
        let l = link("1AbC-d_9", 42, "Page titles").unwrap();
        let u = Url::parse(&l).unwrap();
        assert_eq!(u.host_str(), Some("lookerstudio.google.com"));
        assert_eq!(u.path(), "/reporting/create");
        let q: std::collections::HashMap<_, _> = u.query_pairs().into_owned().collect();
        assert_eq!(q["ds.ds0.connector"], "googleSheets");
        assert_eq!(q["ds.ds0.spreadsheetId"], "1AbC-d_9");
        assert_eq!(q["ds.ds0.worksheetId"], "42");
        assert_eq!(q["r.reportName"], "Page titles");
    }

    #[test]
    fn an_id_that_could_alter_the_link_is_refused() {
        assert!(link("a&b=c", 0, "x").is_err());
        assert!(link("", 0, "x").is_err());
    }
}
