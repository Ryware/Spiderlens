//! Google sign-in for a desktop app (plan D3): the system browser, a
//! one-shot loopback listener on 127.0.0.1 and PKCE (S256), as Google
//! recommends for installed apps. No SpiderLens server is involved.
//!
//! Google does not offer incremental authorization to installed apps, so
//! adding a feature signs in again with the union of the scopes already
//! granted and the new ones.

use std::sync::Mutex;
use std::time::Duration;

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use reqwest::Method;
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::time::Instant;
use url::Url;

use crate::http::{Http, HttpError};
use crate::{Error, Result};

/// Read-only scopes per feature (FR-015).
pub fn scope_for(feature: &str) -> Option<&'static str> {
    match feature {
        "search_console" => Some("https://www.googleapis.com/auth/webmasters.readonly"),
        "analytics" => Some("https://www.googleapis.com/auth/analytics.readonly"),
        "sheets" => Some("https://www.googleapis.com/auth/drive.file"),
        _ => None,
    }
}

/// Always requested, to label the connection with the account's email.
const IDENTITY_SCOPES: &str = "openid email";

#[derive(Debug, Clone)]
pub struct Endpoints {
    pub auth: String,
    pub token: String,
    pub revoke: String,
}

impl Default for Endpoints {
    fn default() -> Self {
        Self {
            auth: "https://accounts.google.com/o/oauth2/v2/auth".into(),
            token: "https://oauth2.googleapis.com/token".into(),
            revoke: "https://oauth2.googleapis.com/revoke".into(),
        }
    }
}

/// A Google OAuth client of type Desktop. Google treats a desktop client's
/// secret as not confidential; it is still kept out of files and logs.
#[derive(Clone)]
pub struct Client {
    pub id: String,
    pub secret: Option<SecretString>,
}

impl std::fmt::Debug for Client {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Client").field("id", &self.id).finish_non_exhaustive()
    }
}

impl Client {
    /// The client compiled into this build, if the owner provided one
    /// (`SPIDERLENS_GOOGLE_CLIENT_ID` and `_SECRET` at build time, FR-016).
    pub fn built_in() -> Option<Self> {
        let id = option_env!("SPIDERLENS_GOOGLE_CLIENT_ID").filter(|s| !s.is_empty())?;
        Some(Self {
            id: id.to_string(),
            secret: option_env!("SPIDERLENS_GOOGLE_CLIENT_SECRET")
                .filter(|s| !s.is_empty())
                .map(SecretString::from),
        })
    }
}

/// What the keychain holds for a Google connection.
#[derive(Serialize, Deserialize)]
pub struct StoredGoogle {
    pub client_id: String,
    #[serde(default)]
    pub client_secret: Option<String>,
    pub refresh_token: String,
}

impl StoredGoogle {
    pub fn to_secret(&self) -> SecretString {
        SecretString::from(serde_json::to_string(self).expect("serializes"))
    }

    pub fn from_secret(s: &SecretString) -> Result<Self> {
        serde_json::from_str(s.expose_secret())
            .map_err(|_| Error::Auth("the stored Google sign-in is damaged; connect again".into()))
    }

    pub fn client(&self) -> Client {
        Client {
            id: self.client_id.clone(),
            secret: self.client_secret.clone().map(SecretString::from),
        }
    }
}

pub struct Authorized {
    pub refresh_token: SecretString,
    pub access_token: SecretString,
    pub expires_in: Duration,
    pub email: String,
    pub scopes: Vec<String>,
}

fn random_url_safe(bytes: usize) -> String {
    let mut b = vec![0u8; bytes];
    getrandom::fill(&mut b).expect("system randomness");
    URL_SAFE_NO_PAD.encode(b)
}

