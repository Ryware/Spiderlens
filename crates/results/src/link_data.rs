//! The Link metrics tab (spec 024 US5): what Ahrefs, Majestic and Moz said
//! about each crawled page, one set of columns per provider. The values are
//! read from `link_metrics` (one row per page and provider, the numbers in a
//! JSON object) by primary key, so a column costs one index probe per row on
//! screen.
//!
//! The table is always named `main.link_metrics`: spec 023's Link Score
//! builds a temporary table with the same name, which would otherwise hide
//! this one.

use std::borrow::Cow;

use crate::tabs::{HTML_OK, Need, TabDef, ValueKind, c, col, filter};

/// One provider metric of a page: `json_extract` of its stored numbers.
macro_rules! metric {
    ($provider:literal, $metric:literal) => {
        concat!(
            "(SELECT json_extract(l.metrics, '$.",
            $metric,
            "') FROM main.link_metrics l WHERE l.url_id = u.id AND l.provider = '",
            $provider,
            "')"
        )
    };
}

const FETCHED: &str = "(SELECT date(MAX(l.fetched_at), 'unixepoch') \
     FROM main.link_metrics l WHERE l.url_id = u.id)";

const WITH_DATA: &str = "EXISTS (SELECT 1 FROM main.link_metrics l WHERE l.url_id = u.id)";

const NO_DATA: &str = "NOT EXISTS (SELECT 1 FROM main.link_metrics l WHERE l.url_id = u.id) \
     AND EXISTS (SELECT 1 FROM integration_status s WHERE s.url_id = u.id \
     AND s.provider IN ('ahrefs', 'majestic', 'moz') AND s.reason = 'no_data')";

const NOT_FETCHED: &str = "NOT EXISTS (SELECT 1 FROM main.link_metrics l WHERE l.url_id = u.id) \
     AND NOT EXISTS (SELECT 1 FROM integration_status s WHERE s.url_id = u.id \
     AND s.provider IN ('ahrefs', 'majestic', 'moz'))";

/// A page with data from one provider.
macro_rules! has {
    ($provider:literal) => {
        concat!(
            "EXISTS (SELECT 1 FROM main.link_metrics l WHERE l.url_id = u.id AND l.provider = '",
            $provider,
            "')"
        )
    };
}

pub(crate) fn tab() -> TabDef {
    use ValueKind::{Decimal, Int, Text};
    let m = |d: crate::tabs::ColumnDef| d.needs(Need::LinkMetrics);
    TabDef {
        id: Cow::Borrowed("link-metrics"),
        title: Cow::Borrowed("Link metrics"),
        eligible: Cow::Borrowed(HTML_OK),
        columns: vec![
            c::ADDRESS,
            m(col(
                "ahrefs_domain_rating",
                "Domain Rating (Ahrefs)",
                metric!("ahrefs", "domain_rating"),
                Int,
                150,
            )),
            m(col(
                "ahrefs_url_rating",
                "URL Rating (Ahrefs)",
                metric!("ahrefs", "url_rating"),
                Int,
                140,
            )),
            m(col(
                "ahrefs_backlinks",
                "Backlinks (Ahrefs)",
                metric!("ahrefs", "backlinks"),
                Int,
                140,
            )
            .hidden()),
            m(col(
                "ahrefs_refdomains",
                "Referring domains (Ahrefs)",
                metric!("ahrefs", "refdomains"),
                Int,
                170,
            )
            .hidden()),
            m(col(
                "majestic_trust_flow",
                "Trust Flow (Majestic)",
                metric!("majestic", "trust_flow"),
                Int,
                150,
            )),
            m(col(
                "majestic_citation_flow",
                "Citation Flow (Majestic)",
                metric!("majestic", "citation_flow"),
                Int,
                160,
            )),
            m(col(
                "majestic_backlinks",
                "Backlinks (Majestic)",
                metric!("majestic", "backlinks"),
                Int,
                150,
            )
            .hidden()),
            m(col(
                "majestic_refdomains",
                "Referring domains (Majestic)",
                metric!("majestic", "refdomains"),
                Int,
                180,
            )
            .hidden()),
            m(col(
                "moz_page_authority",
                "Page Authority (Moz)",
                metric!("moz", "page_authority"),
                Decimal,
                150,
            )),
            m(col(
                "moz_domain_authority",
                "Domain Authority (Moz)",
                metric!("moz", "domain_authority"),
                Decimal,
                160,
            )),
            m(col(
                "moz_linking_domains",
                "Linking domains (Moz)",
                metric!("moz", "linking_domains"),
                Int,
                150,
            )
            .hidden()),
            m(col(
                "moz_backlinks",
                "Backlinks (Moz)",
                metric!("moz", "backlinks"),
                Int,
                130,
            )
            .hidden()),
            m(col(
                "moz_spam_score",
                "Spam Score (Moz)",
                metric!("moz", "spam_score"),
                Decimal,
                130,
            )
            .hidden()),
            m(col("link_fetched", "Link data fetched", FETCHED, Text, 130)),
            c::INDEXABILITY.hidden(),
            c::INLINKS.hidden(),
            c::DEPTH.hidden(),
        ],
        filters: vec![
            filter("all", "All", "1"),
            filter("with_data", "With link data", WITH_DATA),
            filter("no_data", "No data from the provider", NO_DATA),
            filter("not_fetched", "Not fetched", NOT_FETCHED),
            filter("ahrefs", "Has Ahrefs data", has!("ahrefs")),
            filter("majestic", "Has Majestic data", has!("majestic")),
            filter("moz", "Has Moz data", has!("moz")),
        ]
        .into_iter()
        .map(|f| f.needs(Need::LinkMetrics))
        .collect(),
        search_column: None,
        scoped: false,
        rows: None,
    }
}
