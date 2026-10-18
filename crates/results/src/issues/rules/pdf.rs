//! PDFs (spec 027 part C, FR-C07). The PDFs tab filters test the same
//! conditions on the `pdfs` table (spec 009 FR-005); a rule adds the
//! content type test because it runs over every internal URL, where the tab
//! lists PDFs only.

use crate::issues::{Category::Pdf as C, Priority::*, Rule, Scope, rule};
use crate::tabs::Need;

pub const MISSING_TITLE: &str = "content_kind(u.content_type) = 'pdf' \
     AND COALESCE(trim((SELECT p.title FROM pdfs p WHERE p.url_id = u.id)), '') = ''";
pub const NO_TEXT: &str =
    "content_kind(u.content_type) = 'pdf' AND (SELECT p.no_text FROM pdfs p WHERE p.url_id = u.id) = 1";
pub const OVER_SIZE_LIMIT: &str = "content_kind(u.content_type) = 'pdf' \
     AND (SELECT p.status FROM pdfs p WHERE p.url_id = u.id) = 'over_size_limit'";
pub const UNREADABLE: &str = "content_kind(u.content_type) = 'pdf' \
     AND (SELECT p.status FROM pdfs p WHERE p.url_id = u.id) IN ('failed', 'partly_read')";
pub const NOINDEX_LINKED: &str = "content_kind(u.content_type) = 'pdf' AND u.noindex = 1 \
     AND EXISTS (SELECT 1 FROM links l JOIN urls f ON f.id = l.from_id \
     WHERE l.to_id = u.id AND l.kind = 'anchor' AND f.internal = 1 AND f.id <> u.id)";

pub const RULES: &[Rule] = &[
    rule("pdf.missing_title", "PDF: missing title", C, Medium, Scope::InternalUrls, MISSING_TITLE)
        .needs(Need::Pdf)
        .evidence("'The PDF has no title'")
        .standards(&["WCAG 2.2 2.4.2"])
        .explain(
            "Search engines show a PDF's title as the headline of its result, and PDF readers show it in the window. Without one, the file name is used, which is often unclear.",
            "Set the document title in the source file (for example in the Properties of the document) and export the PDF again.",
        ),
    rule("pdf.no_text", "PDF: no text", C, Medium, Scope::InternalUrls, NO_TEXT)
        .needs(Need::Pdf)
        .evidence("'No text found: the pages look like scanned images'")
        .standards(&["WCAG 2.2 1.1.1"])
        .explain(
            "The PDF holds pictures of pages and no text. Search engines cannot read it, and screen reader users cannot either.",
            "Run text recognition (OCR) on the file, or export the PDF again from the original document so the text is kept.",
        ),
    rule("pdf.over_size_limit", "PDF: over size limit", C, Low, Scope::InternalUrls, OVER_SIZE_LIMIT)
        .needs(Need::Pdf)
        .evidence("'Larger than the PDF size limit of this crawl, so it was not read'")
        .explain(
            "The file is larger than the PDF size limit set for the crawl, so its title, text and links were not read. Very large PDFs are also slow to open for visitors.",
            "Compress or split the PDF. If it is fine as it is, raise the PDF size limit in the crawl settings and crawl again.",
        ),
    rule("pdf.unreadable", "PDF: could not be read", C, Medium, Scope::InternalUrls, UNREADABLE)
        .needs(Need::Pdf)
        .evidence(
            "(SELECT CASE p.status WHEN 'partly_read' THEN 'Partly read' ELSE 'Could not be read' END \
             || COALESCE(': ' || p.status_reason, '') FROM pdfs p WHERE p.url_id = u.id)",
        )
        .explain(
            "The file is damaged or uses features that could not be read, so it was read only in part or not at all. Search engines and some readers may have the same trouble.",
            "Open the file yourself to check it. Export it again from the original document, or replace the file.",
        ),
    rule(
        "pdf.noindex_linked",
        "PDF: linked from internal pages but noindex",
        C,
        Medium,
        Scope::InternalUrls,
        NOINDEX_LINKED,
    )
    .site_wide()
    .needs(Need::Pdf)
    .evidence(
        "(SELECT COUNT(DISTINCT l.from_id) || ' internal pages link to this PDF, which asks not to be indexed' \
         FROM links l JOIN urls f ON f.id = l.from_id \
         WHERE l.to_id = u.id AND l.kind = 'anchor' AND f.internal = 1 AND f.id <> u.id)",
    )
    .explain(
        "Your own pages send visitors and link value to a PDF that asks search engines to leave it out of results. Either the links or the directive is probably a mistake.",
        "If the PDF should be found, remove the noindex from its X-Robots-Tag header. If not, stop linking to it or accept that it is for visitors only.",
    ),
];
