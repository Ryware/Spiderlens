//! One HTTP client per run, separate from the crawler's (plan D4).

use std::time::Duration;

use reqwest::{Method, RequestBuilder, StatusCode};
use serde_json::Value;

/// Why a provider request failed. The class names follow spec 004 so the UI
/// and `integration_status` speak the same language as crawl errors.
#[derive(Debug, Clone, thiserror::Error)]
pub enum HttpError {
    /// 429 or a quota message. `quota` is true when the provider says a
    /// daily or project quota is used up, not just the per-minute rate.
    #[error("{provider} asked to slow down{}", retry_after.map(|d| format!(" (retry after {} s)", d.as_secs())).unwrap_or_default())]
    RateLimited {
        provider: String,
        retry_after: Option<Duration>,
        quota: bool,
        message: String,
    },
    #[error("{provider} refused the credentials: {message}")]
    Unauthorized { provider: String, message: String },
    #[error("{provider} denied access: {message}")]
    Forbidden { provider: String, message: String },
    #[error("{provider} has no data for this request")]
    NotFound { provider: String },
    #[error("{provider} answered {status}: {message}")]
    Server {
        provider: String,
        status: u16,
        message: String,
    },
    #[error("{provider} rejected the request ({status}): {message}")]
    Client {
        provider: String,
        status: u16,
        message: String,
    },
    #[error("could not reach {provider}: {message}")]
    Network { provider: String, message: String },
    #[error("{provider} sent an answer SpiderLens cannot read: {message}")]
    Decode { provider: String, message: String },
}

impl HttpError {
    /// Worth another attempt with backoff (spec 004 rules).
    pub fn is_transient(&self) -> bool {
        match self {
            Self::RateLimited { quota, .. } => !quota,
            Self::Server { .. } | Self::Network { .. } => true,
            _ => false,
        }
    }

    pub fn class(&self) -> &'static str {
        match self {
            Self::RateLimited { quota: true, .. } => "quota",
            Self::RateLimited { .. } => "rate_limited",
            Self::Unauthorized { .. } => "unauthorized",
            Self::Forbidden { .. } => "forbidden",
            Self::NotFound { .. } => "not_found",
            Self::Server { .. } => "server_error",
            Self::Client { .. } => "client_error",
            Self::Network { .. } => "network",
            Self::Decode { .. } => "decode",
        }
    }
}

#[derive(Clone)]
pub struct Http {
    client: reqwest::Client,
}

impl Http {
    pub fn new(read_timeout: Duration) -> Self {
        let client = reqwest::Client::builder()
            .user_agent(concat!(
                "SpiderLens/",
                env!("CARGO_PKG_VERSION"),
                " (integrations)"
            ))
            .connect_timeout(Duration::from_secs(10))
            .timeout(read_timeout)
            .build()
            .expect("HTTP client builds");
        Self { client }
    }

    pub fn request(&self, method: Method, url: &str) -> RequestBuilder {
        self.client.request(method, url)
    }

    /// Sends and returns the JSON body, mapping failures to [`HttpError`].
    pub async fn json(&self, provider: &str, req: RequestBuilder) -> Result<Value, HttpError> {
        let resp = req.send().await.map_err(|e| HttpError::Network {
            provider: provider.into(),
            message: e.to_string(),
        })?;
        let status = resp.status();
        let retry_after = resp
            .headers()
            .get(reqwest::header::RETRY_AFTER)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.trim().parse::<u64>().ok())
            .map(Duration::from_secs);
        let body = resp.bytes().await.map_err(|e| HttpError::Network {
            provider: provider.into(),
            message: e.to_string(),
        })?;
        if status.is_success() {
            if body.is_empty() {
                return Ok(Value::Null);
            }
            return serde_json::from_slice(&body).map_err(|e| HttpError::Decode {
                provider: provider.into(),
                message: e.to_string(),
            });
        }
        Err(classify(provider, status, retry_after, &body))
    }
}

/// Google and most providers answer `{"error": {"message": …, "status": …}}`.
fn classify(provider: &str, status: StatusCode, retry_after: Option<Duration>, body: &[u8]) -> HttpError {
    let parsed: Option<Value> = serde_json::from_slice(body).ok();
    let message = parsed
        .as_ref()
        .and_then(|v| {
            v.pointer("/error/message")
                .or_else(|| v.get("error_description"))
                .or_else(|| v.get("message"))
                .or_else(|| v.get("error"))
                .and_then(|m| m.as_str())
                .map(str::to_string)
        })
        .unwrap_or_else(|| String::from_utf8_lossy(body).chars().take(200).collect());
    let provider = provider.to_string();
    let lower = message.to_ascii_lowercase();
    let quota_words = lower.contains("quota") || lower.contains("per day") || lower.contains("daily");
    match status.as_u16() {
        429 => HttpError::RateLimited {
            provider,
            retry_after,
            quota: quota_words && !lower.contains("per minute"),
            message,
        },
        // Google reports some quota errors as 403 RESOURCE_EXHAUSTED.
        403 if quota_words => HttpError::RateLimited {
            provider,
            retry_after,
            quota: true,
            message,
        },
        401 => HttpError::Unauthorized { provider, message },
        403 => HttpError::Forbidden { provider, message },
        404 => HttpError::NotFound { provider },
        s if s >= 500 => HttpError::Server {
            provider,
            status: s,
            message,
        },
        s => HttpError::Client {
            provider,
            status: s,
            message,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c(status: u16, body: &str) -> HttpError {
        classify(
            "Test",
            StatusCode::from_u16(status).unwrap(),
            None,
            body.as_bytes(),
        )
    }

    #[test]
    fn classifies_google_style_errors() {
        assert!(matches!(
            c(
                429,
                r#"{"error":{"message":"Quota exceeded for quota metric 'Queries' and limit 'Queries per day'"}}"#
            ),
            HttpError::RateLimited { quota: true, .. }
        ));
        assert!(matches!(
            c(429, r#"{"error":{"message":"Quota exceeded … per minute"}}"#),
            HttpError::RateLimited { quota: false, .. }
        ));
        assert!(matches!(
            c(
                403,
                r#"{"error":{"message":"User does not have sufficient permission"}}"#
            ),
            HttpError::Forbidden { .. }
        ));
        assert!(matches!(
            c(401, r#"{"error":"invalid_grant"}"#),
            HttpError::Unauthorized { .. }
        ));
        assert!(c(503, "busy").is_transient());
        assert!(!c(400, "{}").is_transient());
    }
}
