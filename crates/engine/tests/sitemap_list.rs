//! A sitemap as the source of a List mode list (spec 011 FR-005, T014).

use spiderlens_config::CrawlConfig;
use spiderlens_engine::crawl::sitemap_list::{SitemapListSource, list_from_sitemap_resolving};
use spiderlens_test_fixtures::SiteBuilder;

const E: &str = "example.test";

fn site() -> spiderlens_test_fixtures::Site {
    let index = "<sitemapindex><sitemap><loc>{base:example.test}/s1.xml</loc></sitemap>\
                 <sitemap><loc>/s2.xml.gz</loc></sitemap>\
                 <sitemap><loc>{base:example.test}/index.xml</loc></sitemap>\
                 <sitemap><loc>{base:example.test}/broken.xml</loc></sitemap></sitemapindex>";
    let s1 = "<urlset><url><loc>{base:example.test}/a</loc></url><url><loc>/rel</loc></url>\
              <url><loc>http://</loc></url></urlset>";
    let s2 = "<urlset><url><loc>{base:example.test}/a</loc></url><url><loc>{base:example.test}/b</loc></url></urlset>";
    SiteBuilder::new()
        .raw(E, "/index.xml", 200, "application/xml", index.as_bytes())
        .raw(E, "/s1.xml", 200, "application/xml", s1.as_bytes())
        .gzip(E, "/s2.xml.gz", "application/gzip", s2)
        .raw(E, "/broken.xml", 200, "application/xml", b"<urlset><url>")
        .start()
}

#[tokio::test]
async fn index_is_walked_once_per_file_and_entries_become_the_list() {
    let site = site();
    let source = SitemapListSource::new(&site.url(E, "/index.xml"), None).unwrap();
    let list = list_from_sitemap_resolving(source, &CrawlConfig::default(), &site.resolve())
        .await
        .unwrap();
    assert_eq!(
        list.urls,
        vec![site.url(E, "/a"), site.url(E, "/rel"), site.url(E, "/b")]
    );
    assert_eq!(list.duplicates, 1);
    assert_eq!(list.invalid_count, 1);
    assert_eq!(list.sitemap, Some(site.url(E, "/index.xml")));
    assert!(
        list.notices[0].starts_with("Read 3 sitemap files (1 index)."),
        "{:?}",
        list.notices
    );
    assert!(
        list.notices.iter().any(|n| n.contains("/broken.xml")),
        "{:?}",
        list.notices
    );
    assert_eq!(
        site.count(E, "/index.xml"),
        1,
        "the loop back to the index is not followed"
    );
}

#[tokio::test]
async fn local_file_resolves_relative_addresses_against_the_base() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("sitemap.xml");
    std::fs::write(
        &path,
        "<urlset><url><loc>/one</loc></url><url><loc>https://other.test/two</loc></url></urlset>",
    )
    .unwrap();
    let source = SitemapListSource::new(path.to_str().unwrap(), Some("https://example.test/")).unwrap();
    let list = list_from_sitemap_resolving(source, &CrawlConfig::default(), &[])
        .await
        .unwrap();
    assert_eq!(
        list.urls,
        vec!["https://example.test/one", "https://other.test/two"]
    );
}

#[tokio::test]
async fn no_readable_file_is_an_error() {
    let source = SitemapListSource::new("/no/such/sitemap.xml", None).unwrap();
    let err = list_from_sitemap_resolving(source, &CrawlConfig::default(), &[])
        .await
        .unwrap_err();
    assert!(err.contains("could not read"), "{err}");
}
