//! Canonicals. The 007 Canonicals filters use these same conditions
//! (FR-005).

use super::{target_not_ok_evidence, target_not_ok_targets};
use crate::issues::{Category::Canonicals as C, Priority::*, Rule, Scope, rule};
use crate::tabs::Need;

pub const CANONICALISED: &str = "u.canonical IS NOT NULL AND u.canonical <> u.url";
pub const MISSING: &str = "u.canonical IS NULL";
pub const CONFLICTING: &str = "u.canonical_count > 1";

pub const RULES: &[Rule] = &[
    rule("canonical.canonicalised", "Canonicalised (points to another URL)", C, Low, Scope::Pages, CANONICALISED)
        .needs(Need::Extraction)
        .evidence("'Canonical: ' || u.canonical")
        .explain(
            "This page tells search engines that another URL is the main version, so this one is usually not shown in results. Right for copies, wrong for pages that should rank.",
            "If this page should rank on its own, make its canonical point to itself. Otherwise make sure internal links go to the canonical URL.",
        ),
    rule("canonical.missing", "Canonical: missing", C, Opportunity, Scope::Pages, MISSING)
        .needs(Need::Extraction)
        .evidence("'No canonical declared'")
        .explain(
            "Without a canonical, search engines guess the main URL when the same page is reachable through several addresses (parameters, trailing slashes, http and https).",
            "Add <link rel=\"canonical\" href=\"...\"> pointing to the page's preferred URL, usually itself.",
        ),
    rule("canonical.conflicting", "Canonical: multiple or conflicting", C, High, Scope::Pages, CONFLICTING)
        .needs(Need::Extraction)
        .evidence("u.canonical_count || ' canonical declarations'")
        .explain(
            "The page declares more than one canonical (in the HTML, the HTTP header, or both). Search engines may ignore all of them.",
            "Keep one canonical declaration, in the HTML head or the Link header, pointing to one URL.",
        ),
    rule(
        "canonical.outside_head",
        "Canonical: outside head",
        C,
        Medium,
        Scope::Pages,
        super::has_flag!("128"),
    )
    .needs(Need::Extraction)
    .evidence("'A canonical link sits in the body'")
    .explain(
        "Search engines ignore canonicals outside the <head>, so the declaration has no effect.",
        "Move the canonical link into the <head>, and check for elements above it that close the head early.",
    ),
    rule(
        "canonical.bad_target",
        "Canonical: points to a non-200 or non-indexable URL",
        C,
        High,
        Scope::Pages,
        super::has_flag!("4096"),
    )
    .site_wide()
    .needs(Need::Extraction)
    .evidence(target_not_ok_evidence!("'canonical'"))
    .targets(target_not_ok_targets!("'canonical'"))
    .explain(
        "The canonical names a URL that redirects, fails, is noindexed or canonicalises elsewhere, so search engines cannot follow it and may ignore it.",
        "Point the canonical at the final, indexable URL that returns 200.",
    ),
];
