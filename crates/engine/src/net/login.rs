//! A login session (spec 025 plan D5): the cookies a person's sign-in left
//! in the login window, or in a browser's exported `cookies.txt` for the
//! CLI. Held in memory only and never written; the crawl sends them to its
//! internal hosts like supplied cookies.

use spiderlens_config::{CheckCondition, Secret};
use url::Url;

/// One cookie of the session. The value is a [`Secret`]: redacted in
/// `Debug`, never serialized, wiped on drop.
#[derive(Debug, Clone, PartialEq)]
pub struct SessionCookie {
    pub name: String,
    pub value: Secret,
    /// Without a leading dot; `host_only` says whether subdomains get it.
    pub domain: String,
    pub host_only: bool,
    pub path: String,
    pub secure: bool,
    pub http_only: bool,
    /// Milliseconds since the epoch; `None` for a session cookie.
    pub expires: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct LoginSession {
    /// The login page, or the cookies file's name for the CLI.
    pub login_url: String,
    /// Milliseconds since the epoch.
    pub captured_at: i64,
    pub cookies: Vec<SessionCookie>,
}

/// What reading a `cookies.txt` found (spec 025 US3 scenario 5).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ImportCounts {
    pub imported: usize,
    pub skipped_expired: usize,
    pub skipped_external: usize,
    pub skipped_malformed: usize,
}

impl LoginSession {
    /// Names per domain, for the UI's summary (values never leave Rust).
    pub fn summary(&self) -> Vec<(String, Vec<String>)> {
        let mut out: Vec<(String, Vec<String>)> = Vec::new();
        for c in &self.cookies {
            match out.iter_mut().find(|(d, _)| *d == c.domain) {
                Some((_, names)) => {
                    if !names.contains(&c.name) {
                        names.push(c.name.clone());
                    }
                }
                None => out.push((c.domain.clone(), vec![c.name.clone()])),
            }
        }
        out
    }

    /// Keeps the cookies for hosts `keep` accepts (the crawl's internal
    /// hosts, FR-008) and returns how many were dropped.
    pub fn retain_hosts(&mut self, keep: impl Fn(&str) -> bool) -> usize {
        let before = self.cookies.len();
        self.cookies.retain(|c| keep(&c.domain));
        before - self.cookies.len()
    }

    /// Reads a Netscape `cookies.txt` as browsers' export extensions write
    /// it: tab-separated domain, include-subdomains, path, secure, expiry
    /// (seconds), name and value; `#HttpOnly_` before the domain marks
    /// HttpOnly cookies; other `#` lines are comments.
    pub fn from_cookies_txt(
        text: &str,
        source: &str,
        now_ms: i64,
        keep_host: impl Fn(&str) -> bool,
    ) -> (Self, ImportCounts) {
        let mut counts = ImportCounts::default();
        let mut cookies = Vec::new();
        for line in text.lines() {
            let line = line.trim_end_matches('\r');
            let (http_only, line) = match line.strip_prefix("#HttpOnly_") {
                Some(rest) => (true, rest),
                None => (false, line),
            };
            if line.trim().is_empty() || line.starts_with('#') {
                continue;
            }
            let f: Vec<&str> = line.split('\t').collect();
            if f.len() != 7 {
                counts.skipped_malformed += 1;
                continue;
            }
            let flag = |s: &str| s.eq_ignore_ascii_case("TRUE");
            let Ok(expiry) = f[4].trim().parse::<i64>() else {
                counts.skipped_malformed += 1;
                continue;
            };
            let expires = (expiry > 0).then(|| expiry.saturating_mul(1000));
            if expires.is_some_and(|e| e <= now_ms) {
                counts.skipped_expired += 1;
                continue;
            }
            let domain = f[0].trim().trim_start_matches('.').to_ascii_lowercase();
            if domain.is_empty() || f[5].is_empty() {
                counts.skipped_malformed += 1;
                continue;
            }
            if !keep_host(&domain) {
                counts.skipped_external += 1;
                continue;
            }
            cookies.push(SessionCookie {
                name: f[5].to_string(),
                value: Secret::new(f[6]),
                host_only: !flag(f[1]) && !f[0].starts_with('.'),
                domain,
                path: if f[2].is_empty() {
                    "/".into()
                } else {
                    f[2].to_string()
                },
                secure: flag(f[3]),
                http_only,
                expires,
            });
            counts.imported += 1;
        }
        (
            Self {
                login_url: source.to_string(),
                captured_at: now_ms,
                cookies,
            },
            counts,
        )
    }
}

