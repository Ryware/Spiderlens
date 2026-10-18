//! AMP (spec 027 part E, FR-E05): one rule per AMP check. The AMP tab
//! filters test the same conditions on the `amp_pages` and `amp_checks`
//! tables (spec 009 FR-005). The checks are a core subset of the official
//! AMP validator, not all of it (FR-E04).
//!
//! The AMP marker check cannot fail on a page already known to be AMP, so
//! its rule flags pages an `amphtml` link points to that do not carry the
//! marker. The pairing rule looks at the canonical page's links, so its
//! result can change until the crawl ends.

use crate::issues::{Category::Amp as C, Priority::*, Rule, Scope, rule};
use crate::tabs::Need;

/// AMP pages whose `check` failed.
macro_rules! failed {
    ($check:literal) => {
        concat!(
            "EXISTS (SELECT 1 FROM amp_checks a WHERE a.url_id = u.id AND a.check_id = '",
            $check,
            "')"
        )
    };
}

/// What the first failure of `check` said.
macro_rules! detail {
    ($check:literal) => {
        concat!(
            "(SELECT a.detail FROM amp_checks a WHERE a.url_id = u.id AND a.check_id = '",
            $check,
            "' ORDER BY a.pos LIMIT 1)"
        )
    };
}

/// Reached through an `amphtml` link but not marked as AMP.
pub const NOT_MARKED: &str = "NOT EXISTS (SELECT 1 FROM amp_pages ap WHERE ap.url_id = u.id) \
     AND EXISTS (SELECT 1 FROM links l WHERE l.to_id = u.id AND l.kind = 'amphtml')";

/// An AMP page whose canonical page has no `amphtml` link back to it.
pub const UNPAIRED: &str = "EXISTS (SELECT 1 FROM amp_pages ap WHERE ap.url_id = u.id AND ap.format = 'website') \
     AND u.canonical IS NOT NULL AND u.canonical <> u.url \
     AND NOT EXISTS (SELECT 1 FROM urls c JOIN links l ON l.from_id = c.id \
         WHERE c.url = u.canonical AND l.kind = 'amphtml' AND l.to_id = u.id)";

pub const RULES: &[Rule] = &[
    rule("amp.not_marked", "AMP: page not marked as AMP", C, Medium, Scope::InternalUrls, NOT_MARKED)
        .needs(Need::Amp)
        .evidence("'Linked as the AMP version of a page, but the html element has no amp or ⚡ attribute'")
        .explain(
            "A page is linked as the AMP version of another page, but its root element does not say it is AMP. Search engines will not treat it as AMP.",
            "Add the amp (or ⚡) attribute to the <html> element and make the page valid AMP, or remove the amphtml link that points to it.",
        ),
    rule("amp.runtime", "AMP: missing runtime script", C, Medium, Scope::InternalUrls, failed!("runtime"))
        .needs(Need::Amp)
        .evidence(detail!("runtime"))
        .explain(
            "Every AMP page must load the AMP runtime script. Without it the page is not valid AMP and will not be served as AMP.",
            "Add <script async src=\"https://cdn.ampproject.org/v0.js\"></script> to the <head>.",
        ),
    rule("amp.boilerplate", "AMP: missing boilerplate style", C, Medium, Scope::InternalUrls, failed!("boilerplate"))
        .needs(Need::Amp)
        .evidence(detail!("boilerplate"))
        .explain(
            "AMP pages must carry the standard boilerplate style that hides the page until the AMP runtime is ready. Without it the page is not valid AMP.",
            "Copy the boilerplate style from the AMP documentation into the <head>, exactly as published.",
        ),
    rule("amp.viewport", "AMP: missing viewport meta", C, Low, Scope::InternalUrls, failed!("viewport"))
        .needs(Need::Amp)
        .evidence(detail!("viewport"))
        .explain(
            "Without a viewport meta tag, phones show the page zoomed out as if it were a desktop page, and the page is not valid AMP.",
            "Add <meta name=\"viewport\" content=\"width=device-width\"> to the <head>.",
        ),
    rule("amp.canonical", "AMP: missing canonical link", C, Medium, Scope::InternalUrls, failed!("canonical"))
        .needs(Need::Amp)
        .evidence(detail!("canonical"))
        .explain(
            "An AMP page must say which page it belongs to. Without a canonical link search engines cannot connect the AMP page to the page it stands in for.",
            "Add <link rel=\"canonical\" href=\"...\"> pointing to the regular page, or to the AMP page itself when there is no other version.",
        ),
    rule("amp.unpaired", "AMP: canonical page does not link back", C, Medium, Scope::InternalUrls, UNPAIRED)
        .site_wide()
        .needs(Need::Amp)
        .evidence("'The canonical page ' || u.canonical || ' has no amphtml link to this page'")
        .explain(
            "The AMP page names a canonical page, but that page does not point back to it with an amphtml link. Search engines may not connect the two, and the AMP page may not be shown.",
            "Add <link rel=\"amphtml\" href=\"...\"> to the canonical page, pointing to this AMP page.",
        ),
    rule("amp.author_script", "AMP: author-written scripts", C, Medium, Scope::InternalUrls, failed!("author_script"))
        .needs(Need::Amp)
        .evidence(detail!("author_script"))
        .explain(
            "AMP does not allow your own JavaScript. Only the AMP runtime, AMP components and data scripts are allowed, so the page is not valid AMP.",
            "Remove the script, or replace it with an AMP component that does the same job.",
        ),
    rule("amp.style_size", "AMP: custom CSS over size limit", C, Medium, Scope::InternalUrls, failed!("style_size"))
        .needs(Need::Amp)
        .evidence(detail!("style_size"))
        .explain(
            "AMP allows at most 75,000 bytes of custom CSS in the page. A page over the limit is not valid AMP.",
            "Remove unused rules and shorten the CSS until it fits.",
        ),
    rule("amp.disallowed", "AMP: disallowed elements", C, Medium, Scope::InternalUrls, failed!("disallowed"))
        .needs(Need::Amp)
        .evidence(detail!("disallowed"))
        .explain(
            "The page uses an element AMP does not allow, such as a frame, an object or an embed. The page is not valid AMP.",
            "Remove the element, or replace it with the matching AMP component (for example amp-iframe).",
        ),
];
