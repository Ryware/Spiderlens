# Data Model: AI Agents (spec 016)

All tables are added by one additive migration step (plan D3). Columns that may be missing in some rows are nullable. `source` is `raw` or `rendered` (spec 014); a crawl without rendering has only `raw` rows.

## Extraction output (in memory, `extract::AgentData`)

```rust
pub struct AgentData {
    pub forms: Vec<Form>,          // in document order
    pub fields: Vec<Field>,        // flat, in document order; Field.form points at its form
    pub actions: Vec<Action>,      // capped at 50, plus actions_total
    pub actions_total: u32,
    pub nameless_buttons: u32,     // native buttons and button roles without an accessible name
    pub links: Vec<AgentLink>,     // markdown alternates, service-desc/doc, api-catalog
    pub challenges: Vec<&'static str>, // widget ids found in the page
}
pub struct Form {
    pub method: FormMethod /* get|post|dialog|other */, pub action: Option<String>,
    pub is_search: bool, // role=search, type=search input or GET form with a query-like field
    pub has_submit: bool,
    pub challenge: Option<&'static str>,
    pub tool: Option<DeclaredTool>,  // WebMCP declarative
}
pub struct Field {
    pub form: Option<u16>,          // index into forms (form ancestor or form= attribute); None = outside any form
    pub name: Option<String>, pub kind: String /* input type, select, textarea */, pub required: bool,
    pub accessible_name: Option<String>, pub name_source: NameSource /* aria_labelledby|aria_label|label|title|placeholder|none */,
    pub description: Option<String>, pub tool_param_description: Option<String>,
}
pub struct DeclaredTool { pub name: String, pub description: Option<String>, pub autosubmit: bool }
pub struct Action { pub kind: ActionKind /* anchor_no_href|anchor_hash|anchor_js|onclick|role_button|role_link */, pub text: String }
pub struct AgentLink { pub kind: AgentLinkKind /* markdown_alternate|service_desc|service_doc|api_catalog */, pub href: String, pub source: RefSource }
```

Link response headers (`Link:`) with the same relations are collected from the response and stored with `via = header`.

Hidden inputs, `type=submit|reset|button|image` and fields inside `<template>` are not fields for labelling purposes (submit and image inputs count as submit controls and need an accessible name).

## Tables

### `page_agent` (summary, one row per page and source)

| column | type | notes |
|---|---|---|
| url_id | INTEGER | FK urls.id |
| source | TEXT | raw, rendered |
| forms | INTEGER | count |
| search_forms | INTEGER | |
| fields | INTEGER | labellable fields |
| unlabelled_fields | INTEGER | name_source none |
| placeholder_only_fields | INTEGER | name only from placeholder |
| nameless_buttons | INTEGER | |
| actions_without_link | INTEGER | total, not capped |
| challenges | INTEGER | forms with a challenge widget |
| forms_without_submit | INTEGER | forms with no submit control |
| declared_tools | INTEGER | WebMCP declarative |
| tools_missing_description | INTEGER | tool or any param undescribed |
| md_alternate | TEXT | first markdown alternate URL, resolved |
| md_status | INTEGER | from the host-file queue, NULL until verified |
| md_outcome | TEXT | ok, wrong_type, error, not_verified |

Primary key `(url_id, source)`. Tab columns read the `raw` row and, when present, the `rendered` row, by primary key.

### `page_forms`

`url_id, source, pos, method, action, is_search, has_submit, field_count, challenge, tool_name, tool_description, tool_autosubmit`. Primary key `(url_id, source, pos)`.

### `form_fields`

`url_id, source, pos, form_pos, name, kind, required, accessible_name, name_source, description, tool_param_description`. `pos` is the field's place in the page; `form_pos` is the form's `pos`, NULL for fields outside any form. Primary key `(url_id, source, pos)`.

### `page_actions`

`url_id, source, pos, kind, text`. At most 50 rows per page and source. Primary key `(url_id, source, pos)`.

### `agent_links`

`url_id, source, pos, kind, href, via`. `kind` is markdown_alternate, service_desc, service_doc or api_catalog; `via` is `html` or `header`. Primary key `(url_id, source, pos)`. The first markdown alternate is also copied to `page_agent.md_alternate`.

### `agent_entry_points` (per host)

| column | type | notes |
|---|---|---|
| id | INTEGER PK | |
| host_key | TEXT | as in `hosts` |
| kind | TEXT | ai_catalog, api_catalog, a2a_card, mcp_server_card, openapi, service_doc, markdown_negotiation |
| url | TEXT | |
| found_via | TEXT | well_known, catalog, page_link, link_header |
| found_on | INTEGER | url_id of the page that linked it, NULL for well-known |
| status | INTEGER | HTTP status, NULL if not fetched |
| outcome | TEXT | ok, absent, blocked_robots, error, too_large, wrong_type, invalid, not_fetched |
| content_type | TEXT | |
| size | INTEGER | |
| convention_version | TEXT | version the parser recognised, or `unrecognised` |
| problems | TEXT | JSON array of `{code, message, line?}` |
| summary | TEXT | JSON: name, description, endpoints (cards), entries (catalogs) |
| mcp_discovery | TEXT | JSON `{answered, protocol_versions, requires_sign_in, error}` for MCP endpoints |

