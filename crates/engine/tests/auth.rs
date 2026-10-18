//! Spec 025 user story 1: HTTP sign-ins (Basic, Digest, NTLM) against the
//! local sign-in fixture site.

mod support;

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use spiderlens_config::{AuthMethod, CrawlConfig, Mode, ModeInput, Secret, SignIn, SpeedSettings};
use spiderlens_engine::crawl::{Control, Crawler, RunCommand};
use spiderlens_test_fixtures::auth_site::{AuthHit, AuthSite, DigestAlgorithm, Scheme};
use support::{Done, quick, run};
use tokio::sync::{mpsc, oneshot};

const HOST: &str = "auth.test";

/// A start page that is open, and three pages under `/p/` behind `scheme`.
fn site_with(scheme: Scheme) -> AuthSite {
    AuthSite::builder(HOST)
        .page("/", &["/p/1", "/p/2", "/p/3"])
        .page("/p/1", &[])
        .page("/p/2", &[])
        .page("/p/3", &[])
        .protect("/p/", scheme)
        .start()
}

fn config(edit: impl FnOnce(&mut CrawlConfig)) -> CrawlConfig {
    let mut c = CrawlConfig {
        mode: Mode::Spider,
        ..CrawlConfig::default()
    };
    quick(&mut c);
    // One request at a time, so the request log shows each exchange.
    c.speed = SpeedSettings {
        max_concurrent: 1,
        max_per_host: 1,
        ..SpeedSettings::FAST
    };
    edit(&mut c);
    c
}

fn sign_in(site: &AuthSite, user: &str, password: &str, method: AuthMethod) -> SignIn {
    SignIn {
        origin: site.origin(),
        user: user.into(),
        password: Secret::new(password),
        method,
    }
}

fn prepare(site: &AuthSite, c: CrawlConfig) -> (tempfile::TempDir, PathBuf, Crawler) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("t.spiderlens");
    let input = ModeInput::Spider {
        start_url: site.url("/"),
    };
    let crawler = Crawler::create(&path, c, &input)
        .unwrap()
        .with_resolve(site.resolve());
    (dir, path, crawler)
}

async fn crawl(site: &AuthSite, edit: impl FnOnce(&mut CrawlConfig)) -> Done {
    let (dir, path, crawler) = prepare(site, config(edit));
    run(dir, path, crawler, Control::Run, None).await
}

/// (url, status, reason_detail) of every internal URL.
type Row = (String, Option<i64>, Option<String>);

fn rows(path: &Path) -> Vec<Row> {
    let conn = rusqlite::Connection::open(path).unwrap();
    conn.prepare("SELECT url, status, reason_detail FROM urls WHERE internal = 1 ORDER BY url")
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect()
}

fn row(rows: &[Row], site: &AuthSite, path: &str) -> Row {
    rows.iter()
        .find(|r| r.0 == site.url(path))
        .unwrap_or_else(|| panic!("{path} was not recorded: {rows:?}"))
        .clone()
}

fn notices_with(d: &Done, text: &str) -> usize {
    d.notices.iter().filter(|n| n.contains(text)).count()
}

/// Requests for the pages behind the sign-in, robots.txt and site files left out.
fn protected_hits(site: &AuthSite) -> Vec<AuthHit> {
    site.hits()
        .into_iter()
        .filter(|h| h.path.starts_with("/p/"))
        .collect()
}

fn assert_all_ok(site: &AuthSite, rows: &[Row]) {
    for p in ["/p/1", "/p/2", "/p/3"] {
        assert_eq!(
            row(rows, site, p),
            (site.url(p), Some(200), None),
            "{p} should be fetched with its sign-in"
        );
    }
}

/// US1 scenario 1: a Basic sign-in set up front reaches the protected pages.
/// With the method fixed to Basic the credentials go out at once; with
/// "any" the first page is asked for twice and the rest pre-emptively.
#[tokio::test]
async fn basic_sign_in_set_up_front_reaches_200() {
    for method in [AuthMethod::Basic, AuthMethod::Any] {
        let site = site_with(Scheme::basic("ann", "pw"));
        let s = sign_in(&site, "ann", "pw", method);
        let d = crawl(&site, |c| c.access.sign_ins.push(s)).await;
        assert_all_ok(&site, &rows(&d.path));
        assert_eq!(notices_with(&d, "sign-in"), 0, "{method:?}: {:?}", d.notices);
        let hits = protected_hits(&site);
        let anonymous = hits.iter().filter(|h| h.scheme.is_none()).count();
        match method {
            AuthMethod::Basic => {
                assert_eq!((hits.len(), anonymous), (3, 0), "{hits:?}");
            }
            _ => assert_eq!((hits.len(), anonymous), (4, 1), "{hits:?}"),
        }
        assert!(hits.iter().all(|h| h.scheme.as_deref().is_none_or(|s| s == "basic")));
    }
}

