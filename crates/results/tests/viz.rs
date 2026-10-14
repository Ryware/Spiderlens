//! Spec 022 trees on a real crawl (SC-001): crawl tree parents and groups,
//! directory folders and counts, budgets and layouts.

use std::path::PathBuf;
use std::time::Duration;

use spiderlens_config::{CrawlConfig, ModeInput, SpeedSettings};
use spiderlens_engine::crawl::{Control, Crawler};
use spiderlens_results::Reader;
use spiderlens_results::viz::{Category, Layout, Scene, SceneNode, ViewKind, ViewState};
use spiderlens_test_fixtures::{Site, SiteBuilder};
use tokio::sync::watch;

const E: &str = "example.test";

fn site() -> Site {
    let sitemap = "<urlset><url><loc>{base:example.test}/hidden</loc></url></urlset>";
    SiteBuilder::new()
        .robots(
            E,
            "User-agent: *\nDisallow: /private\nSitemap: {base:example.test}/sitemap.xml\n",
        )
        .raw(E, "/sitemap.xml", 200, "application/xml", sitemap.as_bytes())
        .page(E, "/", "", &["/a", "/blog/", "/old", "/private"])
        .page(E, "/a", "", &["/a/x"])
        .page(E, "/a/x", "", &["/missing", "/"])
        .page(
            E,
            "/blog/",
            "",
            &["/blog/p1", "/blog/p2?page=1", "/blog/p2?page=2"],
        )
        .page(E, "/blog/p1", "", &[])
        .page(E, "/blog/p2", "", &[])
        .redirect(E, "/old", 301, "/new")
        .page(E, "/new", "", &[])
        .page(E, "/hidden", "", &[])
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
    c.scope.use_sitemaps = true;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("viz.spiderlens");
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

fn node<'a>(s: &'a Scene, label: &str) -> &'a SceneNode {
    s.nodes.iter().find(|n| n.label == label).unwrap_or_else(|| {
        panic!(
            "no node {label}: {:?}",
            s.nodes.iter().map(|n| &n.label).collect::<Vec<_>>()
        )
    })
}

fn parent<'a>(s: &'a Scene, n: &SceneNode) -> &'a str {
    &s.nodes[n.parent.unwrap() as usize].label
}

#[tokio::test]
async fn crawl_tree_parents_groups_and_categories() {
    let c = crawl().await;
    let mut r = Reader::open(&c.path).unwrap();
    let s = r.viz_scene(&ViewState::default()).unwrap();
    assert_eq!(s.view, ViewKind::CrawlTree);
    // Several top-level entries (start URL and sitemap-only page): a crawl root.
    assert_eq!(s.nodes[0].label, "Crawl");
    let start = c.site.url(E, "/");
    let home = s
        .nodes
        .iter()
        .find(|n| n.url.as_deref() == Some(start.as_str()))
        .unwrap();
    assert_eq!(parent(&s, home), "Crawl");
    assert_eq!(parent(&s, node(&s, "/a")), home.label);
    assert_eq!(parent(&s, node(&s, "/a/x")), "/a");
    let missing = node(&s, "/missing");
    assert_eq!(parent(&s, missing), "/a/x");
    assert_eq!(missing.category, Category::ClientError);
    assert_eq!(missing.depth, 3);
    assert_eq!(node(&s, "/old").category, Category::Redirect);
    assert_eq!(parent(&s, node(&s, "/new")), "/old");
    assert_eq!(node(&s, "/private").category, Category::Blocked);
    // Found only in the sitemap: grouped by source, not hung on a page.
    let hidden = node(&s, "/hidden");
    assert_eq!(parent(&s, hidden), "From sitemaps");
    assert_eq!(parent(&s, node(&s, "From sitemaps")), "Found outside links");
    // Counts: every tree URL once.
    let urls = s.nodes.iter().filter(|n| n.url_id.is_some()).count() as u32;
    assert_eq!(s.total, urls);
    assert_eq!(s.nodes[0].count, s.total);
    assert!(s.omitted.is_none());
    let legend: u32 = s.legend.iter().map(|l| l.count).sum();
    assert_eq!(legend, s.total);
    // Parents come before children.
    for (i, n) in s.nodes.iter().enumerate() {
        if let Some(p) = n.parent {
            assert!((p as usize) < i);
        }
    }
}

