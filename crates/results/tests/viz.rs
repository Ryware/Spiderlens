//! Spec 022 trees on a real crawl (SC-001): crawl tree parents and groups,
//! directory folders and counts, budgets and layouts.

use std::path::PathBuf;
use std::time::Duration;

use spiderlens_config::{CrawlConfig, ModeInput, SpeedSettings};
use spiderlens_engine::crawl::{Control, Crawler};
use spiderlens_results::Reader;
use spiderlens_results::viz::{Category, Layout, Scene, SceneNode, Terms, ViewKind, ViewState};
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
    c.scope.sitemaps = spiderlens_config::SitemapSettings::all(true);
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

/// Two cliques of five pages joined by one link (a1 -> b1); the home page
/// links into the first.
fn clusters() -> Site {
    let a: Vec<String> = (1..=5).map(|i| format!("/a{i}")).collect();
    let b: Vec<String> = (1..=5).map(|i| format!("/b{i}")).collect();
    let mut s = SiteBuilder::new().page(E, "/", "", &["/a1"]);
    for set in [&a, &b] {
        for p in set.iter() {
            let mut links: Vec<&str> = set.iter().filter(|x| *x != p).map(String::as_str).collect();
            if p == "/a1" {
                links.push("/b1");
            }
            s = s.page(E, p, "", &links);
        }
    }
    s.start()
}

async fn crawl_site(site: Site) -> Crawled {
    let c = CrawlConfig {
        speed: SpeedSettings::FAST,
        ..CrawlConfig::default()
    };
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("graph.spiderlens");
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

fn graph(r: &mut Reader, focus: Option<String>, distance: u8) -> Scene {
    r.viz_scene(&ViewState {
        view: ViewKind::LinkGraph,
        focus,
        distance,
        ..ViewState::default()
    })
    .unwrap()
}

fn centre<'a>(nodes: impl Iterator<Item = &'a SceneNode>) -> (f32, f32) {
    let v: Vec<_> = nodes.collect();
    let n = v.len() as f32;
    (
        v.iter().map(|p| p.x).sum::<f32>() / n,
        v.iter().map(|p| p.y).sum::<f32>() / n,
    )
}

#[tokio::test]
async fn link_graph_clusters_focus_and_determinism() {
    let c = crawl_site(clusters()).await;
    let mut r = Reader::open(&c.path).unwrap();
    let s = graph(&mut r, None, 1);
    assert_eq!(s.nodes.len(), 11);
    assert!(s.omitted.is_none());
    let at = |s: &Scene, l: &str| s.nodes.iter().position(|n| n.label == l).unwrap() as u32;
    // The joining link is drawn, once, with its direction.
    let (a1, b1) = (at(&s, "/a1"), at(&s, "/b1"));
    assert_eq!(s.edges.iter().filter(|e| (e.a, e.b) == (a1, b1)).count(), 1);
    assert!(!s.edges.iter().any(|e| (e.a, e.b) == (b1, a1)));
    // 1 + 20 + 20 + 1 distinct followed links.
    assert_eq!(s.edges.len(), 42);
    // The clusters are drawn apart.
    let ca = centre(s.nodes.iter().filter(|n| n.label.starts_with("/a")));
    let cb = centre(s.nodes.iter().filter(|n| n.label.starts_with("/b")));
    let spread = s
        .nodes
        .iter()
        .filter(|n| n.label.starts_with("/a"))
        .map(|n| ((n.x - ca.0).powi(2) + (n.y - ca.1).powi(2)).sqrt())
        .fold(0.0f32, f32::max);
    let apart = ((ca.0 - cb.0).powi(2) + (ca.1 - cb.1).powi(2)).sqrt();
    assert!(apart > spread, "clusters {apart} apart, cluster radius {spread}");
    // Same crawl, same settings: the same picture (FR-014).
    let again = graph(&mut r, None, 1);
    let pos = |s: &Scene| {
        s.nodes
            .iter()
            .map(|n| (n.x.to_bits(), n.y.to_bits()))
            .collect::<Vec<_>>()
    };
    assert_eq!(pos(&s), pos(&again));

    // Focus: exactly the neighbours, in and out.
    let b1_key = s.nodes[b1 as usize].key.clone();
    let f = graph(&mut r, Some(b1_key.clone()), 1);
    let mut labels: Vec<_> = f.nodes.iter().map(|n| n.label.as_str()).collect();
    labels.sort();
    assert_eq!(labels, ["/a1", "/b1", "/b2", "/b3", "/b4", "/b5"]);
    assert!(f.directed);
    let centre_node = &f.nodes[at(&f, "/b1") as usize];
    assert_eq!((centre_node.x, centre_node.y, centre_node.depth), (0.0, 0.0, 0));
    assert_eq!(f.omitted.as_ref().unwrap().count, 5);
    let f2 = graph(&mut r, Some(b1_key), 2);
    assert_eq!(f2.nodes.len(), 11, "two steps reach everything");
    assert_eq!(f2.nodes[at(&f2, "/") as usize].depth, 2);

    // A dragged node stays where it was put.
    let a2 = s.nodes[at(&s, "/a2") as usize].key.clone();
    let pinned = r
        .viz_scene(&ViewState {
            view: ViewKind::LinkGraph,
            pinned: [(a2.clone(), [500.0, -300.0])].into(),
            ..ViewState::default()
        })
        .unwrap();
    let n = pinned.nodes.iter().find(|n| n.key == a2).unwrap();
    assert_eq!((n.x, n.y), (500.0, -300.0));

    // A small budget keeps the strongest pages and the start URL.
    let small = r
        .viz_scene(&ViewState {
            view: ViewKind::LinkGraph,
            budget: Some(3),
            ..ViewState::default()
        })
        .unwrap();
    assert_eq!(small.nodes.len(), 3);
    assert!(
        small.nodes.iter().any(|n| n.label == "/"),
        "the start URL is always drawn"
    );
    assert_eq!(small.omitted.unwrap().count, 8);
    assert!(
        r.viz_scene(&ViewState {
            view: ViewKind::LinkGraph,
            focus: Some("u:999999".into()),
            ..ViewState::default()
        })
        .is_err()
    );
}

