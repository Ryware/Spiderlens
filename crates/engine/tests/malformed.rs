//! SC-002: on the malformed-HTML corpus, extraction agrees with headless
//! Chromium on at least 98% of pages (plan D8). The browser's answers are in
//! `golden.json`, written by `scripts/browser-golden.mjs`.

use std::path::Path;

use serde_json::{Value, json};
use spiderlens_engine::crawl::LinkKind;
use spiderlens_engine::extract::{PageInput, RefSource, extract_page};
use url::Url;

fn ours(body: &[u8]) -> Value {
    let url = Url::parse("https://example.test/dir/page.html").unwrap();
    let d = extract_page(&PageInput {
        url: &url,
        charset: None,
        body,
        truncated: false,
        robots_token: "SpiderLens",
        keep_text: false,
        a11y: false,
        hook: None,
    });
    let raws = |kind: LinkKind| -> Vec<String> {
        d.refs
            .iter()
            .filter(|r| r.kind == kind && r.source == RefSource::Html)
            .map(|r| r.raw.trim().to_string())
            .collect()
    };
    json!({
        "title": d.title(),
        "description": d.meta_description(),
        "headings": d.headings.iter().map(|h| json!([h.level, h.text])).collect::<Vec<_>>(),
        "canonical": raws(LinkKind::Canonical),
        "links": raws(LinkKind::Anchor),
    })
}

#[test]
fn sc_002_agrees_with_chromium_on_malformed_pages() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../test-fixtures/malformed");
    let golden: serde_json::Map<String, Value> =
        serde_json::from_str(&std::fs::read_to_string(dir.join("golden.json")).unwrap()).unwrap();
    assert!(golden.len() >= 50, "SC-002 needs at least 50 pages");

    let mut differ = Vec::new();
    for (file, browser) in &golden {
        let got = ours(&std::fs::read(dir.join(file)).unwrap());
        if &got != browser {
            differ.push(format!("{file}\n  browser: {browser}\n  ours:    {got}"));
        }
    }
    let agree = (golden.len() - differ.len()) as f64 / golden.len() as f64;
    println!("{} of {} pages agree", golden.len() - differ.len(), golden.len());
    for d in &differ {
        println!("{d}");
    }
    assert!(agree >= 0.98, "agreement {:.1}% is under 98%", agree * 100.0);
}
