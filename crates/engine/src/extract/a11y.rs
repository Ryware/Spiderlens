//! Static accessibility checks (spec 026, plan D1): a visitor on the single
//! spec 005 walk. It only exists when the crawl asks for it, so a crawl
//! with accessibility off pays one `bool` test per page.
//!
//! Only what the HTML alone can decide is checked here; anything that needs
//! CSS or scripts is left to the full checks in the rendering stage. The
//! four checks spec 009 already reads from the spec 005 data (missing alt,
//! missing language, skipped headings, missing title) are not repeated.

use std::collections::{HashMap, HashSet};

use ego_tree::NodeRef;
use html5ever::{local_name, ns};
use scraper::node::Element;
use scraper::{Html, Node};
use serde::Serialize;

/// Failing elements kept per check; `count` is always the full number
/// (spec 026 FR-010).
pub const NODE_CAP: usize = 20;
/// Longest stored excerpt, in characters.
pub const EXCERPT_CHARS: usize = 300;

/// One static check. Ids are the rule suffixes of spec 026 data-model.md.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum StaticCheck {
    FormLabel,
    ButtonName,
    LinkName,
    FrameTitle,
    LangInvalid,
    ElementLangInvalid,
    ZoomDisabled,
    ViewportMissing,
    ViewportWidth,
    TabindexPositive,
    AccesskeyDuplicate,
    MetaRefreshDelay,
    DeprecatedElement,
    PluginElement,
    AreaAlt,
    InputImageAlt,
}

/// User impact, as axe-core names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Impact {
    Critical,
    Serious,
    Moderate,
    Minor,
}

impl StaticCheck {
    pub const ALL: [Self; 16] = [
        Self::FormLabel,
        Self::ButtonName,
        Self::LinkName,
        Self::FrameTitle,
        Self::LangInvalid,
        Self::ElementLangInvalid,
        Self::ZoomDisabled,
        Self::ViewportMissing,
        Self::ViewportWidth,
        Self::TabindexPositive,
        Self::AccesskeyDuplicate,
        Self::MetaRefreshDelay,
        Self::DeprecatedElement,
        Self::PluginElement,
        Self::AreaAlt,
        Self::InputImageAlt,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::FormLabel => "form_label",
            Self::ButtonName => "button_name",
            Self::LinkName => "link_name",
            Self::FrameTitle => "frame_title",
            Self::LangInvalid => "lang_invalid",
            Self::ElementLangInvalid => "element_lang_invalid",
            Self::ZoomDisabled => "zoom_disabled",
            Self::ViewportMissing => "viewport_missing",
            Self::ViewportWidth => "viewport_width",
            Self::TabindexPositive => "tabindex_positive",
            Self::AccesskeyDuplicate => "accesskey_duplicate",
            Self::MetaRefreshDelay => "meta_refresh_delay",
            Self::DeprecatedElement => "deprecated_element",
            Self::PluginElement => "plugin_element",
            Self::AreaAlt => "area_alt",
            Self::InputImageAlt => "input_image_alt",
        }
    }

    /// WCAG tags in axe-core's spelling (`wcag2a wcag412`), or
    /// `best-practice`.
    pub fn wcag(self) -> &'static str {
        match self {
            Self::FormLabel => "wcag2a wcag131 wcag412",
            Self::ButtonName => "wcag2a wcag412",
            Self::LinkName => "wcag2a wcag244 wcag412",
            Self::FrameTitle => "wcag2a wcag412",
            Self::LangInvalid => "wcag2a wcag311",
            Self::ElementLangInvalid => "wcag2aa wcag312",
            Self::ZoomDisabled => "wcag2aa wcag144",
            Self::MetaRefreshDelay => "wcag2a wcag221",
            Self::DeprecatedElement => "wcag2a wcag222",
            Self::AreaAlt => "wcag2a wcag111 wcag244",
            Self::InputImageAlt => "wcag2a wcag111",
            Self::ViewportMissing
            | Self::ViewportWidth
            | Self::TabindexPositive
            | Self::AccesskeyDuplicate
            | Self::PluginElement => "best-practice",
        }
    }

    pub fn impact(self) -> Impact {
        match self {
            Self::FormLabel
            | Self::ButtonName
            | Self::ZoomDisabled
            | Self::MetaRefreshDelay
            | Self::AreaAlt
            | Self::InputImageAlt => Impact::Critical,
            Self::LinkName
            | Self::FrameTitle
            | Self::LangInvalid
            | Self::ElementLangInvalid
            | Self::TabindexPositive
            | Self::AccesskeyDuplicate
            | Self::DeprecatedElement => Impact::Serious,
            Self::ViewportMissing | Self::ViewportWidth | Self::PluginElement => Impact::Minor,
        }
    }

    /// The axe-core checks that test the same thing; when a full check ran
    /// on the page, its result is kept instead (spec 026 edge cases).
    pub fn same_as(self) -> &'static [&'static str] {
        match self {
            Self::FormLabel => &["label", "select-name"],
            Self::ButtonName => &["button-name", "input-button-name"],
            Self::LinkName => &["link-name"],
            Self::FrameTitle => &["frame-title"],
            Self::LangInvalid => &["html-lang-valid"],
            Self::ElementLangInvalid => &["valid-lang"],
            Self::ZoomDisabled => &["meta-viewport"],
            Self::TabindexPositive => &["tabindex"],
            Self::AccesskeyDuplicate => &["accesskeys"],
            Self::MetaRefreshDelay => &["meta-refresh"],
            Self::DeprecatedElement => &["blink", "marquee"],
            Self::AreaAlt => &["area-alt"],
            Self::InputImageAlt => &["input-image-alt"],
            Self::ViewportMissing | Self::ViewportWidth | Self::PluginElement => &[],
        }
    }
}