/// Known anchor texts (spec 022 US4): English and Hebrew pages, image links
/// with and without alt text, a nofollow link.
fn anchors_site() -> Site {
    let page = |lang: &str, body: &str| {
        format!(
            "<!doctype html><html lang=\"{lang}\"><head><title>t</title></head><body>{body}</body></html>"
        )
    };
    SiteBuilder::new()
        .html(
            E,
            "/",
            &page(
                "en",
                r#"<a href="/shoes">Running shoes</a> <a href="/shoes">Click here</a> <a href="/he">חנות</a>
                   <a href="/a">a</a> <a href="/b">b</a>"#,
            ),
        )
        .html(
            E,
            "/a",
            &page(
                "en",
                r#"<a href="/shoes">Best  running
                   shoes</a> <a href="/shoes"><img src="/x.png" alt="Shoe photo"></a>
                   <a href="/shoes" rel="nofollow">running SHOES</a>"#,
            ),
        )
        .html(
            E,
            "/b",
            &page(
                "en",
                r#"<a href="/shoes"><img src="/y.png"></a> <a href="/he">נעלי ריצה של החנות</a> <a href="/noalt"><img src="/z.png"></a>"#,
            ),
        )
        .html(E, "/shoes", &page("en-GB", "shoes"))
        .html(E, "/he", &page("he", "חנות"))
        .html(E, "/noalt", &page("en", "nothing"))
        .start()
}

fn cloud(
    r: &mut Reader,
    s: &Crawled,
    path: Option<&str>,
    f: impl FnOnce(&mut ViewState),
) -> (ViewState, Scene) {
    let focus = path.map(|p| {
        let db = rusqlite::Connection::open(&s.path).unwrap();
        let id: i64 = db
            .query_row("SELECT id FROM urls WHERE url = ?1", [s.site.url(E, p)], |r| {
                r.get(0)
            })
            .unwrap_or_else(|e| panic!("{p} not crawled: {e}"));
        format!("u:{id}")
    });
    let mut state = ViewState {
        view: ViewKind::Anchors,
        focus,
        ..ViewState::default()
    };
    f(&mut state);
    let scene = r.viz_scene(&state).unwrap();
    (state, scene)
}

fn counts(s: &Scene) -> Vec<(&str, u32)> {
    s.nodes.iter().map(|n| (n.label.as_str(), n.count)).collect()
}

