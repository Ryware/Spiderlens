//! Sitemaps (spec 011 "First rule set", plan D7). The Sitemaps tab filters
//! use these same conditions (FR-005 of spec 009).
//!
//! Three kinds of rule live here:
//! - per-URL rules that compare a sitemap URL with its own crawl data
//!   (redirect, error, noindex, blocked, not crawled, not in a sitemap):
//!   live, so they update while the crawl runs;
//! - rules that read the `sitemap_audit` and `orphans` passes' tables, which
//!   are there once the crawl has ended;
//! - file and site problems with no URL of their own (a sitemap that could
//!   not be read, no sitemap at all): site-wide rules counted on the crawl's
//!   start URL, whose evidence names the files; the Sitemap files view
//!   lists them all (research R8).
//!
//! URLs that were not fetched are "not checked" by the response and
//! indexability rules and show in "Sitemap URL not crawled" instead
//! (FR-017).

use crate::issues::{Category::Sitemaps as S, Priority::*, Rule, Scope, Unit, rule};
use crate::tabs::Need;

/// The first sitemap file that lists this URL.
macro_rules! in_file {
    () => {
        "(SELECT f.location FROM sitemap_entries e JOIN sitemap_files f ON f.id = e.file_id \
         WHERE e.url_id = u.id AND e.kind = 'url' ORDER BY e.id LIMIT 1)"
    };
}

/// `what` and " · in <first sitemap>" when a sitemap lists the URL.
macro_rules! with_file {
    ($what:literal) => {
        concat!("(", $what, ") || COALESCE(' · in ' || ", in_file!(), ", '')")
    };
}

/// A problem with sitemap files rather than URLs, counted once on the start
/// URL.
macro_rules! file_cond {
    ($cond:literal) => {
        concat!(
            "u.source = 'start' AND EXISTS (SELECT 1 FROM sitemap_files f WHERE ",
            $cond,
            ")"
        )
    };
}

/// How many files have the problem, and the first of them.
macro_rules! file_evidence {
    ($cond:literal) => {
        concat!(
            "(SELECT COUNT(*) || CASE COUNT(*) WHEN 1 THEN ' sitemap: ' ELSE ' sitemaps, first: ' END || MIN(f.location) \
             FROM sitemap_files f WHERE ",
            $cond,
            ")"
        )
    };
}

/// A `sitemap_audit` problem recorded for this URL.
macro_rules! problem {
    ($rule:literal) => {
        concat!(
            "u.id IN (SELECT p.url_id FROM sitemap_problems p WHERE p.rule = '",
            $rule,
            "' AND p.url_id IS NOT NULL)"
        )
    };
}

pub const CLIENT_ERROR: &str = "u.in_sitemap = 1 AND u.fetched_at IS NOT NULL AND u.error_class IS NULL AND u.status BETWEEN 400 AND 499";
pub const SERVER_ERROR: &str = "u.in_sitemap = 1 AND u.fetched_at IS NOT NULL \
     AND (u.error_class IS NOT NULL OR u.status IS NULL OR u.status >= 500)";
pub const REDIRECT: &str = "u.in_sitemap = 1 AND u.fetched_at IS NOT NULL AND u.error_class IS NULL AND u.status BETWEEN 300 AND 399";
pub const NOINDEX: &str = "u.in_sitemap = 1 AND u.fetched_at IS NOT NULL AND u.error_class IS NULL \
     AND u.status BETWEEN 200 AND 299 AND u.noindex = 1";
pub const CANONICALISED: &str = "u.in_sitemap = 1 AND u.fetched_at IS NOT NULL AND u.error_class IS NULL \
     AND u.status BETWEEN 200 AND 299 AND u.canonical IS NOT NULL AND u.canonical <> u.url";
pub const BLOCKED: &str = "u.in_sitemap = 1 AND u.reason IN ('blocked_by_robots', 'robots_unavailable')";
pub const NOT_CRAWLED: &str = "u.in_sitemap = 1 AND u.internal = 1 AND u.fetched_at IS NULL \
     AND COALESCE(u.reason, '') NOT IN ('blocked_by_robots', 'robots_unavailable')";
