use serde::Serialize;

/// The named set of failure classes (FR-017). The set is closed: adding a
/// class forces every `match` on it to be updated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorClass {
    Dns,
    DnsTemporary,
    Connection,
    Timeout,
    TlsInvalid,
    RedirectLoop,
    ContentProblem,
    MalformedUrl,
}

impl ErrorClass {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Dns => "dns",
            Self::DnsTemporary => "dns_temporary",
            Self::Connection => "connection",
            Self::Timeout => "timeout",
            Self::TlsInvalid => "tls_invalid",
            Self::RedirectLoop => "redirect_loop",
            Self::ContentProblem => "content_problem",
            Self::MalformedUrl => "malformed_url",
        }
    }
}

impl std::fmt::Display for ErrorClass {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A recorded failure. It is data in the result, not a panic: failures are
/// never silent (principle V).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, thiserror::Error)]
#[error("{class}: {message}")]
pub struct CheckError {
    pub class: ErrorClass,
    pub message: String,
    pub attempts: u32,
}

impl CheckError {
    pub fn new(class: ErrorClass, message: impl Into<String>) -> Self {
        Self {
            class,
            message: message.into(),
            attempts: 1,
        }
    }
}