/// A failing element.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct A11yNode {
    pub selector: String,
    /// The element's start tag, at most [`EXCERPT_CHARS`] characters.
    pub excerpt: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StaticFinding {
    pub check: StaticCheck,
    /// Failing elements, before the cap.
    pub count: u32,
    /// At most [`NODE_CAP`], in document order.
    pub nodes: Vec<A11yNode>,
}

/// The first `<meta name="viewport">` of the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Viewport {
    pub content: String,
    pub device_width: bool,
    pub zoom_disabled: bool,
}

/// Everything the static checks found on one page.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct A11yStatic {
    /// One entry per failing check, ordered by check.
    pub findings: Vec<StaticFinding>,
    pub viewport: Option<Viewport>,
}

impl A11yStatic {
    pub fn finding(&self, check: StaticCheck) -> Option<&StaticFinding> {
        self.findings.iter().find(|f| f.check == check)
    }
}

/// The visitor. Lives for one walk and borrows the parsed document.
pub(super) struct A11yCollector<'d> {
    found: HashMap<StaticCheck, (u32, Vec<NodeRef<'d, Node>>)>,
    viewport: Option<Viewport>,
    viewport_node: Option<NodeRef<'d, Node>>,
    refresh_seen: bool,
    /// Ids named by `<label for>`.
    label_for: HashSet<String>,
    /// Controls whose only possible name is a `<label for>` matching their
    /// id, decided at the end.
    need_label_for: Vec<(NodeRef<'d, Node>, String)>,
    /// Elements whose name depends on `aria-labelledby` targets.
    need_labelledby: Vec<(StaticCheck, NodeRef<'d, Node>, Vec<String>)>,
    accesskeys: HashMap<String, Vec<NodeRef<'d, Node>>>,
}

impl<'d> A11yCollector<'d> {
    pub(super) fn new(doc: &'d Html) -> Self {
        let mut c = Self {
            found: HashMap::new(),
            viewport: None,
            viewport_node: None,
            refresh_seen: false,
            label_for: HashSet::new(),
            need_label_for: Vec::new(),
            need_labelledby: Vec::new(),
            accesskeys: HashMap::new(),
        };
        let root = doc.root_element();
        if let Some(lang) = root.value().attr("lang") {
            let lang = lang.trim();
            // Missing or empty is spec 009's "Missing language attribute".
            if !lang.is_empty() && !lang_ok(lang) {
                c.add(StaticCheck::LangInvalid, *root);
            }
        }
        c
    }

    fn add(&mut self, check: StaticCheck, node: NodeRef<'d, Node>) {
        let entry = self.found.entry(check).or_default();
        entry.0 += 1;
        if entry.1.len() < NODE_CAP {
            entry.1.push(node);
        }
    }

    /// Called for every node the walk enters. `noscript`: inside
    /// `<noscript>`, whose content a browser with scripts never shows.
    pub(super) fn enter(&mut self, node: NodeRef<'d, Node>, noscript: bool) {
        let Node::Element(e) = node.value() else { return };
        if e.name.ns != ns!(html) || noscript {
            return;
        }
        self.attributes(node, e);
        match e.name.local {
            local_name!("meta") => self.meta(node, e),
            local_name!("label") => {
                if let Some(id) = e.attr("for") {
                    self.label_for.insert(id.trim().to_string());
                }
            }
            local_name!("input") => self.input(node, e),
            local_name!("select") | local_name!("textarea") => self.control(node, e),
            local_name!("button") => {
                self.named(StaticCheck::ButtonName, node, e, true);
            }
            local_name!("a") => {
                if e.attr("href").is_some() {
                    self.named(StaticCheck::LinkName, node, e, true);
                }
            }
            local_name!("area") => {
                if e.attr("href").is_some() && !has(e, "alt") {
                    self.named(StaticCheck::AreaAlt, node, e, false);
                }
            }
            local_name!("iframe") | local_name!("frame") => {
                if !has(e, "title") {
                    self.named(StaticCheck::FrameTitle, node, e, false);
                }
            }
            local_name!("blink") | local_name!("marquee") => {
                self.add(StaticCheck::DeprecatedElement, node);
            }
            local_name!("object") | local_name!("embed") | local_name!("applet") => {
                self.add(StaticCheck::PluginElement, node);
            }
            _ => {}
        }
    }

    /// `lang`, `tabindex` and `accesskey`, in one pass over the attributes.
    fn attributes(&mut self, node: NodeRef<'d, Node>, e: &Element) {
        let is_root = node.parent().is_some_and(|p| matches!(p.value(), Node::Document));
        for (name, value) in e.attrs() {
            match name {
                "lang" if !is_root => {
                    let v = value.trim();
                    if !v.is_empty() && !lang_ok(v) {
                        self.add(StaticCheck::ElementLangInvalid, node);
                    }
                }
                "tabindex" => {
                    if value.trim().parse::<i64>().is_ok_and(|n| n > 0) {
                        self.add(StaticCheck::TabindexPositive, node);
                    }
                }
                "accesskey" => {
                    let v = value.trim().to_lowercase();
                    if !v.is_empty() {
                        self.accesskeys.entry(v).or_default().push(node);
                    }
                }
                _ => {}
            }
        }
    }

    fn meta(&mut self, node: NodeRef<'d, Node>, e: &Element) {
        let content = e.attr("content").unwrap_or("");
        if e.attr("name")
            .is_some_and(|n| n.trim().eq_ignore_ascii_case("viewport"))
        {
            if self.viewport.is_none() {
                let vp = parse_viewport(content);
                if vp.zoom_disabled {
                    self.add(StaticCheck::ZoomDisabled, node);
                }
                self.viewport = Some(vp);
                self.viewport_node = Some(node);
            }
        } else if !self.refresh_seen
            && e.attr("http-equiv")
                .is_some_and(|h| h.trim().eq_ignore_ascii_case("refresh"))
            && let Some(r) = super::walk::parse_refresh(content)
        {
            self.refresh_seen = true;
            // axe-core: a delay of 20 hours or more is effectively none.
            if r.delay > 0.0 && r.delay < 72_000.0 {
                self.add(StaticCheck::MetaRefreshDelay, node);
            }
        }
    }

    fn input(&mut self, node: NodeRef<'d, Node>, e: &Element) {
        let ty = e.attr("type").map(|t| t.trim().to_ascii_lowercase());
        match ty.as_deref() {
            Some("hidden") => {}
            // Browsers give these a default name ("Submit", "Reset").
            Some("submit" | "reset") => {}
            Some("button") => {
                if !has(e, "value") {
                    self.named(StaticCheck::ButtonName, node, e, false);
                }
            }
            Some("image") => {
                if !has(e, "alt") {
                    self.named(StaticCheck::InputImageAlt, node, e, false);
                }
            }
            _ => self.control(node, e),
        }
    }

    /// A form field: named by `aria-labelledby`, `aria-label`, a wrapping
    /// `<label>`, a `<label for>`, `title` or `placeholder`.
    fn control(&mut self, node: NodeRef<'d, Node>, e: &Element) {
        if has(e, "aria-label") || has(e, "title") || has(e, "placeholder") {
            return;
        }
        if node.ancestors().any(|a| is_element(a, local_name!("label"))) {
            return;
        }
        if let Some(ids) = labelledby(e) {
            self.need_labelledby.push((StaticCheck::FormLabel, node, ids));
            return;
        }
        match e.id().map(str::trim).filter(|i| !i.is_empty()) {
            Some(id) => self.need_label_for.push((node, id.to_string())),
            None => self.fail(StaticCheck::FormLabel, node),
        }
    }

    /// An element named by `aria-labelledby`, `aria-label`, `title` or,
    /// when `content` is true, its own text and image alternatives.
    fn named(&mut self, check: StaticCheck, node: NodeRef<'d, Node>, e: &Element, content: bool) {
        if has(e, "aria-label") || has(e, "title") || (content && has_content_name(node)) {
            return;
        }
        if let Some(ids) = labelledby(e) {
            self.need_labelledby.push((check, node, ids));
            return;
        }
        self.fail(check, node);
    }

    /// Records a failure unless the element is hidden from assistive
    /// technology by its markup.
    fn fail(&mut self, check: StaticCheck, node: NodeRef<'d, Node>) {
        if !hidden(node) {
            self.add(check, node);
        }
    }

    /// Resolves what needed the whole document and returns the findings.
    pub(super) fn finish(mut self, doc: &'d Html) -> A11yStatic {
        for (node, id) in std::mem::take(&mut self.need_label_for) {
            if !self.label_for.contains(&id) {
                self.fail(StaticCheck::FormLabel, node);
            }
        }
        let pending = std::mem::take(&mut self.need_labelledby);
        if !pending.is_empty() {
            let wanted: HashSet<&str> = pending
                .iter()
                .flat_map(|(_, _, ids)| ids.iter().map(String::as_str))
                .collect();
            let mut named: HashSet<String> = HashSet::new();
            for n in doc.tree.nodes() {
                if let Node::Element(e) = n.value()
                    && let Some(id) = e.id()
                    && wanted.contains(id)
                    && (has(e, "aria-label") || has_content_name(n))
                {
                    named.insert(id.to_string());
                }
            }
            for (check, node, ids) in pending {
                if !ids.iter().any(|i| named.contains(i)) {
                    self.fail(check, node);
                }
            }
        }
        let mut keys: Vec<_> = std::mem::take(&mut self.accesskeys).into_iter().collect();
        keys.sort_by(|a, b| a.0.cmp(&b.0));
        for (_, nodes) in keys {
            if nodes.len() > 1 {
                for n in nodes {
                    self.add(StaticCheck::AccesskeyDuplicate, n);
                }
            }
        }
        match &self.viewport {
            None => {
                self.add(StaticCheck::ViewportMissing, *doc.root_element());
            }
            Some(vp) if !vp.device_width => {
                let node = self.viewport_node.expect("set with the viewport");
                self.add(StaticCheck::ViewportWidth, node);
            }
            Some(_) => {}
        }

        let mut findings: Vec<StaticFinding> = self
            .found
            .into_iter()
            .map(|(check, (count, mut nodes))| {
                nodes.sort_by_key(|n| doc_order(*n));
                StaticFinding {
                    check,
                    count,
                    nodes: nodes.into_iter().map(describe).collect(),
                }
            })
            .collect();
        findings.sort_by_key(|f| f.check);
        A11yStatic {
            findings,
            viewport: self.viewport,
        }
    }
}

fn has(e: &Element, name: &str) -> bool {
    e.attr(name).is_some_and(|v| !v.trim().is_empty())
}

fn is_element(n: NodeRef<'_, Node>, name: html5ever::LocalName) -> bool {
    matches!(n.value(), Node::Element(e) if e.name.ns == ns!(html) && e.name.local == name)
}

fn labelledby(e: &Element) -> Option<Vec<String>> {
    let ids: Vec<String> = e
        .attr("aria-labelledby")?
        .split_ascii_whitespace()
        .map(str::to_string)
        .collect();
    (!ids.is_empty()).then_some(ids)
}

/// Whether the element's content gives it a name: non-blank text, an image
/// with alt text, or a descendant with `aria-label` (an SVG icon, say).
/// Stops at the first hit, so the usual case costs a few nodes.
fn has_content_name(node: NodeRef<'_, Node>) -> bool {
    for d in node.descendants().skip(1) {
        match d.value() {
            Node::Text(t) if !t.trim().is_empty() => {
                // Text inside <script>, <style> or <template> is not shown.
                let hidden_parent = d.parent().is_some_and(|p| {
                    matches!(p.value(), Node::Element(e)
                        if matches!(e.name.local, local_name!("script") | local_name!("style") | local_name!("template")))
                });
                if !hidden_parent {
                    return true;
                }
            }
            Node::Element(e) => {
                if has(e, "aria-label") {
                    return true;
                }
                if e.name.ns == ns!(html) && e.name.local == local_name!("img") && has(e, "alt") {
                    return true;
                }
            }
            _ => {}
        }
    }
    false
}

/// `hidden` or `aria-hidden="true"` on the element or an ancestor.
fn hidden(node: NodeRef<'_, Node>) -> bool {
    std::iter::once(node).chain(node.ancestors()).any(|n| {
        matches!(n.value(), Node::Element(e)
            if e.attr("hidden").is_some()
                || e.attr("aria-hidden").is_some_and(|v| v.trim().eq_ignore_ascii_case("true")))
    })
}

/// A well-formed language tag: a 2 or 3 letter primary subtag (or `x-`,
/// `i-`), then subtags of 1 to 8 letters or digits. Rejects the common
/// mistakes ("english", "en_US", "en-").
pub fn lang_ok(tag: &str) -> bool {
    let parts: Vec<&str> = tag.split('-').collect();
    if parts
        .iter()
        .any(|p| p.is_empty() || p.len() > 8 || !p.bytes().all(|b| b.is_ascii_alphanumeric()))
    {
        return false;
    }
    let first = parts[0];
    if first.eq_ignore_ascii_case("x") || first.eq_ignore_ascii_case("i") {
        return parts.len() > 1;
    }
    (first.len() == 2 || first.len() == 3) && first.bytes().all(|b| b.is_ascii_alphabetic())
}

/// `width=device-width, initial-scale=1, maximum-scale=1, user-scalable=no`.
/// Keys and values are separated by `,` or `;`, as browsers accept.
pub fn parse_viewport(content: &str) -> Viewport {
    let mut device_width = false;
    let mut zoom_disabled = false;
    for pair in content.split([',', ';']) {
        let Some((k, v)) = pair.split_once('=') else {
            continue;
        };
        let k = k.trim().to_ascii_lowercase();
        let v = v.trim().trim_matches(['"', '\'']).to_ascii_lowercase();
        match k.as_str() {
            "width" => device_width = v == "device-width",
            "user-scalable" if v == "no" || v == "0" => zoom_disabled = true,
            "maximum-scale" if v.parse::<f64>().is_ok_and(|s| s < 2.0) => zoom_disabled = true,
            _ => {}
        }
    }
    Viewport {
        content: content.trim().to_string(),
        device_width,
        zoom_disabled,
    }
}

/// Position in document order, for sorting nodes collected at the end.
fn doc_order(node: NodeRef<'_, Node>) -> Vec<usize> {
    let mut path: Vec<usize> = std::iter::once(node)
        .chain(node.ancestors())
        .map(|n| n.prev_siblings().count())
        .collect();
    path.reverse();
    path
}

fn describe(node: NodeRef<'_, Node>) -> A11yNode {
    A11yNode {
        selector: selector(node),
        excerpt: excerpt(node),
    }
}

/// A CSS selector for the element: `tag:nth-of-type(n)` steps from the
/// nearest ancestor with an `id` (or from `html`).
pub(super) fn selector(node: NodeRef<'_, Node>) -> String {
    let mut steps = Vec::new();
    for n in std::iter::once(node).chain(node.ancestors()) {
        let Node::Element(e) = n.value() else { break };
        let name = e.name.local.as_ref();
        if let Some(id) = e.id().filter(|i| css_ident(i)) {
            steps.push(format!("{name}#{id}"));
            break;
        }
        let same = |s: NodeRef<'_, Node>| matches!(s.value(), Node::Element(o) if o.name == e.name);
        let before = n.prev_siblings().filter(|s| same(*s)).count();
        let after = n.next_siblings().filter(|s| same(*s)).count();
        steps.push(if before + after == 0 {
            name.to_string()
        } else {
            format!("{name}:nth-of-type({})", before + 1)
        });
    }
    steps.reverse();
    steps.join(" > ")
}

/// Ids usable as written in a selector (no escaping needed).
fn css_ident(id: &str) -> bool {
    let mut chars = id.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_' || c == '-')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

/// The start tag, attributes in name order (as the parser stores them),
/// cut to [`EXCERPT_CHARS`] characters.
fn excerpt(node: NodeRef<'_, Node>) -> String {
    let Node::Element(e) = node.value() else {
        return String::new();
    };
    let mut s = format!("<{}", e.name.local);
    for (k, v) in e.attrs() {
        s.push(' ');
        s.push_str(k);
        s.push_str("=\"");
        s.push_str(&v.replace('"', "&quot;"));
        s.push('"');
    }
    s.push('>');
    if s.chars().count() > EXCERPT_CHARS {
        let cut: String = s.chars().take(EXCERPT_CHARS - 1).collect();
        return cut + "…";
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extract::{PageInput, extract_page};
    use url::Url;

    fn run(html: &str, a11y: bool) -> crate::extract::PageData {
        let url = Url::parse("https://example.com/").unwrap();
        extract_page(&PageInput {
            url: &url,
            charset: None,
            body: html.as_bytes(),
            truncated: false,
            robots_token: "SpiderLens",
            a11y,
            hook: None,
        })
    }

    #[test]
    fn off_means_none_and_nothing_else_changes() {
        let html = "<html lang=x><a href=/a></a><input type=text><h1>T</h1></html>";
        let off = run(html, false);
        let mut on = run(html, true);
        assert!(off.a11y.is_none());
        assert!(on.a11y.is_some());
        on.a11y = None;
        assert_eq!(off, on);
    }

    #[test]
    fn language_tags() {
        for ok in [
            "en",
            "EN",
            "he",
            "en-GB",
            "zh-Hant-TW",
            "fil",
            "x-klingon",
            "i-navajo",
            "es-419",
        ] {
            assert!(lang_ok(ok), "{ok}");
        }
        for bad in [
            "english",
            "en_US",
            "en-",
            "-en",
            "e",
            "x",
            "toolongsubtag-x",
            "en--GB",
            "123",
        ] {
            assert!(!lang_ok(bad), "{bad}");
        }
    }

    #[test]
    fn viewport_forms() {
        let v = parse_viewport("width=device-width, initial-scale=1");
        assert!(v.device_width && !v.zoom_disabled);
        assert!(parse_viewport("width=device-width,user-scalable=0").zoom_disabled);
        assert!(parse_viewport("width=device-width; maximum-scale=1.5").zoom_disabled);
        assert!(!parse_viewport("maximum-scale=2").zoom_disabled);
        assert!(!parse_viewport("width=1024").device_width);
        assert!(parse_viewport(" WIDTH = 'device-width' ").device_width);
    }

    #[test]
    fn selectors_and_excerpts() {
        let d = run(
            r#"<html lang="en"><body><main id="m"><p>x</p><p><a href="/a" class="c">
            </a></p></main><div><a href="/b"></a><a href="/c"></a></div></body></html>"#,
            true,
        );
        let f = d.a11y.unwrap();
        let links = f.finding(StaticCheck::LinkName).unwrap();
        assert_eq!(links.count, 3);
        let sel: Vec<_> = links.nodes.iter().map(|n| n.selector.as_str()).collect();
        assert_eq!(
            sel,
            [
                "main#m > p:nth-of-type(2) > a",
                "html > body > div > a:nth-of-type(1)",
                "html > body > div > a:nth-of-type(2)"
            ]
        );
        assert_eq!(links.nodes[0].excerpt, r#"<a class="c" href="/a">"#);
    }

    #[test]
    fn long_excerpts_are_cut() {
        let long = "x".repeat(400);
        let d = run(&format!(r#"<html lang="en"><a href="/{long}"></a></html>"#), true);
        let n = &d.a11y.unwrap().findings[0].nodes[0];
        assert_eq!(n.excerpt.chars().count(), EXCERPT_CHARS);
        assert!(n.excerpt.ends_with('…'));
    }

    #[test]
    fn node_cap_keeps_the_full_count() {
        let html = spiderlens_test_fixtures::a11y_site::many_unnamed_links(57);
        let d = run(&html, true);
        let f = d.a11y.unwrap();
        let links = f.finding(StaticCheck::LinkName).unwrap();
        assert_eq!(links.count, 57);
        assert_eq!(links.nodes.len(), NODE_CAP);
        assert_eq!(links.nodes[0].selector, "html > body > a:nth-of-type(1)");
    }

    #[test]
    fn every_check_has_metadata() {
        for c in StaticCheck::ALL {
            assert!(!c.as_str().is_empty());
            let w = c.wcag();
            assert!(w == "best-practice" || w.starts_with("wcag2"), "{w}");
        }
        let mut ids: Vec<_> = StaticCheck::ALL.iter().map(|c| c.as_str()).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), StaticCheck::ALL.len());
    }
}
