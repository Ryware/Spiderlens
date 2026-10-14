//! Spec 006 SC-007: crawl files written by earlier versions open in the
//! results table with the same rows, on every OS CI runs on.

use std::path::Path;

use spiderlens_results::{Reader, UrlScope, ViewQuery};

#[test]
fn reference_files_show_every_crawled_url() {
    let refs = Path::new(env!("CARGO_MANIFEST_DIR")).join("../engine/tests/reference");
    let expected: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(refs.join("expected.json")).unwrap()).unwrap();
    let dir = tempfile::tempdir().unwrap();
    for (name, want) in expected.as_object().unwrap() {
        let path = dir.path().join(name);
        std::fs::copy(refs.join(name), &path).unwrap();
        let mut r = Reader::open(&path).unwrap();
        let query = ViewQuery {
            tab: "internal".into(),
            filter: "all".into(),
            scope: UrlScope::All,
            search: String::new(),
            values: None,
            sort: None,
        };
        let page = r.open_view(&query, 50).unwrap();
        let total = r.materialize(page.view).unwrap();
        assert_eq!(u64::from(total), want["crawled"].as_u64().unwrap(), "{name}");
    }
}
