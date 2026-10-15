//! Robots directives.

use crate::issues::{Category::Directives as C, Priority::*, Rule, Scope, Unit, rule};
use crate::tabs::Need;

/// Directives that apply to this crawl's robots token or to all crawlers.
macro_rules! applies {
    ($d:literal) => {
        concat!("(", $d, ".crawler = 'all' OR ", $d, ".crawler = robots_token())")
    };
}

pub const RULES: &[Rule] = &[
    rule("directives.noindex", "Noindex", C, Medium, Scope::Pages, "u.noindex = 1")
        .needs(Need::Extraction)
        .evidence("'noindex' || COALESCE(' (' || (SELECT group_concat(DISTINCT d.source) FROM directives d WHERE d.url_id = u.id AND d.directive IN ('noindex', 'none')) || ')', '')")
        .explain(
            "Search engines are told not to show this page in results. Right for private or utility pages, costly when it is on a page that should rank.",
            "If the page should be found, remove noindex from the meta robots tag and the X-Robots-Tag header.",
        ),
    rule("directives.nofollow", "Nofollow (page level)", C, Low, Scope::Pages, "u.nofollow = 1")
        .needs(Need::Extraction)
        .evidence("'nofollow on the whole page'")
        .explain(
            "Search engines are told not to follow any link on this page, so pages linked only from here may not be discovered.",
            "Remove page-level nofollow unless every link on the page should be ignored; use rel=\"nofollow\" on individual links instead.",
        ),
    rule(
        "directives.outside_head",
        "Robots meta tag outside head",
        C,
        Medium,
        Scope::Pages,
        "EXISTS (SELECT 1 FROM directives d WHERE d.url_id = u.id AND d.source = 'meta' AND d.in_head = 0)",
    )
    .needs(Need::Extraction)
    .evidence("'A robots meta tag sits in the body'")
    .explain(
        "Robots meta tags outside the <head> may be ignored, so the page may be indexed or not against your intent.",
        "Move the robots meta tag into the <head>.",
    ),
    rule(
        "directives.conflicting",
        "Conflicting directives",
        C,
        Medium,
        Scope::Pages,
        concat!(
            "EXISTS (SELECT 1 FROM directives a JOIN directives b ON b.url_id = a.url_id WHERE a.url_id = u.id AND ",
            applies!("a"),
            " AND ",
            applies!("b"),
            " AND ((a.directive = 'index' AND b.directive IN ('noindex', 'none')) OR (a.directive = 'follow' AND b.directive IN ('nofollow', 'none'))))"
        ),
    )
    .needs(Need::Extraction)
    .evidence(concat!(
        "(SELECT a.directive || ' (' || a.source || ') and ' || b.directive || ' (' || b.source || ')' FROM directives a JOIN directives b ON b.url_id = a.url_id WHERE a.url_id = u.id AND ",
        applies!("a"),
        " AND ",
        applies!("b"),
        " AND ((a.directive = 'index' AND b.directive IN ('noindex', 'none')) OR (a.directive = 'follow' AND b.directive IN ('nofollow', 'none'))) LIMIT 1)"
    ))
    .explain(
        "The page says both index and noindex (or follow and nofollow), for example in the meta tag and the header. Search engines apply the most restrictive one.",
        "Decide what the page should do and remove the other directive from the meta tag or the X-Robots-Tag header.",
    ),
    rule(
        "directives.noindex_linked",
        "Noindexed page linked from {n} or more pages",
        C,
        Medium,
        Scope::Pages,
        "u.noindex = 1 AND (SELECT COUNT(DISTINCT l.from_id) FROM links l WHERE l.to_id = u.id AND l.kind = 'anchor') >= thr('directives.noindex_linked')",
    )
    .site_wide()
    .threshold(Unit::Links, 10.0, 1.0, 100000.0)
    .needs(Need::Extraction)
    .evidence("'Linked from ' || (SELECT COUNT(DISTINCT l.from_id) FROM links l WHERE l.to_id = u.id AND l.kind = 'anchor') || ' pages'")
    .explain(
        "Many pages link to this one, which suggests it matters, yet it is excluded from search results.",
        "Check whether noindex is intended. If it is, consider linking less to it; if not, remove noindex.",
    ),
];
