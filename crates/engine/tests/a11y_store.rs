//! Spec 026 storage: the accessibility and mobile tables, the writers that
//! fill them (`record_static_a11y`, `record_a11y`, `set_a11y_state`,
//! `record_mobile`), de-duplication of static against full results, the
//! node caps and the page counts (T014, FR-010, FR-011).

use std::borrow::Cow;
use std::path::PathBuf;

use spiderlens_engine::crawl::CrawlStore;
use spiderlens_engine::crawl::store::{A11yNodeRow, A11yRow, MobileRow};

fn store() -> (tempfile::TempDir, PathBuf, CrawlStore) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a11y.spiderlens");
    let s = CrawlStore::create(&path).unwrap();
    (dir, path, s)
}

fn nodes(n: usize) -> Vec<A11yNodeRow<'static>> {
    (0..n)
        .map(|i| A11yNodeRow {
            selector: Cow::Owned(format!("#n{i}")),
            excerpt: Cow::Borrowed("<a href=\"/x\">"),
            data: None,
            bbox: None,
        })
        .collect()
}

fn row(
    check: &str,
    kind: &'static str,
    source: &'static str,
    impact: Option<&'static str>,
    wcag: &str,
    total: u32,
    stored_nodes: usize,
) -> A11yRow<'static> {
    A11yRow {
        check_id: Cow::Owned(check.to_string()),
        kind,
        source,
        impact,
        wcag: Cow::Owned(wcag.to_string()),
        nodes_total: total,
        nodes: nodes(stored_nodes),
    }
}

fn count(s: &CrawlStore, sql: &str, id: i64) -> i64 {
    s.conn().query_row(sql, [id], |r| r.get(0)).unwrap()
}

fn check_ids(s: &CrawlStore, id: i64, source: &str) -> Vec<String> {
    let mut stmt = s
        .conn()
        .prepare("SELECT check_id FROM a11y_results WHERE url_id = ?1 AND source = ?2 ORDER BY check_id")
        .unwrap();
    stmt.query_map(rusqlite::params![id, source], |r| r.get(0))
        .unwrap()
        .map(|r| r.unwrap())
        .collect()
}

/// (critical, serious, moderate, minor, needs_review, checks_failed, elements, lowest_level)
type Audit = (i64, i64, i64, i64, i64, i64, i64, Option<String>);

fn audit(s: &CrawlStore, id: i64) -> Audit {
    s.conn()
        .query_row(
            "SELECT critical, serious, moderate, minor, needs_review, checks_failed, elements, lowest_level \
             FROM a11y_audits WHERE url_id = ?1",
            [id],
            |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                    r.get(6)?,
                    r.get(7)?,
                ))
            },
        )
        .unwrap()
}

#[test]
fn a_new_file_has_the_four_accessibility_tables_and_their_indexes() {
    let (_d, _p, s) = store();
    let names = |kind: &str| -> Vec<String> {
        let mut stmt = s
            .conn()
            .prepare("SELECT name FROM sqlite_master WHERE type = ?1 ORDER BY name")
            .unwrap();
        stmt.query_map([kind], |r| r.get(0))
            .unwrap()
            .map(|r| r.unwrap())
            .collect()
    };
    let tables = names("table");
    for t in ["a11y_audits", "a11y_results", "a11y_nodes", "mobile_measures"] {
        assert!(tables.iter().any(|n| n == t), "missing table {t}");
        let rows: i64 = s
            .conn()
            .query_row(&format!("SELECT COUNT(*) FROM {t}"), [], |r| r.get(0))
            .unwrap();
        assert_eq!(rows, 0, "{t} starts empty");
    }
    let indexes = names("index");
    for i in ["a11y_results_url", "a11y_results_check", "a11y_nodes_result"] {
        assert!(indexes.iter().any(|n| n == i), "missing index {i}");
    }
}

