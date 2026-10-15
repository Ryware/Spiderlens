//! What a crawl file records about its issues (spec 009 FR-013, plan D8).

use spiderlens_config::{IssueSettings, RuleOverride};
use spiderlens_results::Reader;
use spiderlens_results::issues::{RULES_VERSION, record_issue_settings};
use spiderlens_results::synth::Synth;

#[test]
fn settings_and_rules_version_are_recorded_and_read_back() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("c.spiderlens");
    Synth {
        urls: 50,
        links_per_page: 2,
        seed: 3,
        ..Synth::default()
    }
    .write(&path)
    .unwrap();
    let r = Reader::open(&path).unwrap();
    assert_eq!(r.recorded_rules_version().unwrap(), None);
    drop(r);

    let mut s = IssueSettings::default();
    s.rules.insert(
        "title.over".into(),
        RuleOverride {
            enabled: None,
            priority: None,
            threshold: Some(70.0),
        },
    );
    record_issue_settings(&path, &s).unwrap();
    let r = Reader::open(&path).unwrap();
    assert_eq!(r.recorded_rules_version().unwrap(), Some(RULES_VERSION));
    assert_eq!(r.stored_issue_settings().unwrap(), Some(s));
}

#[test]
fn a_file_that_is_not_a_crawl_file_is_not_written() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("other.db");
    rusqlite::Connection::open(&path)
        .unwrap()
        .execute_batch("CREATE TABLE meta (key TEXT PRIMARY KEY, value TEXT NOT NULL); INSERT INTO meta VALUES ('format', 'other');")
        .unwrap();
    assert!(record_issue_settings(&path, &IssueSettings::default()).is_err());
    let n: i64 = rusqlite::Connection::open(&path)
        .unwrap()
        .query_row("SELECT COUNT(*) FROM meta", [], |r| r.get(0))
        .unwrap();
    assert_eq!(n, 1);
}

#[test]
fn a_file_without_page_data_shows_those_rules_unavailable_with_the_reason() {
    // Synth files list no data features, like files made before extraction
    // (spec 009 US6).
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("c.spiderlens");
    Synth {
        urls: 50,
        links_per_page: 2,
        seed: 3,
        ..Synth::default()
    }
    .write(&path)
    .unwrap();
    let mut r = Reader::open(&path).unwrap();
    let summary = r.issues_summary(false).unwrap();
    let headings = summary
        .issues
        .iter()
        .find(|i| i.id == "headings.non_sequential")
        .unwrap();
    assert_eq!(headings.count, None);
    assert!(headings.unavailable.as_deref().is_some_and(|m| !m.is_empty()));
    let status = summary
        .issues
        .iter()
        .find(|i| i.id == "response.internal_4xx")
        .unwrap();
    assert!(status.count.is_some() && status.unavailable.is_none());
}