#[tokio::test]
async fn directory_tree_folders_pages_and_query_strings() {
    let c = crawl().await;
    let mut r = Reader::open(&c.path).unwrap();
    let s = r
        .viz_scene(&ViewState {
            view: ViewKind::DirectoryTree,
            ..ViewState::default()
        })
        .unwrap();
    // One host: its folder is the root.
    assert_eq!(
        s.nodes[0].label,
        c.site
            .url(E, "")
            .trim_start_matches("http://")
            .trim_end_matches('/')
    );
    assert_eq!(s.nodes[0].category, Category::Folder);
    let blog = node(&s, "blog/");
    assert_eq!(blog.category, Category::Folder);
    assert_eq!(blog.count, 4, "/blog/, p1 and p2 twice");
    let p2 = node(&s, "p2");
    assert_eq!(p2.count, 2, "query strings share the page");
    assert_eq!(parent(&s, p2), "blog/");
    assert_eq!(parent(&s, node(&s, "x")), "a/");
    // Folders come before pages.
    let root_kids: Vec<&SceneNode> = s.nodes.iter().filter(|n| n.parent == Some(0)).collect();
    let first_page = root_kids
        .iter()
        .position(|n| n.category != Category::Folder)
        .unwrap();
    assert!(
        root_kids[first_page..]
            .iter()
            .all(|n| n.category != Category::Folder)
    );
    // Same URL set as the crawl tree.
    let tree = r.viz_scene(&ViewState::default()).unwrap();
    assert_eq!(s.total, tree.total);
}

#[tokio::test]
async fn budget_folds_and_expanding_opens_a_branch() {
    let c = crawl().await;
    let mut r = Reader::open(&c.path).unwrap();
    let small = ViewState {
        budget: Some(4),
        ..ViewState::default()
    };
    let s = r.viz_scene(&small).unwrap();
    assert!(s.nodes.len() <= 4);
    let omitted = s.omitted.as_ref().expect("something left out");
    let shown = s.nodes.iter().filter(|n| n.url_id.is_some()).count() as u32;
    assert_eq!(omitted.count + shown, s.total);
    let folded = s.nodes.iter().find(|n| n.expandable && !n.expanded).unwrap();
    assert_eq!(folded.hidden as usize, {
        let full = r.viz_scene(&ViewState::default()).unwrap();
        let k = full.nodes.iter().position(|n| n.key == folded.key).unwrap();
        // Descendants in the full tree.
        let mut d = 0;
        let mut stack = vec![k as u32];
        while let Some(p) = stack.pop() {
            for (i, n) in full.nodes.iter().enumerate() {
                if n.parent == Some(p) {
                    d += 1;
                    stack.push(i as u32);
                }
            }
        }
        d
    });
    // Asking for that node open shows its children.
    let open = ViewState {
        expanded: Some(vec![folded.key.clone()]),
        budget: Some(50),
        ..ViewState::default()
    };
    let s2 = r.viz_scene(&open).unwrap();
    let k = s2.nodes.iter().position(|n| n.key == folded.key).unwrap();
    assert!(s2.nodes[k].expanded);
    assert!(s2.nodes.iter().any(|n| n.parent == Some(k as u32)));
}

#[tokio::test]
async fn radial_shows_the_same_nodes() {
    let c = crawl().await;
    let mut r = Reader::open(&c.path).unwrap();
    let tree = r.viz_scene(&ViewState::default()).unwrap();
    let radial = r
        .viz_scene(&ViewState {
            layout: Layout::Radial,
            ..ViewState::default()
        })
        .unwrap();
    let keys = |s: &Scene| s.nodes.iter().map(|n| n.key.clone()).collect::<Vec<_>>();
    assert_eq!(keys(&tree), keys(&radial));
    assert_eq!((radial.nodes[0].x, radial.nodes[0].y), (0.0, 0.0));
    // Deterministic: the same state gives the same positions.
    let again = r.viz_scene(&ViewState::default()).unwrap();
    let pos = |s: &Scene| {
        s.nodes
            .iter()
            .map(|n| (n.x.to_bits(), n.y.to_bits()))
            .collect::<Vec<_>>()
    };
    assert_eq!(pos(&tree), pos(&again));
}
