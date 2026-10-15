//! Spec 011 User Story 5: XML sitemaps generated from a crawl (T021 to
//! T023). Every generated file is checked by the structural validator on
//! all OSes, and by `xmllint` against the protocol schemas where it is
//! installed (required in CI on Linux through SPIDERLENS_REQUIRE_XMLLINT).

use std::path::{Path, PathBuf};
use std::time::Duration;

use spiderlens_config::{
    Changefreq, CrawlConfig, ImageEntries, LastmodSource, ModeInput, PrioritySource, SitemapGenerateOptions,
    SitemapSettings, SpeedSettings,
};
use spiderlens_engine::crawl::sitemap::{Root, read};
use spiderlens_engine::crawl::{Control, Crawler};
use spiderlens_results::{GenerateError, Reader, SitemapOutput, check_sitemap_structure, generate_sitemaps};
use spiderlens_test_fixtures::{Site, SiteBuilder};
use tokio::sync::watch;

const E: &str = "example.test";
const CDN: &str = "cdn.test";

fn html(head: &str, body: &str) -> String {
    format!("<!doctype html><html><head><title>t</title>{head}</head><body>{body}</body></html>")
}

fn site() -> Site {
    let alternates = "<link rel=\"alternate\" hreflang=\"en\" href=\"/en/a\">\
                      <link rel=\"alternate\" hreflang=\"de\" href=\"/de/a\">\
                      <link rel=\"alternate\" hreflang=\"fr\" href=\"/fr/noindex\">\
                      <link rel=\"alternate\" hreflang=\"x-default\" href=\"/en/a\">";
    let home_links = [
        "/en/a",
        "/de/a",
        "/fr/noindex",
        "/canon-other",
        "/canon-self",
        "/moved",
        "/missing",
        "/blog/post",
        "/q?b=2&a=1",
    ]
    .iter()
    .map(|l| format!("<a href=\"{}\">x</a>", l.replace('&', "&amp;")))
    .collect::<String>();
    SiteBuilder::new()
        .html(
            E,
            "/",
            &html(
                "",
                &format!(
                    "<img src=\"/ok.png\"><img src=\"/broken.png\"><img src=\"{{base:{CDN}}}/cdn.png\">{home_links}"
                ),
            ),
        )
        .header(E, "/", "Last-Modified", "Wed, 21 Oct 2015 07:28:00 GMT")
        .html(E, "/en/a", &html(alternates, "<img src=\"/ok.png\">"))
        .html(E, "/de/a", &html(alternates, ""))
        .html(E, "/fr/noindex", &html("<meta name=\"robots\" content=\"noindex\">", ""))
        .html(E, "/canon-other", &html("<link rel=\"canonical\" href=\"/canon-self\">", ""))
        .html(E, "/canon-self", &html("<link rel=\"canonical\" href=\"/canon-self\">", ""))
        .redirect(E, "/moved", 301, "/en/a")
        .raw(E, "/missing", 404, "text/html", b"gone")
        .html(E, "/blog/post", &html("", ""))
        .html(E, "/q?b=2&a=1", &html("", ""))
        .raw(E, "/ok.png", 200, "image/png", b"png")
        .raw(E, "/broken.png", 404, "image/png", b"")
        .raw(CDN, "/cdn.png", 200, "image/png", b"png")
        .start()
}

struct Crawled {
    _dir: tempfile::TempDir,
    path: PathBuf,
    site: Site,
}

async fn crawl() -> Crawled {
    let site = site();
    let mut c = CrawlConfig {
        speed: SpeedSettings::FAST,
        ..CrawlConfig::default()
    };
    c.scope.sitemaps = SitemapSettings::all(false);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("gen.spiderlens");
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
    Crawled {
        _dir: dir,
        path,
        site,
    }
}

/// Every page URL listed in the generated files (via the index when there
/// is one), after checking each file.
fn read_back(dir: &Path, first: &str) -> Vec<String> {
    let bytes = std::fs::read(dir.join(first)).unwrap();
    validate(&dir.join(first), &bytes);
    let mut out = Vec::new();
    let facts = read(&bytes, |e| out.push(e.loc)).unwrap();
    if facts.root == Root::Urlset {
        return out;
    }
    let mut pages = Vec::new();
    for loc in out {
        let name = loc.rsplit('/').next().unwrap().to_string();
        let b = std::fs::read(dir.join(&name)).unwrap();
        validate(&dir.join(&name), &b);
        read(&b, |e| pages.push(e.loc)).unwrap();
    }
    pages
}

