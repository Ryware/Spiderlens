//! Search result snippet widths (spec 027 part A, FR-A08). The 007 Page
//! titles and Meta description pixel filters use these same conditions
//! (spec 009 FR-005). The widths are measured by the `px_width` SQL
//! function, the same measure the search preview uses; the limits are
//! thresholds the user can change.

use crate::issues::{Category::Serp as C, Priority::*, Rule, Scope, Unit, rule};
use crate::tabs::Need;

pub const TITLE_OVER_DESKTOP: &str = "px_width(u.title, 'title') > thr('serp.title_over_desktop')";
pub const TITLE_OVER_MOBILE: &str = "px_width(u.title, 'title') > thr('serp.title_over_mobile')";
pub const TITLE_BELOW: &str =
    "norm_ws(u.title) <> '' AND px_width(u.title, 'title') < thr('serp.title_below')";
pub const DESC_OVER_DESKTOP: &str = "px_width((SELECT p.meta_description FROM pages p WHERE p.url_id = u.id), 'desc') > thr('serp.desc_over_desktop')";
pub const DESC_OVER_MOBILE: &str = "px_width((SELECT p.meta_description FROM pages p WHERE p.url_id = u.id), 'desc') > thr('serp.desc_over_mobile')";

const TITLE_PX: &str = "px_width(u.title, 'title') || ' pixels'";
const DESC_PX: &str =
    "px_width((SELECT p.meta_description FROM pages p WHERE p.url_id = u.id), 'desc') || ' pixels'";

pub const RULES: &[Rule] = &[
    rule(
        "serp.title_over_desktop",
        "Page title: over {n} pixels on desktop",
        C,
        Medium,
        Scope::Pages,
        TITLE_OVER_DESKTOP,
    )
    .threshold(Unit::Px, 600.0, 100.0, 2000.0)
    .evidence(TITLE_PX)
    .explain(
        "Search engines cut a title by its width in pixels, not by its number of characters. A title wider than the limit is cut off on desktop, so the end of it is not seen.",
        "Shorten the title or use narrower letters, and put the most important words first. The search preview shows where the cut falls.",
    ),
    rule(
        "serp.title_over_mobile",
        "Page title: over {n} pixels on mobile",
        C,
        Low,
        Scope::Pages,
        TITLE_OVER_MOBILE,
    )
    .threshold(Unit::Px, 580.0, 100.0, 2000.0)
    .evidence(TITLE_PX)
    .explain(
        "Phones show a little less of the title than desktop screens, so a title that fits on desktop can still be cut off on mobile.",
        "Shorten the title so the words that matter most come first, before the point where the mobile result is cut.",
    ),
    rule(
        "serp.title_below",
        "Page title: below {n} pixels",
        C,
        Low,
        Scope::Pages,
        TITLE_BELOW,
    )
    .threshold(Unit::Px, 200.0, 1.0, 600.0)
    .evidence(TITLE_PX)
    .explain(
        "A title that fills only a small part of the line wastes space in search results that could describe the page and attract the click.",
        "Expand the title with the topic, product or place the page is about.",
    ),
    rule(
        "serp.desc_over_desktop",
        "Meta description: over {n} pixels on desktop",
        C,
        Low,
        Scope::Pages,
        DESC_OVER_DESKTOP,
    )
    .threshold(Unit::Px, 990.0, 100.0, 3000.0)
    .needs(Need::Extraction)
    .evidence(DESC_PX)
    .explain(
        "Search engines cut a description by its width in pixels. A description wider than the limit is cut off on desktop, so the end of it is not seen.",
        "Shorten the description and put the key message first. The search preview shows where the cut falls.",
    ),
    rule(
        "serp.desc_over_mobile",
        "Meta description: over {n} pixels on mobile",
        C,
        Low,
        Scope::Pages,
        DESC_OVER_MOBILE,
    )
    .threshold(Unit::Px, 680.0, 100.0, 3000.0)
    .needs(Need::Extraction)
    .evidence(DESC_PX)
    .explain(
        "Phones show less of the description than desktop screens, so a description that fits on desktop can still be cut off on mobile.",
        "Shorten the description so the key message comes first, before the point where the mobile result is cut.",
    ),
];
