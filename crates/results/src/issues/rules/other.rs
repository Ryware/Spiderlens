//! Pagination and structured data.

use super::{target_not_ok, target_not_ok_evidence};
use crate::issues::{Category, Priority::*, Rule, Scope, rule};
use crate::tabs::Need;

pub const RULES: &[Rule] = &[
    rule(
        "pagination.bad_target",
        "Pagination: next or prev target not 200",
        Category::Pagination,
        Medium,
        Scope::Pages,
        target_not_ok!("'next', 'prev'"),
    )
    .site_wide()
    .needs(Need::Extraction)
    .evidence(target_not_ok_evidence!("'next', 'prev'"))
    .explain(
        "A rel=\"next\" or rel=\"prev\" link points at a URL that fails, redirects or is not indexable, which breaks the series for crawlers.",
        "Point next and prev at the final URLs of the neighbouring pages.",
    ),
    rule(
        "structured.invalid_json",
        "Structured data: JSON-LD is not valid JSON",
        Category::StructuredData,
        Medium,
        Scope::Pages,
        "EXISTS (SELECT 1 FROM structured_data s WHERE s.url_id = u.id AND s.kind = 'json_ld' AND s.parseable = 0)",
    )
    .needs(Need::Extraction)
    .evidence("(SELECT COUNT(*) || ' JSON-LD blocks cannot be parsed' FROM structured_data s WHERE s.url_id = u.id AND s.kind = 'json_ld' AND s.parseable = 0)")
    .explain(
        "Search engines and AI assistants skip structured data they cannot parse, so the page loses rich results and machine-readable facts.",
        "Validate the JSON-LD block (a missing comma or quote is the usual cause) and fix the template that writes it.",
    ),
];