pub fn pkce_challenge(verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

/// Runs the sign-in. `open` receives the authorization URL and opens it in
/// the system browser (the CLI prints it when it cannot). Dropping the
/// returned future cancels the sign-in and closes the listener.
pub async fn authorize(
    http: &Http,
    client: &Client,
    endpoints: &Endpoints,
    scopes: &[&str],
    timeout: Duration,
    open: impl FnOnce(&str),
) -> Result<Authorized> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let redirect = format!("http://127.0.0.1:{}", listener.local_addr()?.port());
    let verifier = random_url_safe(48);
    let state = random_url_safe(16);
    let mut scope = String::from(IDENTITY_SCOPES);
    for s in scopes {
        scope.push(' ');
        scope.push_str(s);
    }
    let mut auth = Url::parse(&endpoints.auth).map_err(|e| Error::Invalid(e.to_string()))?;
    auth.query_pairs_mut()
        .append_pair("client_id", &client.id)
        .append_pair("redirect_uri", &redirect)
        .append_pair("response_type", "code")
        .append_pair("scope", &scope)
        .append_pair("code_challenge", &pkce_challenge(&verifier))
        .append_pair("code_challenge_method", "S256")
        .append_pair("state", &state)
        .append_pair("access_type", "offline")
        .append_pair("prompt", "consent");
    open(auth.as_str());

    let code = tokio::time::timeout(timeout, wait_for_code(&listener, &state))
        .await
        .map_err(|_| Error::Auth("sign-in timed out; try again".into()))??;

    let mut form = vec![
        ("grant_type", "authorization_code".to_string()),
        ("code", code),
        ("redirect_uri", redirect),
        ("client_id", client.id.clone()),
        ("code_verifier", verifier),
    ];
    if let Some(s) = &client.secret {
        form.push(("client_secret", s.expose_secret().to_string()));
    }
    let v = http
        .json("Google sign-in", http.request(Method::POST, &endpoints.token).form(&form))
        .await?;
    let refresh = v["refresh_token"]
        .as_str()
        .ok_or_else(|| Error::Auth("Google did not return a refresh token".into()))?;
    let access = v["access_token"]
        .as_str()
        .ok_or_else(|| Error::Auth("Google did not return an access token".into()))?;
    let email = v["id_token"]
        .as_str()
        .and_then(email_from_id_token)
        .unwrap_or_else(|| "Google account".into());
    let granted = v["scope"]
        .as_str()
        .unwrap_or_default()
        .split_whitespace()
        .map(str::to_string)
        .collect();
    Ok(Authorized {
        refresh_token: SecretString::from(refresh.to_string()),
        access_token: SecretString::from(access.to_string()),
        expires_in: Duration::from_secs(v["expires_in"].as_u64().unwrap_or(3600)),
        email,
        scopes: granted,
    })
}

/// The email claim from an ID token. The token came straight from Google's
/// token endpoint over TLS, so its signature is not checked here; it is only
/// used as a label.
fn email_from_id_token(t: &str) -> Option<String> {
    let payload = t.split('.').nth(1)?;
    let bytes = URL_SAFE_NO_PAD.decode(payload.trim_end_matches('=')).ok()?;
    let v: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
    v["email"].as_str().map(str::to_string)
}

async fn wait_for_code(listener: &TcpListener, state: &str) -> Result<String> {
    loop {
        let (mut sock, _) = listener.accept().await?;
        let mut buf = Vec::with_capacity(2048);
        let mut chunk = [0u8; 1024];
        while !buf.windows(4).any(|w| w == b"\r\n\r\n") && buf.len() < 16 * 1024 {
            let n = sock.read(&mut chunk).await?;
            if n == 0 {
                break;
            }
            buf.extend_from_slice(&chunk[..n]);
        }
        let head = String::from_utf8_lossy(&buf);
        let target = head.split_whitespace().nth(1).unwrap_or("/").to_string();
        let url = Url::parse(&format!("http://127.0.0.1{target}")).map_err(|e| Error::Auth(e.to_string()))?;
        let get = |k: &str| url.query_pairs().find(|(n, _)| n == k).map(|(_, v)| v.into_owned());
        let (status, body, result) = match (get("code"), get("error"), get("state")) {
            (Some(code), _, Some(s)) if s == state => (
                "200 OK",
                "SpiderLens is connected. You can close this tab.",
                Some(Ok(code)),
            ),
            (Some(_), _, _) => (
                "400 Bad Request",
                "This sign-in link is not the one SpiderLens started. Close this tab and try again.",
                Some(Err(Error::Auth("the sign-in answer did not match; try again".into()))),
            ),
            (None, Some(e), _) => (
                "200 OK",
                "Sign-in was cancelled. You can close this tab.",
                Some(Err(Error::Auth(format!("Google sign-in was not completed ({e})")))),
            ),
            _ => ("404 Not Found", "", None),
        };
        let page = format!(
            "<!doctype html><meta charset=utf-8><title>SpiderLens</title><body style=\"font-family:system-ui;padding:3rem\"><p>{body}</p>"
        );
        let resp = format!(
            "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{page}",
            page.len()
        );
        let _ = sock.write_all(resp.as_bytes()).await;
        let _ = sock.shutdown().await;
        if let Some(r) = result {
            return r;
        }
    }
}