/// In a sitemap, answered 200 and indexable: the Sitemaps tab's "In sitemap
/// and indexable".
pub const INDEXABLE: &str = "u.in_sitemap = 1 AND u.fetched_at IS NOT NULL AND u.error_class IS NULL \
     AND u.status BETWEEN 200 AND 299 AND COALESCE(u.noindex, 0) = 0 \
     AND (u.canonical IS NULL OR u.canonical = u.url)";
/// Fetched, in a sitemap and not indexable for any reason.
pub const NOT_INDEXABLE: &str = "u.in_sitemap = 1 AND u.fetched_at IS NOT NULL AND NOT (u.error_class IS NULL \
     AND u.status BETWEEN 200 AND 299 AND COALESCE(u.noindex, 0) = 0 \
     AND (u.canonical IS NULL OR u.canonical = u.url))";
/// Over Internal HTML pages that returned 2xx (the rule's scope); only once
/// the crawl read a URL sitemap, so a crawl without sitemaps flags nothing.
pub const NOT_IN_SITEMAP: &str = "u.in_sitemap = 0 AND COALESCE(u.noindex, 0) = 0 \
     AND (u.canonical IS NULL OR u.canonical = u.url) \
     AND EXISTS (SELECT 1 FROM sitemap_files f WHERE f.kind = 'urlset')";
pub const ORPHAN: &str = "u.id IN (SELECT o.url_id FROM orphans o WHERE o.class = 'orphan')";
pub const NOFOLLOW_ONLY: &str = "u.id IN (SELECT o.url_id FROM orphans o WHERE o.class = 'nofollow_only')";

