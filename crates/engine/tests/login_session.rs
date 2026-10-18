//! Spec 025 user story 2: a crawl signed in with a login session's cookies
//! never requests sign-out links, and pauses when the site ends the session.

mod support;

use std::path::Path;

use spiderlens_config::{
    CheckCondition, CrawlConfig, LoginSettings, Mode, ModeInput, Secret, SignedOutCheck, SpeedSettings,
};
use spiderlens_engine::crawl::{Control, Crawler};
use spiderlens_engine::net::{LoginSession, SessionCookie};
use spiderlens_test_fixtures::login_site::LoginSite;
use support::{Done, quick, run};

const HOST: &str = "login.test";

/// What a person's sign-in in the login window leaves behind: the `sid`
/// cookie of the fixture's session.
fn session_with(site: &LoginSite, sid: &str) -> LoginSession {
    LoginSession {
        login_url: site.url("/login"),
        captured_at: 0,
        cookies: vec![SessionCookie {
            name: "sid".into(),
            value: Secret::new(sid),
            domain: HOST.into(),
            host_only: true,
            path: "/".into(),
            secure: false,
            http_only: true,
            expires: None,
        }],
    }
}

async fn crawl(site: &LoginSite, login: Option<LoginSession>, edit: impl FnOnce(&mut CrawlConfig)) -> Done {
    let mut c = CrawlConfig {
        mode: Mode::Spider,
        ..CrawlConfig::default()
    };
    quick(&mut c);
    // One request at a time, so a pause is seen soon after the session ends.
    c.speed = SpeedSettings {
        max_concurrent: 1,
        max_per_host: 1,
        ..SpeedSettings::FAST
    };
    edit(&mut c);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("t.spiderlens");
    let input = ModeInput::Spider {
        start_url: site.url("/members/"),
    };
    let mut crawler = Crawler::create(&path, c, &input)
        .unwrap()
        .with_resolve(site.resolve());
    if let Some(l) = login {
        crawler.set_login(l);
    }
    run(dir, path, crawler, Control::Run, None).await
}

/// (status, state, reason, reason_detail) of one URL.
type Row = (Option<i64>, String, Option<String>, Option<String>);

fn row(path: &Path, url: &str) -> Row {
    let conn = rusqlite::Connection::open(path).unwrap();
    conn.query_row(
        "SELECT status, state, reason, reason_detail FROM urls WHERE url = ?1",
        [url],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
    )
    .unwrap_or_else(|e| panic!("{url} was not recorded: {e}"))
}

fn file_bytes(path: &Path) -> Vec<u8> {
    let mut bytes = std::fs::read(path).unwrap();
    for side in ["-wal", "-journal"] {
        bytes.extend(std::fs::read(format!("{}{side}", path.display())).unwrap_or_default());
    }
    bytes
}

/// US2 scenario 1: the session's cookie lets the crawl see the members
/// pages, and its value does not reach the crawl file.
#[tokio::test]
async fn login_session_cookie_lets_the_crawl_see_members_pages() {
    let site = LoginSite::builder(HOST).members(3).start();
    let sid = site.mint_session("ann");
    let d = crawl(&site, Some(session_with(&site, &sid)), |_| {}).await;
    for p in ["/members/", "/members/1", "/members/2", "/members/3"] {
        let r = row(&d.path, &site.url(p));
        assert_eq!(r.0, Some(200), "{p}: {r:?}");
        assert!(
            site.hits().iter().any(|h| h.path == p && h.signed_in),
            "{p} was requested with the session"
        );
    }
    let bytes = file_bytes(&d.path);
    assert!(
        !bytes.windows(sid.len()).any(|w| w == sid.as_bytes()),
        "the session value reached the crawl file"
    );
    d.assert_one_final_state();
}

/// Control for the test above: without a session the same start URL sends
/// the crawl to the login page.
#[tokio::test]
async fn without_a_login_session_the_members_area_redirects_to_login() {
    let site = LoginSite::builder(HOST).members(3).start();
    let d = crawl(&site, None, |_| {}).await;
    assert_eq!(row(&d.path, &site.url("/members/")).0, Some(302));
    assert_eq!(site.count("/members/1"), 0);
}

/// US2 scenario 2 and FR-011: the sign-out link is never requested; it is
/// recorded as not crawled because it could end the session, and the
/// session is still alive at the end.
#[tokio::test]
async fn sign_out_link_is_never_requested() {
    let site = LoginSite::builder(HOST).members(3).start();
    let sid = site.mint_session("ann");
    let d = crawl(&site, Some(session_with(&site, &sid)), |_| {}).await;
    assert_eq!(site.count("/logout"), 0, "{:?}", site.hits());
    let r = row(&d.path, &site.url("/logout"));
    assert_eq!(r.1, "not_crawled");
    assert_eq!(r.2.as_deref(), Some("session_guard"));
    assert!(r.3.as_deref().is_some_and(|t| t.contains("logout")), "{r:?}");
    assert!(site.session_is_live(&sid));
    // The pages after the link were still crawled.
    assert_eq!(row(&d.path, &site.url("/members/3")).0, Some(200));
}

/// FR-011: the patterns are the user's to change; a custom list guards its
/// own sign-out address.
#[tokio::test]
async fn custom_sign_out_patterns_replace_the_defaults() {
    let site = LoginSite::builder(HOST).members(2).start();
    let sid = site.mint_session("ann");
    let d = crawl(&site, Some(session_with(&site, &sid)), |c| {
        c.access.login = Some(LoginSettings {
            sign_out_patterns: vec!["/members/2".into()],
            ..LoginSettings::new(site.url("/login"))
        });
    })
    .await;
    let guarded = row(&d.path, &site.url("/members/2"));
    assert_eq!((guarded.1.as_str(), guarded.2.as_deref()), ("not_crawled", Some("session_guard")));
    assert_eq!(site.count("/members/2"), 0);
    // "/logout" is no longer on the list, so it is requested.
    assert_eq!(site.count("/logout"), 1);
}

/// US2 scenario 4 and SC-006: when the site ends the session, the signed-out
/// check notices within 50 requests and the crawl pauses with the reason
/// "session_ended".
#[tokio::test]
async fn crawl_pauses_when_the_session_ends() {
    let site = LoginSite::builder(HOST).members(80).expire_after(40).start();
    let sid = site.mint_session("ann");
    let d = crawl(&site, Some(session_with(&site, &sid)), |c| {
        c.access.login = Some(LoginSettings {
            check: Some(SignedOutCheck {
                url: site.url("/account"),
                when: CheckCondition::Contains("Sign in".into()),
            }),
            ..LoginSettings::new(site.url("/login"))
        });
    })
    .await;
    assert_eq!(site.expired_at(), Some(40), "the site ended the session");
    assert_eq!(d.summary.status, "paused", "{:?}", d.summary);
    assert_eq!(d.summary.pause_reason.as_deref(), Some("session_ended"));
    assert!(
        d.notices.iter().any(|n| n.contains("signed the crawl out")),
        "{:?}",
        d.notices
    );
    let after = site.request_count() - 40;
    assert!(after <= 50, "{after} requests after the session ended");
    // It stopped early: most of the 80 member pages were not requested.
    assert!(d.summary.queued > 0, "{:?}", d.summary);
}
