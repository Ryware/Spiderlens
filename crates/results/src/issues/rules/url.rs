//! URL shape. The 007 URL filters use these same conditions (FR-005).

use crate::issues::{Category::Url as C, Priority::*, Rule, Scope, rule};

pub const UPPERCASE: &str = "url_check(u.url, 'uppercase')";
pub const UNDERSCORES: &str = "url_check(u.url, 'underscore')";
pub const PARAMETERS: &str = "instr(u.url, '?') > 0";
pub const OVER: &str = "length(u.url) > thr('url.over')";
pub const NON_ASCII: &str = "url_check(u.url, 'non_ascii')";
pub const MULTIPLE_SLASHES: &str = "url_check(u.url, 'multiple_slashes')";
pub const SPACES: &str = "url_check(u.url, 'spaces')";

const MAYBE: &str = "Search engines treat URL variants as different pages, and visitors find odd URLs harder to read and share.";

pub const RULES: &[Rule] = &[
    rule("url.uppercase", "URL: uppercase characters", C, Low, Scope::InternalUrls, UPPERCASE)
        .evidence("'Contains uppercase'")
        .explain(
            "URLs are case-sensitive after the host, so /Page and /page can be two pages; mixed case invites duplicates and broken links.",
            "Use lowercase URLs, and redirect uppercase variants to them.",
        ),
    rule("url.underscores", "URL: underscores", C, Low, Scope::InternalUrls, UNDERSCORES)
        .evidence("'Contains _'")
        .explain(
            "Search engines read hyphens as word separators but may not read underscores that way.",
            "Prefer hyphens in new URLs. Changing existing URLs needs redirects, so only do it when other changes are planned.",
        ),
    rule("url.parameters", "URL: query parameters", C, Low, Scope::InternalUrls, PARAMETERS)
        .evidence("'?' || substr(u.url, instr(u.url, '?') + 1)")
        .explain(
            MAYBE,
            "Link to clean URLs where you can, and canonicalise parameter variants (sorting, tracking) to the main URL.",
        ),
    rule("url.over", "URL: over {n} characters", C, Low, Scope::InternalUrls, OVER)
        .threshold(crate::issues::Unit::Chars, 115.0, 20.0, 2048.0)
        .evidence("length(u.url) || ' characters'")
        .explain(
            "Long URLs are cut off in search results and are hard to read and share.",
            "Use short, descriptive paths without repeated or empty segments.",
        ),
    rule("url.non_ascii", "URL: non-ASCII characters", C, Low, Scope::InternalUrls, NON_ASCII)
        .evidence("readable_url(u.url)")
        .explain(
            "Non-ASCII characters are percent-encoded in links and can be handled inconsistently by older tools and sharing.",
            "This is fine for sites in non-Latin languages. Otherwise prefer ASCII paths; keep links consistent in one form.",
        ),
    rule("url.multiple_slashes", "URL: multiple slashes", C, Low, Scope::InternalUrls, MULTIPLE_SLASHES)
        .evidence("'Contains //'")
        .explain(MAYBE, "Fix the links that produce a double slash, and redirect the double-slash URL to the clean one."),
    rule("url.spaces", "URL: spaces", C, Low, Scope::InternalUrls, SPACES)
        .evidence("'Contains a space'")
        .explain(
            "Spaces in URLs are encoded as %20 and often come from a typo in a link; some tools break such URLs.",
            "Replace spaces with hyphens and fix the links that contain them.",
        ),
];
