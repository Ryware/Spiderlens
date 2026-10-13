//! Hreflang annotations.

use super::{target_not_ok, target_not_ok_evidence};
use crate::issues::{Category::Hreflang as C, Priority::*, Rule, Scope, rule};
use crate::tabs::Need;

pub const RULES: &[Rule] = &[
    rule(
        "hreflang.no_return",
        "Hreflang: missing return link",
        C,
        High,
        Scope::Pages,
        "EXISTS (SELECT 1 FROM links l JOIN urls t ON t.id = l.to_id WHERE l.from_id = u.id AND l.kind = 'hreflang' \
         AND t.id <> u.id AND t.fetched_at IS NOT NULL AND t.error_class IS NULL AND t.status BETWEEN 200 AND 299 \
         AND NOT EXISTS (SELECT 1 FROM links r WHERE r.from_id = t.id AND r.kind = 'hreflang' AND r.to_id = u.id))",
    )
    .site_wide()
    .needs(Need::Extraction)
    .evidence(
        "(SELECT t.url || ' does not link back' FROM links l JOIN urls t ON t.id = l.to_id WHERE l.from_id = u.id AND l.kind = 'hreflang' \
         AND t.id <> u.id AND t.fetched_at IS NOT NULL AND t.error_class IS NULL AND t.status BETWEEN 200 AND 299 \
         AND NOT EXISTS (SELECT 1 FROM links r WHERE r.from_id = t.id AND r.kind = 'hreflang' AND r.to_id = u.id) ORDER BY l.pos LIMIT 1)",
    )
    .explain(
        "Hreflang only works when every language version lists the others. If a page you point to does not point back, search engines ignore the pair.",
        "Add the missing hreflang link on the alternate page so each version lists every other version, including itself.",
    ),
    rule(
        "hreflang.bad_target",
        "Hreflang: target not 200 or not indexable",
        C,
        High,
        Scope::Pages,
        target_not_ok!("'hreflang'"),
    )
    .site_wide()
    .needs(Need::Extraction)
    .evidence(target_not_ok_evidence!("'hreflang'"))
    .explain(
        "An alternate language URL redirects, fails, is noindexed or canonicalises elsewhere, so search engines discard it.",
        "Point each hreflang link at the final, indexable URL of that language version.",
    ),
    rule(
        "hreflang.no_self",
        "Hreflang: missing self reference",
        C,
        Low,
        Scope::Pages,
        "EXISTS (SELECT 1 FROM links l WHERE l.from_id = u.id AND l.kind = 'hreflang') \
         AND NOT EXISTS (SELECT 1 FROM links l WHERE l.from_id = u.id AND l.kind = 'hreflang' AND l.to_id = u.id)",
    )
    .needs(Need::Extraction)
    .evidence("'Lists other languages but not itself'")
    .explain(
        "Each language version should include itself in its hreflang set; without it some search engines ignore the annotations.",
        "Add an hreflang link for the page's own language pointing to its own URL.",
    ),
    rule(
        "hreflang.no_x_default",
        "Hreflang: missing x-default",
        C,
        Opportunity,
        Scope::Pages,
        "EXISTS (SELECT 1 FROM links l WHERE l.from_id = u.id AND l.kind = 'hreflang') \
         AND NOT EXISTS (SELECT 1 FROM links l WHERE l.from_id = u.id AND l.kind = 'hreflang' AND lower(l.lang) = 'x-default')",
    )
    .needs(Need::Extraction)
    .evidence("'No x-default alternate'")
    .explain(
        "x-default tells search engines which version to show visitors whose language you do not cover.",
        "Add hreflang=\"x-default\" pointing to your language selector or main version.",
    ),
    rule(
        "hreflang.invalid_code",
        "Hreflang: invalid language or region code",
        C,
        Medium,
        Scope::Pages,
        "EXISTS (SELECT 1 FROM links l WHERE l.from_id = u.id AND l.kind = 'hreflang' AND NOT lang_code_ok(l.lang))",
    )
    .needs(Need::Extraction)
    .evidence("(SELECT 'Code \"' || COALESCE(l.lang, '') || '\"' FROM links l WHERE l.from_id = u.id AND l.kind = 'hreflang' AND NOT lang_code_ok(l.lang) ORDER BY l.pos LIMIT 1)")
    .explain(
        "Search engines ignore hreflang values that are not a valid language code (ISO 639-1), optionally with a region (ISO 3166-1), such as en, en-GB or he-IL.",
        "Fix the code, for example use en-GB instead of en-UK, and he instead of iw.",
    ),
];
