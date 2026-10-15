//! Spec 020 end-to-end: crawl the near duplicate fixture site, run the
//! analysis pass, check pages, clusters, pairs and the comparison.

use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use rusqlite::Connection;
use spiderlens_config::{ContentSettings, CrawlConfig, ModeInput, SpeedSettings};
use spiderlens_engine::crawl::{Control, Crawler};
use spiderlens_results::near_dup::{self, DiffOp, NearDupState, Schema};
use spiderlens_test_fixtures::Site;
use spiderlens_test_fixtures::near_dup::{Expect, HOST, site};
use tokio::sync::watch;

struct Crawled {
    _dir: tempfile::TempDir,
    path: PathBuf,
    site: Site,
}

async fn crawl(content: ContentSettings) -> Crawled {
    let site = site();
    let c = CrawlConfig {
        speed: SpeedSettings::FAST,
        content,
        ..CrawlConfig::default()
    };
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("dup.spiderlens");
    let input = ModeInput::Spider {
        start_url: site.url(HOST, "/"),
    };
    let crawler = Crawler::create(&path, c, &input)
        .unwrap()
        .with_resolve(site.resolve());
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

fn id(conn: &Connection, c: &Crawled, path: &str) -> i64 {
    conn.query_row(
        "SELECT id FROM urls WHERE url = ?1",
        [c.site.url(HOST, path)],
        |r| r.get(0),
    )
    .unwrap_or_else(|_| panic!("{path} crawled"))
}

/// (status, closest_sim, near_count, cluster) for a path.
fn row(conn: &Connection, c: &Crawled, path: &str) -> (String, Option<u8>, u32, Option<u32>) {
    conn.query_row(
        "SELECT status, closest_sim, near_count, cluster FROM near_dup_pages WHERE url_id = ?1",
        [id(conn, c, path)],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
    )
    .unwrap()
}

fn analyse(conn: &Connection, s: &ContentSettings, schema: Schema) -> near_dup::PassSummary {
    near_dup::run_pass(conn, schema, s, &AtomicBool::new(false), |_, _, _| {}).unwrap()
}

#[tokio::test]
async fn us1_finds_near_duplicates_and_us2_clusters() {
    let settings = ContentSettings::default();
    let c = crawl(settings.clone()).await;
    let conn = Connection::open(&c.path).unwrap();
    assert_eq!(
        near_dup::status(&conn, &settings).unwrap().state,
        NearDupState::NotRun
    );
    let summary = analyse(&conn, &settings, Schema::Main);
    assert_eq!(summary.too_short, 2, "/short and the link-only home page");
    assert_eq!(summary.not_indexable, 2);

    // US1.2: different chrome, same content: 100% and exact.
    let (_, sim, _, cluster) = row(&conn, &c, Expect::EXACT[0]);
    assert_eq!(sim, Some(100));
    let shoes = cluster.expect("in a cluster");
    let pairs = near_dup::pairs(&conn, id(&conn, &c, Expect::EXACT[0])).unwrap();
    assert!(pairs.rows[0].exact && pairs.rows[0].sim == 100);
    // US1.1: one sentence changed.
    let (_, sim, n, cluster) = row(&conn, &c, Expect::CLOSE);
    assert!((90..100).contains(&sim.unwrap()), "{sim:?}");
    assert_eq!((n, cluster), (2, Some(shoes)));
    // US1.4: around 85%, shown as closest but not a near duplicate.
    let (_, sim, n, cluster) = row(&conn, &c, Expect::EIGHTY_FIVE);
    assert!((80..90).contains(&sim.unwrap()), "{sim:?}");
    assert_eq!((n, cluster), (0, None));
    // US1.3: same chrome, unrelated content.
    let (_, sim, n, _) = row(&conn, &c, Expect::UNRELATED);
    assert_eq!((sim, n), (None, 0));
    // US4.3, US4.4.
    assert_eq!(row(&conn, &c, Expect::SHORT).0, "too_short");
    assert_eq!(row(&conn, &c, Expect::NOINDEX).0, "not_indexable");
    assert_eq!(row(&conn, &c, Expect::CANONICAL).0, "not_indexable");

    // US2.1: clusters largest first: the 12 city pages, then the shoes.
    let clusters = near_dup::clusters(&conn, 0, 10).unwrap();
    assert_eq!(clusters.rows[0].size as usize, Expect::CITIES);
    assert!(clusters.rows.iter().any(|r| r.id == shoes && r.size == 3));
    // US2.2: a cluster's pages, representative first.
    let m = near_dup::members(&conn, clusters.rows[0].id, 0, 5).unwrap();
    assert_eq!(m.total as usize, Expect::CITIES);
    assert_eq!(m.rows.len(), 5);
    assert!(m.rows[0].representative && m.rows[0].url_id == clusters.rows[0].representative_id);
    assert!(m.rows[1..].iter().all(|r| !r.representative));
    assert_eq!(
        near_dup::status(&conn, &settings).unwrap().state,
        NearDupState::Done
    );

    // US1.5: a lower threshold, from stored data, adds the 85% page.
    let lower = ContentSettings {
        threshold: 80,
        ..settings.clone()
    };
    assert_eq!(
        near_dup::status(&conn, &lower).unwrap().state,
        NearDupState::Stale
    );
    analyse(&conn, &lower, Schema::Main);
    let (_, _, n, cluster) = row(&conn, &c, Expect::EIGHTY_FIVE);
    assert!(n >= 1 && cluster.is_some());

    // US2.3: the comparison marks only the changed words.
    let cmp = near_dup::compare(
        &conn,
        id(&conn, &c, Expect::EXACT[0]),
        id(&conn, &c, Expect::CLOSE),
        false,
        &settings,
    )
    .unwrap();
    let changed: Vec<String> = cmp
        .b
        .as_ref()
        .unwrap()
        .paragraphs
        .iter()
        .flat_map(|p| {
            p.spans
                .iter()
                .filter(|s| s.op == DiffOp::Changed)
                .map(|s| s.text.clone())
        })
        .collect();
    assert_eq!(
        changed
            .join(" ")
            .split(' ')
            .filter(|w| w.starts_with("green"))
            .count(),
        3
    );
    assert!(
        changed
            .iter()
            .all(|w| w.split(' ').all(|x| x.starts_with("green"))),
        "{changed:?}"
    );
    // US2.4: full text shows the left-out chrome, marked.
    let full = near_dup::compare(
        &conn,
        id(&conn, &c, "/shoe-red"),
        id(&conn, &c, "/shoe-blue"),
        true,
        &settings,
    )
    .unwrap();
    assert!(
        full.a
            .unwrap()
            .paragraphs
            .iter()
            .any(|p| p.spans.iter().any(|s| s.op == DiffOp::Excluded))
    );
}

#[tokio::test]
async fn us4_excluding_a_block_removes_the_pair() {
    let settings = ContentSettings::default();
    let c = crawl(settings.clone()).await;
    let conn = Connection::open(&c.path).unwrap();
    analyse(&conn, &settings, Schema::Main);
    // The shared related block makes the bags look alike.
    assert!(row(&conn, &c, Expect::RELATED[0]).2 >= 1);

    // A crawl that excludes the block (selectors apply at extraction).
    let excluded = ContentSettings {
        exclude_selectors: vec![".related-products".into()],
        ..settings
    };
    let c2 = crawl(excluded.clone()).await;
    let conn2 = Connection::open(&c2.path).unwrap();
    analyse(&conn2, &excluded, Schema::Main);
    assert_eq!(row(&conn2, &c2, Expect::RELATED[0]).2, 0);
}

#[tokio::test]
async fn read_only_files_analyse_into_temp() {
    let settings = ContentSettings::default();
    let c = crawl(settings.clone()).await;
    let ro = Connection::open_with_flags(&c.path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    analyse(&ro, &settings, Schema::Temp);
    let n: i64 = ro
        .query_row("SELECT COUNT(*) FROM temp.near_dup_clusters", [], |r| r.get(0))
        .unwrap();
    assert!(n >= 2);
    let main_rows: i64 = ro
        .query_row("SELECT COUNT(*) FROM main.near_dup_pages", [], |r| r.get(0))
        .unwrap_or(0);
    assert_eq!(main_rows, 0, "nothing written to the file");
    assert_eq!(
        near_dup::status(&ro, &settings).unwrap().state,
        NearDupState::Done
    );
}

#[tokio::test]
async fn content_tab_filters_match_the_analysis() {
    use spiderlens_results::{Reader, UrlScope, ViewQuery};
    let settings = ContentSettings::default();
    let c = crawl(settings.clone()).await;
    let conn = Connection::open(&c.path).unwrap();
    analyse(&conn, &settings, Schema::Main);
    let expected: u32 = conn
        .query_row(
            "SELECT COUNT(*) FROM near_dup_pages WHERE near_count > 0",
            [],
            |r| r.get(0),
        )
        .unwrap();
    drop(conn);
    let mut r = Reader::open(&c.path).unwrap();
    let count = |r: &mut Reader, filter: &str| {
        let q = ViewQuery {
            tab: "content".into(),
            filter: filter.into(),
            scope: UrlScope::All,
            search: String::new(),
            sort: None,
            values: None,
            segment: None,
        };
        let page = r.open_view(&q, 100).unwrap();
        let n = r.materialize(page.view).unwrap();
        r.close_view(page.view).unwrap();
        n
    };
    // FR-017: the filter lists exactly the pages with a near duplicate:
    // the three shoes and the twelve city pages, plus the related bags.
    assert_eq!(count(&mut r, "near_duplicates"), expected);
    assert!(expected >= 15);
    assert_eq!(count(&mut r, "low_content"), 2);
    assert_eq!(count(&mut r, "not_compared"), 2);
    // Exact duplicates use the whole-page fingerprint: the red and blue
    // shoes differ in their chrome, so only identical whole pages count.
    let exact = count(&mut r, "exact_duplicates");
    assert!(exact < expected);
}