#[tokio::test]
async fn anchor_clouds_count_terms_and_list_their_links() {
    let s = crawl_site(anchors_site()).await;
    let mut r = Reader::open(&s.path).unwrap();

    let (state, shoes) = cloud(&mut r, &s, Some("/shoes"), |_| {});
    assert_eq!(shoes.layout, Layout::Cloud);
    assert_eq!(shoes.total, 6, "every anchor link to the page");
    assert_eq!(shoes.language.as_deref(), Some("en"));
    // Most used first, then by term; "here" is a stop word; alt text counts.
    assert_eq!(
        counts(&shoes),
        [
            ("running", 3),
            ("shoes", 3),
            ("best", 1),
            ("click", 1),
            ("photo", 1),
            ("shoe", 1)
        ]
    );
    assert!(
        shoes.nodes[0].width.unwrap() > shoes.nodes[5].width.unwrap(),
        "bigger terms are wider"
    );
    assert!(shoes.note.is_none() && shoes.omitted.is_none());

    // The list behind a term has as many links as its count (FR-018).
    let running = r.viz_term_links(&state, "running", 100).unwrap();
    assert_eq!(running.total, 3);
    let from: Vec<_> = running.links.iter().map(|l| l.from_url.clone()).collect();
    assert_eq!(
        from,
        [s.site.url(E, "/"), s.site.url(E, "/a"), s.site.url(E, "/a")]
    );
    assert_eq!(running.links[1].text, "Best running shoes");
    assert_eq!(running.links[2].rel.as_deref(), Some("nofollow"));
    assert!(running.links[2].nofollow && !running.links[1].nofollow);
    let photo = r.viz_term_links(&state, "photo", 100).unwrap();
    assert_eq!(
        (photo.total, photo.links[0].image, photo.links[0].text.as_str()),
        (1, true, "Shoe photo")
    );
    assert_eq!(r.viz_term_links(&state, "running", 1).unwrap().links.len(), 1);

    // Whole anchors, followed only, stop words kept.
    let (_, whole) = cloud(&mut r, &s, Some("/shoes"), |v| v.terms = Terms::Whole);
    assert_eq!(
        counts(&whole),
        [
            ("running shoes", 2),
            ("best running shoes", 1),
            ("click here", 1),
            ("shoe photo", 1)
        ]
    );
    let (_, followed) = cloud(&mut r, &s, Some("/shoes"), |v| v.followed_only = true);
    assert_eq!(followed.total, 5);
    assert_eq!(
        followed
            .nodes
            .iter()
            .find(|n| n.label == "running")
            .unwrap()
            .count,
        2
    );
    let (_, all_words) = cloud(&mut r, &s, Some("/shoes"), |v| v.stop_words = false);
    assert!(all_words.nodes.iter().any(|n| n.label == "here"));

    // Hebrew stop words for a Hebrew page.
    let (_, he) = cloud(&mut r, &s, Some("/he"), |_| {});
    assert_eq!(he.language.as_deref(), Some("he"));
    assert_eq!(counts(&he), [("החנות", 1), ("חנות", 1), ("נעלי", 1), ("ריצה", 1)]);

    // Empty clouds say why (US4 scenario 4).
    let (_, home) = cloud(&mut r, &s, Some("/"), |_| {});
    assert!(home.nodes.is_empty());
    assert_eq!(
        home.note.as_deref(),
        Some("No internal links point to this page.")
    );
    let (_, noalt) = cloud(&mut r, &s, Some("/noalt"), |_| {});
    assert!(
        noalt
            .note
            .unwrap()
            .starts_with("None of these links has anchor text")
    );

    // Site-wide: every internal anchor link, the most common language.
    let (_, site) = cloud(&mut r, &s, None, |_| {});
    assert_eq!(site.total, 11);
    assert_eq!(site.language.as_deref(), Some("en"));
    assert_eq!(site.nodes[0].label, "running");
    // The same state gives the same cloud.
    let (_, again) = cloud(&mut r, &s, None, |_| {});
    assert_eq!(
        site.nodes.iter().map(|n| (n.x, n.y)).collect::<Vec<_>>(),
        again.nodes.iter().map(|n| (n.x, n.y)).collect::<Vec<_>>()
    );
}

