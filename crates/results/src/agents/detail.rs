//! One page's agent findings: what the detail pane, `spiderlens agents
//! page` and the MCP `page_agent_findings` tool show (spec 016 FR-019).

use rusqlite::{OptionalExtension, params};
use serde::Serialize;
use ts_rs::TS;

use crate::reader::Reader;
use crate::tabs::Need;
use crate::{Error, Result};

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct AgentField {
    pub name: Option<String>,
    pub kind: String,
    pub required: bool,
    pub accessible_name: Option<String>,
    /// aria_labelledby, aria_label, label, title, placeholder or none.
    pub name_source: String,
    pub description: Option<String>,
    pub tool_param_description: Option<String>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct AgentForm {
    /// `raw` or `rendered`.
    pub source: String,
    pub method: String,
    pub action: Option<String>,
    pub is_search: bool,
    pub has_submit: bool,
    pub challenge: Option<String>,
    pub tool_name: Option<String>,
    pub tool_description: Option<String>,
    pub tool_autosubmit: Option<bool>,
    pub fields: Vec<AgentField>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct AgentAction {
    pub source: String,
    pub kind: String,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct AgentLinkRow {
    pub source: String,
    pub kind: String,
    pub href: String,
    /// `html` or `header`.
    pub via: String,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct PageAgentFindings {
    #[ts(type = "number")]
    pub id: i64,
    pub url: String,
    /// False when the crawl did not collect agent data (older file), with
    /// the reason in `unavailable_reason`.
    pub available: bool,
    pub unavailable_reason: Option<String>,
    /// False for pages that are not internal HTML pages fetched with 200.
    pub checked: bool,
    pub forms: Vec<AgentForm>,
    /// Fields outside any form.
    pub loose_fields: Vec<AgentField>,
    pub unlabelled_fields: u32,
    pub placeholder_only_fields: u32,
    pub nameless_buttons: u32,
    /// At most 50; `actions_total` counts all.
    pub actions: Vec<AgentAction>,
    pub actions_total: u32,
    pub links: Vec<AgentLinkRow>,
}

impl Reader {
    /// Agent findings for one URL (spec 016 FR-019).
    pub fn agent_findings(&mut self, id: i64) -> Result<PageAgentFindings> {
        let url: String = self
            .conn
            .query_row("SELECT url FROM urls WHERE id = ?1", [id], |r| r.get(0))
            .optional()?
            .ok_or_else(|| Error::invalid(format!("no URL with id {id} in this crawl")))?;
        let mut out = PageAgentFindings {
            id,
            url,
            available: self.features.has(Need::Agents),
            unavailable_reason: None,
            checked: false,
            forms: Vec::new(),
            loose_fields: Vec::new(),
            unlabelled_fields: 0,
            placeholder_only_fields: 0,
            nameless_buttons: 0,
            actions: Vec::new(),
            actions_total: 0,
            links: Vec::new(),
        };
        if !out.available {
            out.unavailable_reason = Some(Need::Agents.unavailable_reason().to_string());
            return Ok(out);
        }
        let summary = self
            .conn
            .query_row(
                "SELECT unlabelled_fields, placeholder_only_fields, nameless_buttons, actions_without_link
                 FROM page_agent WHERE url_id = ?1 AND source = 'raw'",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .optional()?;
        let Some((unlabelled, placeholder, nameless, actions_total)) = summary else {
            return Ok(out);
        };
        out.checked = true;
        out.unlabelled_fields = unlabelled;
        out.placeholder_only_fields = placeholder;
        out.nameless_buttons = nameless;
        out.actions_total = actions_total;

        let mut fields: Vec<(String, Option<i64>, AgentField)> = self
            .conn
            .prepare_cached(
                "SELECT source, form_pos, name, kind, required, accessible_name, name_source, description,
                 tool_param_description FROM form_fields WHERE url_id = ?1 ORDER BY source, pos",
            )?
            .query_map([id], |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    AgentField {
                        name: r.get(2)?,
                        kind: r.get(3)?,
                        required: r.get(4)?,
                        accessible_name: r.get(5)?,
                        name_source: r.get(6)?,
                        description: r.get(7)?,
                        tool_param_description: r.get(8)?,
                    },
                ))
            })?
            .collect::<rusqlite::Result<_>>()?;
        let forms: Vec<(i64, AgentForm)> = self
            .conn
            .prepare_cached(
                "SELECT pos, source, method, action, is_search, has_submit, challenge, tool_name,
                 tool_description, tool_autosubmit FROM page_forms WHERE url_id = ?1 ORDER BY source, pos",
            )?
            .query_map([id], |r| {
                Ok((
                    r.get(0)?,
                    AgentForm {
                        source: r.get(1)?,
                        method: r.get(2)?,
                        action: r.get(3)?,
                        is_search: r.get(4)?,
                        has_submit: r.get(5)?,
                        challenge: r.get(6)?,
                        tool_name: r.get(7)?,
                        tool_description: r.get(8)?,
                        tool_autosubmit: r.get(9)?,
                        fields: Vec::new(),
                    },
                ))
            })?
            .collect::<rusqlite::Result<_>>()?;
        for (pos, mut form) in forms {
            let (mine, rest): (Vec<_>, Vec<_>) = fields
                .into_iter()
                .partition(|(src, fp, _)| *src == form.source && *fp == Some(pos));
            form.fields = mine.into_iter().map(|(_, _, f)| f).collect();
            fields = rest;
            out.forms.push(form);
        }
        out.loose_fields = fields
            .into_iter()
            .filter(|(src, fp, _)| src == "raw" && fp.is_none())
            .map(|(_, _, f)| f)
            .collect();
        out.actions = self
            .conn
            .prepare_cached(
                "SELECT source, kind, text FROM page_actions WHERE url_id = ?1 ORDER BY source, pos",
            )?
            .query_map(params![id], |r| {
                Ok(AgentAction {
                    source: r.get(0)?,
                    kind: r.get(1)?,
                    text: r.get(2)?,
                })
            })?
            .collect::<rusqlite::Result<_>>()?;
        out.links = self
            .conn
            .prepare_cached(
                "SELECT source, kind, href, via FROM agent_links WHERE url_id = ?1 ORDER BY source, pos",
            )?
            .query_map(params![id], |r| {
                Ok(AgentLinkRow {
                    source: r.get(0)?,
                    kind: r.get(1)?,
                    href: r.get(2)?,
                    via: r.get(3)?,
                })
            })?
            .collect::<rusqlite::Result<_>>()?;
        Ok(out)
    }
}
