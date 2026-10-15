//! Spec 025 user stories 3 and 5: supplied cookies, the cookie policy and
//! the cookie audit, against the local fixture site.

use std::path::PathBuf;
use std::time::Duration;

use spiderlens_config::{CookiePolicy, CrawlConfig, HeaderValue, ModeInput, SpeedSettings, SuppliedCookie};
use spiderlens_engine::crawl::{Control, Crawler};
use spiderlens_test_fixtures::{Site, SiteBuilder};
use tokio::sync::watch;

const E: &str = "example.test";
const X: &str = "other.test";

fn site() -> Site {
    SiteBuilder::new()
        .page(E, "/", "", &["/b", "{base:other.test}/x"])
        .header(
            E,
            "/",
            "Set-Cookie",
            "tracker=TRACK-77; Path=/; Max-Age=3600; SameSite=None; Secure",
        )
        .header(E, "/", "Set-Cookie", "sid=SESSION-91; Path=/; HttpOnly")
        .page(E, "/b", "", &[])
        .page(X, "/x", "", &[])
        .header(X, "/x", "Set-Cookie", "ext=EXT-5")
        .start()
}

async fn crawl(site: &Site, edit: impl FnOnce(&mut CrawlConfig)) -> (tempfile::TempDir, PathBuf) {
    let mut c = CrawlConfig {
        speed: SpeedSettings::FAST,
        ..CrawlConfig::default()
    };
    edit(&mut c);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("c.spiderlens");
    let input = ModeInput::Spider {
        start_url: site.url(E, "/"),
    };
    let crawler = Crawler::create(&path, c, &input)
        .unwrap()
        .with_resolve(site.resolve());
    let (_tx, rx) = watch::channel(Control::Run);
    tokio::time::timeout(Duration::from_secs(60), crawler.run(rx, None))
        .await
        .expect("crawl finishes")
        .unwrap();
    (dir, path)
}

fn cookie_sent(site: &Site, host: &str, path: &str) -> Option<String> {
    site.page_hits()
        .into_iter()
        .find(|h| h.host == host && h.path == path)
        .unwrap_or_else(|| panic!("{host}{path} was not requested"))
        .cookie
}

fn supplied(name: &str, value: HeaderValue) -> SuppliedCookie {
    SuppliedCookie {
        name: name.into(),
        value,
        domain: E.into(),
        path: "/".into(),
        secure_only: false,
    }
}

/// US3 scenario 1: a supplied cookie reaches internal pages, never another site.
#[tokio::test]
async fn supplied_cookie_goes_to_internal_hosts_only() {
    let site = site();
    crawl(&site, |c| {
        c.access
            .cookies
            .push(supplied("consent", HeaderValue::Plain("yes".into())))
    })
    .await;
    assert_eq!(cookie_sent(&site, E, "/").as_deref(), Some("consent=yes"));
    assert_eq!(cookie_sent(&site, E, "/b").as_deref(), Some("consent=yes"));
    assert_eq!(cookie_sent(&site, X, "/x"), None);
}

/// US3 scenario 2: by default, cookies the site sets are not sent back.
#[tokio::test]
async fn default_policy_ignores_site_cookies() {
    let site = site();
    crawl(&site, |_| {}).await;
    assert_eq!(cookie_sent(&site, E, "/b"), None);
}

/// US3 scenario 3 and 6: with Keep, later requests carry the site's cookies,
/// except a Secure one over plain http.
#[tokio::test]
async fn keep_policy_sends_site_cookies_back() {
    let site = site();
    crawl(&site, |c| c.access.cookie_policy = CookiePolicy::Keep).await;
    assert_eq!(cookie_sent(&site, E, "/b").as_deref(), Some("sid=SESSION-91"));
    assert_eq!(cookie_sent(&site, X, "/x"), None);
}

type AuditRow = (String, String, String, i64, i64, Option<String>, String, bool);

/// US5 scenarios 1–3 and US6: the audit records every cookie with its
/// attributes, and no cookie value reaches the crawl file.
#[tokio::test]
async fn audit_records_attributes_and_no_values() {
    let site = site();
    let (_dir, path) = crawl(&site, |c| {
        c.access
            .cookies
            .push(supplied("secret", HeaderValue::secret("SUPPLIED-SECRET-3")))
    })
    .await;
    let conn = rusqlite::Connection::open(&path).unwrap();
    let mut rows: Vec<AuditRow> = conn
        .prepare(
            "SELECT c.name, c.domain, c.path, c.secure, c.http_only, c.same_site, c.party, c.expires IS NULL \
             FROM cookies c JOIN urls u ON u.id = c.url_id ORDER BY c.name",
        )
        .unwrap()
        .query_map([], |r| {
            Ok((
                r.get(0)?,
                r.get(1)?,
                r.get(2)?,
                r.get(3)?,
                r.get(4)?,
                r.get(5)?,
                r.get(6)?,
                r.get(7)?,
            ))
        })
        .unwrap()
        .map(Result::unwrap)
        .collect();
    rows.sort();
    assert_eq!(
        rows,
        [
            (
                "ext".into(),
                X.into(),
                "/".into(),
                0,
                0,
                None,
                "first".into(),
                true
            ),
            (
                "sid".into(),
                E.into(),
                "/".into(),
                0,
                1,
                None,
                "first".into(),
                true
            ),
            (
                "tracker".into(),
                E.into(),
                "/".into(),
                1,
                0,
                Some("none".into()),
                "first".into(),
                false
            ),
        ]
    );
    drop(conn);
    let mut bytes = std::fs::read(&path).unwrap();
    for side in ["-wal", "-journal"] {
        bytes.extend(std::fs::read(format!("{}{side}", path.display())).unwrap_or_default());
    }
    for value in ["TRACK-77", "SESSION-91", "EXT-5", "SUPPLIED-SECRET-3"] {
        assert!(
            !bytes.windows(value.len()).any(|w| w == value.as_bytes()),
            "{value} reached the crawl file"
        );
    }
}