fn validate(path: &Path, bytes: &[u8]) {
    let plain = if bytes.starts_with(&[0x1f, 0x8b]) {
        use std::io::Read;
        let mut v = Vec::new();
        flate2::read::GzDecoder::new(bytes).read_to_end(&mut v).unwrap();
        v
    } else {
        bytes.to_vec()
    };
    check_sitemap_structure(&plain).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let xsd_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../specs/011-sitemaps-orphan-pages/xsd");
    let schema = if plain.windows(13).any(|w| w == b"<sitemapindex") {
        xsd_dir.join("siteindex.xsd")
    } else {
        xsd_dir.join("urlset-check.xsd")
    };
    let tmp = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(tmp.path(), &plain).unwrap();
    match std::process::Command::new("xmllint")
        .arg("--noout")
        .arg("--schema")
        .arg(&schema)
        .arg(tmp.path())
        .output()
    {
        Ok(o) => assert!(
            o.status.success(),
            "{} fails the schema: {}",
            path.display(),
            String::from_utf8_lossy(&o.stderr)
        ),
        Err(_) if std::env::var_os("SPIDERLENS_REQUIRE_XMLLINT").is_some() => panic!("xmllint is required"),
        Err(_) => {}
    }
}

fn opts() -> SitemapGenerateOptions {
    SitemapGenerateOptions::default()
}

#[tokio::test]
async fn us5_1_defaults_select_indexable_self_canonical_html_pages() {
    let c = crawl().await;
    let r = Reader::open(&c.path).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let report = generate_sitemaps(
        &r,
        &opts(),
        SitemapOutput::Folder {
            dir: dir.path(),
            replace: false,
        },
    )
    .unwrap();
    let mut got = read_back(dir.path(), "sitemap.xml");
    got.sort();
    let mut want: Vec<String> = ["/", "/en/a", "/de/a", "/canon-self", "/blog/post", "/q?b=2&a=1"]
        .iter()
        .map(|p| c.site.url(E, p))
        .collect();
    want.sort();
    assert_eq!(got, want);
    assert_eq!(report.pages, 6);
    assert_eq!(report.files.len(), 1);
    assert_eq!(report.files[0].name, "sitemap.xml");
    assert_eq!(report.base_url, format!("{}", c.site.url(E, "/")));
    // Entries are in URL order.
    let mut ordered = Vec::new();
    read(&std::fs::read(dir.path().join("sitemap.xml")).unwrap(), |e| {
        ordered.push(e.loc)
    })
    .unwrap();
    let mut sorted = ordered.clone();
    sorted.sort();
    assert_eq!(ordered, sorted);

    // Including noindex and canonicalised pages.
    let o = SitemapGenerateOptions {
        include_noindex: true,
        include_canonicalised: true,
        ..opts()
    };
    let report = generate_sitemaps(&r, &o, SitemapOutput::DryRun { dir: None }).unwrap();
    assert_eq!(report.pages, 8);
}

#[tokio::test]
async fn us5_2_split_by_count_with_an_index_at_the_base_url() {
    let c = crawl().await;
    let r = Reader::open(&c.path).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let o = SitemapGenerateOptions {
        max_urls_per_file: 4,
        base_url: "https://www.example.com/maps".into(),
        file_prefix: "site".into(),
        ..opts()
    };
    let report = generate_sitemaps(
        &r,
        &o,
        SitemapOutput::Folder {
            dir: dir.path(),
            replace: false,
        },
    )
    .unwrap();
    let names: Vec<_> = report.files.iter().map(|f| (f.name.as_str(), f.urls)).collect();
    assert_eq!(
        names,
        vec![("site-1.xml", 4), ("site-2.xml", 2), ("site_index.xml", 2)]
    );
    let index = std::fs::read_to_string(dir.path().join("site_index.xml")).unwrap();
    assert!(
        index.contains("<loc>https://www.example.com/maps/site-1.xml</loc>"),
        "{index}"
    );
    assert_eq!(read_back(dir.path(), "site_index.xml").len(), 6);
    assert!(
        std::fs::read_dir(dir.path()).unwrap().all(|e| !e
            .unwrap()
            .file_name()
            .to_string_lossy()
            .ends_with(".tmp")),
        "no temporary files are left"
    );
}