#[test]
fn static_results_are_stored_with_their_counts_and_the_mobile_row() {
    let (_d, _p, s) = store();
    let rows = [
        row(
            "static.form_label",
            "violation",
            "static",
            Some("critical"),
            "wcag2a wcag131",
            4,
            4,
        ),
        row(
            "static.link_name",
            "violation",
            "static",
            Some("serious"),
            "wcag22aa wcag258",
            3,
            3,
        ),
        row(
            "static.tabindex_positive",
            "violation",
            "static",
            Some("minor"),
            "best-practice",
            2,
            2,
        ),
    ];
    let mobile = MobileRow {
        viewport: Some("width=device-width"),
        viewport_state: Some("ok"),
        plugins: Some(0),
        ..MobileRow::default()
    };
    s.record_static_a11y(1, &rows, &mobile).unwrap();

    assert_eq!(
        audit(&s, 1),
        (1, 1, 0, 1, 0, 3, 9, Some("2.0-a".to_string())),
        "critical, serious, moderate, minor, needs_review, checks, elements, lowest level"
    );
    let state: String = s
        .conn()
        .query_row("SELECT static_state FROM a11y_audits WHERE url_id = 1", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(state, "checked");
    assert_eq!(
        check_ids(&s, 1, "static"),
        [
            "static.form_label",
            "static.link_name",
            "static.tabindex_positive"
        ]
    );
    assert_eq!(
        count(
            &s,
            "SELECT COUNT(*) FROM a11y_nodes WHERE result_id IN (SELECT id FROM a11y_results WHERE url_id = ?1)",
            1
        ),
        9
    );
    let (viewport, state): (String, String) = s
        .conn()
        .query_row(
            "SELECT viewport, viewport_state FROM mobile_measures WHERE url_id = 1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!((viewport.as_str(), state.as_str()), ("width=device-width", "ok"));

    // A page without findings is still a checked page.
    s.record_static_a11y(2, &[], &MobileRow::default()).unwrap();
    assert_eq!(audit(&s, 2), (0, 0, 0, 0, 0, 0, 0, None));
    assert!(check_ids(&s, 2, "static").is_empty());
}

#[test]
fn a_full_result_replaces_the_static_results_that_test_the_same_thing() {
    let (_d, _p, s) = store();
    let statics = || {
        [
            row(
                "static.form_label",
                "violation",
                "static",
                Some("critical"),
                "wcag2a",
                2,
                2,
            ),
            row(
                "static.link_name",
                "violation",
                "static",
                Some("serious"),
                "wcag2a",
                1,
                1,
            ),
            row(
                "static.plugin_element",
                "violation",
                "static",
                Some("moderate"),
                "wcag2a",
                1,
                1,
            ),
        ]
    };
    s.record_static_a11y(1, &statics(), &MobileRow::default())
        .unwrap();

    // `label` is the same as static form_label (same_as); link-name is not run.
    let full = [
        row(
            "label",
            "violation",
            "full",
            Some("critical"),
            "wcag2a wcag412",
            5,
            5,
        ),
        row(
            "color-contrast",
            "violation",
            "full",
            Some("serious"),
            "wcag2aa wcag143",
            7,
            7,
        ),
    ];
    s.record_a11y(1, "full", &full).unwrap();
    assert_eq!(
        check_ids(&s, 1, "static"),
        ["static.link_name", "static.plugin_element"],
        "static.form_label was dropped for the full label result"
    );
    assert_eq!(check_ids(&s, 1, "full"), ["color-contrast", "label"]);
    // Counts cover both sources: label (critical), color-contrast + link_name
    // (serious), plugin_element (moderate); elements add up per result.
    assert_eq!(audit(&s, 1), (1, 2, 1, 0, 0, 4, 14, Some("2.0-a".to_string())));

    // Writing the static results again does not bring form_label back.
    s.record_a11y(1, "static", &statics()).unwrap();
    assert_eq!(
        check_ids(&s, 1, "static"),
        ["static.link_name", "static.plugin_element"]
    );

    // Writing the full results again replaces them, not adds to them.
    s.record_a11y(1, "full", &full[1..]).unwrap();
    assert_eq!(check_ids(&s, 1, "full"), ["color-contrast"]);
    assert_eq!(
        check_ids(&s, 1, "static"),
        ["static.link_name", "static.plugin_element"],
        "form_label stays gone: the page was checked fully once, and nothing writes it again"
    );
}

#[test]
fn nodes_are_capped_at_20_per_result_and_500_per_page() {
    let (_d, _p, s) = store();
    // One result: 50 failing elements, 20 stored, the real number kept.
    s.record_a11y(
        1,
        "full",
        &[row(
            "link-name",
            "violation",
            "full",
            Some("serious"),
            "wcag2a",
            50,
            50,
        )],
    )
    .unwrap();
    assert_eq!(
        count(
            &s,
            "SELECT COUNT(*) FROM a11y_nodes WHERE result_id IN (SELECT id FROM a11y_results WHERE url_id = ?1)",
            1
        ),
        20
    );
    assert_eq!(
        count(&s, "SELECT nodes_total FROM a11y_results WHERE url_id = ?1", 1),
        50
    );
    assert_eq!(audit(&s, 1).6, 50, "the elements count is the real number");

    // 40 results of 20 nodes: 800 offered, 500 stored; every result is kept.
    let many: Vec<A11yRow<'static>> = (0..40)
        .map(|i| {
            row(
                &format!("rule-{i:02}"),
                "violation",
                "full",
                Some("minor"),
                "best-practice",
                20,
                20,
            )
        })
        .collect();
    s.record_a11y(2, "full", &many).unwrap();
    assert_eq!(
        count(
            &s,
            "SELECT COUNT(*) FROM a11y_nodes WHERE result_id IN (SELECT id FROM a11y_results WHERE url_id = ?1)",
            2
        ),
        500
    );
    assert_eq!(
        count(&s, "SELECT COUNT(*) FROM a11y_results WHERE url_id = ?1", 2),
        40
    );
    assert_eq!(audit(&s, 2).5, 40);
    assert_eq!(audit(&s, 2).7.as_deref(), Some("best_practice"));

    // The same cap holds for static results.
    let statics: Vec<A11yRow<'static>> = (0..30)
        .map(|i| {
            row(
                &format!("static.x{i}"),
                "violation",
                "static",
                Some("minor"),
                "wcag2a",
                20,
                20,
            )
        })
        .collect();
    s.record_static_a11y(3, &statics, &MobileRow::default()).unwrap();
    assert_eq!(
        count(
            &s,
            "SELECT COUNT(*) FROM a11y_nodes WHERE result_id IN (SELECT id FROM a11y_results WHERE url_id = ?1)",
            3
        ),
        500
    );
}

#[test]
fn counts_split_by_impact_and_needs_review_and_the_lowest_level_follows_the_tags() {
    let (_d, _p, s) = store();
    let rows = [
        row(
            "image-alt",
            "violation",
            "full",
            Some("critical"),
            "wcag2a wcag111",
            1,
            1,
        ),
        row(
            "color-contrast",
            "violation",
            "full",
            Some("serious"),
            "wcag2aa wcag143",
            2,
            2,
        ),
        row(
            "target-size",
            "violation",
            "full",
            Some("moderate"),
            "wcag22aa wcag258",
            3,
            3,
        ),
        row(
            "region",
            "violation",
            "full",
            Some("moderate"),
            "best-practice",
            4,
            4,
        ),
        row(
            "heading-order",
            "violation",
            "full",
            Some("minor"),
            "best-practice",
            5,
            5,
        ),
        row(
            "color-contrast-enhanced",
            "needs_review",
            "full",
            Some("serious"),
            "wcag2aaa wcag146",
            6,
            6,
        ),
    ];
    s.record_a11y(1, "full", &rows).unwrap();
    // Needs-review results are not violations: they count only as needs_review
    // and add no elements.
    assert_eq!(audit(&s, 1), (1, 1, 2, 1, 1, 5, 15, Some("2.0-a".to_string())));

    // 2.0 AA is lower than 2.1 A; a criterion number alone (wcag143) is no level.
    s.record_a11y(
        2,
        "full",
        &[
            row("a", "violation", "full", Some("minor"), "wcag21a wcag111", 1, 1),
            row("b", "violation", "full", Some("minor"), "wcag2aa wcag143", 1, 1),
        ],
    )
    .unwrap();
    assert_eq!(audit(&s, 2).7.as_deref(), Some("2.0-aa"));

    s.record_a11y(
        3,
        "full",
        &[row("a", "violation", "full", Some("minor"), "wcag22aaa", 1, 1)],
    )
    .unwrap();
    assert_eq!(audit(&s, 3).7.as_deref(), Some("2.2-aaa"));

    // Rows written again for the same source leave only the new counts.
    s.record_a11y(1, "full", &rows[..1]).unwrap();
    assert_eq!(audit(&s, 1), (1, 0, 0, 0, 0, 1, 1, Some("2.0-a".to_string())));
}

#[test]
fn the_state_is_set_without_touching_the_results_and_creates_the_row() {
    let (_d, _p, s) = store();
    s.set_a11y_state(1, "full", "not_checked", Some("too_large"), None)
        .unwrap();
    let (full, reason, ms, stat): (String, Option<String>, Option<i64>, Option<String>) = s
        .conn()
        .query_row(
            "SELECT full_state, full_reason, check_ms, static_state FROM a11y_audits WHERE url_id = 1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .unwrap();
    assert_eq!(
        (full.as_str(), reason.as_deref(), ms, stat),
        ("not_checked", Some("too_large"), None, None)
    );

    s.set_a11y_state(1, "full", "timed_out", None, Some(5000))
        .unwrap();
    s.set_a11y_state(1, "static", "checked", None, None).unwrap();
    let (full, reason, ms, stat): (String, Option<String>, Option<i64>, String) = s
        .conn()
        .query_row(
            "SELECT full_state, full_reason, check_ms, static_state FROM a11y_audits WHERE url_id = 1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .unwrap();
    assert_eq!(
        (full.as_str(), reason, ms, stat.as_str()),
        ("timed_out", None, Some(5000), "checked")
    );

    // Recording results afterwards keeps both states.
    s.record_a11y(
        1,
        "full",
        &[row(
            "label",
            "violation",
            "full",
            Some("critical"),
            "wcag2a",
            1,
            1,
        )],
    )
    .unwrap();
    let (full, stat): (String, String) = s
        .conn()
        .query_row(
            "SELECT full_state, static_state FROM a11y_audits WHERE url_id = 1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!((full.as_str(), stat.as_str()), ("timed_out", "checked"));
    assert_eq!(audit(&s, 1).0, 1);
}

#[test]
fn mobile_measures_keep_what_a_later_write_leaves_out() {
    let (_d, _p, s) = store();
    // The fetch stage writes the HTML facts ...
    s.record_mobile(
        1,
        &MobileRow {
            viewport: Some("width=device-width, initial-scale=1"),
            viewport_state: Some("ok"),
            plugins: Some(2),
            ..MobileRow::default()
        },
    )
    .unwrap();
    // ... and the render stage the layout measures.
    s.record_mobile(
        1,
        &MobileRow {
            window: Some("mobile"),
            measured: Some("yes"),
            content_px: Some(820.0),
            window_px: Some(411.0),
            widest: Some("div.banner"),
            small_text_share: Some(12.5),
            small_targets: Some(3),
            examples: Some("[{\"selector\":\"a.x\",\"w\":10,\"h\":10}]".to_string()),
            ..MobileRow::default()
        },
    )
    .unwrap();
    #[allow(clippy::type_complexity)]
    let got: (
        String,
        String,
        i64,
        String,
        String,
        f64,
        f64,
        String,
        f64,
        i64,
        String,
    ) = s
        .conn()
        .query_row(
            "SELECT viewport, viewport_state, plugins, \"window\", measured, content_px, window_px, widest, \
             small_text_share, small_targets, examples FROM mobile_measures WHERE url_id = 1",
            [],
            |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                    r.get(6)?,
                    r.get(7)?,
                    r.get(8)?,
                    r.get(9)?,
                    r.get(10)?,
                ))
            },
        )
        .unwrap();
    assert_eq!(got.0, "width=device-width, initial-scale=1");
    assert_eq!((got.1.as_str(), got.2), ("ok", 2));
    assert_eq!((got.3.as_str(), got.4.as_str()), ("mobile", "yes"));
    assert_eq!((got.5, got.6, got.7.as_str()), (820.0, 411.0, "div.banner"));
    assert_eq!((got.8, got.9), (12.5, 3));
    assert!(got.10.contains("a.x"));

    // A later write with a value replaces it; one row per page.
    s.record_mobile(
        1,
        &MobileRow {
            measured: Some("timed_out"),
            ..MobileRow::default()
        },
    )
    .unwrap();
    let measured: String = s
        .conn()
        .query_row("SELECT measured FROM mobile_measures WHERE url_id = 1", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(measured, "timed_out");
    assert_eq!(
        count(&s, "SELECT COUNT(*) FROM mobile_measures WHERE url_id = ?1", 1),
        1
    );
}
