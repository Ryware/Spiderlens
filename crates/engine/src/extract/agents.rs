//! What an AI agent needs to act on a page (spec 016 FR-001 to FR-004):
//! forms and their fields with accessible names, controls that act without
//! being links, WebMCP declarative tools, challenge widgets, and the links
//! that point agents at machine-readable versions and service descriptions.
//!
//! Collected by the single walk (spec 016 plan D1): the walker calls
//! [`Collector`] for the few elements that matter and [`Collector::finish`]
//! resolves references by id (`<label for>`, `aria-labelledby`, `form=`)
//! once at the end.

use std::collections::HashMap;

use scraper::node::Element;
use scraper::{Html, Node};
use serde::Serialize;

use super::RefSource;
use super::text::collapse;

/// Actions without a link kept per page; the total is always counted.
pub const MAX_ACTIONS: usize = 50;
/// Longest text kept for a name, label, description or action.
const MAX_TEXT: usize = 300;

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct AgentData {
    pub forms: Vec<Form>,
    /// Fields in document order; `form` points into `forms`.
    pub fields: Vec<Field>,
    /// At most [`MAX_ACTIONS`].
    pub actions: Vec<Action>,
    pub actions_total: u32,
    /// Buttons (native, button inputs and `role=button`) without an
    /// accessible name.
    pub nameless_buttons: u32,
    pub links: Vec<AgentLink>,
    /// Challenge widgets found on the page, by widget id.
    pub challenges: Vec<&'static str>,
}

impl AgentData {
    pub fn unlabelled_fields(&self) -> u32 {
        self.fields
            .iter()
            .filter(|f| f.name_source == NameSource::None)
            .count() as u32
    }

    pub fn placeholder_only_fields(&self) -> u32 {
        self.fields
            .iter()
            .filter(|f| f.name_source == NameSource::Placeholder)
            .count() as u32
    }

