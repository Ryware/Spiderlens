//! Spec 025 user story 4: a crawl through a proxy. The fixture proxy
//! resolves the `.test` names itself, and these crawls get no DNS overrides,
//! so a request that went around the proxy would not reach the site.

mod support;

use std::io::{Read, Write};
use std::net::TcpStream;

use spiderlens_config::{
    AccessSettings, CrawlConfig, Mode, ModeInput, ProxyKind, ProxyMode, ProxySettings, Secret,
};
use spiderlens_engine::CheckOptions;
use spiderlens_engine::crawl::{Control, Crawler};
use spiderlens_engine::net::{ProxyProblem, test_proxy};
use spiderlens_test_fixtures::proxy::{ProxyServer, closed_port};
use spiderlens_test_fixtures::{Site, SiteBuilder};
use support::{Done, quick, run};

const E: &str = "example.test";
const D: &str = "direct.test";

/// A site with robots.txt, a sitemap, pages, and a link to a second host.
fn site() -> Site {
    SiteBuilder::new()
        .robots(
            E,
            "User-agent: *\nAllow: /\n\nSitemap: {base:example.test}/sitemap.xml\n",
        )
        .raw(
            E,
            "/sitemap.xml",
            200,
            "application/xml",
            b"<?xml version=\"1.0\" encoding=\"UTF-8\"?><urlset xmlns=\"http://www.sitemaps.org/schemas/sitemap/0.9\"><url><loc>{base:example.test}/c</loc></url></urlset>",
        )
        .page(E, "/", "", &["/a", "/b", "{base:direct.test}/d"])
        .page(E, "/a", "", &[])
        .page(E, "/b", "", &[])
        .page(E, "/c", "", &[])
        .page(D, "/d", "", &[])
        .start()
}

fn manual(kind: ProxyKind, proxy: &ProxyServer) -> ProxySettings {
    ProxySettings {
        mode: ProxyMode::Manual,
        kind,
        host: proxy.host().into(),
        port: proxy.port(),
        ..ProxySettings::default()
    }
}

/// Crawls the site through `proxy_settings`. With `resolve` the crawler also
/// gets DNS overrides, which only hosts that bypass the proxy can use.
async fn crawl(site: &Site, proxy_settings: ProxySettings, resolve: bool) -> Done {
    let mut c = CrawlConfig {
        mode: Mode::Spider,
        ..CrawlConfig::default()
    };
    quick(&mut c);
    c.access.proxy = proxy_settings;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("t.spiderlens");
    let input = ModeInput::Spider {
        start_url: site.url(E, "/"),
    };
    let mut crawler = Crawler::create(&path, c, &input).unwrap();
    if resolve {
        crawler = crawler.with_resolve(site.resolve());
    }
    run(dir, path, crawler, Control::Run, None).await
}

fn probe(site: &Site) -> url::Url {
    url::Url::parse(&site.url(E, "/robots.txt")).unwrap()
}

fn assert_every_request_came_through(proxy: &ProxyServer) {
    let paths = proxy.paths_to(E);
    for p in ["/robots.txt", "/sitemap.xml", "/", "/a", "/b", "/c"] {
        assert!(
            paths.iter().any(|x| x == p),
            "{p} did not go through the proxy: {paths:?}"
        );
    }
    // The link to the other host is checked through the proxy too.
    assert!(proxy.paths_to(D).iter().any(|x| x == "/d"), "{:?}", proxy.hits());
}

/// US4 scenario 1 and FR-020: with a manual HTTP proxy, every request,
/// robots.txt and the sitemap included, goes through it, and the site sees
/// exactly the requests the proxy forwarded.
#[tokio::test]
async fn manual_http_proxy_carries_every_request() {
    let site = site();
    let proxy = ProxyServer::start(site.resolve());
    let d = crawl(&site, manual(ProxyKind::Http, &proxy), false).await;
    d.assert_one_final_state();
    assert_every_request_came_through(&proxy);
    let forwarded = proxy.hits().iter().filter(|h| h.path.is_some()).count();
    assert_eq!(forwarded, site.hits().len(), "{:?}", proxy.hits());
    assert!(d.summary.crawled >= 4, "{:?}", d.summary);
}

/// US4 scenario 2: the same through a SOCKS5 proxy, with names resolved by
/// the proxy.
#[tokio::test]
async fn socks5_proxy_carries_every_request() {
    let site = site();
    let proxy = ProxyServer::start(site.resolve());
    let d = crawl(&site, manual(ProxyKind::Socks5, &proxy), false).await;
    d.assert_one_final_state();
    assert!(proxy.tunnels_to(E) >= 1, "{:?}", proxy.hits());
    assert_every_request_came_through(&proxy);
}

/// The fixture's own CONNECT support: the tunnel reaches the site and the
/// request inside it is seen.
#[test]
fn connect_tunnel_reaches_the_target() {
    let site = site();
    let proxy = ProxyServer::start(site.resolve());
    let mut s = TcpStream::connect(("127.0.0.1", proxy.port())).unwrap();
    let target = format!("{E}:{}", site.port());
    write!(s, "CONNECT {target} HTTP/1.1\r\nHost: {target}\r\n\r\n").unwrap();
    let mut head = Vec::new();
    let mut byte = [0u8; 1];
    while !head.ends_with(b"\r\n\r\n") {
        s.read_exact(&mut byte).unwrap();
        head.push(byte[0]);
    }
    assert!(String::from_utf8_lossy(&head).contains(" 200 "), "{head:?}");
    write!(
        s,
        "GET /a HTTP/1.1\r\nHost: {target}\r\nConnection: close\r\n\r\n"
    )
    .unwrap();
    let mut answer = String::new();
    s.read_to_string(&mut answer).unwrap();
    assert!(answer.starts_with("HTTP/1.1 200"), "{answer}");
    assert_eq!(proxy.tunnels_to(E), 1);
    assert_eq!(proxy.paths_to(E), ["/a"]);
}