Index `(host_key, kind)`. The `llms.txt` rows stay in spec 015's storage; the summary joins them in.

### `agent_scores`

`url_id` (NULL row = the crawl), `total, access, discovery, reading, acting` (REAL, NULL when the part had no checked rule), `not_checked` (INTEGER), `failed_rules` (JSON array of rule ids, for the detail pane), `weights_version` (INTEGER). Primary key on `url_id` with a unique partial index for the NULL row. Rebuilt by every evaluation pass.

### `meta`

`data_features` gains `agents` (collection ran) and `agent_scores` (evaluation ran). `agent_settings` is part of the stored crawl configuration (spec 002): `entry_points: bool` (default true), `markdown_alternates_max: u32` (default 50), `markdown_negotiation: bool` (default false), `markdown_negotiation_sample: u32` (default 10).

## Rules (category `agents`, spec 009 shape)

Ids are stable and never reused. Part and weight live in 016's score table (`agents/score.rs`, version 1), not in the rule.

| id | part | weight | priority | kind | condition (summary) | needs |
|---|---|---|---|---|---|---|
| agents.form_rendered_only | Acting | 8 | High | page | form in rendered row with no matching raw form (same action and method) | agents, rendering |
| agents.unlabelled_fields | Acting | 5 | Medium | page | unlabelled_fields > 0 | agents |
| agents.placeholder_only_fields | Acting | 2 | Low | page | placeholder_only_fields > 0 | agents |
| agents.nameless_buttons | Acting | 4 | Medium | page | nameless_buttons > 0 | agents |
| agents.actions_without_link | Acting | 4 | Medium | page | actions_without_link > 0 | agents |
| agents.form_challenge | Acting | 5 | Medium | page | challenges > 0 | agents |
| agents.form_no_submit | Acting | 3 | Low | page | forms_without_submit > 0 | agents |
| agents.tool_missing_description | Acting | 1 | Low | page | tools_missing_description > 0 | agents |
| agents.no_search_action | Acting | 1 | Opportunity | site | site has search forms but no `SearchAction` | agents, extraction |
| agents.search_action_out_of_scope | Acting | 2 | Medium | site | `SearchAction` template resolves outside scope | extraction |
| agents.content_rendered_only | Reading | 10 | High | page | 014 overall JavaScript dependence above 014's threshold, or title or H1 only rendered | rendering |
| agents.nav_rendered_only | Reading | 5 | Medium | page | 014 internal links share only after rendering above threshold | rendering |
| agents.no_markdown_alternate | Reading | 1 | Opportunity | page | no markdown alternate | agents |
| agents.markdown_alternate_broken | Reading | 2 | Medium | page | md_outcome in (error, wrong_type) | agents |
| agents.robots_blocks_agents | Access | 10 | High | site | 015 robots evaluation blocks a user-fetch agent | aeo (015) |
| agents.probe_blocked | Access | 10 | High | site | 015 probe: a user-fetch agent got blocked or challenged | aeo_probe (015) |
| agents.bot_challenge_page | Access | 8 | High | page | 014 possible bot challenge flag | rendering |
| agents.entry_point_invalid | Discovery | 4 | Medium | site | any entry point with outcome invalid or wrong_type | agents |
| agents.mcp_endpoint_unreachable | Discovery | 4 | Medium | site | MCP discovery did not answer | agents |
| agents.no_entry_points | Discovery | 2 | Opportunity | site | no entry point of any kind and no llms.txt | agents, aeo (015) |
| agents.llms_txt_problems | Discovery | 2 | Low | site | 015's llms.txt validation failed | aeo (015) |
| agents.prompt_injection_text | Reading | 0 | Low | page | page text matches injection patterns (informational, weight 0) | extraction |

Part weights in the total: Access 30, Reading 25, Acting 30, Discovery 15 (research R7).

## App settings (`assistants.json`, app config directory, not in crawl profiles)

```json
{
  "version": 1,
  "allow_read": false,
  "allowed_folders": [],
  "allow_crawl": false,
  "allowed_profiles": []
}
```

## Activity log (`assistant-activity.jsonl`, app data directory)

One line per call: `{"at": "...", "client": "claude-desktop 1.4", "tool": "query_pages", "crawl": "c_ab12", "bytes": 5120, "refused": false, "error": null}`. At most 2,000 lines.