    pub fn markdown_alternate(&self) -> Option<&AgentLink> {
        self.links
            .iter()
            .find(|l| l.kind == AgentLinkKind::MarkdownAlternate)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FormMethod {
    Get,
    Post,
    Dialog,
    Other,
}

impl FormMethod {
    fn parse(v: Option<&str>) -> Self {
        match v.map(|m| m.trim().to_ascii_lowercase()).as_deref() {
            None | Some("") | Some("get") => Self::Get,
            Some("post") => Self::Post,
            Some("dialog") => Self::Dialog,
            Some(_) => Self::Other,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Get => "get",
            Self::Post => "post",
            Self::Dialog => "dialog",
            Self::Other => "other",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Form {
    pub method: FormMethod,
    /// The `action` attribute as written; None = submits to the page itself.
    pub action: Option<String>,
    /// `role=search`, a search input, or a GET form whose only text field is
    /// named like a query.
    pub is_search: bool,
    pub has_submit: bool,
    /// Challenge widget inside the form.
    pub challenge: Option<&'static str>,
    /// WebMCP declarative tool (`toolname`).
    pub tool: Option<DeclaredTool>,
    #[serde(skip)]
    id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DeclaredTool {
    pub name: String,
    pub description: Option<String>,
    pub autosubmit: bool,
}

/// Where a field's accessible name came from, best first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NameSource {
    LabelledBy,
    AriaLabel,
    Label,
    Title,
    /// Only a placeholder: disappears once the field has a value.
    Placeholder,
    None,
}

impl NameSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::LabelledBy => "aria_labelledby",
            Self::AriaLabel => "aria_label",
            Self::Label => "label",
            Self::Title => "title",
            Self::Placeholder => "placeholder",
            Self::None => "none",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Field {
    /// Index into [`AgentData::forms`]; None = not in a form.
    pub form: Option<u16>,
    pub name: Option<String>,
    /// Input type, `select` or `textarea`.
    pub kind: String,
    pub required: bool,
    pub accessible_name: Option<String>,
    pub name_source: NameSource,
    pub description: Option<String>,
    /// WebMCP `toolparamdescription`.
    pub tool_param_description: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionKind {
    /// `<a>` without `href`.
    AnchorNoHref,
    /// `href=""` or `href="#…"` used as a button.
    AnchorHash,
    /// `href="javascript:…"`.
    AnchorJs,
    /// A non-interactive element with `onclick`.
    Onclick,
    /// `role=button` on a non-button element.
    RoleButton,
    /// `role=link` on a non-link element.
    RoleLink,
}

impl ActionKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::AnchorNoHref => "anchor_no_href",
            Self::AnchorHash => "anchor_hash",
            Self::AnchorJs => "anchor_js",
            Self::Onclick => "onclick",
            Self::RoleButton => "role_button",
            Self::RoleLink => "role_link",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Action {
    pub kind: ActionKind,
    pub text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentLinkKind {
    MarkdownAlternate,
    ServiceDesc,
    ServiceDoc,
    ApiCatalog,
}

impl AgentLinkKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::MarkdownAlternate => "markdown_alternate",
            Self::ServiceDesc => "service_desc",
            Self::ServiceDoc => "service_doc",
            Self::ApiCatalog => "api_catalog",
        }
    }

    /// The agent link a `rel` (and `type`) names, if any.
    pub fn from_rel(rel: Option<&str>, mime: Option<&str>) -> Option<Self> {
        let rel = rel?;
        let has = |t: &str| rel.split_ascii_whitespace().any(|r| r.eq_ignore_ascii_case(t));
        if has("alternate")
            && mime.is_some_and(|m| m.trim().to_ascii_lowercase().starts_with("text/markdown"))
        {
            Some(Self::MarkdownAlternate)
        } else if has("service-desc") {
            Some(Self::ServiceDesc)
        } else if has("service-doc") {
            Some(Self::ServiceDoc)
        } else if has("api-catalog") {
            Some(Self::ApiCatalog)
        } else {
            None
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AgentLink {
    pub kind: AgentLinkKind,
    /// As written.
    pub href: String,
    pub source: RefSource,
}

/// Known CAPTCHA and bot challenge widgets (spec 016 FR-003).
struct Widget {
    id: &'static str,
    script_hosts: &'static [&'static str],
    classes: &'static [&'static str],
}

/// Versioned with the app. Script hosts match the `src` host or any parent
/// path segment of it; classes match one class of an element.
const WIDGETS: &[Widget] = &[
    Widget {
        id: "recaptcha",
        script_hosts: &[
            "www.google.com/recaptcha",
            "www.gstatic.com/recaptcha",
            "www.recaptcha.net",
        ],
        classes: &["g-recaptcha"],
    },
    Widget {
        id: "hcaptcha",
        script_hosts: &["js.hcaptcha.com", "hcaptcha.com"],
        classes: &["h-captcha"],
    },
    Widget {
        id: "turnstile",
        script_hosts: &["challenges.cloudflare.com"],
        classes: &["cf-turnstile"],
    },
    Widget {
        id: "friendly_captcha",
        script_hosts: &[
            "cdn.jsdelivr.net/npm/friendly-challenge",
            "cdn.jsdelivr.net/npm/@friendlycaptcha",
        ],
        classes: &["frc-captcha"],
    },
    Widget {
        id: "geetest",
        script_hosts: &["static.geetest.com", "static.geevisit.com"],
        classes: &["geetest_captcha"],
    },
    Widget {
        id: "arkose",
        script_hosts: &["client-api.arkoselabs.com", "api.funcaptcha.com"],
        classes: &[],
    },
    Widget {
        id: "aws_waf_captcha",
        script_hosts: &[".captcha.awswaf.com", ".token.awswaf.com"],
        classes: &[],
    },
];

pub const WIDGETS_VERSION: u32 = 1;

fn widget_for_script(src: &str) -> Option<&'static str> {
    let s = src.trim();
    let rest = s
        .strip_prefix("https://")
        .or_else(|| s.strip_prefix("http://"))
        .or_else(|| s.strip_prefix("//"))?;
    let rest = rest.to_ascii_lowercase();
    WIDGETS.iter().find_map(|w| {
        w.script_hosts
            .iter()
            .any(|h| {
                if let Some(suffix) = h.strip_prefix('.') {
                    let host = rest.split(['/', '?', '#']).next().unwrap_or("");
                    host.ends_with(suffix) && host.len() > suffix.len()
                } else {
                    rest.starts_with(h)
                }
            })
            .then_some(w.id)
    })
}

fn widget_for_class(class: &str) -> Option<&'static str> {
    class.split_ascii_whitespace().find_map(|c| {
        WIDGETS
            .iter()
            .find(|w| w.classes.iter().any(|k| k.eq_ignore_ascii_case(c)))
            .map(|w| w.id)
    })
}

/// Agent context carried down the walk with the walker's own context.
#[derive(Debug, Clone, Copy, Default)]
pub(super) struct AgentCtx {
    form: Option<u16>,
    label: Option<u32>,
    search: bool,
    /// Inside a native link or button: nested click handlers are part of it.
    in_control: bool,
}

struct Label {
    for_id: Option<String>,
    text: String,
}

struct FieldRefs {
    id: Option<String>,
    form_attr: Option<String>,
    wrapped_label: Option<u32>,
    labelledby: Option<String>,
    aria_label: Option<String>,
    title: Option<String>,
    placeholder: Option<String>,
    describedby: Option<String>,
}

/// A button whose name needs `aria-labelledby` resolved at the end.
struct PendingButton {
    labelledby: String,
}

#[derive(Default)]
pub(super) struct Collector {
    data: AgentData,
    labels: Vec<Label>,
    refs: Vec<FieldRefs>,
    pending_buttons: Vec<PendingButton>,
}

fn attr<'e>(e: &'e Element, name: &str) -> Option<&'e str> {
    e.attr(name).map(str::trim).filter(|v| !v.is_empty())
}

fn clip(s: String) -> String {
    if s.len() <= MAX_TEXT {
        return s;
    }
    let mut end = MAX_TEXT;
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    s[..end].to_string()
}

fn text_of(s: &str) -> Option<String> {
    let t = collapse(s);
    (!t.is_empty()).then(|| clip(t))
}

/// Visible text of a subtree plus the `alt` of its images: what a button or
/// label shows. Skips scripts, styles and templates.
fn subtree_name(node: ego_tree::NodeRef<'_, Node>) -> String {
    let mut s = String::new();
    let mut stack: Vec<_> = node.children().collect();
    stack.reverse();
    while let Some(n) = stack.pop() {
        match n.value() {
            Node::Text(t) => s.push_str(t),
            Node::Element(e) => {
                let name = e.name.local.as_ref();
                if matches!(name, "script" | "style" | "template" | "select" | "textarea") {
                    continue;
                }
                if name == "img"
                    && let Some(alt) = e.attr("alt")
                {
                    s.push(' ');
                    s.push_str(alt);
                    s.push(' ');
                }
                if let Some(label) = attr(e, "aria-label") {
                    s.push(' ');
                    s.push_str(label);
                    s.push(' ');
                    continue;
                }
                let mut children: Vec<_> = n.children().collect();
                children.reverse();
                stack.extend(children);
            }
            _ => {}
        }
        if s.len() > MAX_TEXT * 4 {
            break;
        }
    }
    collapse(&s)
}

const QUERY_NAMES: &[&str] = &["q", "s", "query", "search", "keyword", "keywords", "term", "k"];

impl Collector {
    /// Called by the walker for every HTML element, before its children.
    /// Returns the context for the children.
    pub(super) fn enter(
        &mut self,
        node: ego_tree::NodeRef<'_, Node>,
        e: &Element,
        mut ctx: AgentCtx,
    ) -> AgentCtx {
        let name = e.name.local.as_ref();
        if attr(e, "role").is_some_and(|r| r.eq_ignore_ascii_case("search")) {
            ctx.search = true;
        }
        if let Some(class) = e.attr("class")
            && let Some(w) = widget_for_class(class)
        {
            self.challenge(w, ctx);
        }
        match name {
            "form" => {
                let tool = attr(e, "toolname").map(|n| DeclaredTool {
                    name: clip(n.to_string()),
                    description: attr(e, "tooldescription").and_then(text_of),
                    autosubmit: e.attr("toolautosubmit").is_some(),
                });
                self.data.forms.push(Form {
                    method: FormMethod::parse(e.attr("method")),
                    action: e.attr("action").map(|a| a.trim().to_string()),
                    is_search: ctx.search,
                    has_submit: false,
                    challenge: None,
                    tool,
                    id: attr(e, "id").map(str::to_string),
                });
                if self.data.forms.len() <= usize::from(u16::MAX) {
                    ctx.form = Some((self.data.forms.len() - 1) as u16);
                }
            }
            "label" => {
                self.labels.push(Label {
                    for_id: attr(e, "for").map(str::to_string),
                    text: subtree_name(node),
                });
                ctx.label = Some((self.labels.len() - 1) as u32);
            }
            "input" => self.input(e, ctx),
            "select" | "textarea" => self.field(e, name.to_string(), ctx),
            "button" => {
                let kind = attr(e, "type").map(str::to_ascii_lowercase);
                if matches!(kind.as_deref(), None | Some("submit")) {
                    self.mark_submit(e, ctx);
                }
                self.button(e, Some(node), ctx);
                ctx.in_control = true;
            }
            "a" | "area" => {
                match e.attr("href").map(str::trim) {
                    None if name == "a" && !ctx.in_control => {
                        // A bare anchor is a placeholder unless something
                        // makes it act.
                        if e.attr("onclick").is_some()
                            || attr(e, "role").is_some()
                            || attr(e, "tabindex").is_some()
                        {
                            self.action(ActionKind::AnchorNoHref, node);
                        }
                    }
                    Some(h) if !ctx.in_control => {
                        let lower = h.to_ascii_lowercase();
                        if lower.starts_with("javascript:") {
                            self.action(ActionKind::AnchorJs, node);
                        } else if h.is_empty()
                            || h.starts_with('#') && e.attr("onclick").is_some()
                            || h == "#"
                        {
                            self.action(ActionKind::AnchorHash, node);
                        }
                    }
                    _ => {}
                }
                ctx.in_control = true;
            }
            "link" => {
                if let Some(href) = e.attr("href")
                    && let Some(kind) = AgentLinkKind::from_rel(e.attr("rel"), e.attr("type"))
                {
                    self.data.links.push(AgentLink {
                        kind,
                        href: href.trim().to_string(),
                        source: RefSource::Html,
                    });
                }
            }
            "script" => {
                if let Some(src) = e.attr("src")
                    && let Some(w) = widget_for_script(src)
                {
                    self.challenge(w, ctx);
                }
            }
            _ => {
                if !ctx.in_control {
                    let role = attr(e, "role").map(str::to_ascii_lowercase);
                    match role.as_deref() {
                        Some("button") => {
                            self.action(ActionKind::RoleButton, node);
                            self.button(e, Some(node), ctx);
                            ctx.in_control = true;
                        }
                        Some("link") => {
                            self.action(ActionKind::RoleLink, node);
                            ctx.in_control = true;
                        }
                        _ if e.attr("onclick").is_some() && !matches!(name, "body" | "html") => {
                            self.action(ActionKind::Onclick, node);
                            ctx.in_control = true;
                        }
                        _ => {}
                    }
                }
            }
        }
        ctx
    }

