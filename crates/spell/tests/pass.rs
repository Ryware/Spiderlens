//! Spec 027 part D: the spelling pass over a real crawl, read back through
//! the Spelling tab and the detail pane.

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use spiderlens_config::{CrawlConfig, ModeInput, SpeedSettings, SpellingSettings};
use spiderlens_engine::crawl::{Control, Crawler};
use spiderlens_results::{Need, Reader, UrlScope, ViewQuery};
use spiderlens_spell::Store;
use spiderlens_test_fixtures::{Site, SiteBuilder};
use tokio::sync::watch;

const E: &str = "example.test";
const FOOTER: &str = "<footer>Copyright notise for the whole site</footer>";

fn doc(lang: &str, title: &str, description: &str, body: &str) -> String {
    format!(
        "<!doctype html><html lang=\"{lang}\"><head><title>{title}</title>\
         <meta name=\"description\" content=\"{description}\"></head><body>{body}{FOOTER}</body></html>"
    )
}

fn site() -> Site {
    let mismatch = "שלום ".repeat(25);
    SiteBuilder::new()
        .html(
            E,
            "/",
            &doc(
                "en",
                "Welcome home",
                "A friendly place",
                "<p>We recieve guests every day.</p><pre>recieve in code is fine</pre>\
                 <a href=\"/about\">About</a> <a href=\"/blog\">Blog</a> <a href=\"/contact\">Contact</a> \
                 <a href=\"/he\">Hebrew</a> <a href=\"/fr\">French</a> <a href=\"/mismatch\">Other</a>",
            ),
        )
        .html(
            E,
            "/about",
            &doc(
                "en",
                "About us",
                "Who we are",
                "<p>We build tools for NASA and you.</p>",
            ),
        )
        .html(
            E,
            "/blog",
            &doc(
                "en",
                "Blog",
                "Latest posts",
                "<h2>Teh news</h2><p>Plain words here.</p>",
            ),
        )
        .html(
            E,
            "/contact",
            &doc("en", "Contact", "Contakt us any time", "<p>Write to us.</p>"),
        )
        .html(E, "/he", &doc("he", "בית", "שלום", "<p>ושלום בָּיִת שלוםם</p>"))
        .html(
            E,
            "/fr",
            &doc("fr", "Bonjour", "Le site", "<p>Nous sommes ici.</p>"),
        )
        .html(
            E,
            "/mismatch",
            &doc("en", "Other", "Other", &format!("<p>{mismatch}</p>")),
        )
        .start()
}

struct Crawled {
    _dir: tempfile::TempDir,
    path: PathBuf,
    site: Site,
    dicts: Store,
}

async fn crawl(store_html: bool) -> Crawled {
    let site = site();
    let mut c = CrawlConfig {
        speed: SpeedSettings::FAST,
        ..CrawlConfig::default()
    };
    c.extraction.store_html = store_html;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("spell.spiderlens");
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
    // A small Hebrew dictionary with the ו prefix, added from files.
    let src = dir.path().join("he-src");
    std::fs::create_dir_all(&src).unwrap();
    std::fs::write(src.join("he.aff"), "SET UTF-8\nPFX A Y 1\nPFX A 0 ו .\n").unwrap();
    std::fs::write(src.join("he.dic"), "3\nשלום/A\nבית/A\nספר\n").unwrap();
    let dicts = Store::new(dir.path().join("dictionaries"));
    dicts
        .add_files("he", &src.join("he.aff"), &src.join("he.dic"))
        .unwrap();
    Crawled {
        _dir: dir,
        path,
        site,
        dicts,
    }
}

fn check(c: &Crawled) -> spiderlens_spell::SpellSummary {
    let mut last = None;
    let s = spiderlens_spell::run(
        &c.path,
        &SpellingSettings::default(),
        &c.dicts,
        &AtomicBool::new(false),
        |p| last = Some(p),
    )
    .unwrap();
    let p = last.unwrap();
    assert_eq!(p.checked, p.total);
    s
}

fn q(filter: &str) -> ViewQuery {
    ViewQuery {
        tab: "spelling".into(),
        filter: filter.into(),
        scope: UrlScope::All,
        search: String::new(),
        sort: None,
        segment: None,
    }
}

fn paths(r: &mut Reader, c: &Crawled, filter: &str) -> BTreeSet<String> {
    let page = r.open_view(&q(filter), 500).unwrap();
    r.materialize(page.view).unwrap();
    let rows = r.rows(page.view, 0, 1000).unwrap().rows;
    r.close_view(page.view).unwrap();
    let base = c.site.url(E, "");
    rows.iter()
        .map(|row| row[1].as_str().unwrap().strip_prefix(&base).unwrap().to_string())
        .collect()
}

fn set(items: &[&str]) -> BTreeSet<String> {
    items.iter().map(|s| s.to_string()).collect()
}

