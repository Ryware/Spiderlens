//! The Index status and Analytics results tabs (spec 024 US2 and US4): what
//! Google's URL Inspection and Google Analytics 4 say about each crawled
//! page, fetched after the crawl from Integrations. Each value is one
//! primary-key lookup per visible row.

use std::borrow::Cow;

use crate::tabs::{HTML_OK, Need, TabDef, ValueKind, c, col, filter};

/// One value from a page's URL Inspection row.
macro_rules! insp {
    ($field:literal) => {
        concat!(
            "(SELECT i.",
            $field,
            " FROM gsc_inspection i WHERE i.url_id = u.id)"
        )
    };
}

/// One value from a page's Analytics row.
macro_rules! ga {
    ($field:literal) => {
        concat!("(SELECT g.", $field, " FROM ga4_pages g WHERE g.url_id = u.id)")
    };
}

const INSPECTED: &str = "EXISTS (SELECT 1 FROM gsc_inspection i WHERE i.url_id = u.id)";
const NOT_INDEXED: &str =
    "EXISTS (SELECT 1 FROM gsc_inspection i WHERE i.url_id = u.id AND COALESCE(i.verdict, '') <> 'PASS')";
const CANONICAL_MISMATCH: &str =
    "EXISTS (SELECT 1 FROM gsc_inspection i WHERE i.url_id = u.id AND i.canonical_mismatch = 1)";
const BLOCKED_BY_ROBOTS: &str = "EXISTS (SELECT 1 FROM gsc_inspection i WHERE i.url_id = u.id \
     AND (i.robots_state = 'DISALLOWED' OR i.page_fetch_state = 'BLOCKED_ROBOTS_TXT'))";

/// Why a page has no result: not asked yet, the daily allowance ran out, or
/// Google answered with an error.
const INSPECTION_STATUS: &str = "(CASE \
     WHEN EXISTS (SELECT 1 FROM gsc_inspection i WHERE i.url_id = u.id) THEN 'Inspected' \
     WHEN EXISTS (SELECT 1 FROM integration_status s WHERE s.url_id = u.id AND s.provider = 'url_inspection' \
       AND s.reason = 'quota') THEN 'Not inspected: daily allowance used' \
     WHEN EXISTS (SELECT 1 FROM integration_status s WHERE s.url_id = u.id AND s.provider = 'url_inspection' \
       AND s.reason = 'no_data') THEN 'No data from Google' \
     WHEN EXISTS (SELECT 1 FROM integration_status s WHERE s.url_id = u.id AND s.provider = 'url_inspection') \
       THEN 'Not inspected: Google returned an error' \
     ELSE 'Not inspected' END)";

const VERDICT: &str = "(SELECT CASE i.verdict WHEN 'PASS' THEN 'Pass' WHEN 'PARTIAL' THEN 'Partial' \
     WHEN 'FAIL' THEN 'Fail' WHEN 'NEUTRAL' THEN 'Neutral' ELSE i.verdict END \
     FROM gsc_inspection i WHERE i.url_id = u.id)";

pub(crate) fn index_status_tab() -> TabDef {
    use ValueKind::*;
    let needs = Need::UrlInspection;
    TabDef {
        id: Cow::Borrowed("index-status"),
        title: Cow::Borrowed("Index status"),
        eligible: Cow::Borrowed(HTML_OK),
        columns: vec![
            c::ADDRESS,
            col("index_verdict", "Verdict (Google)", VERDICT, Text, 110).needs(needs),
            col("index_coverage", "Coverage (Google)", insp!("coverage_state"), Text, 240).needs(needs),
            col(
                "google_canonical",
                "Google-selected canonical",
                insp!("google_canonical"),
                Url,
                320,
            )
            .needs(needs),
            col(
                "canonical_mismatch",
                "Canonical mismatch",
                insp!("canonical_mismatch"),
                Bool,
                140,
            )
            .needs(needs),
            col(
                "google_last_crawl",
                "Last crawled by Google",
                "(SELECT strftime('%Y-%m-%d %H:%M', i.last_crawl, 'unixepoch') FROM gsc_inspection i WHERE i.url_id = u.id)",
                Text,
                160,
            )
            .needs(needs),
            col("google_robots", "robots.txt (Google)", insp!("robots_state"), Text, 140).needs(needs),
            c::INDEXABILITY,
            c::TITLE,
            col(
                "user_canonical",
                "User-declared canonical",
                insp!("user_canonical"),
                Url,
                320,
            )
            .needs(needs)
            .hidden(),
            col("crawled_as", "Crawled as", insp!("crawled_as"), Text, 110)
                .needs(needs)
                .hidden(),
            col("page_fetch", "Page fetch (Google)", insp!("page_fetch_state"), Text, 160)
                .needs(needs)
                .hidden(),
            col("indexing_state", "Indexing (Google)", insp!("indexing_state"), Text, 170)
                .needs(needs)
                .hidden(),
            col("inspection", "Inspection", INSPECTION_STATUS, Text, 220)
                .needs(needs)
                .hidden(),
            c::DEPTH.hidden(),
        ],
        filters: [
            filter("all", "All", "1"),
            filter("inspected", "Inspected", INSPECTED),
            filter(
                "not_inspected",
                "Not inspected",
                "NOT EXISTS (SELECT 1 FROM gsc_inspection i WHERE i.url_id = u.id)",
            ),
            filter("not_indexed", "Not indexed", NOT_INDEXED),
            filter("canonical_mismatch", "Canonical mismatch", CANONICAL_MISMATCH),
            filter("blocked_by_robots", "Blocked by robots per Google", BLOCKED_BY_ROBOTS),
        ]
        .into_iter()
        .map(|f| f.needs(needs))
        .collect(),
        search_column: Some(Cow::Borrowed("title")),
        scoped: false,
        rows: None,
    }
}

