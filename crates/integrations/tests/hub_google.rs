//! The shared entry point (`Hub`) for URL Inspection and Analytics (spec 024
//! slices 2 and 4): both off until turned on, the plan against the daily
//! allowance, pause and resume through `Hub::resume`, and removal.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use spiderlens_config::{CrawlConfig, ModeInput, SpeedSettings};
use spiderlens_engine::crawl::{Control as CrawlControl, Crawler};
use spiderlens_integrations::credentials::MemoryStore;
use spiderlens_integrations::ga4::{Ga4Api, Ga4Request};
use spiderlens_integrations::gsc::Api;
use spiderlens_integrations::hub::{GoogleApis, Hub};
use spiderlens_integrations::inspect::{InspectApi, InspectRequest};
use spiderlens_integrations::oauth::{Client, Endpoints};
use spiderlens_integrations::run::Control;
use spiderlens_integrations::settings::{Folder, ProviderSettings};
use spiderlens_integrations::store::RunState;
use spiderlens_integrations::{Error, Provider};
use spiderlens_test_fixtures::google_apis::{
    AnalyticsData, FakeAnalytics, FakeInspection, GaProperty, GaRow, GaStream,
};
use spiderlens_test_fixtures::providers::{FakeGoogle, GoogleData};
use spiderlens_test_fixtures::{Site, SiteBuilder};
use tokio::sync::watch;

const HOST: &str = "127.0.0.1";

fn site() -> Site {
    let mut b = SiteBuilder::new();
    for i in 0..5 {
        b = b.page(HOST, &format!("/p/{i}"), "", &[&format!("/p/{}", (i + 1) % 5)]);
    }
    b.start()
}

fn crawl(site: &Site, dir: &Path) -> PathBuf {
    let path = dir.join("c.spiderlens");
    let crawler = Crawler::create(
        &path,
        CrawlConfig {
            speed: SpeedSettings::FAST,
            ..CrawlConfig::default()
        },
        &ModeInput::Spider {
            start_url: site.url(HOST, "/p/0"),
        },
    )
    .unwrap();
    let (_tx, rx) = watch::channel(CrawlControl::Run);
    tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(crawler.run(rx, None))
        .unwrap();
    path
}

fn follow(url: &str) {
    // Stands in for the system browser.
    let url = url.to_string();
    tokio::spawn(async move {
        reqwest::get(url).await.unwrap();
    });
}

fn enable(h: &Hub, p: Provider) {
    h.save_settings(
        p,
        ProviderSettings {
            enabled: true,
            ..ProviderSettings::default()
        },
    )
    .unwrap();
}

#[test]
fn inspection_and_analytics_through_the_hub() {
    let site = site();
    let dir = tempfile::tempdir().unwrap();
    let path = crawl(&site, dir.path());
    let g = FakeGoogle::start(GoogleData {
        email: "ana@example.com".into(),
        ..GoogleData::default()
    });
    let insp = FakeInspection::start(vec![site.url(HOST, "/")], 2000);
    let ga = FakeAnalytics::start(AnalyticsData {
        properties: vec![GaProperty {
            id: "123".into(),
            name: "Shop".into(),
            streams: vec![GaStream::web("456", "Website", &format!("http://{HOST}"))],
        }],
        rows: vec![GaRow::new(
            "456",
            &format!("{HOST}:{}", site.port()),
            "/p/1",
            9,
            5,
        )],
        ..AnalyticsData::default()
    });
    let h = Hub::new(
        Folder::new(dir.path()),
        Arc::new(MemoryStore::default()),
        Some(dir.path().join("cache.sqlite")),
    )
    .with_google(
        Endpoints {
            auth: g.auth_url(),
            token: g.token_url(),
            revoke: g.revoke_url(),
        },
        Api {
            base: g.webmasters_url(),
        },
        Some(Client {
            id: "test-client".into(),
            secret: None,
        }),
    )
    .with_google_apis(GoogleApis {
        inspection: InspectApi { base: insp.base() },
        analytics: Ga4Api {
            admin_base: ga.admin_base(),
            data_base: ga.data_base(),
        },
    });
    let req = InspectRequest::indexable(site.url(HOST, "/"));

    // Both are in this build, but nothing is connected or on yet.
    let status = h.status();
    for p in [Provider::UrlInspection, Provider::Analytics] {
        assert!(
            status
                .providers
                .iter()
                .find(|s| s.provider == p)
                .unwrap()
                .available
        );
    }
    assert!(matches!(
        h.analytics_streams(None, Some(&path)),
        Err(Error::NoConnection(_))
    ));
    let c = h
        .connect_google(
            &["search_console", "analytics"],
            Duration::from_secs(20),
            &Control::default(),
            follow,
        )
        .unwrap();
    assert_eq!(
        c.scopes,
        vec!["search_console".to_string(), "analytics".to_string()]
    );
    let status = h.status();
    for p in [Provider::UrlInspection, Provider::Analytics] {
        let s = status.providers.iter().find(|s| s.provider == p).unwrap();
        assert_eq!(s.connections, vec!["ana@example.com".to_string()]);
    }

    // Off until the user turns them on (FR-001).
    assert!(
        h.fetch_inspection(&path, None, &req, &Control::default(), &mut |_| {})
            .is_err()
    );
    enable(&h, Provider::UrlInspection);
    enable(&h, Provider::Analytics);

    // The plan shows the count against the day's allowance.
    let plan = h.inspection_plan(&path, None, &req).unwrap();
    assert_eq!(
        (plan.selected, plan.remaining_today, plan.days_needed),
        (5, 2000, 1)
    );

    // Paused by the user before the first URL, then resumed through the hub.
    let control = Control::default();
    control.pause();
    let p = h
        .fetch_inspection(&path, None, &req, &control, &mut |_| {})
        .unwrap();
    assert_eq!(p.state, RunState::PausedUser);
    let p = h.resume(&path, None, &Control::default(), &mut |_| {}).unwrap();
    assert_eq!(p.provider, Provider::UrlInspection);
    assert_eq!(p.state, RunState::Finished);
    assert_eq!(p.done, 5);
    let plan = h.inspection_plan(&path, None, &req).unwrap();
    assert_eq!((plan.used_today, plan.remaining_today), (5, 1995));

    // Analytics: the stream for the crawled site is suggested.
    let streams = h.analytics_streams(None, Some(&path)).unwrap();
    assert!(streams[0].suggested);
    let s = h.suggested_stream(None, &path).unwrap();
    assert_eq!(
        (s.property.as_str(), s.stream.as_str()),
        ("properties/123", "456")
    );
    let areq = Ga4Request::last_28_days(s.property, s.stream);
    let control = Control::default();
    control.pause();
    let p = h
        .fetch_analytics(&path, None, &areq, &control, &mut |_| {})
        .unwrap();
    assert_eq!(p.state, RunState::PausedUser);
    let p = h.resume(&path, None, &Control::default(), &mut |_| {}).unwrap();
    assert_eq!(p.provider, Provider::Analytics);
    assert_eq!(p.state, RunState::Finished);
    assert_eq!(p.matched, Some(1));
    assert_eq!(h.runs(&path).unwrap().len(), 2);

    h.remove(&path, Some(Provider::UrlInspection)).unwrap();
    assert_eq!(h.runs(&path).unwrap().len(), 1);
    h.remove(&path, Some(Provider::Analytics)).unwrap();
    assert!(h.runs(&path).unwrap().is_empty());
}