    fn challenge(&mut self, w: &'static str, ctx: AgentCtx) {
        if !self.data.challenges.contains(&w) {
            self.data.challenges.push(w);
        }
        // Inside a form, or right after one (widgets are often siblings of
        // the submit row rather than children of the form).
        let target = ctx
            .form
            .map(usize::from)
            .or_else(|| self.data.forms.len().checked_sub(1));
        if let Some(i) = target
            && let Some(f) = self.data.forms.get_mut(i)
            && f.challenge.is_none()
        {
            f.challenge = Some(w);
        }
    }

    fn action(&mut self, kind: ActionKind, node: ego_tree::NodeRef<'_, Node>) {
        self.data.actions_total += 1;
        if self.data.actions.len() < MAX_ACTIONS {
            self.data.actions.push(Action {
                kind,
                text: clip(subtree_name(node)),
            });
        }
    }

    fn mark_submit(&mut self, e: &Element, ctx: AgentCtx) {
        let form = ctx.form.map(usize::from);
        let form = form.or_else(|| {
            let id = attr(e, "form")?;
            self.data.forms.iter().position(|f| f.id.as_deref() == Some(id))
        });
        if let Some(i) = form {
            self.data.forms[i].has_submit = true;
        }
    }

    /// Counts a nameless button, or defers the decision to `finish` when the
    /// name comes from `aria-labelledby`.
    fn button(&mut self, e: &Element, node: Option<ego_tree::NodeRef<'_, Node>>, _ctx: AgentCtx) {
        if attr(e, "aria-label").is_some() || attr(e, "title").is_some() {
            return;
        }
        if let Some(ids) = attr(e, "aria-labelledby") {
            self.pending_buttons.push(PendingButton {
                labelledby: ids.to_string(),
            });
            return;
        }
        let shown = node.map(subtree_name).unwrap_or_default();
        let value = attr(e, "value").unwrap_or("");
        if shown.is_empty() && value.is_empty() {
            self.data.nameless_buttons += 1;
        }
    }

