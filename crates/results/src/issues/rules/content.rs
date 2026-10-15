//! Page content.

use crate::issues::{Category::Content as C, Priority::*, Rule, Scope, Unit, rule};
use crate::tabs::{Derived, Need};

/// Flesch reading ease, 0 to 100, for pages with at least 100 words whose
/// sentences and syllables were counted (spec 009 FR-031, plan D11a).
macro_rules! readability {
    () => {
        "(SELECT CASE WHEN p.word_count >= 100 AND p.sentence_count > 0 AND p.syllable_count > 0 \
     THEN round(max(0.0, min(100.0, 206.835 - 1.015 * p.word_count / (p.sentence_count * 1.0) \
     - 84.6 * p.syllable_count / (p.word_count * 1.0))), 1) END FROM pages p WHERE p.url_id = u.id)"
    };
}

pub(crate) use readability;

pub const READABILITY: &str = readability!();

pub const RULES: &[Rule] = &[
    rule(
        "content.duplicate",
        "Exact duplicate content",
        C,
        Medium,
        Scope::Pages,
        "(SELECT p.text_hash FROM pages p WHERE p.url_id = u.id) IN (SELECT o.v FROM temp.occ_text o WHERE o.n > 1)",
    )
    .site_wide()
    .needs(Need::Extraction)
    .reads(&[Derived::OccText])
    .evidence("(SELECT o.n FROM temp.occ_text o WHERE o.v = (SELECT p.text_hash FROM pages p WHERE p.url_id = u.id)) || ' pages have the same text'")
    .explain(
        "Several URLs show exactly the same text. Search engines pick one to show and may pick the wrong one; the others compete for nothing.",
        "Keep one URL for the content. Redirect or canonicalise the copies to it, or make each page's text distinct.",
    ),
    rule(
        "content.thin",
        "Thin content (below {n} words)",
        C,
        Low,
        Scope::Pages,
        "(SELECT p.word_count FROM pages p WHERE p.url_id = u.id) < thr('content.thin')",
    )
    .threshold(Unit::Words, 200.0, 1.0, 10000.0)
    .needs(Need::Extraction)
    .evidence("(SELECT p.word_count FROM pages p WHERE p.url_id = u.id) || ' words'")
    .explain(
        "Pages with very little text rarely answer a searcher's question, and many of them can lower how search engines rate the site.",
        "Add useful content, merge the page with a related one, or keep it out of the index if it is not meant to rank (contact forms, filters).",
    ),
    rule(
        "content.partial",
        "Partially read or badly encoded page",
        C,
        Medium,
        Scope::Pages,
        "EXISTS (SELECT 1 FROM pages p WHERE p.url_id = u.id AND (p.partial = 1 OR p.encoding_errors > 0 OR p.extract_error IS NOT NULL))",
    )
    .needs(Need::Extraction)
    .evidence(
        "(SELECT CASE WHEN p.extract_error IS NOT NULL THEN p.extract_error WHEN p.partial = 1 THEN 'Body cut off before the end' \
         ELSE p.encoding_errors || ' characters could not be decoded' || COALESCE(' as ' || p.encoding, '') END FROM pages p WHERE p.url_id = u.id)",
    )
    .explain(
        "The page could only be partly read: it was cut off, or its text is not valid in the declared character set. Visitors may see garbled text and search engines may miss content.",
        "Serve the full page within the size limit, and declare the right character set (UTF-8 is safest) in the Content-Type header or a <meta charset>.",
    ),
    rule("content.large_html", "Large HTML page (over {n} bytes)", C, Low, Scope::Pages, "u.size_bytes > thr('content.large_html')")
        .threshold(Unit::Bytes, 1_048_576.0, 10_240.0, 52_428_800.0)
        .needs(Need::Timing)
        .evidence("u.size_bytes || ' bytes'")
        .explain(
            "Very large HTML takes long to download and parse, especially on phones, and search engines may stop reading before the end.",
            "Remove inline data, repeated markup and unused scripts from the HTML; load secondary content on demand.",
        ),
    rule(
        "content.missing_lang",
        "Missing language attribute",
        C,
        Low,
        Scope::Pages,
        "COALESCE((SELECT p.lang FROM pages p WHERE p.url_id = u.id), '') = ''",
    )
    .needs(Need::Extraction)
    .evidence("'No lang on <html>'")
    .standards(&["WCAG 2.2 3.1.1"])
    .explain(
        "Without a lang attribute, screen readers may read the page with the wrong voice, and browsers cannot offer the right translation or hyphenation.",
        "Add the page language to the root element, for example <html lang=\"en\">.",
    ),
    rule(
        "content.readability",
        "Difficult to read (reading ease below {n})",
        C,
        Opportunity,
        Scope::Pages,
        concat!(readability!(), " < thr('content.readability')"),
    )
    .threshold(Unit::Score, 30.0, 0.0, 100.0)
    .needs(Need::Readability)
    .evidence(concat!("'Reading ease ' || ", readability!()))
    .explain(
        "Long sentences and long words make text hard to follow. The Flesch reading ease score is below the threshold (0 is very hard, 60 to 70 is plain English).",
        "Shorten sentences, prefer common words and break long paragraphs. The score is computed for English text only.",
    ),
];
