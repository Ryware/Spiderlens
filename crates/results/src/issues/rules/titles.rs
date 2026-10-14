//! Page titles. The 007 Page titles filters use these same conditions
//! (FR-005).

use crate::issues::{Category::PageTitles as C, Priority::*, Rule, Scope, Unit, rule};
use crate::tabs::{Derived, Need};

pub const MISSING: &str = "(u.title IS NULL OR norm_ws(u.title) = '')";
pub const DUPLICATE: &str = "norm_ws(u.title) IN (SELECT o.v FROM temp.occ_title o WHERE o.n > 1)";
pub const OVER: &str = "length(norm_ws(u.title)) > thr('title.over')";
pub const BELOW: &str = "norm_ws(u.title) <> '' AND length(norm_ws(u.title)) < thr('title.below')";
pub const SAME_AS_H1: &str = "norm_ws(u.title) <> '' AND lower(norm_ws(u.title)) = lower((SELECT p.h1_1 FROM pages p WHERE p.url_id = u.id))";
pub const MULTIPLE: &str = "(SELECT p.title_count FROM pages p WHERE p.url_id = u.id) > 1";

const LENGTH: &str = "length(norm_ws(u.title)) || ' characters'";

pub const RULES: &[Rule] = &[
    rule("title.missing", "Page title: missing", C, High, Scope::Pages, MISSING)
        .evidence("'No title'")
        .standards(&["WCAG 2.2 2.4.2"])
        .explain(
            "The title is the headline of the page in search results and browser tabs. Without one, search engines make one up and the page is hard to recognise.",
            "Add a unique <title> in the <head> that says what the page is about, in about 30 to 60 characters.",
        ),
    rule("title.duplicate", "Page title: duplicate", C, Medium, Scope::Pages, DUPLICATE)
        .site_wide()
        .reads(&[Derived::OccTitle])
        .evidence("(SELECT o.n FROM temp.occ_title o WHERE o.v = norm_ws(u.title)) || ' pages share this title'")
        .explain(
            "Pages with the same title compete with each other in search results and look the same to users choosing a result.",
            "Give each page a title that names what makes it different. If the pages are the same content, keep one and canonicalise or redirect the others.",
        ),
    rule("title.over", "Page title: over {n} characters", C, Medium, Scope::Pages, OVER)
        .threshold(Unit::Chars, 60.0, 10.0, 1000.0)
        .evidence(LENGTH)
        .explain(
            "Long titles are cut off in search results, so the end of the title is not seen.",
            "Shorten the title and put the most important words first.",
        ),
    rule("title.below", "Page title: below {n} characters", C, Medium, Scope::Pages, BELOW)
        .threshold(Unit::Chars, 30.0, 1.0, 200.0)
        .evidence(LENGTH)
        .explain(
            "Very short titles waste space in search results that could describe the page and attract the click.",
            "Expand the title with the topic, product or place the page is about.",
        ),
    rule("title.multiple", "Page title: multiple title elements", C, Medium, Scope::Pages, MULTIPLE)
        .needs(Need::Extraction)
        .evidence("(SELECT p.title_count FROM pages p WHERE p.url_id = u.id) || ' title elements'")
        .explain(
            "Browsers and search engines use the first title only, so the others are ignored and may show the wrong text.",
            "Keep one <title> in the <head>; check templates and plugins that add their own.",
        ),
    rule("title.same_as_h1", "Page title: same as H1", C, Low, Scope::Pages, SAME_AS_H1)
        .needs(Need::Extraction)
        .evidence("'Title and H1: ' || norm_ws(u.title)")
        .explain(
            "Title and H1 serve different readers: the title sells the click in results, the H1 greets the visitor. Identical text misses a chance to cover more wording.",
            "Optional: vary the title or the H1 so together they cover the topic's main phrasings.",
        ),
    rule(
        "title.outside_head",
        "Page title: outside head",
        C,
        Medium,
        Scope::Pages,
        super::has_flag!("32768"),
    )
    .needs(Need::Extraction)
    .evidence("'A <title> sits in the body'")
    .explain(
        "A title in the body is usually a sign of broken markup that closed the <head> early; search engines may ignore it.",
        "Move the <title> into the <head>, and check for stray elements above it that end the head early.",
    ),
];
