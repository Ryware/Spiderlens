//! Spec 016: forms, actions and agent links are stored with each page.

use std::path::PathBuf;
use std::time::Duration;

use spiderlens_config::{CrawlConfig, ModeInput, SpeedSettings};
use spiderlens_engine::crawl::{Control, CrawlStore, Crawler};
use spiderlens_test_fixtures::{Site, SiteBuilder};
use tokio::sync::watch;

const E: &str = "example.test";

const CONTACT: &str = r##"<!doctype html><html><head><title>Contact</title>
<link rel="alternate" type="text/markdown" href="/contact.md"></head><body>
<form action="/send" method="post" toolname="contact" tooldescription="Send us a message">
  <label for="email">Email</label><input id="email" name="email" type="email" required>
  <input name="phone" placeholder="Phone">
  <textarea name="msg"></textarea>
  <div class="cf-turnstile"></div>
  <button><svg></svg></button>
</form>
<form role="search" action="/search"><input name="q" aria-label="Search"></form>
<a href="#" onclick="menu()">Menu</a><div onclick="go()">Go</div>
</body></html>"##;

async fn crawl(site: &Site, start: &str) -> (tempfile::TempDir, PathBuf) {
    let c = CrawlConfig {
        speed: SpeedSettings::FAST,
        ..CrawlConfig::default()
    };
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("t.spiderlens");
    let input = ModeInput::Spider {
        start_url: site.url(E, start),
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

#[tokio::test]
#[allow(clippy::type_complexity)]
async fn forms_actions_and_links_are_stored() {
    let site = SiteBuilder::new()
        .html(E, "/contact", CONTACT)
        .header(E, "/contact", "Link", r#"</openapi.json>; rel="service-desc""#)
        .start();
    let (_dir, path) = crawl(&site, "/contact").await;
    let store = CrawlStore::open(&path).unwrap();
    let conn = store.conn_for_tests();
    let id: i64 = conn
        .query_row(
            "SELECT id FROM urls WHERE url = ?1",
            [site.url(E, "/contact")],
            |r| r.get(0),
        )
        .unwrap();

    let summary: (
        i64,
        i64,
        i64,
        i64,
        i64,
        i64,
        i64,
        i64,
        i64,
        i64,
        i64,
        Option<String>,
    ) = conn
        .query_row(
            "SELECT forms, search_forms, fields, unlabelled_fields, placeholder_only_fields,
             nameless_buttons, actions_without_link, challenges, forms_without_submit, declared_tools,
             tools_missing_description, md_alternate FROM page_agent WHERE url_id = ?1 AND source = 'raw'",
            [id],
            |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                    r.get(6)?,
                    r.get(7)?,
                    r.get(8)?,
                    r.get(9)?,
                    r.get(10)?,
                    r.get(11)?,
                ))
            },
        )
        .unwrap();
    // 2 forms (one search), 4 fields (msg unlabelled, phone placeholder only),
    // 1 nameless button, 2 actions, 1 challenge, the search form has no
    // submit control, 1 tool whose fields lack parameter descriptions.
    assert_eq!(
        summary,
        (2, 1, 4, 1, 1, 1, 2, 1, 1, 1, 1, Some("/contact.md".into()))
    );

    let challenge: Option<String> = conn
        .query_row(
            "SELECT challenge FROM page_forms WHERE url_id = ?1 AND pos = 0",
            [id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(challenge.as_deref(), Some("turnstile"));

    let email: (String, String, bool) = conn
        .query_row(
            "SELECT accessible_name, name_source, required FROM form_fields
             WHERE url_id = ?1 AND name = 'email'",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!(email, ("Email".into(), "label".into(), true));

    let links: Vec<(String, String, String)> = conn
        .prepare("SELECT kind, href, via FROM agent_links WHERE url_id = ?1 ORDER BY pos")
        .unwrap()
        .query_map([id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(
        links,
        [
            ("markdown_alternate".into(), "/contact.md".into(), "html".into()),
            ("service_desc".into(), "/openapi.json".into(), "header".into()),
        ]
    );
    let features: String = conn
        .query_row("SELECT value FROM meta WHERE key = 'data_features'", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert!(features.split(',').any(|f| f == "agents"), "{features}");
}