#[tokio::test]
async fn us5_3_images_inline_and_separate() {
    let c = crawl().await;
    let r = Reader::open(&c.path).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let o = SitemapGenerateOptions {
        images: ImageEntries::Inline,
        ..opts()
    };
    let report = generate_sitemaps(
        &r,
        &o,
        SitemapOutput::Folder {
            dir: dir.path(),
            replace: false,
        },
    )
    .unwrap();
    let xml = std::fs::read_to_string(dir.path().join("sitemap.xml")).unwrap();
    validate(&dir.path().join("sitemap.xml"), xml.as_bytes());
    let mut home = None;
    read(xml.as_bytes(), |e| {
        if e.loc == c.site.url(E, "/") {
            home = Some(e.images);
        }
    })
    .unwrap();
    assert_eq!(
        home.unwrap(),
        vec![c.site.url(E, "/ok.png")],
        "broken and other-host images left out"
    );
    assert_eq!(report.images, 2, "home and /en/a");

    let o = SitemapGenerateOptions {
        images: ImageEntries::Separate,
        image_hosts: vec![CDN.into()],
        ..opts()
    };
    let dir = tempfile::tempdir().unwrap();
    let report = generate_sitemaps(
        &r,
        &o,
        SitemapOutput::Folder {
            dir: dir.path(),
            replace: false,
        },
    )
    .unwrap();
    let names: Vec<_> = report.files.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(
        names,
        vec!["sitemap-pages.xml", "sitemap-images-1.xml", "sitemap_index.xml"]
    );
    let pages = std::fs::read_to_string(dir.path().join("sitemap-pages.xml")).unwrap();
    assert!(!pages.contains("image:"), "page sitemap has no images");
    let images = std::fs::read(dir.path().join("sitemap-images-1.xml")).unwrap();
    validate(Path::new("sitemap-images-1.xml"), &images);
    let mut home = None;
    read(&images, |e| {
        if e.loc == c.site.url(E, "/") {
            home = Some(e.images);
        }
    })
    .unwrap();
    let mut want = vec![c.site.url(CDN, "/cdn.png"), c.site.url(E, "/ok.png")];
    want.sort();
    assert_eq!(home.unwrap(), want);
}

#[tokio::test]
async fn us5_4_hreflang_lists_only_included_alternates() {
    let c = crawl().await;
    let r = Reader::open(&c.path).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let o = SitemapGenerateOptions {
        hreflang: true,
        ..opts()
    };
    generate_sitemaps(
        &r,
        &o,
        SitemapOutput::Folder {
            dir: dir.path(),
            replace: false,
        },
    )
    .unwrap();
    let bytes = std::fs::read(dir.path().join("sitemap.xml")).unwrap();
    validate(Path::new("sitemap.xml"), &bytes);
    let mut en = None;
    read(&bytes, |e| {
        if e.loc == c.site.url(E, "/en/a") {
            en = Some(e.alternates);
        }
    })
    .unwrap();
    let langs: Vec<_> = en.unwrap().into_iter().map(|a| (a.lang, a.href)).collect();
    assert_eq!(
        langs,
        vec![
            ("de".to_string(), c.site.url(E, "/de/a")),
            ("en".to_string(), c.site.url(E, "/en/a")),
            ("x-default".to_string(), c.site.url(E, "/en/a")),
        ],
        "the noindex French page is not written, so it is not an alternate"
    );
}

