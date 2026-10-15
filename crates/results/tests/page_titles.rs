//! Page titles tab (T039): its rows, values, sorts, filters and counts read
//! from `temp.titles`; checked against the rules computed here from the raw
//! crawl file, and against the Internal tab's own title order.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use rusqlite::{Connection, params};
use spiderlens_results::synth::Synth;
use spiderlens_results::{Reader, Sort, UrlScope, ViewQuery, norm_ws};

fn q(tab: &str, filter: &str, sort: Option<(&str, bool)>) -> ViewQuery {
    ViewQuery {
        tab: tab.into(),
        filter: filter.into(),
        scope: UrlScope::All,
        search: String::new(),
        sort: sort.map(|(c, desc)| Sort {
            column: c.into(),
            desc,
        }),
        segment: None,
        values: None,
    }
}

/// Every row of a view, in order.
fn rows(r: &mut Reader, query: &ViewQuery) -> Vec<Vec<serde_json::Value>> {
    let page = r.open_view(query, 1).unwrap();
    let total = r.materialize(page.view).unwrap();
    let mut out = Vec::new();
    for from in (0..total).step_by(1000) {
        out.extend(r.rows(page.view, from, 1000).unwrap().rows);
    }
    r.close_view(page.view).unwrap();
    out
}

fn ids(rows: &[Vec<serde_json::Value>]) -> Vec<i64> {
    rows.iter().map(|r| r[0].as_i64().unwrap()).collect()
}

/// One Page titles row as the spec defines it, from the raw file.
struct Page {
    id: i64,
    title: Option<String>,
    norm: String,
    h1: Option<String>,
    title_count: Option<i64>,
}

fn pages(path: &Path) -> Vec<Page> {
    let c = Connection::open(path).unwrap();
    let mut stmt = c
        .prepare(
            "SELECT u.id, u.title, p.h1_1, p.title_count FROM urls u LEFT JOIN pages p ON p.url_id = u.id
             WHERE u.internal = 1 AND u.state = 'crawled' AND u.error_class IS NULL
               AND u.status BETWEEN 200 AND 299 AND u.content_type LIKE 'text/html%' ORDER BY u.id",
        )
        .unwrap();
    stmt.query_map([], |r| {
        let title: Option<String> = r.get(1)?;
        Ok(Page {
            id: r.get(0)?,
            norm: title.as_deref().map(norm_ws).unwrap_or_default(),
            title,
            h1: r.get(2)?,
            title_count: r.get(3)?,
        })
    })
    .unwrap()
    .collect::<rusqlite::Result<_>>()
    .unwrap()
}

/// Adds pages whose titles differ only in whitespace or case, and ones
/// that sort naturally, to the synthetic crawl.
fn add_edge_pages(path: &Path) {
    let c = Connection::open(path).unwrap();
    let titles = [
        "Home ",
        "  Home",
        "Home\tpage",
        "Home  page",
        "",
        "home",
        "Page 2",
        "page 10",
        "Page 1",
        "Café straße",
    ];
    for (i, t) in titles.iter().enumerate() {
        c.execute(
            "INSERT INTO urls (url, host_key, internal, depth, state, source, status, content_type, title, fetched_at)
             VALUES (?1, 'https://example.test:443', 1, 1, 'crawled', 'link', 200, 'text/html', ?2, 1)",
            params![format!("https://example.test/edge/{i}"), t],
        )
        .unwrap();
        let id = c.last_insert_rowid();
        c.execute(
            "INSERT INTO pages (url_id, title_count, meta_description_count, h1_1, h1_count, h2_count, heading_counts,
               word_count, partial, encoding_errors, no_content, html_stored)
             VALUES (?1, ?2, 0, ?3, 1, 0, '[]', 10, 0, 0, 0, 0)",
            params![id, 1 + (i % 2) as i64, t.trim().to_uppercase()],
        )
        .unwrap();
    }
}

