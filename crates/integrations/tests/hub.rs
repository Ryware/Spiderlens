//! The shared entry point (`Hub`) the app and the CLI use: connect, settings,
//! fetch, resume, forget, against fake Google services.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use spiderlens_config::{CrawlConfig, ModeInput, SpeedSettings};
use spiderlens_engine::crawl::{Control as CrawlControl, Crawler};
use spiderlens_integrations::credentials::{CredentialId, CredentialStore, MemoryStore};
use spiderlens_integrations::gsc::{Api, PageRequest};
use spiderlens_integrations::hub::Hub;
use spiderlens_integrations::oauth::{Client, Endpoints};
use spiderlens_integrations::run::Control;
use spiderlens_integrations::settings::{Folder, ProviderSettings};
use spiderlens_integrations::store::RunState;
use spiderlens_integrations::{Error, Provider};
use spiderlens_test_fixtures::providers::{FakeGoogle, GoogleData, GscRow};
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

fn hub(g: &FakeGoogle, dir: &Path, store: Arc<MemoryStore>) -> Hub {
    Hub::new(Folder::new(dir), store, Some(dir.join("cache.sqlite"))).with_google(
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
}

fn follow(url: &str) {
    // Stands in for the system browser.
    let url = url.to_string();
    tokio::spawn(async move {
        reqwest::get(url).await.unwrap();
    });
}

#[test]
fn connect_fetch_resume_and_forget() {
    let site = site();
    let dir = tempfile::tempdir().unwrap();
    let path = crawl(&site, dir.path());
    let g = FakeGoogle::start(GoogleData {
        email: "ana@example.com".into(),
        sites: vec![(site.url(HOST, "/"), "siteOwner".into())],
        rows: (0..3)
            .map(|i| GscRow {
                page: site.url(HOST, &format!("/p/{i}")),
                clicks: 5,
                impressions: 50,
                position: 3.0,
            })
            .collect(),
        ..GoogleData::default()
    });
    let store = Arc::new(MemoryStore::default());
    let h = hub(&g, dir.path(), store.clone());

    // Not connected yet.
    assert!(matches!(
        h.properties(None, Some(&path)),
        Err(Error::NoConnection(_))
    ));

    let c = h
        .connect_google(
            &["search_console"],
            Duration::from_secs(20),
            &Control::default(),
            follow,
        )
        .unwrap();
    assert_eq!(c.label, "ana@example.com");
    assert_eq!(c.scopes, vec!["search_console".to_string()]);
    assert!(
        store
            .get(&CredentialId::new("google", "ana@example.com"))
            .unwrap()
            .is_some()
    );
    // Nothing secret in the settings folder.
    let listing = std::fs::read_to_string(dir.path().join("integrations/connections.json")).unwrap();
    assert!(!listing.contains("refresh"));

    let status = h.status();
    let sc = status
        .providers
        .iter()
        .find(|p| p.provider == Provider::SearchConsole)
        .unwrap();
    assert!(sc.available);
    assert_eq!(sc.connections, vec!["ana@example.com".to_string()]);

    let property = h.suggested_property(None, &path).unwrap();
    assert_eq!(property, site.url(HOST, "/"));
    let req = PageRequest::last_28_days(property);

    // Off until the user turns it on (FR-001).
    assert!(
        h.fetch_search_console(&path, None, &req, &Control::default(), &mut |_| {})
            .is_err()
    );
    h.save_settings(
        Provider::SearchConsole,
        ProviderSettings {
            enabled: true,
            ..ProviderSettings::default()
        },
    )
    .unwrap();

    // Paused by the user before the first page, then resumed.
    let control = Control::default();
    control.pause();
    let p = h
        .fetch_search_console(&path, None, &req, &control, &mut |_| {})
        .unwrap();
    assert_eq!(p.state, RunState::PausedUser);
    let p = h.resume(&path, None, &Control::default(), &mut |_| {}).unwrap();
    assert_eq!(p.state, RunState::Finished);
    assert_eq!(p.matched, Some(3));
    assert_eq!(h.runs(&path).unwrap().len(), 1);

    h.remove(&path, Some(Provider::SearchConsole)).unwrap();
    assert!(h.runs(&path).unwrap().is_empty());

    assert!(h.forget("google", "ana@example.com").unwrap());
    assert_eq!(g.revoked().len(), 1);
    assert!(
        store
            .get(&CredentialId::new("google", "ana@example.com"))
            .unwrap()
            .is_none()
    );
    assert!(h.status().connections.is_empty());
}

#[test]
fn keys_need_a_name_and_google_providers_refuse_keys() {
    let dir = tempfile::tempdir().unwrap();
    let g = FakeGoogle::start(GoogleData::default());
    let store = Arc::new(MemoryStore::default());
    let h = hub(&g, dir.path(), store.clone());
    assert!(h.save_key(Provider::SearchConsole, "x", "k".into()).is_err());
    assert!(h.save_key(Provider::Ahrefs, " ", "k".into()).is_err());
    let c = h.save_key(Provider::Ahrefs, "Agency", "k-1".into()).unwrap();
    assert_eq!(c.kind, "ahrefs");
    assert!(
        !std::fs::read_to_string(dir.path().join("integrations/connections.json"))
            .unwrap()
            .contains("k-1")
    );
    assert!(!h.forget("ahrefs", "Agency").unwrap());
    assert!(
        store
            .get(&CredentialId::new("ahrefs", "Agency"))
            .unwrap()
            .is_none()
    );
}
