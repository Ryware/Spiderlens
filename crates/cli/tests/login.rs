//! Spec 025 CLI: login session from cookies.txt, sign-ins and proxy flags.

use assert_cmd::Command;
use predicates::str::contains;
use spiderlens_test_fixtures::SiteBuilder;

const HOST: &str = "127.0.0.1";

fn spiderlens() -> Command {
    Command::cargo_bin("spiderlens").expect("binary built")
}

#[test]
fn a_cookies_file_signs_the_crawl_in_and_sign_out_links_are_skipped() {
    let site = SiteBuilder::new()
        .page(HOST, "/", "", &["/account", "/account/logout"])
        .page(HOST, "/account", "", &[])
        .page(HOST, "/account/logout", "", &[])
        .start();
    let dir = tempfile::tempdir().unwrap();
    let cookies = dir.path().join("cookies.txt");
    std::fs::write(
        &cookies,
        "# Netscape HTTP Cookie File\n\
         #HttpOnly_127.0.0.1\tFALSE\t/\tFALSE\t0\tsid\tLOGIN-SECRET-9\n\
         .tracker.test\tTRUE\t/\tFALSE\t0\tid\tOTHER\n",
    )
    .unwrap();
    let file = dir.path().join("l.spiderlens");
    spiderlens()
        .arg("crawl")
        .arg(site.url(HOST, "/"))
        .arg("--cookies-file")
        .arg(&cookies)
        .args(["--quiet", "--out"])
        .arg(&file)
        .assert()
        .success();
    let account = site.page_hits().into_iter().find(|h| h.path == "/account").unwrap();
    assert_eq!(account.cookie.as_deref(), Some("sid=LOGIN-SECRET-9"));
    assert_eq!(site.count(HOST, "/account/logout"), 0, "sign-out link requested");
    let bytes = std::fs::read(&file).unwrap();
    assert!(
        !bytes.windows(14).any(|w| w == b"LOGIN-SECRET-9"),
        "login cookie stored"
    );
}

#[test]
fn a_sign_in_needs_its_password_in_the_environment() {
    spiderlens()
        .args(["crawl", "https://127.0.0.1:9/", "--auth", "https://127.0.0.1:9=ann", "--quiet"])
        .env_remove("SPIDERLENS_PASSWORD_127_0_0_1_9")
        .assert()
        .code(2)
        .stderr(contains("SPIDERLENS_PASSWORD_127_0_0_1_9"));
}

#[test]
fn an_unencrypted_sign_in_needs_allow_insecure_auth() {
    spiderlens()
        .args(["crawl", "http://127.0.0.1:9/", "--auth", "http://127.0.0.1:9=ann", "--quiet"])
        .env("SPIDERLENS_PASSWORD_127_0_0_1_9", "pw")
        .assert()
        .code(2)
        .stderr(contains("--allow-insecure-auth"));
}

#[test]
fn a_proxy_password_is_never_a_plain_argument() {
    spiderlens()
        .args(["crawl", "http://127.0.0.1:9/", "--proxy", "http://ann:pw@127.0.0.1:3128", "--quiet"])
        .assert()
        .code(2)
        .stderr(contains("SPIDERLENS_PROXY_PASSWORD"));
}

#[test]
fn proxy_test_reports_an_unreachable_proxy() {
    let closed = spiderlens_test_fixtures::closed_port_url();
    spiderlens()
        .args(["crawl", "http://127.0.0.1:9/", "--proxy-test", "--proxy"])
        .arg(&closed)
        .assert()
        .code(2)
        .stderr(contains("proxy problem"));
}
