//! Security.

use crate::issues::{Category::Security as C, Priority::*, Rule, Scope, rule};
use crate::tabs::Need;

pub const RULES: &[Rule] = &[
    rule("security.http_page", "Page served over HTTP", C, High, Scope::Pages, "u.url LIKE 'http:%'")
        .evidence("'Not HTTPS'")
        .explain(
            "Browsers mark HTTP pages as not secure, and anything typed into them can be read on the way.",
            "Serve the page over HTTPS and redirect the HTTP URL to it with a 301.",
        ),
    rule(
        "security.mixed_content",
        "Mixed content",
        C,
        Medium,
        Scope::Pages,
        concat!("u.url LIKE 'https:%' AND ", super::links::has_link!("32")),
    )
    .evidence(
        "(SELECT COUNT(*) || ' HTTP images, first: ' || MIN(COALESCE(t.url, l.target)) FROM links l LEFT JOIN urls t ON t.id = l.to_id \
         WHERE l.from_id = u.id AND l.kind = 'image' AND COALESCE(t.url, l.target) LIKE 'http:%')",
    )
    .explain(
        "This HTTPS page loads images over plain HTTP. Browsers block or warn about such content and the padlock is lost.",
        "Load every resource over HTTPS (or with a relative URL).",
    ),
    rule(
        "security.no_hsts",
        "Missing HSTS header",
        C,
        Low,
        Scope::Pages,
        "u.url LIKE 'https:%' AND has_header((SELECT h.headers FROM headers h WHERE h.url_id = u.id), 'strict-transport-security') = 0",
    )
    .needs(Need::Extraction)
    .evidence("'No Strict-Transport-Security header'")
    .explain(
        "Without HSTS, a visitor's first request can still go over HTTP, where it can be intercepted.",
        "Send Strict-Transport-Security: max-age=31536000; includeSubDomains on HTTPS responses once the whole site works over HTTPS.",
    ),
    rule(
        "security.no_nosniff",
        "Missing X-Content-Type-Options header",
        C,
        Low,
        Scope::Pages,
        "has_header((SELECT h.headers FROM headers h WHERE h.url_id = u.id), 'x-content-type-options') = 0",
    )
    .needs(Need::Extraction)
    .evidence("'No X-Content-Type-Options: nosniff'")
    .explain(
        "Without nosniff, browsers may guess a file's type and run content as a script that was not meant to be one.",
        "Send X-Content-Type-Options: nosniff on all responses.",
    ),
];