fn check(path: &Path, extraction: bool) {
    check_reader(&mut Reader::open(path).unwrap(), path, extraction);
}

fn check_reader(r: &mut Reader, path: &Path, extraction: bool) {
    let pages = pages(path);
    let mut occ: BTreeMap<&str, i64> = BTreeMap::new();
    for p in &pages {
        if !p.norm.is_empty() {
            *occ.entry(&p.norm).or_default() += 1;
        }
    }
    let len = |p: &Page| p.title.as_ref().map(|_| p.norm.chars().count() as i64);
    let n = |p: &Page| occ.get(p.norm.as_str()).copied();

    // Rows and their values, in discovery order.
    let all = rows(r, &q("page-titles", "all", None));
    assert_eq!(ids(&all), pages.iter().map(|p| p.id).collect::<Vec<_>>());
    for (row, p) in all.iter().zip(&pages) {
        assert_eq!(row[2].as_str(), p.title.as_deref(), "title of {}", p.id);
        assert_eq!(row[3].as_i64(), len(p), "length of {}", p.id);
        assert_eq!(row[4].as_i64(), n(p), "occurrences of {}", p.id);
    }

    // Filters and counts.
    type Rule<'a> = Box<dyn Fn(&Page) -> bool + 'a>;
    let mut rules: Vec<(&str, Rule)> = vec![
        ("all", Box::new(|_| true)),
        ("missing", Box::new(|p| p.norm.is_empty())),
        ("duplicate", Box::new(|p| n(p).unwrap_or(0) > 1)),
        ("over_60", Box::new(|p| len(p).unwrap_or(0) > 60)),
        (
            "below_30",
            Box::new(|p| !p.norm.is_empty() && len(p).unwrap_or(0) < 30),
        ),
    ];
    if extraction {
        rules.push((
            "same_as_h1",
            Box::new(|p| !p.norm.is_empty() && p.h1.as_ref().is_some_and(|h| p.norm.eq_ignore_ascii_case(h))),
        ));
        rules.push(("multiple", Box::new(|p| p.title_count.unwrap_or(0) > 1)));
    }
    let counts = r.counts("page-titles", UrlScope::All).unwrap();
    for (filter, rule) in &rules {
        let expected: Vec<i64> = pages.iter().filter(|p| rule(p)).map(|p| p.id).collect();
        assert!(!expected.is_empty(), "the fixture covers {filter}");
        assert_eq!(
            ids(&rows(r, &q("page-titles", filter, None))),
            expected,
            "{filter}"
        );
        assert_eq!(
            counts.counts[*filter],
            Some(expected.len() as u64),
            "count of {filter}"
        );
    }
    if !extraction {
        assert_eq!(counts.counts["same_as_h1"], None);
        assert!(r.open_view(&q("page-titles", "same_as_h1", None), 10).is_err());
    }

    // Sorting by title orders exactly as the Internal tab does.
    let in_tab: BTreeSet<i64> = pages.iter().map(|p| p.id).collect();
    for desc in [false, true] {
        let internal: Vec<i64> = ids(&rows(r, &q("internal", "all", Some(("title", desc)))))
            .into_iter()
            .filter(|id| in_tab.contains(id))
            .collect();
        let titles = ids(&rows(r, &q("page-titles", "all", Some(("title", desc)))));
        assert_eq!(titles, internal, "title order, desc = {desc}");
    }

    // Numbers: empty last, then by value, ties by discovery order.
    for (column, key) in [
        ("title_length", Box::new(len) as Box<dyn Fn(&Page) -> Option<i64>>),
        ("occurrences", Box::new(n)),
    ] {
        for desc in [false, true] {
            let mut expected: Vec<&Page> = pages.iter().collect();
            expected.sort_by(|a, b| {
                let (x, y) = (key(a), key(b));
                x.is_none()
                    .cmp(&y.is_none())
                    .then(if desc { y.cmp(&x) } else { x.cmp(&y) })
                    .then(a.id.cmp(&b.id))
            });
            let expected: Vec<i64> = expected.iter().map(|p| p.id).collect();
            let got = ids(&rows(r, &q("page-titles", "all", Some((column, desc)))));
            assert_eq!(got, expected, "{column}, desc = {desc}");
        }
    }
}

