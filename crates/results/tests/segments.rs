//! Spec 027 part B: segments on a real crawl. Membership, first match wins,
//! the Segment column, the segment filter on views and counts, and editing
//! segments on an open crawl.

use std::path::PathBuf;
use std::time::Duration;

use spiderlens_config::{
    Clause, CrawlConfig, ModeInput, Segment, SegmentField, SegmentMatch, SegmentOp, SpeedSettings,
};
use spiderlens_engine::crawl::{Control, Crawler};
use spiderlens_results::{Reader, UrlScope, ViewQuery};
use spiderlens_test_fixtures::{Site, SiteBuilder};
use tokio::sync::watch;

const E: &str = "example.test";

fn seg(id: u32, name: &str, mode: SegmentMatch, clauses: &[(SegmentField, SegmentOp, &str)]) -> Segment {
    Segment {
        id,
        name: name.into(),
        mode,
        clauses: clauses
            .iter()
            .map(|&(field, op, value)| Clause {
                field,
                op,
                value: value.into(),
            })
            .collect(),
        ..Segment::default()
    }
}

fn segments() -> Vec<Segment> {
    use SegmentField::*;
    use SegmentOp::*;
    vec![
        seg(1, "Blog", SegmentMatch::All, &[(Path, StartsWith, "/blog/")]),
        seg(
            2,
            "Shop",
            SegmentMatch::Any,
            &[(Path, StartsWith, "/shop/"), (Path, Equals, "/cart")],
        ),
        // After Blog: /blog/missing stays in Blog (first match wins).
        seg(3, "Errors", SegmentMatch::All, &[(StatusClass, Equals, "4xx")]),
    ]
}

struct Crawled {
    _dir: tempfile::TempDir,
    path: PathBuf,
    site: Site,
}