    fn input(&mut self, e: &Element, ctx: AgentCtx) {
        let kind = attr(e, "type").map_or_else(|| "text".to_string(), str::to_ascii_lowercase);
        match kind.as_str() {
            "hidden" => {}
            "submit" => {
                self.mark_submit(e, ctx);
                // A submit input without a value is named "Submit" by browsers.
            }
            "image" => {
                self.mark_submit(e, ctx);
                if attr(e, "alt").is_none() && attr(e, "aria-label").is_none() && attr(e, "title").is_none() {
                    self.data.nameless_buttons += 1;
                }
            }
            "reset" | "button" => self.button(e, None, ctx),
            _ => {
                if kind == "search"
                    && let Some(i) = ctx.form
                {
                    self.data.forms[usize::from(i)].is_search = true;
                }
                self.field(e, kind, ctx);
            }
        }
    }

    fn field(&mut self, e: &Element, kind: String, ctx: AgentCtx) {
        self.data.fields.push(Field {
            form: ctx.form,
            name: attr(e, "name").map(|n| clip(n.to_string())),
            kind,
            required: e.attr("required").is_some()
                || attr(e, "aria-required").is_some_and(|v| v.eq_ignore_ascii_case("true")),
            accessible_name: None,
            name_source: NameSource::None,
            description: attr(e, "aria-description").and_then(text_of),
            tool_param_description: attr(e, "toolparamdescription").and_then(text_of),
        });
        self.refs.push(FieldRefs {
            id: attr(e, "id").map(str::to_string),
            form_attr: attr(e, "form").map(str::to_string),
            wrapped_label: ctx.label,
            labelledby: attr(e, "aria-labelledby").map(str::to_string),
            aria_label: attr(e, "aria-label").and_then(text_of),
            title: attr(e, "title").and_then(text_of),
            placeholder: attr(e, "placeholder").and_then(text_of),
            describedby: attr(e, "aria-describedby").map(str::to_string),
        });
    }

