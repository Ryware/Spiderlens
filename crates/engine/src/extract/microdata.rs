//! Microdata items as written in the page (FR-029): type, id, properties,
//! nested items. Values are stored as written, not resolved.

use ego_tree::NodeRef;
use html5ever::{local_name, ns};
use scraper::Node;
use serde_json::{Map, Value, json};

use super::text::collapse;
use super::{StructuredBlock, StructuredKind};

/// The block for one top-level item (an `itemscope` without `itemprop`).
pub(super) fn item_block(node: NodeRef<'_, Node>) -> StructuredBlock {
    StructuredBlock {
        kind: StructuredKind::Microdata,
        raw: item(node).to_string(),
        parseable: true,
    }
}

fn item(node: NodeRef<'_, Node>) -> Value {
    let e = node.value().as_element().expect("items are elements");
    let mut obj = Map::new();
    if let Some(t) = e.attr("itemtype") {
        obj.insert(
            "type".into(),
            Value::Array(t.split_ascii_whitespace().map(|s| json!(s)).collect()),
        );
    }
    if let Some(id) = e.attr("itemid") {
        obj.insert("id".into(), json!(id.trim()));
    }
    let mut props: Map<String, Value> = Map::new();
    // Walk the item's subtree without entering nested items; a nested item
    // with `itemprop` is a property value.
    let mut stack: Vec<NodeRef<'_, Node>> = node.children().collect();
    stack.reverse();
    while let Some(n) = stack.pop() {
        let Some(el) = n.value().as_element() else { continue };
        let nested = el.attr("itemscope").is_some();
        if let Some(names) = el.attr("itemprop") {
            let value = if nested { item(n) } else { property_value(n) };
            for name in names.split_ascii_whitespace() {
                match props.get_mut(name) {
                    Some(Value::Array(list)) => list.push(value.clone()),
                    _ => {
                        props.insert(name.to_string(), Value::Array(vec![value.clone()]));
                    }
                }
            }
        }
        if !nested {
            let mut children: Vec<_> = n.children().collect();
            children.reverse();
            stack.extend(children);
        }
    }
    obj.insert("properties".into(), Value::Object(props));
    Value::Object(obj)
}

/// The HTML microdata property value rules.
fn property_value(n: NodeRef<'_, Node>) -> Value {
    let e = n.value().as_element().expect("element");
    let attr = |a: &str| json!(e.attr(a).unwrap_or("").trim());
    if e.name.ns != ns!(html) {
        return json!(collapse(&text_of(n)));
    }
    match e.name.local {
        local_name!("meta") => attr("content"),
        local_name!("audio")
        | local_name!("embed")
        | local_name!("iframe")
        | local_name!("img")
        | local_name!("source")
        | local_name!("track")
        | local_name!("video") => attr("src"),
        local_name!("a") | local_name!("area") | local_name!("link") => attr("href"),
        local_name!("object") => attr("data"),
        local_name!("data") | local_name!("meter") => attr("value"),
        local_name!("time") if e.attr("datetime").is_some() => attr("datetime"),
        _ => json!(collapse(&text_of(n))),
    }
}

fn text_of(n: NodeRef<'_, Node>) -> String {
    let mut s = String::new();
    for d in n.descendants() {
        if let Node::Text(t) = d.value() {
            s.push_str(t);
        }
    }
    s
}
