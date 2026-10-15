//! Spec 026 static checks over the fixture pages (T010, SC-001 static part):
//! every page produces exactly the findings it lists, no more and no less.

use spiderlens_engine::extract::{PageInput, extract_page};
use spiderlens_test_fixtures::a11y_site::STATIC_PAGES;
use url::Url;

#[test]
fn every_fixture_page_gives_exactly_its_findings() {
    let mut failures = Vec::new();
    for page in STATIC_PAGES {
        let url = Url::parse(&format!("https://example.com{}", page.path)).unwrap();
        let data = extract_page(&PageInput {
            url: &url,
            charset: None,
            body: page.html.as_bytes(),
            truncated: false,
            robots_token: "SpiderLens",
            content: None,
            a11y: true,
            hook: None,
        });
        let found = data.a11y.expect("checks were asked for");
        let mut got: Vec<(&str, u32)> = found
            .findings
            .iter()
            .map(|f| (f.check.as_str(), f.count))
            .collect();
        got.sort_unstable();
        let mut want = page.expect.to_vec();
        want.sort_unstable();
        if got != want {
            failures.push(format!("{}: expected {want:?}, got {got:?}", page.path));
        }
        for f in &found.findings {
            assert!(
                !f.nodes.is_empty(),
                "{}: {} has no node",
                page.path,
                f.check.as_str()
            );
        }
    }
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}

#[test]
fn every_check_has_a_failing_and_a_clean_page() {
    use spiderlens_engine::extract::a11y::StaticCheck;
    for c in StaticCheck::ALL {
        assert!(
            STATIC_PAGES
                .iter()
                .any(|p| p.expect.iter().any(|(id, _)| *id == c.as_str())),
            "no failing page for {}",
            c.as_str()
        );
    }
    assert!(STATIC_PAGES.iter().filter(|p| p.expect.is_empty()).count() >= 10);
}

/// T009: the checks never panic on malformed pages and never change what
/// spec 005 extracts from them.
#[test]
fn malformed_pages_are_safe_and_unchanged() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../test-fixtures/malformed");
    let mut n = 0;
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_none_or(|e| e != "html") {
            continue;
        }
        let body = std::fs::read(&path).unwrap();
        let url = Url::parse("https://example.test/dir/page.html").unwrap();
        let input = |a11y| PageInput {
            url: &url,
            charset: None,
            body: &body,
            truncated: false,
            robots_token: "SpiderLens",
            content: None,
            a11y,
            hook: None,
        };
        let off = extract_page(&input(false));
        let mut on = extract_page(&input(true));
        assert!(on.a11y.take().is_some(), "{}", path.display());
        assert_eq!(off, on, "{}", path.display());
        n += 1;
    }
    assert!(n >= 50, "corpus has {n} pages");
}
