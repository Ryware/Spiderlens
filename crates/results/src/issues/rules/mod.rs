//! The first rule set (spec 009, "First rule set" table), one file per
//! category.

pub mod canonicals;
pub mod content;
pub mod directives;
pub mod headings;
pub mod hreflang;
pub mod images;
pub mod links;
pub mod meta;
pub mod other;
pub mod response;
pub mod security;
pub mod titles;
pub mod url;

/// True when this page has the `tabs::page_flag` bit given (as a number
/// literal).
macro_rules! has_flag {
    ($bit:literal) => {
        concat!(
            "(COALESCE((SELECT s.flags FROM temp.page_flags s WHERE s.url_id = u.id), 0) & ",
            $bit,
            ") <> 0"
        )
    };
}

/// The first link of `kind` from this page to a URL that was fetched and
/// failed or did not return 2xx, with its state, as evidence. Targets never
/// fetched are "not checked" (FR-011); the matching condition is a
/// `temp.page_flags` bit.
macro_rules! target_not_ok_evidence {
    ($kind:literal) => {
        concat!(
            "(SELECT t.url || ' (' || CASE WHEN t.error_class IS NOT NULL THEN error_label(t.error_class) \
             WHEN t.status NOT BETWEEN 200 AND 299 THEN t.status ELSE 'not indexable' END || ')' \
             FROM links l JOIN urls t ON t.id = l.to_id WHERE l.from_id = u.id AND l.kind IN (",
            $kind,
            ") AND t.id <> u.id AND t.fetched_at IS NOT NULL \
             AND (t.error_class IS NOT NULL OR t.status IS NULL OR t.status NOT BETWEEN 200 AND 299 OR t.noindex = 1 \
             OR (t.canonical IS NOT NULL AND t.canonical <> t.url)) ORDER BY l.pos LIMIT 1)"
        )
    };
}

pub(crate) use {has_flag, target_not_ok_evidence};