async fn crawl(segments: Vec<Segment>) -> Crawled {
    let site = SiteBuilder::new()
        .page(
            E,
            "/",
            "",
            &[
                "/blog/a",
                "/blog/b",
                "/blog/missing",
                "/shop/x",
                "/cart",
                "/about",
                "/gone",
            ],
        )
        .page(E, "/blog/a", "", &[])
        .page(E, "/blog/b", "", &[])
        .page(E, "/shop/x", "", &[])
        .page(E, "/cart", "", &[])
        .page(E, "/about", "", &[])
        .start();
    let c = CrawlConfig {
        speed: SpeedSettings::FAST,
        segments,
        ..CrawlConfig::default()
    };
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("segments.spiderlens");
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

fn q(segment: Option<u32>) -> ViewQuery {
    ViewQuery {
        tab: "internal".into(),
        filter: "all".into(),
        scope: UrlScope::All,
        search: String::new(),
        sort: None,
        segment,
        values: None,
    }
}

/// Path → Segment column value, for a view.
fn rows(r: &mut Reader, c: &Crawled, segment: Option<u32>) -> Vec<(String, Option<String>)> {
    let page = r.open_view(&q(segment), 500).unwrap();
    let base = c.site.url(E, "");
    // Row values follow the id, in the tab's column order.
    let seg = 1 + spiderlens_results::tab("internal")
        .unwrap()
        .columns
        .iter()
        .position(|c| c.id == "segment")
        .unwrap();
    let mut out: Vec<_> = page
        .rows
        .iter()
        .map(|row| {
            let url = row[1].as_str().unwrap();
            let path = url
                .strip_prefix(base.trim_end_matches('/'))
                .unwrap_or(url)
                .to_string();
            (path, row[seg].as_str().map(str::to_string))
        })
        .collect();
    out.sort();
    out
}

fn pairs(v: &[(&str, Option<&str>)]) -> Vec<(String, Option<String>)> {
    let mut v: Vec<_> = v
        .iter()
        .map(|(p, s)| (p.to_string(), s.map(str::to_string)))
        .collect();
    v.sort();
    v
}

#[tokio::test]
async fn segments_from_the_crawl_settings_fill_the_column_and_filter() {
    let c = crawl(segments()).await;
    let mut r = Reader::open(&c.path).unwrap();
    assert_eq!(r.segments().len(), 3);
    let all = rows(&mut r, &c, None);
    assert_eq!(
        all,
        pairs(&[
            ("/", None),
            ("/about", None),
            ("/blog/a", Some("Blog")),
            ("/blog/b", Some("Blog")),
            ("/blog/missing", Some("Blog")),
            ("/cart", Some("Shop")),
            ("/gone", Some("Errors")),
            ("/shop/x", Some("Shop")),
        ])
    );
    // The segment filter on views.
    let blog = rows(&mut r, &c, Some(1));
    assert_eq!(blog.len(), 3);
    assert!(blog.iter().all(|(_, s)| s.as_deref() == Some("Blog")));
    let none = rows(&mut r, &c, Some(0));
    assert_eq!(none, pairs(&[("/", None), ("/about", None)]));
    // Counts: per segment and per filter within a segment.
    let counts = r.segment_counts().unwrap();
    assert_eq!(counts.counts.get(&1), Some(&3));
    assert_eq!(counts.counts.get(&2), Some(&2));
    assert_eq!(counts.counts.get(&3), Some(&1));
    assert_eq!(counts.no_segment, 2);
    let f = r.counts_in("internal", UrlScope::All, Some(2)).unwrap();
    assert_eq!(f.counts["all"], Some(2));
    let f = r.counts_in("response-codes", UrlScope::All, Some(1)).unwrap();
    assert_eq!(f.counts["all"], Some(3));
    assert_eq!(r.segment_id("blog"), Some(1));
    assert_eq!(r.segment_id("nope"), None);
}

#[tokio::test]
async fn editing_segments_rebuilds_membership_and_order_decides() {
    let c = crawl(Vec::new()).await;
    let mut r = Reader::open(&c.path).unwrap();
    assert!(r.segments().is_empty());
    assert!(rows(&mut r, &c, None).iter().all(|(_, s)| s.is_none()));

    // Errors first: now /blog/missing is an error.
    let mut s = segments();
    s.rotate_right(1);
    let counts = r.set_segments(s.clone()).unwrap();
    assert_eq!(counts.counts.get(&3), Some(&2));
    assert_eq!(counts.counts.get(&1), Some(&2));
    let all = rows(&mut r, &c, None);
    assert!(all.contains(&("/blog/missing".into(), Some("Errors".into()))));

    // A disabled segment groups nothing and its URLs fall through.
    s[0].enabled = false;
    let counts = r.set_segments(s).unwrap();
    assert!(!counts.counts.contains_key(&3));
    let all = rows(&mut r, &c, None);
    assert!(all.contains(&("/blog/missing".into(), Some("Blog".into()))));
    assert!(all.contains(&("/gone".into(), None)));
}

#[tokio::test]
async fn page_facts_segment_by_word_count_and_indexability() {
    use SegmentField::*;
    use SegmentOp::*;
    let s = vec![seg(
        7,
        "Indexable HTML",
        SegmentMatch::All,
        &[
            (Indexability, Equals, "indexable"),
            (ContentKind, Equals, "html"),
            (WordCount, Ge, "0"),
        ],
    )];
    let c = crawl(s).await;
    let mut r = Reader::open(&c.path).unwrap();
    let all = rows(&mut r, &c, None);
    for (path, s) in &all {
        let want = !matches!(path.as_str(), "/blog/missing" | "/gone");
        assert_eq!(s.is_some(), want, "{path}");
    }
}

#[tokio::test]
async fn saved_segments_replace_the_crawl_settings_ones() {
    use SegmentField::*;
    use SegmentOp::*;
    let c = crawl(segments()).await;
    let shop_only = vec![seg(9, "Shop", SegmentMatch::All, &[(Path, StartsWith, "/shop/")])];
    spiderlens_engine::crawl::file::save_segments(&c.path, &shop_only).unwrap();
    let mut r = Reader::open(&c.path).unwrap();
    assert_eq!(r.segments(), shop_only.as_slice());
    let counts = r.segment_counts().unwrap();
    assert_eq!(counts.counts.get(&9), Some(&1));
    assert_eq!(counts.no_segment, 7);

    // A crawl file someone is writing is not changed.
    let _lock = spiderlens_engine::crawl::WriterLock::acquire(&c.path).unwrap();
    assert!(spiderlens_engine::crawl::file::save_segments(&c.path, &[]).is_err());
}