#[tokio::test]
async fn us5_5_lastmod_sources() {
    let c = crawl().await;
    let r = Reader::open(&c.path).unwrap();
    let lastmods = |o: &SitemapGenerateOptions| {
        let dir = tempfile::tempdir().unwrap();
        generate_sitemaps(
            &r,
            o,
            SitemapOutput::Folder {
                dir: dir.path(),
                replace: false,
            },
        )
        .unwrap();
        let bytes = std::fs::read(dir.path().join("sitemap.xml")).unwrap();
        validate(Path::new("sitemap.xml"), &bytes);
        let mut v = Vec::new();
        read(&bytes, |e| v.push((e.loc, e.lastmod))).unwrap();
        v
    };
    let v = lastmods(&opts());
    for (loc, lm) in &v {
        if *loc == c.site.url(E, "/") {
            assert_eq!(lm.as_deref(), Some("2015-10-21T07:28:00+00:00"));
        } else {
            assert_eq!(*lm, None, "{loc}: no header, no invented date");
        }
    }
    let v = lastmods(&SitemapGenerateOptions {
        lastmod: LastmodSource::Fixed("2026-10-01".into()),
        ..opts()
    });
    assert!(v.iter().all(|(_, l)| l.as_deref() == Some("2026-10-01")));
    let v = lastmods(&SitemapGenerateOptions {
        lastmod: LastmodSource::CrawlDate,
        ..opts()
    });
    assert!(
        v.iter()
            .all(|(_, l)| l.as_deref().is_some_and(|l| l.ends_with("+00:00")))
    );
    let v = lastmods(&SitemapGenerateOptions {
        lastmod: LastmodSource::None,
        changefreq: Some(Changefreq::Weekly),
        priority: PrioritySource::ByDepth,
        ..opts()
    });
    assert!(v.iter().all(|(_, l)| l.is_none()));
}

#[tokio::test]
async fn us5_6_patterns_and_us5_7_existing_files_and_byte_identical_output() {
    let c = crawl().await;
    let r = Reader::open(&c.path).unwrap();
    let o = SitemapGenerateOptions {
        exclude: vec!["/blog/".into()],
        ..opts()
    };
    let report = generate_sitemaps(&r, &o, SitemapOutput::DryRun { dir: None }).unwrap();
    assert_eq!((report.pages, report.skipped_by_pattern), (5, 1));
    let o = SitemapGenerateOptions {
        include: vec!["/nothing/".into()],
        ..opts()
    };
    assert!(matches!(
        generate_sitemaps(&r, &o, SitemapOutput::DryRun { dir: None }),
        Err(GenerateError::NoPages)
    ));

    for gzip in [false, true] {
        let o = SitemapGenerateOptions {
            gzip,
            max_urls_per_file: 4,
            images: ImageEntries::Inline,
            hreflang: true,
            ..opts()
        };
        let dir = tempfile::tempdir().unwrap();
        let first = generate_sitemaps(
            &r,
            &o,
            SitemapOutput::Folder {
                dir: dir.path(),
                replace: false,
            },
        )
        .unwrap();
        let snapshot: Vec<(String, Vec<u8>)> = first
            .files
            .iter()
            .map(|f| (f.name.clone(), std::fs::read(dir.path().join(&f.name)).unwrap()))
            .collect();
        let preview = generate_sitemaps(
            &r,
            &o,
            SitemapOutput::DryRun {
                dir: Some(dir.path()),
            },
        )
        .unwrap();
        assert_eq!(
            preview.existing.len(),
            first.files.len(),
            "the preview names the files it would replace"
        );
        match generate_sitemaps(
            &r,
            &o,
            SitemapOutput::Folder {
                dir: dir.path(),
                replace: false,
            },
        ) {
            Err(GenerateError::Exists(names)) => assert_eq!(names.len(), first.files.len()),
            other => panic!("expected Exists, got {other:?}"),
        }
        let again = generate_sitemaps(
            &r,
            &o,
            SitemapOutput::Folder {
                dir: dir.path(),
                replace: true,
            },
        )
        .unwrap();
        assert_eq!(again.files, first.files);
        assert_eq!(
            again.existing.len(),
            first.files.len(),
            "replacing reports what it replaced"
        );
        for (name, bytes) in snapshot {
            assert_eq!(
                std::fs::read(dir.path().join(&name)).unwrap(),
                bytes,
                "{name} is byte-identical (SC-006)"
            );
        }
        let index = if gzip {
            "sitemap_index.xml.gz"
        } else {
            "sitemap_index.xml"
        };
        assert_eq!(read_back(dir.path(), index).len(), 6);
    }
}