/// Exchanges a refresh token for an access token.
pub async fn refresh(
    http: &Http,
    client: &Client,
    endpoints: &Endpoints,
    refresh_token: &SecretString,
) -> std::result::Result<(SecretString, Duration), HttpError> {
    let mut form = vec![
        ("grant_type", "refresh_token".to_string()),
        ("refresh_token", refresh_token.expose_secret().to_string()),
        ("client_id", client.id.clone()),
    ];
    if let Some(s) = &client.secret {
        form.push(("client_secret", s.expose_secret().to_string()));
    }
    let v = http
        .json("Google sign-in", http.request(Method::POST, &endpoints.token).form(&form))
        .await?;
    let token = v["access_token"].as_str().ok_or_else(|| HttpError::Decode {
        provider: "Google sign-in".into(),
        message: "no access token".into(),
    })?;
    Ok((
        SecretString::from(token.to_string()),
        Duration::from_secs(v["expires_in"].as_u64().unwrap_or(3600)),
    ))
}

/// Revokes a token at Google (FR-017).
pub async fn revoke(http: &Http, endpoints: &Endpoints, token: &SecretString) -> std::result::Result<(), HttpError> {
    http.json(
        "Google sign-in",
        http.request(Method::POST, &endpoints.revoke)
            .form(&[("token", token.expose_secret())]),
    )
    .await
    .map(|_| ())
}

/// A signed-in Google account for one run: hands out access tokens and
/// refreshes them when they expire or a provider answers 401.
pub struct Session {
    http: Http,
    endpoints: Endpoints,
    stored: StoredGoogle,
    access: Mutex<Option<(SecretString, Instant)>>,
}

impl Session {
    pub fn new(http: Http, endpoints: Endpoints, stored: StoredGoogle) -> Self {
        Self {
            http,
            endpoints,
            stored,
            access: Mutex::new(None),
        }
    }

    pub async fn bearer(&self) -> std::result::Result<String, HttpError> {
        if let Some((t, exp)) = self.access.lock().expect("lock").as_ref()
            && Instant::now() + Duration::from_secs(60) < *exp
        {
            return Ok(t.expose_secret().to_string());
        }
        let refresh_token = SecretString::from(self.stored.refresh_token.clone());
        let (token, ttl) = refresh(&self.http, &self.stored.client(), &self.endpoints, &refresh_token).await?;
        let s = token.expose_secret().to_string();
        *self.access.lock().expect("lock") = Some((token, Instant::now() + ttl));
        Ok(s)
    }

    /// Forgets the current access token after a 401.
    pub fn invalidate(&self) {
        *self.access.lock().expect("lock") = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pkce_matches_rfc7636_example() {
        assert_eq!(
            pkce_challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
    }

    #[test]
    fn email_is_read_from_the_id_token_payload() {
        let payload = URL_SAFE_NO_PAD.encode(br#"{"email":"ana@example.com"}"#);
        assert_eq!(
            email_from_id_token(&format!("x.{payload}.y")).as_deref(),
            Some("ana@example.com")
        );
    }

    #[test]
    fn stored_google_never_shows_in_debug() {
        let c = Client {
            id: "id".into(),
            secret: Some(SecretString::from("shh-secret".to_string())),
        };
        assert!(!format!("{c:?}").contains("shh-secret"));
    }
}
