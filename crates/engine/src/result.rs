use serde::Serialize;

use crate::CheckError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FinalState {
    Fetched,
    BlockedByRobots,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RedirectHop {
    pub url: String,
    pub status: u16,
    pub location: String,
}

/// Page title outcome. Serialized as a tagged union, for example
/// `{"kind":"text","value":"Home"}` or `{"kind":"missing"}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum Title {
    Text(String),
    /// HTML with no title, or an empty one.
    Missing,
    /// The response is not HTML.
    NotApplicable,
}

/// Outcome of one single-page check (spec 001, Key Entities).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PageCheckResult {
    pub requested_url: String,
    pub state: FinalState,
    pub redirects: Vec<RedirectHop>,
    pub final_url: Option<String>,
    pub status: Option<u16>,
    pub content_type: Option<String>,
    pub response_time_ms: Option<u64>,
    pub title: Option<Title>,
    pub error: Option<CheckError>,
    pub attempts: u32,
}

impl PageCheckResult {
    pub(crate) fn empty(requested_url: impl Into<String>, state: FinalState) -> Self {
        Self {
            requested_url: requested_url.into(),
            state,
            redirects: Vec::new(),
            final_url: None,
            status: None,
            content_type: None,
            response_time_ms: None,
            title: None,
            error: None,
            attempts: 0,
        }
    }

    pub(crate) fn failed(requested_url: impl Into<String>, error: CheckError) -> Self {
        let mut r = Self::empty(requested_url, FinalState::Failed);
        r.attempts = error.attempts;
        r.error = Some(error);
        r
    }
}
