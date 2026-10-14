//! Links between pages.

use crate::issues::{Category::Links as C, Priority::*, Rule, Scope, Unit, rule};
use crate::tabs::{Derived, Need};

/// True when this page has a link of the kind given by a
/// `tabs::link_flag` bit (as a number literal).
macro_rules! has_link {
    ($bit:literal) => {
        concat!(
            "(COALESCE((SELECT s.flags FROM temp.link_stats s WHERE s.url_id = u.id), 0) & ",
            $bit,
            ") <> 0"
        )
    };
}
pub(crate) use has_link;

macro_rules! broken_evidence {
    ($internal:literal) => {
        concat!(
            "(SELECT COUNT(DISTINCT t.id) || CASE COUNT(DISTINCT t.id) WHEN 1 THEN ' broken link: ' ELSE ' broken links, first: ' END \
             || (SELECT t2.url FROM links l2 JOIN urls t2 ON t2.id = l2.to_id WHERE l2.from_id = u.id AND l2.kind = 'anchor' \
                 AND t2.internal = ",
            $internal,
            " AND (t2.error_class IS NOT NULL OR t2.status >= 400) ORDER BY l2.pos LIMIT 1) \
             FROM links l JOIN urls t ON t.id = l.to_id WHERE l.from_id = u.id AND l.kind = 'anchor' AND t.internal = ",
            $internal,
            " AND (t.error_class IS NOT NULL OR t.status >= 400))"
        )
    };
}

pub const RULES: &[Rule] = &[
    rule("links.broken_internal", "Pages with broken internal links", C, High, Scope::Pages, has_link!("1"))
        .site_wide()
        .reads(&[Derived::LinkStats])
        .evidence(broken_evidence!("1"))
        .explain(
            "This page links to internal URLs that return an error, so visitors hit dead ends and crawlers waste time.",
            "Fix or remove each broken link on this page; the Outlinks section shows every target and its state.",
        ),
    rule("links.broken_external", "Pages with broken external links", C, Medium, Scope::Pages, has_link!("2"))
        .site_wide()
        .reads(&[Derived::LinkStats])
        .evidence(broken_evidence!("0"))
        .explain(
            "This page links to outside pages that return an error, which hurts visitors' trust in the page.",
            "Replace each broken link with a working source, or remove it.",
        ),
    rule(
        "links.to_redirect",
        "Internal links to redirects",
        C,
        Low,
        Scope::Pages,
        has_link!("4"),
    )
    .site_wide()
    .reads(&[Derived::LinkStats])
    .evidence(
        "(SELECT COUNT(DISTINCT t.id) || ' links to redirects' FROM links l JOIN urls t ON t.id = l.to_id \
         WHERE l.from_id = u.id AND l.kind = 'anchor' AND t.internal = 1 AND t.error_class IS NULL AND t.status BETWEEN 300 AND 399)",
    )
    .explain(
        "Links that go through a redirect cost visitors an extra round trip and pass less value than a direct link.",
        "Change each link to point at the redirect's final destination.",
    ),
    rule(
        "links.nofollow_internal",
        "Nofollowed internal links",
        C,
        Low,
        Scope::Pages,
        has_link!("8"),
    )
    .reads(&[Derived::LinkStats])
    .evidence(
        "(SELECT COUNT(*) || ' nofollow links to this site' FROM links l JOIN urls t ON t.id = l.to_id \
         WHERE l.from_id = u.id AND l.kind = 'anchor' AND l.nofollow = 1 AND t.internal = 1)",
    )
    .explain(
        "Nofollow on links within your own site tells search engines not to pass value through them, which can leave pages poorly discovered.",
        "Remove rel=\"nofollow\" from internal links unless the target should really not be crawled (then prefer robots.txt or noindex).",
    ),
    rule(
        "links.no_internal_outlinks",
        "Pages without internal outlinks",
        C,
        Low,
        Scope::Pages,
        concat!("NOT ", has_link!("16")),
    )
    .reads(&[Derived::LinkStats])
    .evidence("'No links to other pages of the site'")
    .explain(
        "A page with no links to the rest of the site is a dead end for visitors and passes no value to other pages.",
        "Add navigation or contextual links to related pages.",
    ),
    rule(
        "links.weak_anchor",
        "Non-descriptive anchor text",
        C,
        Opportunity,
        Scope::Pages,
        "EXISTS (SELECT 1 FROM links l WHERE l.from_id = u.id AND l.kind = 'anchor' AND weak_anchor(l.text))",
    )
    .needs(Need::Extraction)
    .evidence(
        "(SELECT COUNT(*) || ' links, for example \"' || (SELECT l2.text FROM links l2 WHERE l2.from_id = u.id AND l2.kind = 'anchor' \
         AND weak_anchor(l2.text) ORDER BY l2.pos LIMIT 1) || '\"' FROM links l WHERE l.from_id = u.id AND l.kind = 'anchor' AND weak_anchor(l.text))",
    )
    .explain(
        "Link text like \"click here\" or \"read more\" tells search engines, screen reader users and AI assistants nothing about the target.",
        "Rewrite the link text to describe the target page, for example \"Shipping rates for Europe\".",
    ),
    rule("links.deep", "Pages deeper than {n} clicks", C, Opportunity, Scope::Pages, "u.depth > thr('links.deep')")
        .threshold(Unit::Depth, 4.0, 1.0, 100.0)
        .evidence("'Depth ' || u.depth")
        .explain(
            "Pages many clicks from the start page are found and crawled less often, and visitors rarely reach them.",
            "Link to important deep pages from higher-level pages, hubs or navigation.",
        ),
];