/// FR-021 and FR-022: a proxy that wants a user name and password gets
/// them (HTTP and SOCKS5), and the test says which proxy is in effect.
#[tokio::test]
async fn proxy_credentials_are_sent() {
    for kind in [ProxyKind::Http, ProxyKind::Socks5] {
        let site = site();
        let proxy = ProxyServer::builder()
            .routes(site.resolve())
            .require_credentials("ann", "pw")
            .start();
        let settings = ProxySettings {
            user: Some("ann".into()),
            password: Secret::new("pw"),
            ..manual(kind, &proxy)
        };
        let access = AccessSettings {
            proxy: settings.clone(),
            ..AccessSettings::default()
        };
        let effect = test_proxy(&access, &CheckOptions::default(), &probe(&site))
            .await
            .unwrap_or_else(|e| panic!("{kind:?}: {e:?}"));
        assert_eq!(effect.mode, "manual");
        assert_eq!(effect.port, Some(proxy.port()));
        assert_eq!(effect.user.as_deref(), Some("ann"));
        let d = crawl(&site, settings, false).await;
        d.assert_one_final_state();
        assert_eq!(proxy.denied(), 0, "{kind:?}");
        assert!(proxy.paths_to(E).iter().any(|p| p == "/a"), "{kind:?}");
    }
}

/// FR-022: a wrong password is reported as rejected by the proxy (407), and
/// nothing reaches the site.
#[tokio::test]
async fn proxy_with_a_wrong_password_is_rejected() {
    let site = site();
    let proxy = ProxyServer::builder()
        .routes(site.resolve())
        .require_credentials("ann", "pw")
        .start();
    let access = AccessSettings {
        proxy: ProxySettings {
            user: Some("ann".into()),
            password: Secret::new("wrong"),
            ..manual(ProxyKind::Http, &proxy)
        },
        ..AccessSettings::default()
    };
    let err = test_proxy(&access, &CheckOptions::default(), &probe(&site))
        .await
        .err()
        .expect("the proxy refuses the password");
    assert_eq!(err, ProxyProblem::Rejected);
    assert!(proxy.denied() >= 1);
    assert!(site.hits().is_empty(), "{:?}", site.hits());
}

/// A SOCKS5 proxy that refuses the credentials fails the test as well; the
/// handshake error is not a 407, so it is not necessarily "rejected".
#[tokio::test]
async fn socks5_proxy_with_a_wrong_password_fails() {
    let site = site();
    let proxy = ProxyServer::builder()
        .routes(site.resolve())
        .require_credentials("ann", "pw")
        .start();
    let access = AccessSettings {
        proxy: ProxySettings {
            user: Some("ann".into()),
            password: Secret::new("wrong"),
            ..manual(ProxyKind::Socks5, &proxy)
        },
        ..AccessSettings::default()
    };
    let result = test_proxy(&access, &CheckOptions::default(), &probe(&site)).await;
    assert!(result.is_err());
    assert!(proxy.denied() >= 1);
    assert!(site.hits().is_empty(), "{:?}", site.hits());
}

/// FR-022: a proxy nobody listens on is reported as unreachable.
#[tokio::test]
async fn unreachable_proxy_is_reported() {
    let site = site();
    for kind in [ProxyKind::Http, ProxyKind::Socks5] {
        let access = AccessSettings {
            proxy: ProxySettings {
                mode: ProxyMode::Manual,
                kind,
                host: "127.0.0.1".into(),
                port: closed_port(),
                ..ProxySettings::default()
            },
            ..AccessSettings::default()
        };
        let err = test_proxy(&access, &CheckOptions::default(), &probe(&site))
            .await
            .err()
            .expect("nothing listens there");
        assert!(matches!(err, ProxyProblem::Unreachable(_)), "{kind:?}: {err:?}");
    }
}

/// FR-023: hosts on the bypass list are reached directly, the others go
/// through the proxy.
#[tokio::test]
async fn bypass_list_host_goes_direct() {
    let site = site();
    let proxy = ProxyServer::start(site.resolve());
    let settings = ProxySettings {
        bypass: vec![D.into()],
        ..manual(ProxyKind::Http, &proxy)
    };
    // Only the bypassed host needs DNS overrides; example.test goes via the proxy.
    let d = crawl(&site, settings, true).await;
    d.assert_one_final_state();
    assert_eq!(proxy.requests_to(D), 0, "{:?}", proxy.hits());
    assert_eq!(proxy.tunnels_to(D), 0);
    assert!(
        site.hits().iter().any(|h| h.host == D && h.path == "/d"),
        "the bypassed host was not reached: {:?}",
        site.hits()
    );
    assert!(proxy.requests_to(E) >= 4, "{:?}", proxy.hits());
}
