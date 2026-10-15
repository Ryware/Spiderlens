//! Response codes and redirects.

use crate::issues::{Category::ResponseCodes as C, Priority::*, Rule, Scope, Unit, rule};
use crate::tabs::{Derived, Need};

const STATUS_EVIDENCE: &str = "u.status || COALESCE(' ' || status_text(u.status), '')";

pub const RULES: &[Rule] = &[
    rule(
        "response.internal_4xx",
        "Internal client error (4xx)",
        C,
        High,
        Scope::InternalUrls,
        "u.error_class IS NULL AND u.status BETWEEN 400 AND 499",
    )
    .evidence(STATUS_EVIDENCE)
    .explain(
        "Visitors and search engines that follow a link to this URL reach an error page instead of content, and the links pointing here waste the authority they pass.",
        "Restore the page, redirect it with a 301 to its closest replacement, or update the links that point to it (see Inlinks).",
    ),
    rule(
        "response.internal_5xx",
        "Internal server error (5xx)",
        C,
        High,
        Scope::InternalUrls,
        "u.error_class IS NULL AND u.status BETWEEN 500 AND 599",
    )
    .evidence(STATUS_EVIDENCE)
    .explain(
        "The server failed to answer. Search engines slow down crawling and may drop pages that keep failing.",
        "Check the server logs for this URL, fix the error, and recrawl. If it only fails under load, lower the crawl speed and check hosting capacity.",
    ),
    rule(
        "response.internal_no_response",
        "Internal URL with no response",
        C,
        High,
        Scope::InternalUrls,
        "u.error_class IS NOT NULL",
    )
    .evidence("error_label(u.error_class) || COALESCE(': ' || u.error_message, '')")
    .explain(
        "The URL could not be fetched at all (timeout, connection, TLS, redirect or content problem), so nobody can reach it.",
        "Open the URL to see the error, then fix the cause named in the evidence: DNS, certificate, timeouts, or the redirect it points to.",
    ),
    rule(
        "response.external_error",
        "External link target with an error",
        C,
        Medium,
        Scope::ExternalUrls,
        "u.error_class IS NOT NULL OR u.status >= 400",
    )
    .evidence("CASE WHEN u.error_class IS NOT NULL THEN error_label(u.error_class) ELSE u.status || COALESCE(' ' || status_text(u.status), '') END")
    .explain(
        "The site links to an outside page that is broken, which frustrates visitors and signals a poorly maintained page.",
        "Update the link to a working page, or remove it. The Inlinks section lists every page that links here.",
    ),
    rule(
        "response.internal_redirect",
        "Internal redirect (3xx)",
        C,
        Low,
        Scope::InternalUrls,
        "u.error_class IS NULL AND u.status BETWEEN 300 AND 399",
    )
    .evidence("u.status || COALESCE(' to ' || (SELECT r.url FROM urls r WHERE r.id = u.redirect_to), '')")
    .explain(
        "An internal URL redirects. Each redirect adds a round trip for visitors and crawlers, and links that point at it should point at the destination.",
        "Update internal links to point straight at the redirect target. Keep the redirect for outside links and bookmarks.",
    ),
    rule(
        "response.redirect_loop",
        "Redirect loop",
        C,
        High,
        Scope::AllUrls,
        "(u.error_class = 'redirect_loop' OR u.id IN (SELECT r.id FROM temp.redirects r WHERE r.is_loop))",
    )
    .site_wide()
    .reads(&[Derived::Redirects])
    .evidence("'Redirects back to a URL already in its chain'")
    .explain(
        "The redirects never reach a page: browsers show an error and search engines give up.",
        "Follow the chain in the Redirects section and change one hop so it ends at a page that returns 200.",
    ),
    rule(
        "response.redirect_chain",
        "Redirect chain of {n} or more hops",
        C,
        Medium,
        Scope::AllUrls,
        "u.id IN (SELECT r.id FROM temp.redirects r WHERE NOT r.is_loop AND r.hops >= thr('response.redirect_chain'))",
    )
    .site_wide()
    .threshold(Unit::Hops, 2.0, 2.0, 20.0)
    .reads(&[Derived::Redirects])
    .evidence("(SELECT r.hops FROM temp.redirects r WHERE r.id = u.id) || ' hops'")
    .explain(
        "Every extra hop slows the page down, and search engines may stop following long chains before they reach the content.",
        "Point the first URL straight at the final destination, and update links to use the final URL.",
    ),
    rule(
        "response.blocked_robots",
        "Internal URL blocked by robots.txt",
        C,
        Low,
        Scope::AllUrls,
        "u.internal = 1 AND u.reason IN ('blocked_by_robots', 'robots_unavailable')",
    )
    .evidence("CASE u.reason WHEN 'robots_unavailable' THEN 'robots.txt unavailable' ELSE COALESCE('Disallow: ' || u.robots_rule, 'Disallowed') END")
    .explain(
        "Search engines cannot crawl this URL, so its content cannot rank. That is right for private sections and a mistake for pages you want found.",
        "If the page should be found, remove or narrow the Disallow rule in robots.txt. If not, no action is needed; consider turning this rule off.",
    ),
    rule(
        "response.slow",
        "Slow response (over {n} ms)",
        C,
        Low,
        Scope::Pages,
        "u.response_ms > thr('response.slow')",
    )
    .threshold(Unit::Ms, 1000.0, 50.0, 60000.0)
    .needs(Need::Timing)
    .evidence("u.response_ms || ' ms'")
    .explain(
        "Slow server responses delay everything else on the page and limit how much of the site search engines crawl.",
        "Check server time for this page: caching, database queries, and hosting. Response time was measured from this computer, so compare a few pages before acting.",
    ),
];