#[tokio::test]
async fn exports_svg_png_and_csv_of_the_view() {
    use spiderlens_results::viz::export::{ExportFormat, ExportOptions, export};
    use spiderlens_results::viz::palette::{Palette, Theme};
    use spiderlens_results::viz::{ColourBy, png, svg};

    let c = crawl().await;
    let mut r = Reader::open(&c.path).unwrap();
    let state = ViewState::default();
    let scene = r.viz_scene(&state).unwrap();
    let header = r.viz_header().unwrap();
    assert_eq!(
        header.host,
        url::Url::parse(&c.site.url(E, "/")).unwrap().host_str().unwrap()
    );
    assert_eq!(header.date.len(), 10, "YYYY-MM-DD: {}", header.date);

    // SVG: the same bytes every time; header, legend and labels are text.
    let light = Palette::theme(Theme::Light);
    let a = svg::svg(&scene, &light, ColourBy::Status, &header);
    assert_eq!(a, svg::svg(&scene, &light, ColourBy::Status, &header));
    assert!(a.starts_with("<?xml"));
    assert!(a.contains(">Crawl tree</text>"));
    assert!(a.contains(&format!("{} · {}", header.host, header.date)));
    assert!(a.contains("Success (2xx)  "), "legend with counts");
    assert!(a.contains("<title>Crawl tree · "));
    assert!(
        a.contains(">/a</text>") || a.contains(">/a  (+"),
        "labels are selectable text"
    );
    let dark = svg::svg(&scene, &Palette::theme(Theme::Dark), ColourBy::Status, &header);
    assert_ne!(a, dark);
    assert!(a.contains(&light.hex("--bg")) && dark.contains(&Palette::theme(Theme::Dark).hex("--bg")));

    let dir = tempfile::tempdir().unwrap();
    // PNG at 2x is twice the SVG's size.
    let p = dir.path().join("tree.png");
    let (w1, h1) = png::size(&a, 1.0).unwrap();
    let done = export(
        &scene,
        ColourBy::Status,
        &header,
        &ExportOptions {
            format: ExportFormat::Png,
            scale: 2,
            ..Default::default()
        },
        &p,
    )
    .unwrap();
    assert_eq!(done.files, [p.display().to_string()]);
    let bytes = std::fs::read(&p).unwrap();
    assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n");
    let be = |o: usize| u32::from_be_bytes(bytes[o..o + 4].try_into().unwrap());
    assert_eq!((be(16), be(20)), (w1 * 2, h1 * 2));
    assert!(!dir.path().join("tree.png.part").exists());

    // CSV: one row per node, and the tree lines in a second file.
    let p = dir.path().join("tree.csv");
    let done = export(
        &scene,
        ColourBy::Status,
        &header,
        &ExportOptions {
            format: ExportFormat::Csv,
            ..Default::default()
        },
        &p,
    )
    .unwrap();
    assert_eq!(done.files.len(), 2);
    let nodes = std::fs::read_to_string(&p).unwrap();
    assert!(
        nodes.starts_with(
            "\u{feff}Node,Kind,URL,Label,Depth,Response,Link Score,Inlinks,Parent,URLs,Folded\r\n"
        )
    );
    assert_eq!(nodes.lines().count(), scene.nodes.len() + 1);
    let links = std::fs::read_to_string(&done.files[1]).unwrap();
    assert_eq!(
        links.lines().count(),
        scene.nodes.iter().filter(|n| n.parent.is_some()).count() + 1
    );
    assert!(links.contains(&format!("{},{},tree", c.site.url(E, "/"), c.site.url(E, "/a"))));

    // A big scene is drawn smaller, as fitting it to a window would.
    let mut huge = scene.clone();
    for nd in &mut huge.nodes {
        nd.y *= 100.0;
    }
    huge.bounds.height *= 100.0;
    let big = svg::svg(&huge, &light, ColourBy::Status, &header);
    let (_, h) = png::size(&big, 1.0).unwrap();
    assert!(h <= svg::MAX_PICTURE as u32 + 400, "{h}");
    // Anything still too big for a PNG is refused.
    let wide = "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"5000\" height=\"10\"/>";
    let e = png::png(wide, 4.0).unwrap_err();
    assert!(e.to_string().contains("export it as SVG"), "{e}");
}
