//! H1, H2 and heading order. The 007 H1 and H2 filters use these same
//! conditions (FR-005).

use crate::issues::{Category, Priority::*, Rule, Scope, Unit, rule};
use crate::tabs::{Derived, Need};

macro_rules! first {
    ($h:literal) => {
        concat!("(SELECT p.", $h, "_1 FROM pages p WHERE p.url_id = u.id)")
    };
}

pub const H1_MISSING: &str = concat!("COALESCE(", first!("h1"), ", '') = ''");
pub const H1_DUPLICATE: &str = concat!(first!("h1"), " IN (SELECT o.v FROM temp.occ_h1 o WHERE o.n > 1)");
pub const H1_OVER: &str = concat!("length(", first!("h1"), ") > thr('h1.over')");
pub const H1_MULTIPLE: &str = "(SELECT p.h1_count FROM pages p WHERE p.url_id = u.id) > 1";
pub const H2_MISSING: &str = concat!("COALESCE(", first!("h2"), ", '') = ''");
pub const H2_DUPLICATE: &str = concat!(first!("h2"), " IN (SELECT o.v FROM temp.occ_h2 o WHERE o.n > 1)");
pub const H2_OVER: &str = concat!("length(", first!("h2"), ") > thr('h2.over')");
pub const H2_MULTIPLE: &str = "(SELECT p.h2_count FROM pages p WHERE p.url_id = u.id) > 1";

pub const RULES: &[Rule] = &[
    rule("h1.missing", "H1: missing", Category::H1, Medium, Scope::Pages, H1_MISSING)
        .needs(Need::Extraction)
        .evidence("'No H1 heading'")
        .explain(
            "The H1 tells visitors and search engines what the page is about at a glance; screen reader users jump to it first.",
            "Add one H1 at the top of the main content that names the page's topic.",
        ),
    rule("h1.duplicate", "H1: duplicate", Category::H1, Low, Scope::Pages, H1_DUPLICATE)
        .site_wide()
        .needs(Need::Extraction)
        .reads(&[Derived::OccH1])
        .evidence(concat!("(SELECT o.n FROM temp.occ_h1 o WHERE o.v = ", first!("h1"), ") || ' pages share this H1'"))
        .explain(
            "The same H1 on several pages makes them hard to tell apart for visitors and search engines.",
            "Make each H1 describe what is specific to its page.",
        ),
    rule("h1.over", "H1: over {n} characters", Category::H1, Low, Scope::Pages, H1_OVER)
        .threshold(Unit::Chars, 70.0, 10.0, 1000.0)
        .needs(Need::Extraction)
        .evidence(concat!("length(", first!("h1"), ") || ' characters'"))
        .explain(
            "A very long H1 reads like a paragraph and dilutes the main topic.",
            "Shorten the H1 to the page's topic and move detail into the text below it.",
        ),
    rule("h1.multiple", "H1: multiple", Category::H1, Low, Scope::Pages, H1_MULTIPLE)
        .needs(Need::Extraction)
        .evidence("(SELECT p.h1_count FROM pages p WHERE p.url_id = u.id) || ' H1 headings'")
        .explain(
            "Several H1s blur which topic the page is mainly about. It is valid HTML, but one clear H1 is easier to scan.",
            "Keep one H1 and turn the others into H2s.",
        ),
    rule("h2.missing", "H2: missing", Category::H2, Low, Scope::Pages, H2_MISSING)
        .needs(Need::Extraction)
        .evidence("'No H2 heading'")
        .explain(
            "Subheadings break content into sections that visitors scan and search engines and AI assistants use to understand structure.",
            "Add H2 subheadings for the main sections of longer pages.",
        ),
    rule("h2.duplicate", "H2: duplicate", Category::H2, Low, Scope::Pages, H2_DUPLICATE)
        .site_wide()
        .needs(Need::Extraction)
        .reads(&[Derived::OccH2])
        .evidence(concat!("(SELECT o.n FROM temp.occ_h2 o WHERE o.v = ", first!("h2"), ") || ' pages share this first H2'"))
        .explain(
            "The same first subheading on many pages often means template text rather than content.",
            "Make the first H2 specific to the page, or check whether the pages are near copies.",
        ),
    rule("h2.over", "H2: over {n} characters", Category::H2, Low, Scope::Pages, H2_OVER)
        .threshold(Unit::Chars, 70.0, 10.0, 1000.0)
        .needs(Need::Extraction)
        .evidence(concat!("length(", first!("h2"), ") || ' characters'"))
        .explain(
            "Very long subheadings are hard to scan.",
            "Shorten the H2 and move detail into the paragraph below it.",
        ),
    rule(
        "headings.non_sequential",
        "Headings: non-sequential",
        Category::Headings,
        Low,
        Scope::Pages,
        super::has_flag!("65536"),
    )
    .needs(Need::Extraction)
    .evidence(concat!(
        "(SELECT 'H' || prev || ' then H' || level FROM ",
        "(SELECT t.level AS level, LAG(t.level) OVER (ORDER BY t.pos) AS prev FROM page_text t WHERE t.url_id = u.id AND t.kind = 'heading')",
        " WHERE prev IS NOT NULL AND level > prev + 1 LIMIT 1)"
    ))
    .standards(&["WCAG 2.2 1.3.1"])
    .explain(
        "Skipping heading levels (an H1 followed by an H3) breaks the outline screen reader users navigate by, and blurs the page structure for search engines.",
        "Use heading levels in order: H2 under H1, H3 under H2. Style headings with CSS rather than picking a level for its size.",
    ),
];
