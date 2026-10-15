//! Meta descriptions. The 007 Meta description filters use these same
//! conditions (FR-005).

use crate::issues::{Category::MetaDescription as C, Priority::*, Rule, Scope, Unit, rule};
use crate::tabs::{Derived, Need};

pub const MISSING: &str = "COALESCE((SELECT p.meta_description FROM pages p WHERE p.url_id = u.id), '') = ''";
pub const DUPLICATE: &str = "(SELECT p.meta_description FROM pages p WHERE p.url_id = u.id) IN (SELECT o.v FROM temp.occ_md o WHERE o.n > 1)";
pub const OVER: &str =
    "length((SELECT p.meta_description FROM pages p WHERE p.url_id = u.id)) > thr('meta.over')";
pub const BELOW: &str = "length((SELECT p.meta_description FROM pages p WHERE p.url_id = u.id)) >= 1 \
     AND length((SELECT p.meta_description FROM pages p WHERE p.url_id = u.id)) < thr('meta.below')";
pub const MULTIPLE: &str = "(SELECT p.meta_description_count FROM pages p WHERE p.url_id = u.id) > 1";

const LENGTH: &str =
    "length((SELECT p.meta_description FROM pages p WHERE p.url_id = u.id)) || ' characters'";

pub const RULES: &[Rule] = &[
    rule("meta.missing", "Meta description: missing", C, Low, Scope::Pages, MISSING)
        .needs(Need::Extraction)
        .evidence("'No meta description'")
        .explain(
            "Without a description, search engines pick a snippet from the page, which is often less convincing than one you write.",
            "Add a <meta name=\"description\"> that summarises the page and invites the click, in about 70 to 155 characters.",
        ),
    rule("meta.duplicate", "Meta description: duplicate", C, Low, Scope::Pages, DUPLICATE)
        .site_wide()
        .needs(Need::Extraction)
        .reads(&[Derived::OccMeta])
        .evidence("(SELECT o.n FROM temp.occ_md o WHERE o.v = (SELECT p.meta_description FROM pages p WHERE p.url_id = u.id)) || ' pages share this description'")
        .explain(
            "Repeated descriptions make different pages look the same in search results.",
            "Write a description for each page that says what is specific to it.",
        ),
    rule("meta.over", "Meta description: over {n} characters", C, Low, Scope::Pages, OVER)
        .threshold(Unit::Chars, 155.0, 10.0, 1000.0)
        .needs(Need::Extraction)
        .evidence(LENGTH)
        .explain(
            "Long descriptions are cut off in search results.",
            "Shorten the description and put the key message first.",
        ),
    rule("meta.below", "Meta description: below {n} characters", C, Low, Scope::Pages, BELOW)
        .threshold(Unit::Chars, 70.0, 1.0, 500.0)
        .needs(Need::Extraction)
        .evidence(LENGTH)
        .explain(
            "Very short descriptions leave room unused that could persuade searchers to choose this page.",
            "Expand the description with what the visitor will find and why it is useful.",
        ),
    rule("meta.multiple", "Meta description: multiple", C, Medium, Scope::Pages, MULTIPLE)
        .needs(Need::Extraction)
        .evidence("(SELECT p.meta_description_count FROM pages p WHERE p.url_id = u.id) || ' meta descriptions'")
        .explain(
            "When a page has several descriptions, search engines may use any of them, or none.",
            "Keep one meta description; check templates and plugins that add their own.",
        ),
];
