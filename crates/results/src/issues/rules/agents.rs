//! AI agents (spec 016 plan D5). The Agents tab filters use these same
//! conditions (FR-005 of spec 009), taken from `agents::conditions`.
//!
//! Part and weight of each rule live in `agents::score`, not here, so the
//! score can change without a new rule set version.
//!
//! Three kinds of rule live here:
//! - per-page rules over the `page_agent` summary of the raw HTML;
//! - rules over the 014 rendering tables (`renders`, `render_comparisons`),
//!   which read what a browser saw and the raw HTML did not hold;
//! - site-wide rules counted once on the crawl's start URL: entry points
//!   (including the MCP discovery answers the `agent_entry_points` pass copies
//!   from `agent_mcp_checks`), `llms.txt` and the `SearchAction` of the site.
//!
//! Not here, because the data they need is not stored yet:
//! `search_action_out_of_scope` and `prompt_injection_text`.
//!
//! `robots_blocks_agents` and `probe_blocked` read spec 015's stored robots
//! evaluation (`aeo_bot_access`) and user agent probe (`aeo_probe`) for the
//! crawlers the registry marks as user-fetch (`ai_bot_purpose`).

use crate::agents::conditions as cond;
use crate::issues::{Category::Agents as A, Priority::*, Rule, Scope, rule};
use crate::tabs::Need;

/// One `page_agent` value of the page's raw HTML.
macro_rules! raw {
    ($field:literal) => {
        concat!(
            "(SELECT a.",
            $field,
            " FROM page_agent a WHERE a.url_id = u.id AND a.source = 'raw')"
        )
    };
}

/// A count of something on the page, with the text after it.
macro_rules! count_of {
    ($field:literal, $text:literal) => {
        concat!(raw!($field), " || '", $text, "'")
    };
}

/// One value of the page's rendering comparison, by primary key.
macro_rules! cmp {
    ($field:literal) => {
        concat!(
            "(SELECT c.",
            $field,
            " FROM render_comparisons c WHERE c.url_id = u.id)"
        )
    };
}

/// A crawl-wide problem, counted once on the start URL.
macro_rules! site {
    ($cond:literal) => {
        concat!("u.source = 'start' AND ", $cond)
    };
}

/// The share (percent) of rendered-only text, or of internal links, above
/// which a page counts as hidden from agents that do not run scripts:
/// spec 014 FR-030's default.
macro_rules! hidden_share {
    () => {
        "50"
    };
}

