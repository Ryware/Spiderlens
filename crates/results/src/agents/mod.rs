//! Agent readiness on the read side (spec 016): the Agents tab and one
//! page's findings. The filter conditions here are the single definition the
//! agent rules in spec 009's catalogue reuse (009 FR-005).

mod detail;

pub use detail::{AgentAction, AgentField, AgentForm, AgentLinkRow, PageAgentFindings};

use crate::tabs::{FilterDef, HTML_OK, Need, TabDef, ValueKind, c, col, filter};

/// One summary value of the page's raw HTML (`page_agent`), by primary key.
macro_rules! raw {
    ($field:literal) => {
        concat!(
            "(SELECT a.",
            $field,
            " FROM page_agent a WHERE a.url_id = u.id AND a.source = 'raw')"
        )
    };
}

/// A page whose raw HTML has at least one of something.
macro_rules! any_raw {
    ($field:literal) => {
        concat!("COALESCE(", raw!($field), ", 0) > 0")
    };
}

/// Filter conditions shared with the agent rules.
pub mod conditions {
    pub const HAS_FORMS: &str = any_raw!("forms");
    pub const SEARCH_FORMS: &str = any_raw!("search_forms");
    pub const UNLABELLED_FIELDS: &str = any_raw!("unlabelled_fields");
    pub const PLACEHOLDER_ONLY_FIELDS: &str = any_raw!("placeholder_only_fields");
    pub const NAMELESS_BUTTONS: &str = any_raw!("nameless_buttons");
    pub const ACTIONS_WITHOUT_LINK: &str = any_raw!("actions_without_link");
    pub const FORM_CHALLENGE: &str = any_raw!("challenges");
    pub const FORMS_WITHOUT_SUBMIT: &str = any_raw!("forms_without_submit");
    pub const DECLARED_TOOLS: &str = any_raw!("declared_tools");
    pub const TOOLS_MISSING_DESCRIPTION: &str = any_raw!("tools_missing_description");
    pub const HAS_MARKDOWN: &str = concat!(raw!("md_alternate"), " IS NOT NULL");
    /// Checked pages only: a page without a `page_agent` row was not checked.
    pub const NO_MARKDOWN: &str = "EXISTS (SELECT 1 FROM page_agent a WHERE a.url_id = u.id AND a.source = 'raw' AND a.md_alternate IS NULL)";
}

fn agent_filter(id: &'static str, label: &'static str, cond: &'static str) -> FilterDef {
    filter(id, label, cond).needs(Need::Agents)
}

/// The Agents tab (spec 016 FR-017). Score columns join it once evaluation
/// lands (spec 016 plan D5).
pub(crate) fn tab() -> TabDef {
    use ValueKind::Int;
    let n = |id, label, expr, width| col(id, label, expr, Int, width).needs(Need::Agents);
    TabDef {
        id: "agents",
        title: "Agents",
        eligible: HTML_OK,
        columns: vec![
            c::ADDRESS,
            n("forms", "Forms", raw!("forms"), 80),
            n("search_forms", "Search forms", raw!("search_forms"), 110).hidden(),
            n("fields", "Form fields", raw!("fields"), 100).hidden(),
            n(
                "unlabelled_fields",
                "Unlabelled fields",
                raw!("unlabelled_fields"),
                130,
            ),
            n(
                "nameless_buttons",
                "Buttons without a name",
                raw!("nameless_buttons"),
                160,
            ),
            n(
                "actions_without_link",
                "Actions without a link",
                raw!("actions_without_link"),
                160,
            ),
            n("challenges", "Challenges", raw!("challenges"), 100),
            n(
                "declared_tools",
                "Declared agent tools",
                raw!("declared_tools"),
                150,
            ),
            col(
                "markdown",
                "Markdown version",
                raw!("md_alternate"),
                ValueKind::Url,
                260,
            )
            .needs(Need::Agents),
        ],
        filters: vec![
            filter("all", "All", "1"),
            agent_filter("has_forms", "Has forms", conditions::HAS_FORMS),
            agent_filter("search_forms", "Search forms", conditions::SEARCH_FORMS),
            agent_filter(
                "unlabelled_fields",
                "Form fields without labels",
                conditions::UNLABELLED_FIELDS,
            ),
            agent_filter(
                "placeholder_only_fields",
                "Fields named only by a placeholder",
                conditions::PLACEHOLDER_ONLY_FIELDS,
            ),
            agent_filter(
                "nameless_buttons",
                "Buttons without a name",
                conditions::NAMELESS_BUTTONS,
            ),
            agent_filter(
                "actions_without_link",
                "Actions without a link",
                conditions::ACTIONS_WITHOUT_LINK,
            ),
            agent_filter(
                "challenge",
                "Action behind a challenge",
                conditions::FORM_CHALLENGE,
            ),
            agent_filter(
                "no_submit",
                "Forms without a submit button",
                conditions::FORMS_WITHOUT_SUBMIT,
            ),
            agent_filter(
                "declared_tools",
                "Declares agent tools",
                conditions::DECLARED_TOOLS,
            ),
            agent_filter(
                "tool_missing_description",
                "Agent tools missing descriptions",
                conditions::TOOLS_MISSING_DESCRIPTION,
            ),
            agent_filter("has_markdown", "Has a Markdown version", conditions::HAS_MARKDOWN),
            agent_filter("no_markdown", "No Markdown version", conditions::NO_MARKDOWN),
        ],
        search_column: None,
        scoped: false,
    }
}