#[test]
fn page_titles_match_the_rules_with_page_content() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("titles.spiderlens");
    Synth {
        urls: 3_000,
        links_per_page: 2,
        seed: 13,
        pages: true,
        ..Default::default()
    }
    .write(&path)
    .unwrap();
    add_edge_pages(&path);
    check(&path, true);
}

#[test]
fn page_titles_match_the_rules_without_page_content() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("base.spiderlens");
    Synth {
        urls: 2_000,
        links_per_page: 2,
        seed: 17,
        ..Default::default()
    }
    .write(&path)
    .unwrap();
    check(&path, false);
}

#[test]
fn page_titles_follow_the_live_crawl() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("live.spiderlens");
    Synth {
        urls: 1_000,
        links_per_page: 2,
        seed: 19,
        pages: true,
        ..Default::default()
    }
    .write(&path)
    .unwrap();
    let mut r = Reader::open(&path).unwrap();
    let before = r.counts("page-titles", UrlScope::All).unwrap();
    let dup = q("page-titles", "duplicate", Some(("occurrences", true)));
    let page = r.open_view(&dup, 10).unwrap();
    r.materialize(page.view).unwrap();

    // The crawl commits two pages sharing a new title, as a live crawl does.
    add_edge_pages(&path);
    let w = Connection::open(&path).unwrap();
    for i in 0..2 {
        w.execute(
            "INSERT INTO urls (url, host_key, internal, depth, state, source, status, content_type, title, fetched_at)
             VALUES (?1, 'https://example.test:443', 1, 1, 'crawled', 'link', 200, 'text/html', 'Brand new', 1)",
            params![format!("https://example.test/new/{i}")],
        )
        .unwrap();
    }
    let refresh = r.refresh(page.view, &[]).unwrap().expect("data changed");
    let after = r.counts("page-titles", UrlScope::All).unwrap();
    assert_eq!(after.counts["all"], before.counts["all"].map(|n| n + 12));
    // "Home " and "  Home" join the synthetic "Home" pages; "Home\tpage" and
    // "Home  page" are one title, and so are the two "Brand new" pages.
    let grew = |f: &str| after.counts[f].unwrap() - before.counts[f].unwrap();
    assert_eq!(grew("duplicate"), 6);
    assert_eq!(refresh.total as u64, after.counts["duplicate"].unwrap());
    assert!(after.version > before.version);
    // The tab updated in place reads the same as one built afresh.
    check_reader(&mut r, &path, true);
    check(&path, true);
}

#[test]
fn page_titles_export_lists_the_view() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("export.spiderlens");
    Synth {
        urls: 1_500,
        links_per_page: 2,
        seed: 23,
        pages: true,
        ..Default::default()
    }
    .write(&path)
    .unwrap();
    let query = q("page-titles", "duplicate", Some(("title_length", true)));
    let mut r = Reader::open(&path).unwrap();
    let expected = rows(&mut r, &query);
    let mut out = Vec::new();
    let n = spiderlens_results::write_csv(
        &path,
        &query,
        &["address".into(), "title_length".into(), "occurrences".into()],
        None,
        None,
        &mut out,
        &std::sync::atomic::AtomicBool::new(false),
        &mut |_, _| {},
    )
    .unwrap();
    assert_eq!(n as usize, expected.len());
    let text = String::from_utf8(out).unwrap();
    let lines: Vec<&str> = text.trim_start_matches('\u{feff}').lines().skip(1).collect();
    for (line, row) in lines.iter().zip(&expected) {
        let want = format!(
            "{},{},{}",
            row[1].as_str().unwrap(),
            row[3].as_i64().unwrap(),
            row[4].as_i64().unwrap()
        );
        assert_eq!(*line, want);
    }
}
