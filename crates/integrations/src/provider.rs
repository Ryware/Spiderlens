use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// An external data source (spec 024 key entity "Integration Provider").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum Provider {
    SearchConsole,
    UrlInspection,
    Analytics,
    FieldData,
    LabTests,
    Ahrefs,
    Majestic,
    Moz,
    Sheets,
}

impl Provider {
    pub const ALL: [Provider; 9] = [
        Self::SearchConsole,
        Self::UrlInspection,
        Self::Analytics,
        Self::FieldData,
        Self::LabTests,
        Self::Ahrefs,
        Self::Majestic,
        Self::Moz,
        Self::Sheets,
    ];

    pub fn id(self) -> &'static str {
        match self {
            Self::SearchConsole => "search_console",
            Self::UrlInspection => "url_inspection",
            Self::Analytics => "analytics",
            Self::FieldData => "field_data",
            Self::LabTests => "lab_tests",
            Self::Ahrefs => "ahrefs",
            Self::Majestic => "majestic",
            Self::Moz => "moz",
            Self::Sheets => "sheets",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|p| p.id() == id)
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::SearchConsole => "Search Console",
            Self::UrlInspection => "URL Inspection",
            Self::Analytics => "Google Analytics 4",
            Self::FieldData => "Chrome UX Report (field data)",
            Self::LabTests => "PageSpeed Insights (lab tests)",
            Self::Ahrefs => "Ahrefs",
            Self::Majestic => "Majestic",
            Self::Moz => "Moz",
            Self::Sheets => "Google Sheets",
        }
    }

    /// Signs in with a Google account rather than an API key.
    pub fn uses_google_account(self) -> bool {
        matches!(
            self,
            Self::SearchConsole | Self::UrlInspection | Self::Analytics | Self::Sheets
        )
    }

    /// What leaves the user's machine, shown before anything is sent (FR-019).
    pub fn sends(self) -> &'static str {
        match self {
            Self::SearchConsole => {
                "Sends the property and date range to Google. The list of crawled URLs is not sent."
            }
            Self::UrlInspection => "Sends each selected URL and its property to Google.",
            Self::Analytics => {
                "Sends the property, data stream and date range to Google. The list of crawled URLs is not sent."
            }
            Self::FieldData => "Sends each crawled page URL to Google's Chrome UX Report.",
            Self::LabTests => "Sends each selected URL to Google PageSpeed Insights, which loads the page.",
            Self::Ahrefs => "Sends each selected URL to Ahrefs and uses your plan's API units.",
            Self::Majestic => "Sends each selected URL to Majestic and uses your plan's units.",
            Self::Moz => "Sends each selected URL to Moz and uses your plan's rows.",
            Self::Sheets => "Creates a new spreadsheet in your Google Drive with the exported rows.",
        }
    }

    pub fn terms_url(self) -> &'static str {
        match self {
            Self::SearchConsole | Self::UrlInspection => "https://developers.google.com/webmaster-tools/terms",
            Self::Analytics => "https://developers.google.com/analytics/terms",
            Self::FieldData => "https://developer.chrome.com/docs/crux/api",
            Self::LabTests => "https://developers.google.com/speed/docs/insights/v5/about",
            Self::Ahrefs => "https://ahrefs.com/legal/terms-of-service",
            Self::Majestic => "https://majestic.com/policy/terms-and-conditions",
            Self::Moz => "https://moz.com/terms-of-use",
            Self::Sheets => "https://developers.google.com/workspace/terms",
        }
    }

    /// The `meta.data_features` id recorded when a run finishes (plan D8).
    pub fn feature_id(self) -> &'static str {
        match self {
            Self::Ahrefs | Self::Majestic | Self::Moz => "link_metrics",
            other => other.id(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_round_trip_and_serde_agrees() {
        for p in Provider::ALL {
            assert_eq!(Provider::from_id(p.id()), Some(p));
            assert_eq!(serde_json::to_string(&p).unwrap(), format!("\"{}\"", p.id()));
        }
    }
}
