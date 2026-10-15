//! The Cookies view (spec 025 user story 5) on a synthetic crawl file.

use rusqlite::{Connection, params};
use spiderlens_results::synth::Synth;
use spiderlens_results::{CookieFilter, Reader};

fn file() -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("s.spiderlens");
    Synth {
        urls: 20,
        ..Synth::default()
    }
    .write(&path)
    .unwrap();
    let conn = Connection::open(&path).unwrap();
    let mut ins = conn
        .prepare(
            "INSERT INTO cookies (url_id, name, domain, host_only, path, expires, max_age, secure, http_only, \
             same_site, size, party, source, extra) VALUES (?1, ?2, ?3, 1, '/', ?4, NULL, ?5, ?6, ?7, 10, ?8, 'response', NULL)",
        )
        .unwrap();
    // sid on 3 URLs, session, Secure and HttpOnly.
    for id in [1, 2, 3] {
        ins.execute(params![
            id,
            "sid",
            "example.test",
            None::<i64>,
            true,
            true,
            "lax",
            "first"
        ])
        .unwrap();
    }
    // ad on 2 URLs, persistent, third party, SameSite=None, no HttpOnly.
    for id in [2, 4] {
        ins.execute(params![
            id,
            "ad",
            "ads.test",
            Some(2_000_000_000_000_i64),
            true,
            false,
            "none",
            "third"
        ])
        .unwrap();
    }
    // theme on 1 URL, no Secure on https.
    ins.execute(params![
        5,
        "theme",
        "example.test",
        None::<i64>,
        false,
        false,
        None::<String>,
        "first"
    ])
    .unwrap();
    (dir, path)
}

#[test]
fn distinct_cookies_with_counts_and_filters() {
    let (_dir, path) = file();
    let r = Reader::open(&path).unwrap();
    let page = r.cookies(CookieFilter::All, None, true, 0, 50).unwrap();
    let names: Vec<(&str, i64)> = page.rows.iter().map(|c| (c.name.as_str(), c.urls)).collect();
    assert_eq!(names, [("sid", 3), ("ad", 2), ("theme", 1)]);
    assert_eq!(page.total, 3);
    assert_eq!(page.counts["third_party"], 1);
    assert_eq!(page.counts["missing_http_only"], 2);
    assert_eq!(page.counts["missing_secure"], 1);
    assert_eq!(page.counts["same_site_none"], 1);
    assert_eq!(page.counts["session"], 2);
    assert_eq!(page.counts["persistent"], 1);

    let third = r.cookies(CookieFilter::ThirdParty, None, true, 0, 50).unwrap();
    assert_eq!(third.rows.len(), 1);
    assert!(third.rows[0].third_party && !third.rows[0].session);

    let by_name = r.cookies(CookieFilter::All, Some("name"), false, 0, 2).unwrap();
    assert_eq!(
        by_name.rows.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(),
        ["ad", "sid"]
    );
    assert!(r.cookies(CookieFilter::All, Some("value"), false, 0, 2).is_err());
}

#[test]
fn urls_of_a_cookie_and_cookies_of_a_url() {
    let (_dir, path) = file();
    let r = Reader::open(&path).unwrap();
    let urls = r.cookie_urls("ad", "ads.test", "/", 0, 50).unwrap();
    assert_eq!(urls.total, 2);
    assert_eq!(urls.rows.iter().map(|u| u.id).collect::<Vec<_>>(), [2, 4]);
    let two = r.url_cookies(2).unwrap();
    assert_eq!(
        two.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(),
        ["sid", "ad"]
    );
}
