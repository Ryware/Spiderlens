//! Engine integration tests against the local fixture website.

use std::time::{Duration, Instant};

use spiderlens_engine::{CheckOptions, ErrorClass, FinalState, Title, check_page};
use spiderlens_test_fixtures::{Fixture, RobotsMode, UNRESOLVABLE_URL, closed_port_url};

fn opts() -> CheckOptions {
    CheckOptions::default()
}

fn text(s: &str) -> Option<Title> {
    Some(Title::Text(s.into()))
}

#[tokio::test]
async fn fetches_page_with_title() {
    let f = Fixture::start();
    let r = check_page(&f.url("/home"), &opts()).await;
    assert_eq!(r.state, FinalState::Fetched);
    assert_eq!(r.status, Some(200));
    assert_eq!(r.content_type.as_deref(), Some("text/html"));
    assert_eq!(r.title, text("Home | Example"));
    assert_eq!(r.attempts, 1);
    assert!(r.response_time_ms.is_some());
}

#[tokio::test]
async fn records_each_redirect_hop() {
    let f = Fixture::start();
    let r = check_page(&f.url("/old"), &opts()).await;
    assert_eq!(r.state, FinalState::Fetched);
    let hops: Vec<_> = r
        .redirects
        .iter()
        .map(|h| (h.status, h.location.clone()))
        .collect();
    assert_eq!(hops, vec![(308, f.url("/mid")), (302, f.url("/home"))]);
    assert_eq!(r.final_url, Some(f.url("/home")));
    assert_eq!(r.title, text("Home | Example"));
}

#[tokio::test]
async fn ten_hops_is_allowed_eleven_is_not() {
    let f = Fixture::start();
    let ok = check_page(&f.url("/chain/10"), &opts()).await;
    assert_eq!(ok.state, FinalState::Fetched);
    assert_eq!(ok.redirects.len(), 10);

    let over = check_page(&f.url("/chain/11"), &opts()).await;
    assert_eq!(over.state, FinalState::Failed);
    assert_eq!(over.error.unwrap().class, ErrorClass::RedirectLoop);
}

#[tokio::test]
async fn redirect_loop_is_detected() {
    let f = Fixture::start();
    let r = check_page(&f.url("/loop-a"), &opts()).await;
    assert_eq!(r.state, FinalState::Failed);
    assert_eq!(r.redirects.len(), 2);
    let e = r.error.unwrap();
    assert_eq!(e.class, ErrorClass::RedirectLoop);
    assert_eq!(e.attempts, 1);
}

#[tokio::test]
async fn http_error_status_is_still_fetched() {
    let f = Fixture::start();
    let r = check_page(&f.url("/nothing-here"), &opts()).await;
    assert_eq!(r.state, FinalState::Fetched);
    assert_eq!(r.status, Some(404));
    assert_eq!(r.title, text("Not found"));
}

#[tokio::test]
async fn title_variants() {
    let f = Fixture::start();
    let cases = [
        ("/no-title", Some(Title::Missing)),
        ("/empty-title", Some(Title::Missing)),
        ("/entities", text("Tom & Jerry — ☺")),
        ("/whitespace", text("Lots of space")),
        ("/hebrew-1255", text("שלום עולם")),
        ("/shift-jis-meta", text("こんにちは")),
        ("/image.png", Some(Title::NotApplicable)),
        ("/doc.pdf", Some(Title::NotApplicable)),
    ];
    for (path, expected) in cases {
        let r = check_page(&f.url(path), &opts()).await;
        assert_eq!(r.state, FinalState::Fetched, "{path}");
        assert_eq!(r.title, expected, "{path}");
    }
}

#[tokio::test]
async fn robots_disallow_blocks_without_fetching() {
    let f = Fixture::start();
    let r = check_page(&f.url("/private/page"), &opts()).await;
    assert_eq!(r.state, FinalState::BlockedByRobots);
    assert_eq!(r.error, None);
    assert_eq!(r.attempts, 0);
    assert_eq!(r.status, None);
}

#[tokio::test]
async fn robots_4xx_allows_and_5xx_or_429_blocks() {
    let missing = Fixture::start_with(RobotsMode::Status(404));
    assert_eq!(
        check_page(&missing.url("/private/page"), &opts()).await.state,
        FinalState::Fetched
    );

    for code in [500, 503, 429] {
        let f = Fixture::start_with(RobotsMode::Status(code));
        let r = check_page(&f.url("/home"), &opts()).await;
        assert_eq!(r.state, FinalState::BlockedByRobots, "robots status {code}");
    }
}

#[tokio::test]
async fn unresolvable_host_is_a_dns_error() {
    let r = check_page(UNRESOLVABLE_URL, &opts()).await;
    assert_eq!(r.state, FinalState::Failed);
    let e = r.error.unwrap();
    assert_eq!(e.class, ErrorClass::Dns, "{}", e.message);
    assert_eq!(e.attempts, 1);
}

#[tokio::test]
async fn refused_connection_is_a_connection_error() {
    let r = check_page(&closed_port_url(), &opts()).await;
    assert_eq!(r.error.unwrap().class, ErrorClass::Connection);
}

#[tokio::test]
async fn self_signed_certificate_is_tls_invalid() {
    let f = Fixture::start_self_signed_tls();
    let r = check_page(&f.url("/home"), &opts()).await;
    let e = r.error.unwrap();
    assert_eq!(e.class, ErrorClass::TlsInvalid, "{}", e.message);
}

#[tokio::test]
async fn slow_server_times_out_promptly() {
    let f = Fixture::start();
    let o = CheckOptions {
        read_timeout: Duration::from_millis(500),
        ..opts()
    };
    let started = Instant::now();
    let r = check_page(&f.url("/slow"), &o).await;
    assert_eq!(r.error.unwrap().class, ErrorClass::Timeout);
    // SC-004: ends no later than 5 s after the limit.
    assert!(started.elapsed() < Duration::from_millis(500) + Duration::from_secs(5));
}

#[tokio::test]
async fn oversized_body_is_a_content_problem() {
    let f = Fixture::start();
    let o = CheckOptions {
        max_body_bytes: 1024 * 1024,
        ..opts()
    };
    let r = check_page(&f.url("/huge"), &o).await;
    assert_eq!(r.error.unwrap().class, ErrorClass::ContentProblem);
}

#[tokio::test]
async fn malformed_url_makes_no_request() {
    let r = check_page("http://exa mple.com", &opts()).await;
    assert_eq!(r.state, FinalState::Failed);
    let e = r.error.unwrap();
    assert_eq!(e.class, ErrorClass::MalformedUrl);
    assert_eq!(e.attempts, 0);
}

#[tokio::test]
async fn result_serializes_to_the_documented_shape() {
    let f = Fixture::start();
    let r = check_page(&f.url("/home"), &opts()).await;
    let v = serde_json::to_value(&r).unwrap();
    assert_eq!(v["state"], "fetched");
    assert_eq!(v["title"]["kind"], "text");
    assert_eq!(v["title"]["value"], "Home | Example");
    assert!(v["error"].is_null());
}
