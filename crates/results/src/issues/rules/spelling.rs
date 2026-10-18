//! Spelling and grammar (spec 027 part D, FR-D12). The Spelling tab filters
//! test the same conditions on the `spell_pages` and `spell_findings`
//! tables (spec 009 FR-005). Pages the spelling check did not run on have no
//! row there, so no rule flags them; the rules are unavailable until the
//! check has run on the crawl file.

use crate::issues::{Category::Spelling as C, Priority::*, Rule, Scope, rule};
use crate::tabs::Need;

pub const ERRORS: &str = "(SELECT s.errors FROM spell_pages s WHERE s.url_id = u.id) > 0";
pub const GRAMMAR: &str =
    "EXISTS (SELECT 1 FROM spell_findings f WHERE f.url_id = u.id AND f.kind = 'grammar')";
pub const NO_DICTIONARY: &str =
    "(SELECT s.status FROM spell_pages s WHERE s.url_id = u.id) = 'no_dictionary'";
pub const LANG_MISMATCH: &str = "(SELECT s.lang_mismatch FROM spell_pages s WHERE s.url_id = u.id) = 1";

pub const RULES: &[Rule] = &[
    rule("spell.errors", "Spelling errors on page", C, Low, Scope::Pages, ERRORS)
        .needs(Need::Spelling)
        .evidence(
            "(SELECT s.errors FROM spell_pages s WHERE s.url_id = u.id) || ' spelling errors, for example ' \
             || COALESCE((SELECT group_concat(word, ', ') FROM (SELECT DISTINCT f.word FROM spell_findings f \
             WHERE f.url_id = u.id AND f.kind = 'spelling' ORDER BY f.pos LIMIT 5)), '')",
        )
        .explain(
            "Misspelt words look careless, lower trust in the page, and can keep it from showing for searches that use the correct spelling.",
            "Correct the words listed in the page's details. Names and product terms that are right can be added to the ignore list in the spelling settings.",
        ),
    rule("spell.grammar", "Grammar issues on page", C, Opportunity, Scope::Pages, GRAMMAR)
        .needs(Need::Spelling)
        .evidence(
            "(SELECT COUNT(*) || ' grammar issues, for example \"' || MIN(f.word) || '\"' \
             FROM spell_findings f WHERE f.url_id = u.id AND f.kind = 'grammar')",
        )
        .explain(
            "Grammar slips make text harder to read and look unprofessional. The grammar check is for English text only.",
            "Read the sentences listed in the page's details and reword them. Some suggestions are matters of style, so use your judgement.",
        ),
    rule(
        "spell.no_dictionary",
        "Page not checked: no dictionary",
        C,
        Opportunity,
        Scope::Pages,
        NO_DICTIONARY,
    )
    .needs(Need::Spelling)
    .evidence(
        "'No dictionary installed for ' || COALESCE((SELECT s.lang FROM spell_pages s WHERE s.url_id = u.id), 'this language')",
    )
    .explain(
        "The page is in a language that has no dictionary installed, so its spelling was not checked and its errors, if any, are not counted.",
        "Add a dictionary for the language in the spelling settings (a download, or files from your computer) and run the check again.",
    ),
    rule(
        "spell.lang_mismatch",
        "Declared language does not match the text",
        C,
        Medium,
        Scope::Pages,
        LANG_MISMATCH,
    )
    .needs(Need::Spelling)
    .evidence(
        "'Declared language ' || COALESCE((SELECT s.lang FROM spell_pages s WHERE s.url_id = u.id), 'unknown') \
         || ', but most of the text is written in another script'",
    )
    .standards(&["WCAG 2.2 3.1.1"])
    .explain(
        "The language set on the page does not fit its text. Screen readers may read it with the wrong voice, browsers may offer the wrong translation, and spelling cannot be checked properly.",
        "Set the lang attribute on the root element to the language the text is really in, for example <html lang=\"he\">.",
    ),
];