const GA_WITH_SESSIONS: &str = "COALESCE((SELECT g.sessions FROM ga4_pages g WHERE g.url_id = u.id), 0) > 0";
const GA_INDEXABLE_NO_SESSIONS: &str = "indexability(u.state, u.error_class, u.status, u.content_type, u.noindex, u.canonical, u.url) = 'Indexable' \
     AND COALESCE((SELECT g.sessions FROM ga4_pages g WHERE g.url_id = u.id), 0) = 0";
const GA_SESSIONS_NOT_INDEXABLE: &str = "indexability(u.state, u.error_class, u.status, u.content_type, u.noindex, u.canonical, u.url) <> 'Indexable' \
     AND COALESCE((SELECT g.sessions FROM ga4_pages g WHERE g.url_id = u.id), 0) > 0";
const GA_NO_DATA: &str = "EXISTS (SELECT 1 FROM integration_status s WHERE s.url_id = u.id \
     AND s.provider = 'analytics' AND s.reason = 'no_data')";
const GA_DATA: &str = "(CASE WHEN EXISTS (SELECT 1 FROM integration_status s WHERE s.url_id = u.id \
     AND s.provider = 'analytics' AND s.reason = 'no_data') THEN 'No data from Analytics' \
     WHEN EXISTS (SELECT 1 FROM ga4_pages g WHERE g.url_id = u.id) THEN 'Has data' ELSE 'Not fetched' END)";

pub(crate) fn analytics_tab() -> TabDef {
    use ValueKind::*;
    let needs = Need::Analytics;
    TabDef {
        id: Cow::Borrowed("analytics"),
        title: Cow::Borrowed("Analytics"),
        eligible: Cow::Borrowed(HTML_OK),
        columns: vec![
            c::ADDRESS,
            col("ga_sessions", "Sessions (Analytics)", ga!("sessions"), Int, 130).needs(needs),
            col(
                "ga_engaged_sessions",
                "Engaged sessions (Analytics)",
                ga!("engaged_sessions"),
                Int,
                160,
            )
            .needs(needs),
            col(
                "ga_engagement_rate",
                "Engagement rate (Analytics)",
                "(SELECT g.engagement_rate * 100 FROM ga4_pages g WHERE g.url_id = u.id)",
                Percent,
                160,
            )
            .needs(needs),
            col(
                "ga_key_events",
                "Key events (Analytics)",
                ga!("key_events"),
                Decimal,
                140,
            )
            .needs(needs),
            col(
                "ga_revenue",
                "Total revenue (Analytics)",
                ga!("revenue"),
                Decimal,
                150,
            )
            .needs(needs),
            c::INDEXABILITY,
            c::TITLE,
            c::INLINKS,
            c::DEPTH,
            col("ga_data", "Analytics data", GA_DATA, Text, 170)
                .needs(needs)
                .hidden(),
        ],
        filters: [
            filter("all", "All", "1"),
            filter("with_sessions", "With sessions", GA_WITH_SESSIONS),
            filter("no_sessions", "Indexable, no sessions", GA_INDEXABLE_NO_SESSIONS),
            filter(
                "sessions_not_indexable",
                "Has sessions, not indexable",
                GA_SESSIONS_NOT_INDEXABLE,
            ),
            filter("no_data", "No data from Analytics", GA_NO_DATA),
        ]
        .into_iter()
        .map(|f| f.needs(needs))
        .collect(),
        search_column: Some(Cow::Borrowed("title")),
        scoped: false,
        rows: None,
    }
}