    /// Resolves names by id and returns the page's agent data.
    pub(super) fn finish(mut self, doc: &Html) -> AgentData {
        let needs_ids = self
            .refs
            .iter()
            .any(|r| r.labelledby.is_some() || r.describedby.is_some())
            || !self.pending_buttons.is_empty();
        let by_id = if needs_ids {
            text_by_id(doc)
        } else {
            HashMap::new()
        };
        let lookup = |ids: &str| -> Option<String> {
            let parts: Vec<&str> = ids
                .split_ascii_whitespace()
                .filter_map(|id| by_id.get(id).map(String::as_str))
                .filter(|t| !t.is_empty())
                .collect();
            text_of(&parts.join(" "))
        };

        let mut for_labels: HashMap<&str, &str> = HashMap::new();
        for l in &self.labels {
            if let Some(id) = &l.for_id {
                for_labels.entry(id.as_str()).or_insert(l.text.as_str());
            }
        }
        let form_ids: HashMap<&str, u16> = self
            .data
            .forms
            .iter()
            .enumerate()
            .filter_map(|(i, f)| Some((f.id.as_deref()?, i as u16)))
            .collect();

        for (field, r) in self.data.fields.iter_mut().zip(&self.refs) {
            if let Some(fid) = &r.form_attr
                && let Some(&i) = form_ids.get(fid.as_str())
            {
                field.form = Some(i);
            }
            let (name, source) = if let Some(n) = r.labelledby.as_deref().and_then(&lookup) {
                (Some(n), NameSource::LabelledBy)
            } else if let Some(n) = r.aria_label.clone() {
                (Some(n), NameSource::AriaLabel)
            } else if let Some(n) =
                r.id.as_deref()
                    .and_then(|id| for_labels.get(id))
                    .and_then(|t| text_of(t))
                    .or_else(|| {
                        r.wrapped_label
                            .and_then(|i| self.labels.get(i as usize))
                            .and_then(|l| text_of(&l.text))
                    })
            {
                (Some(n), NameSource::Label)
            } else if let Some(n) = r.title.clone() {
                (Some(n), NameSource::Title)
            } else if let Some(n) = r.placeholder.clone() {
                (Some(n), NameSource::Placeholder)
            } else {
                (None, NameSource::None)
            };
            field.accessible_name = name;
            field.name_source = source;
            if field.description.is_none() {
                field.description = r.describedby.as_deref().and_then(&lookup);
            }
        }

        for b in &self.pending_buttons {
            if lookup(&b.labelledby).is_none() {
                self.data.nameless_buttons += 1;
            }
        }

        // A GET form with one text-like field named like a query is a search
        // form even without role=search.
        for (i, form) in self.data.forms.iter_mut().enumerate() {
            if form.is_search || form.method != FormMethod::Get {
                continue;
            }
            let mut texts = self
                .data
                .fields
                .iter()
                .filter(|f| f.form == Some(i as u16) && matches!(f.kind.as_str(), "text" | "search"));
            if let (Some(only), None) = (texts.next(), texts.next())
                && only
                    .name
                    .as_deref()
                    .is_some_and(|n| QUERY_NAMES.iter().any(|q| q.eq_ignore_ascii_case(n)))
            {
                form.is_search = true;
            }
        }
        self.data
    }
}

/// Text of every element with an id, for `aria-labelledby` and
/// `aria-describedby`. Only built when a page uses them.
fn text_by_id(doc: &Html) -> HashMap<String, String> {
    let mut out = HashMap::new();
    for node in doc.tree.root().descendants() {
        if let Node::Element(e) = node.value()
            && let Some(id) = e.attr("id")
        {
            let text = attr(e, "aria-label").map_or_else(|| subtree_name(node), str::to_string);
            out.entry(id.to_string()).or_insert(text);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn widget_scripts() {
        assert_eq!(
            widget_for_script("https://www.google.com/recaptcha/api.js"),
            Some("recaptcha")
        );
        assert_eq!(
            widget_for_script("https://challenges.cloudflare.com/turnstile/v0/api.js"),
            Some("turnstile")
        );
        assert_eq!(
            widget_for_script("https://abc123.edge.captcha.awswaf.com/x/jsapi.js"),
            Some("aws_waf_captcha")
        );
        assert_eq!(
            widget_for_script("https://captcha.awswaf.com.evil.test/x.js"),
            None
        );
        assert_eq!(widget_for_script("/local/recaptcha.js"), None);
        assert_eq!(widget_for_class("btn g-recaptcha"), Some("recaptcha"));
        assert_eq!(widget_for_class("captcha"), None);
    }

    #[test]
    fn rels() {
        use AgentLinkKind::*;
        assert_eq!(
            AgentLinkKind::from_rel(Some("alternate"), Some("text/markdown")),
            Some(MarkdownAlternate)
        );
        assert_eq!(
            AgentLinkKind::from_rel(Some("alternate"), Some("text/html")),
            None
        );
        assert_eq!(
            AgentLinkKind::from_rel(Some("service-desc"), None),
            Some(ServiceDesc)
        );
        assert_eq!(
            AgentLinkKind::from_rel(Some("API-Catalog"), None),
            Some(ApiCatalog)
        );
        assert_eq!(AgentLinkKind::from_rel(None, None), None);
    }
}
