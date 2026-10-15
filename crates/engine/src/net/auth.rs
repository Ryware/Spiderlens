//! HTTP sign-ins: Basic and Digest, and the choice of NTLM (spec 025 plan
//! D3, research R3).
//!
//! A sign-in belongs to one origin (scheme, host and port) and is never
//! sent anywhere else, redirect targets included. Basic is sent
//! pre-emptively once the origin asked for it; Digest keeps the server's
//! nonce and a counter so it answers pre-emptively too, without doubling
//! the requests. NTLM authenticates a connection, so its requests go
//! through [`super::ntlm::NtlmPool`].

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex, PoisonError, RwLock};

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as B64;
use reqwest::header::{HeaderMap, HeaderValue, WWW_AUTHENTICATE};
use spiderlens_config::{AuthMethod, Secret, SignIn};
use url::Url;

/// One challenge of a `WWW-Authenticate` header.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct Challenge {
    /// Lowercase: `basic`, `digest`, `ntlm`, `negotiate`, ...
    pub scheme: String,
    /// Parameter names lowercase, values unquoted.
    pub params: HashMap<String, String>,
    /// The token68 form (`NTLM TlRMTVNTUAAC...`).
    pub token: Option<String>,
}

/// Splits on commas outside quoted strings.
fn split_items(value: &str) -> Vec<&str> {
    let mut items = Vec::new();
    let (mut start, mut quoted, mut escaped) = (0, false, false);
    for (i, b) in value.bytes().enumerate() {
        match b {
            _ if escaped => escaped = false,
            b'\\' if quoted => escaped = true,
            b'"' => quoted = !quoted,
            b',' if !quoted => {
                items.push(&value[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    items.push(&value[start..]);
    items
}

fn unquote(v: &str) -> String {
    let v = v.trim();
    match v.strip_prefix('"').and_then(|v| v.strip_suffix('"')) {
        Some(inner) => {
            let mut out = String::with_capacity(inner.len());
            let mut chars = inner.chars();
            while let Some(c) = chars.next() {
                if c == '\\' {
                    if let Some(n) = chars.next() {
                        out.push(n);
                    }
                } else {
                    out.push(c);
                }
            }
            out
        }
        None => v.to_string(),
    }
}

/// `name=value` with a non-empty value that is not more `=` (base64
/// padding makes a token68 look like a parameter).
fn as_param(item: &str) -> Option<(String, String)> {
    let (name, value) = item.split_once('=')?;
    let name = name.trim();
    let value = value.trim();
    let token = |s: &str| {
        !s.is_empty()
            && s.bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
    };
    (token(name) && !value.is_empty() && !value.starts_with('='))
        .then(|| (name.to_ascii_lowercase(), unquote(value)))
}

/// Parses every challenge in the `WWW-Authenticate` values (a server may
/// send several schemes in one header or in several).
pub(crate) fn parse_challenges<'a>(values: impl IntoIterator<Item = &'a str>) -> Vec<Challenge> {
    let mut out: Vec<Challenge> = Vec::new();
    for value in values {
        for item in split_items(value) {
            let item = item.trim();
            if item.is_empty() {
                continue;
            }
            let first_space = item.find(char::is_whitespace);
            let first_eq = item.find('=');
            let is_param = match (first_eq, first_space) {
                (Some(eq), Some(sp)) => eq < sp,
                (Some(_), None) => true,
                _ => false,
            };
            if is_param && let Some(current) = out.last_mut() {
                if let Some((k, v)) = as_param(item) {
                    current.params.insert(k, v);
                }
                continue;
            }
            let (scheme, rest) = match first_space {
                Some(sp) => (&item[..sp], item[sp..].trim()),
                None => (item, ""),
            };
            let mut challenge = Challenge {
                scheme: scheme.to_ascii_lowercase(),
                ..Challenge::default()
            };
            if !rest.is_empty() {
                match as_param(rest) {
                    Some((k, v)) => {
                        challenge.params.insert(k, v);
                    }
                    None => challenge.token = Some(rest.to_string()),
                }
            }
            out.push(challenge);
        }
    }
    out
}

pub(crate) fn challenges_of(headers: &HeaderMap) -> Vec<Challenge> {
    parse_challenges(
        headers
            .get_all(WWW_AUTHENTICATE)
            .iter()
            .filter_map(|v| v.to_str().ok()),
    )
}

pub(crate) fn basic_header(user: &str, password: &str) -> String {
    format!("Basic {}", B64.encode(format!("{user}:{password}")))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DigestAlgorithm {
    Md5,
    Md5Sess,
    Sha256,
    Sha256Sess,
}

impl DigestAlgorithm {
    fn parse(s: Option<&str>) -> Option<Self> {
        match s.map(str::to_ascii_uppercase).as_deref() {
            None | Some("MD5") => Some(Self::Md5),
            Some("MD5-SESS") => Some(Self::Md5Sess),
            Some("SHA-256") => Some(Self::Sha256),
            Some("SHA-256-SESS") => Some(Self::Sha256Sess),
            _ => None,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Md5 => "MD5",
            Self::Md5Sess => "MD5-sess",
            Self::Sha256 => "SHA-256",
            Self::Sha256Sess => "SHA-256-sess",
        }
    }

    fn hash(self, data: &str) -> String {
        use md5::Digest as _;
        match self {
            Self::Md5 | Self::Md5Sess => hex(&md5::Md5::digest(data.as_bytes())),
            Self::Sha256 | Self::Sha256Sess => hex(&sha2::Sha256::digest(data.as_bytes())),
        }
    }

    fn sess(self) -> bool {
        matches!(self, Self::Md5Sess | Self::Sha256Sess)
    }
}

/// Algorithms we answer, best first, for servers that offer several.
fn digest_rank(c: &Challenge) -> u8 {
    match DigestAlgorithm::parse(c.params.get("algorithm").map(String::as_str)) {
        Some(DigestAlgorithm::Sha256 | DigestAlgorithm::Sha256Sess) => 2,
        Some(_) => 1,
        None => 0,
    }
}

pub(crate) fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        let _ = write!(s, "{b:02x}");
    }
    s
}

fn cnonce() -> String {
    let mut b = [0u8; 12];
    if getrandom::fill(&mut b).is_err() {
        b.iter_mut().for_each(|x| *x = fastrand::u8(..));
    }
    hex(&b)
}

/// What a Digest origin asked for, kept to answer pre-emptively (R3).
#[derive(Debug)]
pub(crate) struct DigestState {
    realm: String,
    nonce: String,
    opaque: Option<String>,
    algorithm: DigestAlgorithm,
    /// `qop=auth` offered; without it the RFC 2069 form is used.
    qop_auth: bool,
    nc: AtomicU32,
}

impl DigestState {
    fn from_challenge(c: &Challenge) -> Option<Self> {
        let algorithm = DigestAlgorithm::parse(c.params.get("algorithm").map(String::as_str))?;
        let qop_auth = c
            .params
            .get("qop")
            .is_some_and(|q| q.split(',').any(|v| v.trim().eq_ignore_ascii_case("auth")));
        // `qop=auth-int` alone would need the body hashed: not supported.
        if c.params.contains_key("qop") && !qop_auth {
            return None;
        }
        Some(Self {
            realm: c.params.get("realm").cloned().unwrap_or_default(),
            nonce: c.params.get("nonce")?.clone(),
            opaque: c.params.get("opaque").cloned(),
            algorithm,
            qop_auth,
            nc: AtomicU32::new(0),
        })
    }

    /// The `Authorization` value for a GET of `url` (RFC 7616 section 3.4).
    fn answer(&self, user: &str, password: &str, url: &Url) -> String {
        self.answer_with(user, password, url, &cnonce())
    }

    fn answer_with(&self, user: &str, password: &str, url: &Url, cnonce: &str) -> String {
        let uri = match url.query() {
            Some(q) => format!("{}?{q}", url.path()),
            None => url.path().to_string(),
        };
        let h = |s: &str| self.algorithm.hash(s);
        let mut ha1 = h(&format!("{user}:{}:{password}", self.realm));
        if self.algorithm.sess() {
            ha1 = h(&format!("{ha1}:{}:{cnonce}", self.nonce));
        }
        let ha2 = h(&format!("GET:{uri}"));
        let nc = format!("{:08x}", self.nc.fetch_add(1, Ordering::Relaxed) + 1);
        let response = if self.qop_auth {
            h(&format!("{ha1}:{}:{nc}:{cnonce}:auth:{ha2}", self.nonce))
        } else {
            h(&format!("{ha1}:{}:{ha2}", self.nonce))
        };
        let q = |s: &str| s.replace('\\', "\\\\").replace('"', "\\\"");
        let mut header = format!(
            "Digest username=\"{}\", realm=\"{}\", nonce=\"{}\", uri=\"{}\", algorithm={}, response=\"{response}\"",
            q(user),
            q(&self.realm),
            q(&self.nonce),
            q(&uri),
            self.algorithm.name(),
        );
        if let Some(o) = &self.opaque {
            header.push_str(&format!(", opaque=\"{}\"", q(o)));
        }
        if self.qop_auth {
            header.push_str(&format!(", qop=auth, nc={nc}, cnonce=\"{cnonce}\""));
        }
        header
    }
}

/// The scheme an origin settled on after its first challenge.
#[derive(Debug)]
enum Active {
    Basic,
    Digest(DigestState),
    Ntlm,
}

struct Entry {
    user: String,
    password: Secret,
    method: AuthMethod,
    state: Mutex<Option<Active>>,
}

/// What a 401 means for the request that got it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthOutcome {
    /// Send the request again, once, with credentials.
    RetryWithCredentials,
    /// The origin is NTLM: send the request through the NTLM connections.
    UseNtlm,
    /// Credentials were sent and refused (FR-005).
    Rejected { method: &'static str },
    /// No sign-in for this origin, or none that answers its challenge
    /// (FR-004). `supported` is false for schemes such as Negotiate.
    Required { schemes: Vec<String>, supported: bool },
}

/// The sign-ins of one crawl. The list can grow while the crawl runs
/// (FR-004), so it sits behind a lock; `any` keeps the default path (no
/// sign-in) to one atomic load per request.
#[derive(Default)]
pub(crate) struct Auth {
    any: AtomicBool,
    entries: RwLock<HashMap<String, Arc<Entry>>>,
}

pub(crate) fn origin_of(url: &Url) -> String {
    url.origin().ascii_serialization()
}

impl Auth {
    pub fn new(sign_ins: &[SignIn]) -> Self {
        let auth = Self::default();
        for s in sign_ins {
            auth.add(s);
        }
        auth
    }

    pub fn add(&self, s: &SignIn) {
        let Some(origin) = s.origin_key() else { return };
        let preset = (s.method == AuthMethod::Basic).then_some(Active::Basic);
        let entry = Arc::new(Entry {
            user: s.user.clone(),
            password: s.password.clone(),
            method: s.method,
            state: Mutex::new(preset),
        });
        self.entries
            .write()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(origin, entry);
        self.any.store(true, Ordering::Release);
    }

    fn entry(&self, url: &Url) -> Option<Arc<Entry>> {
        if !self.any.load(Ordering::Acquire) {
            return None;
        }
        self.entries
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .get(&origin_of(url))
            .cloned()
    }

    /// The `Authorization` value to send pre-emptively, if the origin has
    /// settled on Basic or Digest.
    pub fn authorize(&self, url: &Url) -> Option<HeaderValue> {
        let entry = self.entry(url)?;
        let password = entry.password.expose()?;
        let state = entry.state.lock().unwrap_or_else(PoisonError::into_inner);
        let value = match state.as_ref()? {
            Active::Basic => basic_header(&entry.user, password),
            Active::Digest(d) => d.answer(&entry.user, password, url),
            Active::Ntlm => return None,
        };
        let mut v = HeaderValue::from_str(&value).ok()?;
        v.set_sensitive(true);
        Some(v)
    }

    /// The origin settled on NTLM.
    pub fn is_ntlm(&self, url: &Url) -> bool {
        self.entry(url).is_some_and(|e| {
            matches!(
                *e.state.lock().unwrap_or_else(PoisonError::into_inner),
                Some(Active::Ntlm)
            )
        })
    }

    /// The user and password for NTLM.
    pub fn ntlm_credentials(&self, url: &Url) -> Option<(String, String)> {
        let e = self.entry(url)?;
        Some((e.user.clone(), e.password.expose()?.to_string()))
    }

    /// Reads a 401's challenges. `sent` says whether the request carried
    /// credentials.
    pub fn on_unauthorized(&self, url: &Url, headers: &HeaderMap, sent: bool) -> AuthOutcome {
        let challenges = challenges_of(headers);
        let schemes: Vec<String> = challenges.iter().map(|c| c.scheme.clone()).collect();
        let supported = schemes
            .iter()
            .any(|s| matches!(s.as_str(), "basic" | "digest" | "ntlm"));
        let Some(entry) = self.entry(url) else {
            return AuthOutcome::Required { schemes, supported };
        };
        if entry.password.expose().is_none() {
            return AuthOutcome::Required { schemes, supported };
        }
        let mut state = entry.state.lock().unwrap_or_else(PoisonError::into_inner);
        let wants = |m: AuthMethod| entry.method == AuthMethod::Any || entry.method == m;
        let find = |scheme: &'static str| challenges.iter().filter(move |c| c.scheme == scheme);

        // A stale Digest nonce is not a rejection (RFC 7616 section 3.3).
        if matches!(*state, Some(Active::Digest(_)))
            && let Some(c) = find("digest").find(|c| {
                c.params
                    .get("stale")
                    .is_some_and(|v| v.eq_ignore_ascii_case("true"))
            })
            && let Some(d) = DigestState::from_challenge(c)
        {
            *state = Some(Active::Digest(d));
            return AuthOutcome::RetryWithCredentials;
        }
        if sent {
            let method = match state.as_ref() {
                Some(Active::Basic) => "basic",
                Some(Active::Digest(_)) => "digest",
                Some(Active::Ntlm) => "ntlm",
                None => entry.method.as_str(),
            };
            return AuthOutcome::Rejected { method };
        }
        if wants(AuthMethod::Ntlm) && find("ntlm").next().is_some() {
            *state = Some(Active::Ntlm);
            return AuthOutcome::UseNtlm;
        }
        if wants(AuthMethod::Digest)
            && let Some(d) = find("digest")
                .max_by_key(|c| digest_rank(c))
                .and_then(DigestState::from_challenge)
        {
            *state = Some(Active::Digest(d));
            return AuthOutcome::RetryWithCredentials;
        }
        if wants(AuthMethod::Basic) && find("basic").next().is_some() {
            *state = Some(Active::Basic);
            return AuthOutcome::RetryWithCredentials;
        }
        AuthOutcome::Required {
            schemes,
            supported: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn several_challenges_in_one_header_and_in_several() {
        let got = parse_challenges([
            r#"Digest realm="api@example.org", qop="auth, auth-int", algorithm=SHA-256, nonce="7ypf/xlj9XXwfDPEoM4URrv/xwf94BcCAzFZH4GiTo0v", opaque="FQhe/qaU925kfnzjCev0ciny7QMkPqMAFRtzCUYo5tdS", Basic realm="x""#,
            "NTLM, Negotiate",
            "NTLM TlRMTVNTUAACAAAAAAAAAAAAAAA=",
        ]);
        let schemes: Vec<_> = got.iter().map(|c| c.scheme.as_str()).collect();
        assert_eq!(schemes, ["digest", "basic", "ntlm", "negotiate", "ntlm"]);
        assert_eq!(got[0].params["qop"], "auth, auth-int");
        assert_eq!(got[0].params["algorithm"], "SHA-256");
        assert_eq!(got[1].params["realm"], "x");
        assert_eq!(got[4].token.as_deref(), Some("TlRMTVNTUAACAAAAAAAAAAAAAAA="));
    }

    #[test]
    fn basic_header_is_base64_of_user_and_password() {
        assert_eq!(
            basic_header("Aladdin", "open sesame"),
            "Basic QWxhZGRpbjpvcGVuIHNlc2FtZQ=="
        );
    }

    /// RFC 7616 section 3.9.1, SHA-256 and MD5.
    #[test]
    fn digest_matches_rfc_7616_examples() {
        let url = Url::parse("http://www.example.org/dir/index.html").unwrap();
        for (algorithm, response) in [
            (
                "SHA-256",
                "753927fa0e85d155564e2e272a28d1802ca10daf4496794697cf8db5856cb6c1",
            ),
            ("MD5", "8ca523f5e9506fed4657c9700eebdbec"),
        ] {
            let header = format!(
                r#"Digest realm="http-auth@example.org", qop="auth, auth-int", algorithm={algorithm}, nonce="7ypf/xlj9XXwfDPEoM4URrv/xwf94BcCAzFZH4GiTo0v", opaque="FQhe/qaU925kfnzjCev0ciny7QMkPqMAFRtzCUYo5tdS""#
            );
            let parsed = parse_challenges([header.as_str()]);
            let d = DigestState::from_challenge(&parsed[0]).unwrap();
            let header = d.answer_with(
                "Mufasa",
                "Circle of Life",
                &url,
                "f2/wE4q74E6zIJEtWaHKaf5wv/H5QzzpXusqGemxURZJ",
            );
            assert!(
                header.contains(&format!("response=\"{response}\"")),
                "{algorithm}: {header}"
            );
            assert!(header.contains("nc=00000001"));
            // The counter goes up for the next pre-emptive answer.
            assert!(d.answer("Mufasa", "Circle of Life", &url).contains("nc=00000002"));
        }
    }

    fn sign_in(method: AuthMethod) -> SignIn {
        SignIn {
            origin: "https://staging.example.com".into(),
            user: "ann".into(),
            password: Secret::new("pw"),
            method,
        }
    }

    fn challenge(value: &str) -> HeaderMap {
        let mut h = HeaderMap::new();
        h.append(WWW_AUTHENTICATE, HeaderValue::from_str(value).unwrap());
        h
    }

    #[test]
    fn basic_goes_pre_emptive_after_the_first_challenge_and_only_to_its_origin() {
        let auth = Auth::new(&[sign_in(AuthMethod::Any)]);
        let page = Url::parse("https://staging.example.com/a").unwrap();
        let other = Url::parse("https://cdn.example.com/a").unwrap();
        assert!(auth.authorize(&page).is_none());
        assert_eq!(
            auth.on_unauthorized(&page, &challenge(r#"Basic realm="x""#), false),
            AuthOutcome::RetryWithCredentials
        );
        assert!(auth.authorize(&page).is_some());
        assert!(auth.authorize(&other).is_none());
        assert_eq!(
            auth.on_unauthorized(&page, &challenge(r#"Basic realm="x""#), true),
            AuthOutcome::Rejected { method: "basic" }
        );
    }

    #[test]
    fn no_sign_in_reports_the_schemes_and_whether_they_are_supported() {
        let auth = Auth::new(&[]);
        let page = Url::parse("https://intranet.example.com/").unwrap();
        assert_eq!(
            auth.on_unauthorized(&page, &challenge("Negotiate"), false),
            AuthOutcome::Required {
                schemes: vec!["negotiate".into()],
                supported: false
            }
        );
        assert_eq!(
            auth.on_unauthorized(&page, &challenge(r#"Digest realm="r", nonce="n""#), false),
            AuthOutcome::Required {
                schemes: vec!["digest".into()],
                supported: true
            }
        );
    }

    #[test]
    fn method_basic_is_sent_from_the_first_request_and_ntlm_is_chosen_first() {
        let page = Url::parse("https://staging.example.com/").unwrap();
        assert!(
            Auth::new(&[sign_in(AuthMethod::Basic)])
                .authorize(&page)
                .is_some()
        );
        let auth = Auth::new(&[sign_in(AuthMethod::Any)]);
        let mut h = challenge("NTLM");
        h.append(WWW_AUTHENTICATE, HeaderValue::from_static("Basic realm=\"x\""));
        assert_eq!(auth.on_unauthorized(&page, &h, false), AuthOutcome::UseNtlm);
        assert!(auth.is_ntlm(&page));
    }

    #[test]
    fn stale_digest_nonce_is_refreshed_not_rejected() {
        let auth = Auth::new(&[sign_in(AuthMethod::Digest)]);
        let page = Url::parse("https://staging.example.com/").unwrap();
        let first = challenge(r#"Digest realm="r", nonce="one", qop="auth""#);
        assert_eq!(
            auth.on_unauthorized(&page, &first, false),
            AuthOutcome::RetryWithCredentials
        );
        let stale = challenge(r#"Digest realm="r", nonce="two", qop="auth", stale=true"#);
        assert_eq!(
            auth.on_unauthorized(&page, &stale, true),
            AuthOutcome::RetryWithCredentials
        );
        let v = auth.authorize(&page).unwrap();
        assert!(v.to_str().unwrap().contains("nonce=\"two\""));
    }
}