fn id_of(r: &mut Reader, c: &Crawled, path: &str) -> i64 {
    let page = r.open_view(&q("all"), 500).unwrap();
    let want = c.site.url(E, path);
    page.rows
        .iter()
        .find(|row| row[1].as_str() == Some(want.as_str()))
        .map(|row| row[0].as_i64().unwrap())
        .unwrap()
}

#[tokio::test]
async fn spelling_filters_list_exactly_the_seeded_pages() {
    let c = crawl(true).await;
    // Before the pass, the tab says why it is empty.
    let r = Reader::open(&c.path).unwrap();
    assert!(!r.features().has(Need::Spelling));
    drop(r);

    let s = check(&c);
    assert_eq!(s.pages, 7);
    assert_eq!(s.no_dictionary, 1);
    assert_eq!(s.missing.get("fr"), Some(&1));
    assert_eq!(s.site_wide_blocks, 1);
    assert_eq!(s.dictionaries, ["en", "he"]);

    let mut r = Reader::open(&c.path).unwrap();
    assert!(r.features().has(Need::Spelling));
    assert_eq!(
        paths(&mut r, &c, "errors"),
        set(&["/", "/about", "/blog", "/contact", "/he", "/mismatch"])
    );
    assert_eq!(paths(&mut r, &c, "in_body"), set(&["/", "/he"]));
    assert_eq!(paths(&mut r, &c, "in_headings"), set(&["/blog"]));
    assert_eq!(paths(&mut r, &c, "in_description"), set(&["/contact"]));
    assert!(paths(&mut r, &c, "in_title").is_empty());
    assert_eq!(paths(&mut r, &c, "site_wide").len(), 6);
    assert_eq!(paths(&mut r, &c, "no_dictionary"), set(&["/fr"]));
    assert_eq!(paths(&mut r, &c, "lang_mismatch"), set(&["/mismatch"]));
    assert!(paths(&mut r, &c, "partly_checked").is_empty());

    let counts = r.counts("spelling", UrlScope::All).unwrap();
    assert_eq!(counts.counts["errors"], Some(6));
}

#[tokio::test]
async fn details_list_each_word_with_its_suggestions() {
    let c = crawl(true).await;
    check(&c);
    let mut r = Reader::open(&c.path).unwrap();

    let home = id_of(&mut r, &c, "/");
    let d = r.detail(home).unwrap().spelling.unwrap();
    assert_eq!(d.lang.as_deref(), Some("en"));
    assert_eq!(d.status, "checked");
    let words: Vec<(&str, bool)> = d
        .findings
        .iter()
        .map(|f| (f.word.as_str(), f.site_wide))
        .collect();
    // Code is skipped; the footer typo is marked site-wide.
    assert_eq!(words, [("recieve", false), ("notise", true)]);
    assert_eq!(
        d.findings[0].suggestions.first().map(String::as_str),
        Some("receive")
    );
    assert!(d.findings[0].context.contains("We recieve guests"));

    let he = id_of(&mut r, &c, "/he");
    let d = r.detail(he).unwrap().spelling.unwrap();
    assert_eq!(d.lang.as_deref(), Some("he"));
    let words: Vec<&str> = d
        .findings
        .iter()
        .filter(|f| !f.site_wide)
        .map(|f| f.word.as_str())
        .collect();
    // The prefixed word and the word with niqqud are accepted.
    assert_eq!(words, ["שלוםם"]);
}

#[tokio::test]
async fn a_crawl_without_html_checks_titles_and_headings_only() {
    let c = crawl(false).await;
    let s = check(&c);
    assert_eq!(s.partly_checked, 6);
    let mut r = Reader::open(&c.path).unwrap();
    assert_eq!(paths(&mut r, &c, "partly_checked").len(), 6);
    assert_eq!(paths(&mut r, &c, "errors"), set(&["/blog", "/contact"]));
}

#[tokio::test]
async fn ignored_words_and_a_second_pass_replace_the_first() {
    let c = crawl(true).await;
    check(&c);
    let settings = SpellingSettings {
        ignore: vec!["Recieve".into(), "notise".into()],
        ..SpellingSettings::default()
    };
    spiderlens_spell::run(&c.path, &settings, &c.dicts, &AtomicBool::new(false), |_| {}).unwrap();
    let mut r = Reader::open(&c.path).unwrap();
    assert_eq!(paths(&mut r, &c, "errors"), set(&["/blog", "/contact", "/he"]));
}

/// The crawl never depends on dictionaries (plan D5): the engine and results
/// crates do not list this crate.
#[test]
fn engine_and_results_do_not_depend_on_spelling() {
    for krate in ["engine", "results"] {
        let toml =
            std::fs::read_to_string(format!("{}/../{krate}/Cargo.toml", env!("CARGO_MANIFEST_DIR"))).unwrap();
        assert!(
            !toml.contains("spiderlens-spell"),
            "{krate} depends on spiderlens-spell"
        );
    }
}
