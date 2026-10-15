//! The Sitemaps results tab (spec 011 FR-026, plan D8): URLs listed in a
//! sitemap plus the internal HTML pages, so "Not in a sitemap" lives in the
//! same tab. Filters that test what a rule tests share its condition.

use std::borrow::Cow;

use crate::issues::rules::sitemaps as sm;
use crate::tabs::{HTML_OK, Need, TabDef, ValueKind, c, col, filter};

pub(crate) fn tab() -> TabDef {
    TabDef {
        id: Cow::Borrowed("sitemaps"),
        title: Cow::Borrowed("Sitemaps"),
        eligible: Cow::Owned(format!("u.in_sitemap = 1 OR ({HTML_OK})")),
        columns: vec![
            c::ADDRESS,
            c::STATUS_CODE,
            c::INDEXABILITY,
            c::FINAL_STATE,
            col(
                "sitemaps",
                "Sitemaps",
                "(SELECT COUNT(DISTINCT e.file_id) FROM sitemap_entries e WHERE e.url_id = u.id AND e.kind = 'url')",
                ValueKind::Int,
                90,
            )
            .needs(Need::SitemapEntries),
            col(
                "lastmod",
                "Lastmod",
                "(SELECT e.lastmod_raw FROM sitemap_entries e WHERE e.url_id = u.id AND e.kind = 'url' \
                 ORDER BY e.lastmod IS NULL, e.lastmod DESC LIMIT 1)",
                ValueKind::Text,
                190,
            )
            .needs(Need::SitemapEntries),
            col(
                "orphan_class",
                "Orphan class",
                "(SELECT CASE o.class WHEN 'orphan' THEN 'Orphan' ELSE 'Linked only through nofollow' END \
                 FROM orphans o WHERE o.url_id = u.id)",
                ValueKind::Text,
                200,
            )
            .needs(Need::Orphans),
            col(
                "first_sitemap",
                "First sitemap",
                "(SELECT f.location FROM sitemap_entries e JOIN sitemap_files f ON f.id = e.file_id \
                 WHERE e.url_id = u.id AND e.kind = 'url' ORDER BY e.id LIMIT 1)",
                ValueKind::Url,
                320,
            )
            .needs(Need::SitemapEntries)
            .hidden(),
            c::SOURCE,
            c::INLINKS,
        ],
        filters: vec![
            filter("all", "All sitemap URLs", "u.in_sitemap = 1"),
            filter("indexable", "In sitemap and indexable", sm::INDEXABLE).needs(Need::Extraction),
            filter("not_indexable", "In sitemap and not indexable", sm::NOT_INDEXABLE).needs(Need::Extraction),
            filter("not_in_sitemap", "Not in a sitemap", sm::NOT_IN_SITEMAP).needs(Need::SitemapEntries),
            filter("orphans", "Orphan pages", sm::ORPHAN).needs(Need::Orphans),
            filter("nofollow_only", "Linked only through nofollow", sm::NOFOLLOW_ONLY).needs(Need::Orphans),
            filter("not_crawled", "Not crawled", sm::NOT_CRAWLED),
        ],
        search_column: None,
        scoped: false,
        rows: None,
    }
}