/// US1 scenario 1 and FR-003: Digest (MD5 and SHA-256) reaches the pages,
/// and after the first challenge the nonce is reused, so the requests are
/// not doubled.
#[tokio::test]
async fn digest_sign_in_reaches_200_without_doubling_requests() {
    for algorithm in [DigestAlgorithm::Md5, DigestAlgorithm::Sha256] {
        let site = site_with(Scheme::digest("ann", "pw", algorithm));
        let s = sign_in(&site, "ann", "pw", AuthMethod::Digest);
        let d = crawl(&site, |c| c.access.sign_ins.push(s)).await;
        assert_all_ok(&site, &rows(&d.path));
        let hits = protected_hits(&site);
        // One challenge for the first page, then one request per page.
        assert_eq!(hits.len(), 4, "{algorithm:?}: {hits:?}");
        assert_eq!(hits.iter().filter(|h| h.scheme.is_none()).count(), 1);
        assert_eq!(
            hits.iter()
                .filter(|h| h.scheme.as_deref() == Some("digest"))
                .count(),
            3
        );
    }
}

/// US1 scenarios 3 and 4: without a sign-in the pages are recorded as 401
/// with the schemes the server offered, once each, and the user is told once
/// for the origin.
#[tokio::test]
async fn without_a_sign_in_401_is_recorded_with_one_notice_per_origin() {
    let site = site_with(Scheme::basic("ann", "pw"));
    let d = crawl(&site, |_| {}).await;
    let rows = rows(&d.path);
    for p in ["/p/1", "/p/2", "/p/3"] {
        assert_eq!(
            row(&rows, &site, p),
            (site.url(p), Some(401), Some("auth_required:basic".into()))
        );
        assert_eq!(site.count(p), 1, "{p} must not be retried");
    }
    assert_eq!(row(&rows, &site, "/").1, Some(200));
    assert_eq!(notices_with(&d, "asks for a sign-in"), 1, "{:?}", d.notices);
    d.assert_one_final_state();
}

/// US1 scenario 3 with Digest: the recorded detail names the scheme.
#[tokio::test]
async fn digest_challenge_without_a_sign_in_is_recorded() {
    let site = site_with(Scheme::digest("ann", "pw", DigestAlgorithm::Sha256));
    let d = crawl(&site, |_| {}).await;
    let rows = rows(&d.path);
    assert_eq!(
        row(&rows, &site, "/p/1").2.as_deref(),
        Some("auth_required:digest")
    );
    assert_eq!(notices_with(&d, "asks for a sign-in"), 1, "{:?}", d.notices);
}

/// FR-005: a wrong password is reported as rejected, once for the origin,
/// and the crawl does not try again and again.
#[tokio::test]
async fn wrong_password_is_rejected_without_a_retry_loop() {
    for (scheme, method) in [
        (Scheme::basic("ann", "pw"), "basic"),
        (Scheme::digest("ann", "pw", DigestAlgorithm::Md5), "digest"),
    ] {
        let site = site_with(scheme);
        let s = sign_in(&site, "ann", "wrong", AuthMethod::Any);
        let d = crawl(&site, |c| c.access.sign_ins.push(s)).await;
        let rows = rows(&d.path);
        for p in ["/p/1", "/p/2", "/p/3"] {
            assert_eq!(
                row(&rows, &site, p),
                (site.url(p), Some(401), Some(format!("auth_rejected:{method}")))
            );
            // At most the challenge and one try with the credentials.
            assert!(site.count(p) <= 2, "{method}: {p} requested {} times", site.count(p));
        }
        assert_eq!(notices_with(&d, "refused the sign-in"), 1, "{:?}", d.notices);
        assert_eq!(notices_with(&d, "asks for a sign-in"), 0);
        d.assert_one_final_state();
    }
}