/// Whether a URL is a sign-out link: a pattern appears in its path or
/// query, ignoring case (FR-011). Patterns are plain text, not regexes.
pub fn is_sign_out(url: &Url, patterns: &[String]) -> Option<String> {
    let mut target = url.path().to_ascii_lowercase();
    if let Some(q) = url.query() {
        target.push('?');
        target.push_str(&q.to_ascii_lowercase());
    }
    patterns
        .iter()
        .map(|p| p.trim())
        .find(|p| !p.is_empty() && target.contains(&p.to_ascii_lowercase()))
        .map(str::to_string)
}

/// One answer of the signed-out check URL.
pub struct CheckResponse<'a> {
    pub status: u16,
    /// Where it redirected, if it did.
    pub location: Option<&'a str>,
    pub body: Option<&'a str>,
}

/// Whether the check's answer means the session ended (FR-012).
pub fn signed_out(when: &CheckCondition, r: &CheckResponse<'_>) -> bool {
    match when {
        CheckCondition::RedirectsTo(text) => r
            .location
            .is_some_and(|l| l.to_ascii_lowercase().contains(&text.to_ascii_lowercase())),
        CheckCondition::Status(code) => r.status == *code,
        CheckCondition::Contains(text) => r.body.is_some_and(|b| b.contains(text.as_str())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_760_000_000_000;

    #[test]
    fn cookies_txt_from_browser_extensions() {
        let text = "# Netscape HTTP Cookie File\n\
            #HttpOnly_.example.com\tTRUE\t/\tTRUE\t0\tsid\tabc\n\
            shop.example.com\tFALSE\t/cart\tFALSE\t1900000000\tcart\t3\n\
            .example.com\tTRUE\t/\tFALSE\t1000\told\tx\n\
            .tracker.test\tTRUE\t/\tFALSE\t0\tid\t9\n\
            broken line\n";
        let internal = |h: &str| h == "example.com" || h.ends_with(".example.com");
        let (s, counts) = LoginSession::from_cookies_txt(text, "cookies.txt", NOW, internal);
        assert_eq!(
            counts,
            ImportCounts {
                imported: 2,
                skipped_expired: 1,
                skipped_external: 1,
                skipped_malformed: 1
            }
        );
        assert_eq!(s.cookies[0].name, "sid");
        assert!(s.cookies[0].http_only && s.cookies[0].secure && !s.cookies[0].host_only);
        assert_eq!(s.cookies[0].value.expose(), Some("abc"));
        assert!(s.cookies[1].host_only);
        assert_eq!(s.cookies[1].expires, Some(1_900_000_000_000));
        assert!(!format!("{s:?}").contains("abc"));
        assert_eq!(
            s.summary(),
            [
                ("example.com".to_string(), vec!["sid".to_string()]),
                ("shop.example.com".to_string(), vec!["cart".to_string()])
            ]
        );
    }

    #[test]
    fn sign_out_links_match_by_path_or_query() {
        let p: Vec<String> = spiderlens_config::DEFAULT_SIGN_OUT_PATTERNS
            .iter()
            .map(|s| s.to_string())
            .collect();
        let u = |s: &str| Url::parse(s).unwrap();
        assert_eq!(
            is_sign_out(&u("https://example.com/account/LogOut"), &p).as_deref(),
            Some("logout")
        );
        assert_eq!(
            is_sign_out(&u("https://example.com/?action=sign-out"), &p).as_deref(),
            Some("sign-out")
        );
        assert_eq!(is_sign_out(&u("https://logout.example.com/blog"), &p), None);
    }

    #[test]
    fn signed_out_conditions() {
        let r = CheckResponse {
            status: 302,
            location: Some("https://example.com/Login?next=/account"),
            body: None,
        };
        assert!(signed_out(&CheckCondition::RedirectsTo("/login".into()), &r));
        assert!(!signed_out(&CheckCondition::Status(401), &r));
        let page = CheckResponse {
            status: 200,
            location: None,
            body: Some("<a>Sign in</a>"),
        };
        assert!(signed_out(&CheckCondition::Contains("Sign in".into()), &page));
    }
}