pub const RULES: &[Rule] = &[
    rule("sitemap.client_error", "Client error in sitemap (4xx)", S, High, Scope::Known, CLIENT_ERROR)
        .needs(Need::SitemapEntries)
        .evidence(with_file!("u.status"))
        .explain(
            "The sitemap asks search engines to index a URL that answers with a client error, so they waste crawl budget on it and trust the sitemap less.",
            "Remove the URL from the sitemap, or restore the page if it should exist.",
        ),
    rule(
        "sitemap.server_error",
        "Server error or no response in sitemap",
        S,
        High,
        Scope::Known,
        SERVER_ERROR,
    )
    .needs(Need::SitemapEntries)
    .evidence(with_file!(
        "CASE WHEN u.error_class IS NOT NULL THEN error_label(u.error_class) ELSE COALESCE(u.status, 'No response') END"
    ))
    .explain(
        "A URL in the sitemap failed with a server error or did not answer. Search engines slow down on sites that fail, and cannot index the page.",
        "Fix the server error for this URL, or remove it from the sitemap until it works.",
    ),
    rule("sitemap.redirect", "Redirect in sitemap", S, Medium, Scope::Known, REDIRECT)
        .needs(Need::SitemapEntries)
        .evidence(with_file!(
            "'Redirects to ' || COALESCE((SELECT t.url FROM urls t WHERE t.id = u.redirect_to), 'another URL')"
        ))
        .explain(
            "Sitemaps should list final URLs. A redirect costs search engines an extra request and the listed URL is not the one that gets indexed.",
            "List the redirect's final URL in the sitemap instead.",
        ),
    rule("sitemap.noindex", "Noindex URL in sitemap", S, High, Scope::Known, NOINDEX)
        .needs(Need::SitemapEntries)
        .evidence(with_file!("'noindex'"))
        .explain(
            "The sitemap asks for the page to be indexed while the page asks not to be: a contradiction that makes search engines trust the sitemap less.",
            "Remove the page from the sitemap, or remove the noindex if the page should be indexed.",
        ),
    rule("sitemap.canonicalised", "Canonicalised URL in sitemap", S, Medium, Scope::Known, CANONICALISED)
        .needs(Need::SitemapEntries)
        .evidence(with_file!("'Canonical: ' || u.canonical"))
        .explain(
            "The page names another URL as its main version, so the listed URL is usually not the one indexed.",
            "List the canonical URL in the sitemap instead of this one.",
        ),
    rule("sitemap.blocked", "Blocked URL in sitemap (robots.txt)", S, Medium, Scope::Known, BLOCKED)
        .needs(Need::SitemapEntries)
        .evidence(with_file!("COALESCE('Blocked by ' || u.robots_rule, 'Blocked by robots.txt')"))
        .explain(
            "robots.txt stops search engines from fetching a URL the sitemap asks them to index, so they cannot see its content.",
            "Remove the URL from the sitemap, or allow it in robots.txt if it should be indexed.",
        ),
    rule("sitemap.not_crawled", "Sitemap URL not crawled", S, Low, Scope::Known, NOT_CRAWLED)
        .needs(Need::SitemapEntries)
        .evidence(with_file!(
            "COALESCE(final_state(u.state, u.reason, u.reason_detail, u.check_only, u.fetched_at, u.error_class, u.status), 'Not crawled')"
        ))
        .explain(
            "This sitemap URL was not requested (a crawl rule, a limit, or crawling sitemap-only URLs is off), so the other sitemap checks could not look at it.",
            "Change the crawl's include and exclude rules or limits, or turn on Crawl URLs found only in sitemaps, to check it.",
        ),
    rule("sitemap.not_in_sitemap", "Indexable page not in sitemap", S, Medium, Scope::Pages, NOT_IN_SITEMAP)
        .needs(Need::SitemapEntries)
        .evidence("'Indexable, listed in no sitemap'")
        .explain(
            "The page is indexable but missing from the sitemaps, so search engines learn about it only through links and may find it late.",
            "Add the page to the sitemap, or generate a new sitemap from this crawl.",
        ),
    rule("sitemap.orphan", "Orphan page", S, High, Scope::Known, ORPHAN)
        .site_wide()
        .needs(Need::Orphans)
        .evidence(with_file!("'Found in ' || (SELECT o.sources FROM orphans o WHERE o.url_id = u.id) || ', linked from no crawled page'"))
        .explain(
            "No crawled page links to this URL, so visitors cannot reach it and search engines see it as unimportant.",
            "Link to the page from related pages, or remove it from the sitemap if it is no longer wanted.",
        ),
    rule("sitemap.nofollow_only", "Linked only through nofollow", S, Medium, Scope::Known, NOFOLLOW_ONLY)
        .site_wide()
        .needs(Need::Orphans)
        .evidence(with_file!("'Only nofollow links point here'"))
        .explain(
            "The only links to this page are nofollow, so it is reachable but receives no link value from the site.",
            "Add a followed link to the page from a related page.",
        ),
    rule(
        "sitemap.duplicate",
        "URL listed in more than one sitemap entry",
        S,
        Low,
        Scope::Known,
        problem!("sitemap.duplicate"),
    )
    .site_wide()
    .needs(Need::SitemapAudit)
    .evidence(with_file!(
        "(SELECT p.detail FROM sitemap_problems p WHERE p.rule = 'sitemap.duplicate' AND p.url_id = u.id LIMIT 1) || ' entries list it'"
    ))
    .explain(
        "The same URL appears in several sitemap entries. It is harmless for indexing but often a sign of a sitemap built from overlapping sources, with dates that disagree.",
        "Keep one entry per URL across all sitemap files.",
    ),
    rule(
        "sitemap.broken_image",
        "Broken image in sitemap",
        S,
        Medium,
        Scope::Known,
        "u.fetched_at IS NOT NULL AND (u.error_class IS NOT NULL OR u.status IS NULL OR u.status NOT BETWEEN 200 AND 299) \
         AND u.id IN (SELECT r.url_id FROM sitemap_refs r WHERE r.kind = 'image' AND r.url_id IS NOT NULL)",
    )
    .needs(Need::SitemapEntries)
    .evidence(
        "(CASE WHEN u.error_class IS NOT NULL THEN error_label(u.error_class) ELSE COALESCE(u.status, 'No response') END) \
         || COALESCE(' · listed with ' || (SELECT pu.url FROM sitemap_refs r JOIN sitemap_entries e ON e.id = r.entry_id \
         JOIN urls pu ON pu.id = e.url_id WHERE r.url_id = u.id AND r.kind = 'image' ORDER BY r.entry_id LIMIT 1), '')",
    )
    .explain(
        "An image sitemap entry points to an image that does not load, so search engines cannot index it.",
        "Fix the image address in the sitemap, or restore the image.",
    ),
    rule(
        "sitemap.invalid_url",
        "Invalid URL in sitemap",
        S,
        Medium,
        Scope::Known,
        "u.source = 'start' AND EXISTS (SELECT 1 FROM sitemap_entries e WHERE e.invalid IS NOT NULL)",
    )
    .site_wide()
    .needs(Need::SitemapEntries)
    .evidence(
        "(SELECT COUNT(*) FROM sitemap_entries e WHERE e.invalid IS NOT NULL) || ' entries, first: ' || \
         (SELECT e.loc || ' (' || e.invalid || ')' FROM sitemap_entries e WHERE e.invalid IS NOT NULL ORDER BY e.id LIMIT 1)",
    )
    .explain(
        "Some sitemap entries are not usable URLs (relative, malformed, or not http or https), so search engines skip them.",
        "Write every <loc> as a full absolute address, such as https://example.com/page. The Sitemap files view lists the entries.",
    ),
    rule(
        "sitemap.outside_location",
        "Sitemap URL outside the sitemap's location",
        S,
        Medium,
        Scope::Known,
        problem!("sitemap.outside_location"),
    )
    .site_wide()
    .needs(Need::SitemapAudit)
    .evidence(
        "'Sitemap folder: ' || (SELECT p.detail FROM sitemap_problems p WHERE p.rule = 'sitemap.outside_location' AND p.url_id = u.id LIMIT 1)",
    )
    .explain(
        "Search engines ignore sitemap URLs on another host or outside the sitemap file's folder, unless that host's robots.txt lists the sitemap.",
        "Move the sitemap to the site root, or list it in the robots.txt of the host its URLs are on.",
    ),
    rule(
        "sitemap.lastmod_invalid",
        "Invalid lastmod",
        S,
        Low,
        Scope::Known,
        "u.id IN (SELECT e.url_id FROM sitemap_entries e WHERE e.kind = 'url' AND e.lastmod_raw IS NOT NULL \
         AND e.lastmod IS NULL AND e.url_id IS NOT NULL)",
    )
    .needs(Need::SitemapEntries)
    .evidence(with_file!(
        "'Lastmod: ' || (SELECT e.lastmod_raw FROM sitemap_entries e WHERE e.url_id = u.id AND e.lastmod_raw IS NOT NULL AND e.lastmod IS NULL LIMIT 1)"
    ))
    .explain(
        "The lastmod value is not a W3C date such as 2026-10-13 or 2026-10-13T08:00:00+00:00, so search engines ignore it.",
        "Write lastmod in the W3C datetime format, or leave it out.",
    ),
    rule(
        "sitemap.lastmod_future",
        "Lastmod more than {n} days in the future",
        S,
        Low,
        Scope::Known,
        "u.id IN (SELECT p.url_id FROM sitemap_problems p WHERE p.rule = 'sitemap.lastmod_future' \
         AND CAST(p.detail AS REAL) > thr('sitemap.lastmod_future'))",
    )
    .threshold(Unit::Days, 1.0, 0.0, 365.0)
    .site_wide()
    .needs(Need::SitemapAudit)
    .evidence(
        "(SELECT 'Lastmod ' || COALESCE(e.lastmod_raw, '') || ', ' || p.detail || ' days after the crawl started' \
         FROM sitemap_problems p JOIN sitemap_entries e ON e.id = p.entry_id \
         WHERE p.rule = 'sitemap.lastmod_future' AND p.url_id = u.id ORDER BY CAST(p.detail AS REAL) DESC LIMIT 1)",
    )
    .explain(
        "A date in the future cannot be when the page last changed, so search engines learn to ignore this sitemap's dates.",
        "Set lastmod to the date the page content last changed.",
    ),
    rule(
        "sitemap.lastmod_old",
        "Lastmod over {n} days older than the page",
        S,
        Opportunity,
        Scope::Known,
        "u.id IN (SELECT p.url_id FROM sitemap_problems p WHERE p.rule = 'sitemap.lastmod_old' \
         AND CAST(p.detail AS INTEGER) > thr('sitemap.lastmod_old'))",
    )
    .threshold(Unit::Days, 30.0, 1.0, 3650.0)
    .site_wide()
    .needs(Need::SitemapAudit)
    .evidence(
        "(SELECT 'Lastmod ' || COALESCE(e.lastmod_raw, '') || ', the page''s Last-Modified is ' || p.detail || ' days later' \
         FROM sitemap_problems p JOIN sitemap_entries e ON e.id = p.entry_id \
         WHERE p.rule = 'sitemap.lastmod_old' AND p.url_id = u.id ORDER BY CAST(p.detail AS INTEGER) DESC LIMIT 1)",
    )
    .explain(
        "The page says it changed well after the sitemap's lastmod, so the sitemap is not telling search engines about the change.",
        "Update lastmod whenever the page content changes, for example by generating the sitemap from the CMS.",
    ),
    rule(
        "sitemap.lastmod_identical",
        "All lastmod values identical in a sitemap of {n} entries or more",
        S,
        Low,
        Scope::Known,
        "u.id IN (SELECT e.url_id FROM sitemap_entries e JOIN sitemap_problems p \
         ON p.file_id = e.file_id AND p.rule = 'sitemap.lastmod_identical' \
         WHERE e.kind = 'url' AND e.url_id IS NOT NULL AND CAST(p.detail AS INTEGER) >= thr('sitemap.lastmod_identical'))",
    )
    .threshold(Unit::Entries, 100.0, 2.0, 50_000.0)
    .site_wide()
    .needs(Need::SitemapAudit)
    .evidence(
        "(SELECT 'Every entry of ' || f.location || ' has lastmod ' || COALESCE(e.lastmod_raw, '') \
         FROM sitemap_entries e JOIN sitemap_files f ON f.id = e.file_id \
         JOIN sitemap_problems p ON p.file_id = e.file_id AND p.rule = 'sitemap.lastmod_identical' \
         WHERE e.url_id = u.id LIMIT 1)",
    )
    .explain(
        "When every entry has the same date, it is usually the time the sitemap was generated, not when pages changed, and search engines stop using it.",
        "Give each entry the date its page last changed, or leave lastmod out.",
    ),
    rule(
        "sitemap.no_lastmod",
        "Entries without lastmod",
        S,
        Opportunity,
        Scope::Known,
        "u.id IN (SELECT e.url_id FROM sitemap_entries e WHERE e.kind = 'url' AND e.lastmod_raw IS NULL AND e.url_id IS NOT NULL)",
    )
    .site_wide()
    .needs(Need::SitemapEntries)
    .evidence(with_file!("'No lastmod'"))
    .explain(
        "Without lastmod, search engines cannot tell from the sitemap which pages changed and recrawl them on their own schedule.",
        "Add an accurate lastmod to each entry when the site can provide one.",
    ),
    rule(
        "sitemap.fetch_failed",
        "Sitemap could not be fetched",
        S,
        High,
        Scope::Known,
        file_cond!("(f.error_class IS NOT NULL AND f.error_class NOT IN ('invalid_xml', 'not_a_sitemap', 'compression', 'too_deep', 'loop')) OR f.http_status >= 400"),
    )
    .site_wide()
    .needs(Need::SitemapEntries)
    .evidence(file_evidence!("(f.error_class IS NOT NULL AND f.error_class NOT IN ('invalid_xml', 'not_a_sitemap', 'compression', 'too_deep', 'loop')) OR f.http_status >= 400"))
    .explain(
        "A sitemap failed to download, so none of its URLs reach search engines through it.",
        "Make sure the sitemap address answers 200 to crawlers, and fix or remove the reference to it.",
    ),
    rule(
        "sitemap.invalid",
        "Sitemap is not valid",
        S,
        High,
        Scope::Known,
        file_cond!("f.error_class IN ('invalid_xml', 'not_a_sitemap', 'compression')"),
    )
    .site_wide()
    .needs(Need::SitemapEntries)
    .evidence(file_evidence!("f.error_class IN ('invalid_xml', 'not_a_sitemap', 'compression')"))
    .explain(
        "A sitemap is not valid XML, its root is neither urlset nor sitemapindex, or its compression is broken, so search engines cannot read it.",
        "Fix the file at the line the Sitemap files view shows, and check it against the sitemap protocol.",
    ),
    rule(
        "sitemap.over_limit",
        "Sitemap over size limit",
        S,
        High,
        Scope::Known,
        file_cond!("f.over_limit = 1"),
    )
    .site_wide()
    .needs(Need::SitemapEntries)
    .evidence(file_evidence!("f.over_limit = 1"))
    .explain(
        "A sitemap has more than 50,000 URLs or is larger than 50 MB uncompressed. Search engines read only up to the limit, or reject the file.",
        "Split the sitemap into several files listed by a sitemap index.",
    ),
    rule(
        "sitemap.redirects",
        "Sitemap redirects",
        S,
        Low,
        Scope::Known,
        file_cond!("f.redirected = 1"),
    )
    .site_wide()
    .needs(Need::SitemapEntries)
    .evidence(file_evidence!("f.redirected = 1"))
    .explain(
        "A sitemap address redirects. Search engines may not follow it, and the location rule then applies to the address they did read.",
        "Reference the sitemap's final address in robots.txt and indexes.",
    ),
    rule(
        "sitemap.empty",
        "Empty sitemap",
        S,
        Medium,
        Scope::Known,
        file_cond!("f.kind IS NOT NULL AND f.entry_count = 0 AND f.error_class IS NULL"),
    )
    .site_wide()
    .needs(Need::SitemapEntries)
    .evidence(file_evidence!("f.kind IS NOT NULL AND f.entry_count = 0 AND f.error_class IS NULL"))
    .explain(
        "A sitemap lists nothing, usually a broken generator or a section of the site that was removed.",
        "Fix the generator, or remove the empty sitemap and its references.",
    ),
    rule(
        "sitemap.too_deep",
        "Sitemap index nested too deep or looping",
        S,
        Medium,
        Scope::Known,
        file_cond!("f.error_class IN ('too_deep', 'loop')"),
    )
    .site_wide()
    .needs(Need::SitemapEntries)
    .evidence(file_evidence!("f.error_class IN ('too_deep', 'loop')"))
    .explain(
        "Sitemap indexes are nested more than 3 levels deep or list themselves, so search engines may not read the files below.",
        "List every sitemap file directly in one sitemap index.",
    ),
    rule(
        "sitemap.not_in_robots",
        "No sitemap listed in robots.txt",
        S,
        Opportunity,
        Scope::Known,
        "u.source = 'start' AND (SELECT m.value FROM meta m WHERE m.key = 'sitemaps_read') = 'true' \
         AND NOT EXISTS (SELECT 1 FROM sitemap_files f WHERE f.source = 'robots')",
    )
    .site_wide()
    .needs(Need::SitemapEntries)
    .evidence("'robots.txt has no Sitemap: line'")
    .explain(
        "robots.txt lists no sitemap, so search engines find the sitemap only if it is submitted to them or at a default address.",
        "Add a line such as Sitemap: https://example.com/sitemap.xml to robots.txt.",
    ),
    rule(
        "sitemap.none_found",
        "No sitemap found",
        S,
        Medium,
        Scope::Known,
        "u.source = 'start' AND (SELECT m.value FROM meta m WHERE m.key = 'sitemaps_read') = 'true' \
         AND NOT EXISTS (SELECT 1 FROM sitemap_files f WHERE f.kind IS NOT NULL)",
    )
    .site_wide()
    .needs(Need::SitemapEntries)
    .evidence("'No sitemap in robots.txt, at /sitemap.xml or at /sitemap_index.xml'")
    .explain(
        "The site has no sitemap SpiderLens could read, so search engines rely on links alone to find its pages.",
        "Publish an XML sitemap (Generate XML sitemap builds one from this crawl) and list it in robots.txt.",
    ),
];
