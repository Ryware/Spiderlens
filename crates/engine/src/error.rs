use serde::Serialize;

/// The named set of failure classes (spec 001 FR-017, spec 004 FR-012). The
/// set is closed: adding a class forces every `match` on it to be updated.
///
/// These are failures with no usable response. HTTP answers (404, 503...)
/// are classed from their status code by [`FinalClass`] instead.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorClass {
    Dns,
    DnsTemporary,
    Connection,
    Timeout,
    TlsInvalid,
    /// Loop, chain over the limit or invalid target (see [`ErrorReason`]).
    /// Files from before spec 004 store `redirect_loop`.
    RedirectProblem,
    ContentProblem,
    MalformedUrl,
    /// The host was given up after repeated pauses (spec 004 FR-026).
    HostUnavailable,
}

impl ErrorClass {
    pub const ALL: &'static [Self] = &[
        Self::Dns,
        Self::DnsTemporary,
        Self::Connection,
        Self::Timeout,
        Self::TlsInvalid,
        Self::RedirectProblem,
        Self::ContentProblem,
        Self::MalformedUrl,
        Self::HostUnavailable,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Dns => "dns",
            Self::DnsTemporary => "dns_temporary",
            Self::Connection => "connection",
            Self::Timeout => "timeout",
            Self::TlsInvalid => "tls_invalid",
            Self::RedirectProblem => "redirect_problem",
            Self::ContentProblem => "content_problem",
            Self::MalformedUrl => "malformed_url",
            Self::HostUnavailable => "host_unavailable",
        }
    }

    /// Also reads `redirect_loop`, written by files from before spec 004.
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "redirect_loop" => Some(Self::RedirectProblem),
            _ => Self::ALL.iter().copied().find(|c| c.as_str() == s),
        }
    }

    /// Whether trying again later can help (spec 004 FR-012). Content
    /// problems depend on the sub-reason: only a truncated body is transient.
    pub fn is_transient(self, reason: Option<ErrorReason>) -> bool {
        match self {
            Self::DnsTemporary | Self::Connection | Self::Timeout | Self::HostUnavailable => true,
            Self::ContentProblem => reason == Some(ErrorReason::Truncated),
            Self::Dns | Self::TlsInvalid | Self::RedirectProblem | Self::MalformedUrl => false,
        }
    }
}

impl std::fmt::Display for ErrorClass {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The sub-reason of an error class, where the spec names one (FR-013).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorReason {
    /// Timeout while connecting.
    Connect,
    /// Timeout after connecting: waiting for the answer or reading it.
    Read,
    Refused,
    Reset,
    Expired,
    Hostname,
    Untrusted,
    Loop,
    OverLimit,
    InvalidTarget,
    Truncated,
    OverSize,
    Decompression,
}

impl ErrorReason {
    pub const ALL: &'static [Self] = &[
        Self::Connect,
        Self::Read,
        Self::Refused,
        Self::Reset,
        Self::Expired,
        Self::Hostname,
        Self::Untrusted,
        Self::Loop,
        Self::OverLimit,
        Self::InvalidTarget,
        Self::Truncated,
        Self::OverSize,
        Self::Decompression,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Connect => "connect",
            Self::Read => "read",
            Self::Refused => "refused",
            Self::Reset => "reset",
            Self::Expired => "expired",
            Self::Hostname => "hostname",
            Self::Untrusted => "untrusted",
            Self::Loop => "loop",
            Self::OverLimit => "over_limit",
            Self::InvalidTarget => "invalid_target",
            Self::Truncated => "truncated",
            Self::OverSize => "over_size",
            Self::Decompression => "decompression",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|r| r.as_str() == s)
    }
}

/// A recorded failure. It is data in the result, not a panic: failures are
/// never silent (principle V).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, thiserror::Error)]
#[error("{class}: {message}")]
pub struct CheckError {
    pub class: ErrorClass,
    /// The sub-reason, where one is known (spec 004 FR-013).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<ErrorReason>,
    pub message: String,
    pub attempts: u32,
}

impl CheckError {
    pub fn new(class: ErrorClass, message: impl Into<String>) -> Self {
        Self {
            class,
            reason: None,
            message: message.into(),
            attempts: 1,
        }
    }

    pub fn with_reason(mut self, reason: ErrorReason) -> Self {
        self.reason = Some(reason);
        self
    }

    pub fn is_transient(&self) -> bool {
        self.class.is_transient(self.reason)
    }
}

/// The one class a URL's outcome falls in (spec 004 FR-012): a failure with
/// no usable response, or an HTTP answer classed by its status code. Not
/// stored for HTTP answers; derived from `status` wherever it is needed so
/// the two can never disagree (plan D2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FinalClass {
    /// 1xx, 2xx, 3xx: not an error.
    Ok,
    Http429,
    Http4xx,
    Http5xx,
    HttpOther,
    Failed(ErrorClass),
}