pub const RULES: &[Rule] = &[
    rule(
        "agents.form_rendered_only",
        "Form only after JavaScript",
        A,
        High,
        Scope::Pages,
        cond::FORM_RENDERED_ONLY,
    )
    .needs(Need::Rendering)
    .evidence(concat!(
        "(SELECT COUNT(*) FROM page_forms r WHERE r.url_id = u.id AND r.source = 'rendered' \
         AND NOT EXISTS (SELECT 1 FROM page_forms w WHERE w.url_id = r.url_id AND w.source = 'raw' \
         AND w.method = r.method AND COALESCE(w.action, '') = COALESCE(r.action, ''))) \
         || ' forms exist only after rendering'",
        " || COALESCE(', first: ' || upper((SELECT r.method FROM page_forms r WHERE r.url_id = u.id \
         AND r.source = 'rendered' AND NOT EXISTS (SELECT 1 FROM page_forms w WHERE w.url_id = r.url_id \
         AND w.source = 'raw' AND w.method = r.method AND COALESCE(w.action, '') = COALESCE(r.action, '')) \
         ORDER BY r.pos LIMIT 1)) || ' ' || (SELECT COALESCE(NULLIF(r.action, ''), 'the page itself') \
         FROM page_forms r WHERE r.url_id = u.id AND r.source = 'rendered' AND NOT EXISTS \
         (SELECT 1 FROM page_forms w WHERE w.url_id = r.url_id AND w.source = 'raw' AND w.method = r.method \
         AND COALESCE(w.action, '') = COALESCE(r.action, '')) ORDER BY r.pos LIMIT 1), '')"
    ))
    .explain(
        "Most agents and AI crawlers read the HTML as sent and never run scripts. A form that a script builds is not in that HTML, so they cannot find it, fill it or send it.",
        "Put the form in the server-sent HTML, and let scripts enhance it.",
    ),
    rule(
        "agents.unlabelled_fields",
        "Form fields without a label",
        A,
        Medium,
        Scope::Pages,
        cond::UNLABELLED_FIELDS,
    )
    .needs(Need::Agents)
    .evidence(concat!(
        raw!("unlabelled_fields"),
        " || ' of ' || ",
        raw!("fields"),
        " || ' form fields have no label'"
    ))
    .standards(&["WCAG 2.2 SC 3.3.2", "WCAG 2.2 SC 4.1.2"])
    .explain(
        "An agent that reads the page's structure finds a field by its name. A field with no label, title or aria name has none, so the agent has to guess what to type into it.",
        "Give every field a visible <label for=...>, or an aria-label where a visible label does not fit.",
    ),
    rule(
        "agents.placeholder_only_fields",
        "Form fields named only by a placeholder",
        A,
        Low,
        Scope::Pages,
        cond::PLACEHOLDER_ONLY_FIELDS,
    )
    .needs(Need::Agents)
    .evidence(count_of!(
        "placeholder_only_fields",
        " fields are named only by their placeholder"
    ))
    .standards(&["WCAG 2.2 SC 3.3.2"])
    .explain(
        "A placeholder disappears once the field has a value and is not a reliable name for assistive software. Some agents read it, others ignore it.",
        "Add a real label and keep the placeholder as an example of the format.",
    ),
    rule(
        "agents.nameless_buttons",
        "Buttons without a name",
        A,
        Medium,
        Scope::Pages,
        cond::NAMELESS_BUTTONS,
    )
    .needs(Need::Agents)
    .evidence(count_of!("nameless_buttons", " buttons have no accessible name"))
    .standards(&["WCAG 2.2 SC 4.1.2"])
    .explain(
        "A button with no text, aria-label or title (an icon button, usually) tells an agent nothing about what it does, so the agent cannot choose it on purpose.",
        "Give the button text, or an aria-label that says what it does.",
    ),
    rule(
        "agents.actions_without_link",
        "Actions without a link",
        A,
        Medium,
        Scope::Pages,
        cond::ACTIONS_WITHOUT_LINK,
    )
    .needs(Need::Agents)
    .evidence(concat!(
        raw!("actions_without_link"),
        " || ' clickable elements have no link or button behind them'",
        " || COALESCE(', first: \"' || (SELECT p.text FROM page_actions p WHERE p.url_id = u.id \
         AND p.source = 'raw' AND p.text <> '' ORDER BY p.pos LIMIT 1) || '\"', '')"
    ))
    .standards(&["WCAG 2.2 SC 2.1.1", "WCAG 2.2 SC 4.1.2"])
    .explain(
        "Elements that act on a click but are not a real link or button (an anchor with no href, a div with an onclick) cannot be found or used by an agent that follows the page's structure.",
        "Use <a href> for navigation and <button> for actions, so they are exposed as links and buttons.",
    ),
    rule(
        "agents.form_challenge",
        "Form behind a challenge",
        A,
        Medium,
        Scope::Pages,
        cond::FORM_CHALLENGE,
    )
    .needs(Need::Agents)
    .evidence(concat!(
        "COALESCE('Form protected by ' || (SELECT f.challenge FROM page_forms f WHERE f.url_id = u.id \
         AND f.source = 'raw' AND f.challenge IS NOT NULL ORDER BY f.pos LIMIT 1), ",
        raw!("challenges"),
        " || ' forms behind a challenge')"
    ))
    .explain(
        "A CAPTCHA or similar widget on a form stops an agent acting for a person, even when the person is the one asking.",
        "Where you can, protect the form another way (rate limits, honeypot fields, server-side checks), or leave the challenge for the final step only.",
    ),
    rule(
        "agents.form_no_submit",
        "Form without a submit button",
        A,
        Low,
        Scope::Pages,
        cond::FORMS_WITHOUT_SUBMIT,
    )
    .needs(Need::Agents)
    .evidence(count_of!("forms_without_submit", " forms have no submit button"))
    .standards(&["WCAG 2.2 SC 3.2.2"])
    .explain(
        "A form with no submit control is sent by a script on some other event. An agent cannot tell how to send it.",
        "Add a submit button, even if a script handles it.",
    ),
    rule(
        "agents.tool_missing_description",
        "Declared agent tool without a description",
        A,
        Low,
        Scope::Pages,
        cond::TOOLS_MISSING_DESCRIPTION,
    )
    .needs(Need::Agents)
    .evidence(count_of!(
        "tools_missing_description",
        " declared tools lack a description for the tool or one of its parameters"
    ))
    .explain(
        "A form declared as an agent tool (WebMCP) is chosen by its description, and its parameters are filled from theirs. Without them the agent has only the field names.",
        "Add toolname and tooldescription to the form, and toolparamdescription to each field.",
    ),
    rule(
        "agents.no_search_action",
        "Site search not declared",
        A,
        Opportunity,
        Scope::Known,
        site!(
            "EXISTS (SELECT 1 FROM page_agent a WHERE a.source = 'raw' AND a.search_forms > 0) \
             AND NOT EXISTS (SELECT 1 FROM structured_data s WHERE s.raw LIKE '%SearchAction%')"
        ),
    )
    .site_wide()
    .needs(Need::Agents)
    .evidence(
        "(SELECT COUNT(*) || ' pages have a search form; no structured data declares a SearchAction' \
         FROM page_agent a WHERE a.source = 'raw' AND a.search_forms > 0)",
    )
    .explain(
        "The site has a search form, but no WebSite SearchAction says how to use it. Agents then have to find the form and work out its address from the page.",
        "Add a WebSite entity with a potentialAction of type SearchAction and a target URL template to the home page.",
    ),
    rule(
        "agents.no_markdown_alternate",
        "No Markdown version",
        A,
        Opportunity,
        Scope::Pages,
        cond::NO_MARKDOWN,
    )
    .needs(Need::Agents)
    .evidence("'No <link rel=\"alternate\" type=\"text/markdown\"> and no matching Link header'")
    .explain(
        "A Markdown copy of the page is much smaller than its HTML and is read by agents without extra work. Without a link to one, agents fetch and strip the HTML.",
        "Serve a Markdown version of the page and point to it with <link rel=\"alternate\" type=\"text/markdown\" href=...>.",
    ),
    rule(
        "agents.markdown_alternate_broken",
        "Markdown version does not work",
        A,
        Medium,
        Scope::Pages,
        cond::MARKDOWN_BROKEN,
    )
    .needs(Need::Agents)
    .evidence(
        "(SELECT CASE a.md_outcome WHEN 'wrong_type' THEN 'The Markdown version at ' || a.md_alternate || ' is served as something other than Markdown' \
         ELSE 'The Markdown version at ' || a.md_alternate || ' did not load' || COALESCE(' (HTTP ' || a.md_status || ')', '') END \
         FROM page_agent a WHERE a.url_id = u.id AND a.source = 'raw')",
    )
    .explain(
        "The page points to a Markdown version, but the address does not answer with Markdown: it fails, or it returns HTML or another type. An agent that trusts the link gets nothing useful.",
        "Make the address return the page as Markdown with a text/markdown (or text/plain) content type, or remove the link.",
    ),
    rule(
        "agents.mcp_endpoint_unreachable",
        "MCP endpoint does not answer",
        A,
        Medium,
        Scope::Known,
        site!(
            "EXISTS (SELECT 1 FROM agent_entry_points e WHERE e.mcp_discovery IS NOT NULL \
             AND json_extract(e.mcp_discovery, '$.answered') = 0 \
             AND json_extract(e.mcp_discovery, '$.requires_sign_in') = 0)"
        ),
    )
    .site_wide()
    .needs(Need::Agents)
    .evidence(
        "(SELECT COUNT(*) || CASE COUNT(*) WHEN 1 THEN ' MCP endpoint did not answer: ' ELSE ' MCP endpoints did not answer, first: ' END \
         || MIN(e.url) FROM agent_entry_points e WHERE e.mcp_discovery IS NOT NULL \
         AND json_extract(e.mcp_discovery, '$.answered') = 0 \
         AND json_extract(e.mcp_discovery, '$.requires_sign_in') = 0)",
    )
    .explain(
        "The site's catalog or card lists an MCP server, but a discovery request to it got no MCP answer (an error, a timeout or a page that is not MCP). Agents that follow the listing cannot connect.",
        "Check that the address in the catalog or card is the MCP endpoint, that it accepts POST requests with JSON, and that it is reachable without a browser.",
    ),
    rule(
        "agents.entry_point_invalid",
        "Agent entry point invalid",
        A,
        Medium,
        Scope::Known,
        site!("EXISTS (SELECT 1 FROM agent_entry_points e WHERE e.outcome IN ('invalid', 'wrong_type'))"),
    )
    .site_wide()
    .needs(Need::Agents)
    .evidence(
        "(SELECT COUNT(*) || CASE COUNT(*) WHEN 1 THEN ' entry point: ' ELSE ' entry points, first: ' END \
         || e.kind || ' at ' || MIN(e.url) FROM agent_entry_points e WHERE e.outcome IN ('invalid', 'wrong_type'))",
    )
    .explain(
        "An agent entry point (an AI Catalog, an API catalog, an A2A or MCP card, an OpenAPI file) exists but cannot be read: it is not valid for its convention, or it is served with the wrong content type.",
        "Fix the file the problem list names, or remove it if it is not meant to be found.",
    ),
    rule(
        "agents.no_entry_points",
        "No agent entry points",
        A,
        Opportunity,
        Scope::Known,
        site!(
            "EXISTS (SELECT 1 FROM agent_entry_points e) \
             AND NOT EXISTS (SELECT 1 FROM agent_entry_points e WHERE e.outcome = 'ok') \
             AND EXISTS (SELECT 1 FROM site_files f WHERE f.path = '/llms.txt' AND f.outcome = 'not_found') \
             AND NOT EXISTS (SELECT 1 FROM site_files f WHERE f.path = '/llms.txt' AND f.outcome = 'ok')"
        ),
    )
    .site_wide()
    .needs(Need::Agents)
    .evidence("'No catalog, card or OpenAPI file was found, and no llms.txt'")
    .explain(
        "Nothing on the site tells an agent what it offers: no AI Catalog, API catalog, agent card, MCP server card or OpenAPI file, and no llms.txt.",
        "Publish what fits your site, such as an llms.txt that points to your key pages or an OpenAPI file for your API.",
    ),
    rule(
        "agents.llms_txt_problems",
        "llms.txt has problems",
        A,
        Low,
        Scope::Known,
        site!(
            "EXISTS (SELECT 1 FROM aeo_hosts h WHERE json_extract(h.llms_txt, '$.outcome') = 'ok' \
             AND COALESCE(json_array_length(json_extract(h.llms_txt, '$.report.problems')), 0) > 0)"
        ),
    )
    .site_wide()
    .needs(Need::AiChecks)
    .evidence(
        "(SELECT json_array_length(json_extract(h.llms_txt, '$.report.problems')) || ' problems, first: ' \
         || json_extract(h.llms_txt, '$.report.problems[0].message') FROM aeo_hosts h \
         WHERE json_extract(h.llms_txt, '$.outcome') = 'ok' \
         AND COALESCE(json_array_length(json_extract(h.llms_txt, '$.report.problems')), 0) > 0 LIMIT 1)",
    )
    .explain(
        "The llms.txt file does not follow the llms.txt proposal (a title, an optional summary, then sections of links), so tools that read it may skip it or misread it.",
        "Fix the problems the AI search checks list for the file, or publish the draft SpiderLens generates.",
    ),
    rule(
        "agents.robots_blocks_agents",
        "robots.txt blocks AI agents acting for a person",
        A,
        High,
        Scope::Known,
        site!(
            "EXISTS (SELECT 1 FROM aeo_bot_access b WHERE ai_bot_purpose(b.bot) = 'user_fetch' \
             AND (b.home_verdict = 'blocked' OR b.urls_blocked > 0))"
        ),
    )
    .site_wide()
    .needs(Need::AiChecks)
    .evidence(
        "(SELECT 'robots.txt blocks ' || group_concat(n, ', ') FROM (SELECT DISTINCT ai_bot_name(b.bot) AS n \
         FROM aeo_bot_access b WHERE ai_bot_purpose(b.bot) = 'user_fetch' \
         AND (b.home_verdict = 'blocked' OR b.urls_blocked > 0) ORDER BY b.bot))",
    )
    .explain(
        "robots.txt turns away at least one AI agent that fetches pages because a person asked an assistant to. The person gets no answer from your site, or an answer built from other sources.",
        "Allow the user-fetch agents the evidence names in robots.txt, or keep the block on purpose and accept that assistants cannot open these pages for people.",
    ),
    rule(
        "agents.probe_blocked",
        "AI agents blocked when tested",
        A,
        High,
        Scope::Known,
        site!(
            "EXISTS (SELECT 1 FROM aeo_probe p WHERE ai_bot_purpose(p.bot) = 'user_fetch' \
             AND p.outcome IN ('blocked', 'challenge'))"
        ),
    )
    .site_wide()
    .needs(Need::AiChecks)
    .evidence(
        "(SELECT group_concat(n, ', ') FROM (SELECT DISTINCT ai_bot_name(p.bot) || CASE p.outcome \
         WHEN 'challenge' THEN ' was challenged' ELSE ' was blocked' END AS n FROM aeo_probe p \
         WHERE ai_bot_purpose(p.bot) = 'user_fetch' AND p.outcome IN ('blocked', 'challenge') ORDER BY p.bot))",
    )
    .explain(
        "When SpiderLens fetched pages with the user agent of an AI agent that acts for a person, the site refused it or answered with a challenge page. Bot protection is turning these agents away whatever robots.txt says.",
        "Let the agents the evidence names through your firewall or bot protection, using the vendor's published IP ranges, or serve them the page without a challenge.",
    ),
    rule(
        "agents.content_rendered_only",
        "Content only after JavaScript runs",
        A,
        High,
        Scope::Pages,
        concat!(
            "EXISTS (SELECT 1 FROM render_comparisons c WHERE c.url_id = u.id \
             AND (c.dependence > ",
            hidden_share!(),
            " OR c.title_diff = 'added' OR c.h1_diff = 'added'))"
        ),
    )
    .needs(Need::Rendering)
    .evidence(concat!(
        "CASE WHEN COALESCE(",
        cmp!("dependence"),
        ", 0) > ",
        hidden_share!(),
        " THEN CAST(ROUND(",
        cmp!("dependence"),
        ") AS INTEGER) || '% of the text exists only after rendering' WHEN ",
        cmp!("title_diff"),
        " = 'added' THEN 'The title exists only after rendering' \
         ELSE 'The main heading exists only after rendering' END"
    ))
    .explain(
        "Most agents and AI crawlers read the HTML as sent and never run scripts. The page's text, or its title or main heading, is missing from that HTML, so they see an empty page.",
        "Render the content on the server, or send it in the first HTML, and leave scripts to enhance it.",
    ),
    rule(
        "agents.nav_rendered_only",
        "Links only after JavaScript runs",
        A,
        Medium,
        Scope::Pages,
        concat!(
            "EXISTS (SELECT 1 FROM render_comparisons c WHERE c.url_id = u.id \
             AND c.dep_internal_links * 100 > ",
            hidden_share!(),
            ")"
        ),
    )
    .needs(Need::Rendering)
    .evidence(concat!(
        "CAST(ROUND(",
        cmp!("dep_internal_links"),
        " * 100) AS INTEGER) || '% of the internal links exist only after rendering'"
    ))
    .explain(
        "An agent that does not run scripts finds few of this page's links in the HTML, so it cannot follow them to the rest of the site.",
        "Put the navigation and content links in the server-sent HTML as ordinary <a href> links.",
    ),
    rule(
        "agents.bot_challenge_page",
        "Page shows a bot challenge",
        A,
        High,
        Scope::Pages,
        "EXISTS (SELECT 1 FROM renders r WHERE r.url_id = u.id AND r.bot_challenge = 1)",
    )
    .needs(Need::Rendering)
    .evidence("'The rendered page looks like a bot check, not the page'")
    .explain(
        "When a browser opened the page it was shown a bot check or an access wall instead of the content. Agents that act for people are met by the same page.",
        "Let known assistants through your bot protection, or serve this page without a challenge.",
    ),
];