/// FR-003: a stale nonce (the server rotated it) is answered with the new
/// one; it is not reported as a wrong password.
#[tokio::test]
async fn digest_stale_nonce_is_refreshed_not_rejected() {
    let site = Arc::new(
        AuthSite::builder(HOST)
            .page("/", &["/p/1", "/slow", "/p/2"])
            .page("/p/1", &[])
            .page("/p/2", &[])
            .slow_page("/slow", 2000)
            .protect("/p/", Scheme::digest("ann", "pw", DigestAlgorithm::Md5))
            .start(),
    );
    let s = sign_in(&site, "ann", "pw", AuthMethod::Digest);
    let (dir, path, crawler) = prepare(
        &site,
        config(|c| {
            c.access.sign_ins.push(s);
            c.network.read_timeout_secs = 10.0;
            c.network.connect_timeout_secs = 10.0;
        }),
    );
    let s2 = site.clone();
    tokio::spawn(async move {
        // While the slow page is out, /p/1 is done: the server now rotates
        // its nonce, so /p/2 is first answered "stale".
        for _ in 0..500 {
            if s2.count("/slow") >= 1 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        s2.expire_nonce();
    });
    let d = run(dir, path, crawler, Control::Run, None).await;
    let rows = rows(&d.path);
    for p in ["/p/1", "/p/2"] {
        assert_eq!(row(&rows, &site, p), (site.url(p), Some(200), None), "{p}");
    }
    assert_eq!(site.count("/p/2"), 2, "one stale answer, then the new nonce");
    assert_eq!(notices_with(&d, "refused the sign-in"), 0, "{:?}", d.notices);
}

/// T023 / FR-004: a sign-in added while the crawl runs fetches the URLs that
/// answered 401 again, and they end as 200.
#[tokio::test]
async fn adding_a_sign_in_mid_crawl_fetches_the_401_urls_again() {
    let site = Arc::new(
        AuthSite::builder(HOST)
            .page("/", &["/p/1", "/p/2", "/slow"])
            .page("/p/1", &[])
            .page("/p/2", &[])
            // Keeps the crawl alive while the sign-in is added.
            .slow_page("/slow", 4000)
            .protect("/p/", Scheme::basic("ann", "pw"))
            .start(),
    );
    let (dir, path, crawler) = prepare(
        &site,
        config(|c| {
            c.network.read_timeout_secs = 10.0;
            c.network.connect_timeout_secs = 10.0;
        }),
    );
    let (commands, rx) = mpsc::unbounded_channel();
    let (done_tx, done_rx) = oneshot::channel();
    let s2 = site.clone();
    let add = sign_in(&site, "ann", "pw", AuthMethod::Any);
    tokio::spawn(async move {
        for _ in 0..500 {
            if s2.count("/slow") >= 1 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        let (reply, answer) = oneshot::channel();
        commands
            .send(RunCommand::AddSignIn {
                sign_in: add,
                reply,
            })
            .unwrap();
        done_tx.send(answer.await.unwrap()).ok();
        // Keep the channel open until the crawl ends.
        tokio::time::sleep(Duration::from_secs(60)).await;
    });
    let d = run(dir, path, crawler, Control::Run, Some(rx)).await;
    assert_eq!(done_rx.await.unwrap(), 2, "both 401 URLs are queued again");
    let rows = rows(&d.path);
    for p in ["/p/1", "/p/2"] {
        assert_eq!(row(&rows, &site, p), (site.url(p), Some(200), None), "{p}");
    }
    assert_eq!(notices_with(&d, "asks for a sign-in"), 1, "{:?}", d.notices);
    assert_eq!(notices_with(&d, "Signed in to"), 1, "{:?}", d.notices);
    d.assert_one_final_state();
}

fn ntlm_site() -> AuthSite {
    site_with(Scheme::ntlm("CORP\\ann", "pw"))
}

/// US1 scenario 2: NTLM with a `DOMAIN\user` name signs in a connection and
/// the pages are fetched on it.
#[tokio::test]
async fn ntlm_sign_in_with_a_domain_user_reaches_200() {
    let site = ntlm_site();
    let s = sign_in(&site, "CORP\\ann", "pw", AuthMethod::Ntlm);
    let d = crawl(&site, |c| c.access.sign_ins.push(s)).await;
    assert_all_ok(&site, &rows(&d.path));
    let hits = protected_hits(&site);
    let handshake: HashSet<usize> = hits
        .iter()
        .filter(|h| h.scheme.as_deref() == Some("ntlm"))
        .map(|h| h.connection)
        .collect();
    assert_eq!(handshake.len(), 1, "the handshake stays on one connection: {hits:?}");
    // Once signed in, the connection carries the later pages without headers.
    let signed_in = *handshake.iter().next().unwrap();
    assert!(
        hits.iter()
            .filter(|h| h.connection == signed_in)
            .any(|h| h.scheme.is_none()),
        "{hits:?}"
    );
    assert_eq!(notices_with(&d, "sign-in"), 0, "{:?}", d.notices);
}

/// FR-005 for NTLM: the server refuses the user name; the pages are 401
/// with `auth_rejected:ntlm`. (The fixture checks the user name only.)
#[tokio::test]
async fn ntlm_with_the_wrong_user_is_rejected() {
    let site = ntlm_site();
    let s = sign_in(&site, "CORP\\bob", "pw", AuthMethod::Any);
    let d = crawl(&site, |c| c.access.sign_ins.push(s)).await;
    let rows = rows(&d.path);
    for p in ["/p/1", "/p/2", "/p/3"] {
        assert_eq!(
            row(&rows, &site, p),
            (site.url(p), Some(401), Some("auth_rejected:ntlm".into()))
        );
        // The challenge, the first message and the answer at most.
        assert!(site.count(p) <= 3, "{p} requested {} times", site.count(p));
    }
    assert_eq!(notices_with(&d, "refused the sign-in"), 1, "{:?}", d.notices);
}