impl FinalClass {
    /// Classes an answer with this status code.
    pub fn of_status(status: u16) -> Self {
        match status {
            429 => Self::Http429,
            100..=399 => Self::Ok,
            400..=499 => Self::Http4xx,
            500..=599 => Self::Http5xx,
            _ => Self::HttpOther,
        }
    }

    /// Classes a stored outcome: the error class wins over the status.
    pub fn of(status: Option<u16>, error: Option<ErrorClass>) -> Self {
        match (error, status) {
            (Some(c), _) => Self::Failed(c),
            (None, Some(s)) => Self::of_status(s),
            (None, None) => Self::Ok,
        }
    }

    /// Whether a later attempt can help. 501 and 505 are permanent server
    /// errors (FR-012).
    pub fn is_transient(self, status: Option<u16>, reason: Option<ErrorReason>) -> bool {
        match self {
            Self::Ok | Self::Http4xx | Self::HttpOther => false,
            Self::Http429 => true,
            Self::Http5xx => !matches!(status, Some(501 | 505)),
            Self::Failed(c) => c.is_transient(reason),
        }
    }

    pub fn is_error(self) -> bool {
        self != Self::Ok
    }

    /// The stable id used in summaries and filters.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::Http429 => "http_429",
            Self::Http4xx => "http_4xx",
            Self::Http5xx => "http_5xx",
            Self::HttpOther => "http_other",
            Self::Failed(c) => c.as_str(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classification_table_matches_the_spec() {
        // (status, class, reason, final class id, transient), FR-012.
        let rows: &[(Option<u16>, Option<ErrorClass>, Option<ErrorReason>, &str, bool)] = &[
            (Some(200), None, None, "ok", false),
            (Some(301), None, None, "ok", false),
            (Some(404), None, None, "http_4xx", false),
            (Some(410), None, None, "http_4xx", false),
            (Some(401), None, None, "http_4xx", false),
            (Some(403), None, None, "http_4xx", false),
            (Some(429), None, None, "http_429", true),
            (Some(500), None, None, "http_5xx", true),
            (Some(503), None, None, "http_5xx", true),
            (Some(501), None, None, "http_5xx", false),
            (Some(505), None, None, "http_5xx", false),
            (Some(999), None, None, "http_other", false),
            (Some(0), None, None, "http_other", false),
            (None, Some(ErrorClass::Dns), None, "dns", false),
            (None, Some(ErrorClass::DnsTemporary), None, "dns_temporary", true),
            (None, Some(ErrorClass::Connection), Some(ErrorReason::Reset), "connection", true),
            (None, Some(ErrorClass::Timeout), Some(ErrorReason::Connect), "timeout", true),
            (None, Some(ErrorClass::Timeout), Some(ErrorReason::Read), "timeout", true),
            (None, Some(ErrorClass::TlsInvalid), Some(ErrorReason::Expired), "tls_invalid", false),
            (None, Some(ErrorClass::RedirectProblem), Some(ErrorReason::Loop), "redirect_problem", false),
            (None, Some(ErrorClass::ContentProblem), Some(ErrorReason::Truncated), "content_problem", true),
            (None, Some(ErrorClass::ContentProblem), Some(ErrorReason::OverSize), "content_problem", false),
            (None, Some(ErrorClass::ContentProblem), Some(ErrorReason::Decompression), "content_problem", false),
            (None, Some(ErrorClass::MalformedUrl), None, "malformed_url", false),
            (None, Some(ErrorClass::HostUnavailable), None, "host_unavailable", true),
            // A body problem on a 200 answer: the class wins.
            (Some(200), Some(ErrorClass::ContentProblem), Some(ErrorReason::Truncated), "content_problem", true),
        ];
        for &(status, class, reason, id, transient) in rows {
            let f = FinalClass::of(status, class);
            assert_eq!(f.as_str(), id, "{status:?} {class:?}");
            assert_eq!(f.is_transient(status, reason), transient, "{status:?} {class:?} {reason:?}");
        }
    }

    #[test]
    fn strings_round_trip_and_old_name_reads() {
        for c in ErrorClass::ALL {
            assert_eq!(ErrorClass::parse(c.as_str()), Some(*c));
            assert_eq!(serde_json::to_value(c).unwrap(), c.as_str());
        }
        for r in ErrorReason::ALL {
            assert_eq!(ErrorReason::parse(r.as_str()), Some(*r));
            assert_eq!(serde_json::to_value(r).unwrap(), r.as_str());
        }
        assert_eq!(ErrorClass::parse("redirect_loop"), Some(ErrorClass::RedirectProblem));
    }
}
