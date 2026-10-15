//! Spec 011 User Story 3: the Sitemap files view, newest and older files.

use std::path::PathBuf;
use std::time::Duration;

use spiderlens_config::{CrawlConfig, ModeInput, SpeedSettings};
use spiderlens_engine::crawl::{Control, Crawler};
use spiderlens_results::Reader;
use spiderlens_test_fixtures::{Site, SiteBuilder};
use tokio::sync::watch;

const E: &str = "example.test";

fn site() -> Site {
    let index = "<sitemapindex><sitemap><loc>{base:example.test}/a.xml</loc></sitemap>\
                 <sitemap><loc>{base:example.test}/b.xml.gz</loc></sitemap>\
                 <sitemap><loc>{base:example.test}/gone.xml</loc></sitemap></sitemapindex>";
    let a =
        "<urlset><url><loc>{base:example.test}/</loc></url><url><loc>javascript:void(0)</loc></url></urlset>";
    let b = "<urlset><url><loc>{base:example.test}/b</loc></url></urlset>";
    SiteBuilder::new()
        .robots(
            E,
            "User-agent: *\nAllow: /\nSitemap: {base:example.test}/index.xml\n",
        )
        .page(E, "/", "", &[])
        .page(E, "/b", "", &[])
        .raw(E, "/index.xml", 200, "application/xml", index.as_bytes())
        .raw(E, "/a.xml", 200, "application/xml", a.as_bytes())
        .gzip(E, "/b.xml.gz", "application/gzip", b)
        .raw(E, "/gone.xml", 404, "text/html", b"nope")
        .start()
}

async fn crawl(site: &Site, dir: &tempfile::TempDir) -> PathBuf {
    let c = CrawlConfig {
        speed: SpeedSettings::FAST,
        ..CrawlConfig::default()
    };
    let path = dir.path().join("s.spiderlens");
    let input = ModeInput::Spider {
        start_url: site.url(E, "/"),
    };
    let crawler = Crawler::create(&path, c, &input)
        .unwrap()
        .with_resolve(site.resolve())
        .with_retry_base(Duration::from_millis(10));
    let (_tx, rx) = watch::channel(Control::Run);
    tokio::time::timeout(Duration::from_secs(60), crawler.run(rx, None))
        .await
        .expect("crawl finishes")
        .unwrap();
    path
}

#[tokio::test]
async fn us3_files_are_a_tree_with_their_facts() {
    let site = site();
    let dir = tempfile::tempdir().unwrap();
    let path = crawl(&site, &dir).await;
    let v = Reader::open(&path).unwrap().sitemap_files().unwrap();
    assert!(v.available);
    let rows: Vec<(String, u32, u32)> = v
        .files
        .iter()
        .map(|f| {
            (
                f.location.rsplit('/').next().unwrap().to_string(),
                f.level,
                f.children,
            )
        })
        .collect();
    assert_eq!(
        rows,
        [
            ("index.xml".to_string(), 0, 3),
            ("a.xml".to_string(), 1, 0),
            ("b.xml.gz".to_string(), 1, 0),
            ("gone.xml".to_string(), 1, 0),
        ]
    );
    let [index, a, b, gone] = &v.files[..] else {
        unreachable!()
    };
    assert_eq!(
        (index.source.as_deref(), index.kind.as_deref(), index.entries),
        (Some("robots"), Some("index"), 3)
    );
    assert_eq!((a.entries, a.invalid_entries, a.http_status), (2, 1, Some(200)));
    assert!(b.compressed);
    assert_eq!(gone.http_status, Some(404));
    assert!(gone.error_class.is_some() && gone.kind.is_none(), "{gone:?}");
}

#[tokio::test]
async fn us6_4_older_files_show_the_summary_rows() {
    let site = site();
    let dir = tempfile::tempdir().unwrap();
    let path = crawl(&site, &dir).await;
    {
        // What a crawl file from before spec 011 holds.
        let conn = rusqlite::Connection::open(&path).unwrap();
        conn.execute_batch(
            "DELETE FROM sitemap_files; UPDATE meta SET value = replace(value, ',sitemap_entries', '')
             WHERE key = 'data_features';",
        )
        .unwrap();
    }
    let v = Reader::open(&path).unwrap().sitemap_files().unwrap();
    assert!(!v.available);
    assert_eq!(v.files.len(), 4);
    assert_eq!((v.files[0].level, v.files[0].children), (0, 3));
    assert!(v.files[3].error.is_some());
    assert!(v.files.iter().all(|f| f.source.is_none()));
}
