//! A scene's nodes and links as CSV (spec 022 FR-024), under the 007 CSV
//! rules: byte order mark, CRLF, quoting, and values a spreadsheet would
//! run as a formula neutralised.

use std::io::{self, Write};

use super::{Scene, SceneNode, ViewKind};
use crate::export::csv_field;

fn row(out: &mut impl Write, cells: &[&str]) -> io::Result<()> {
    for (i, c) in cells.iter().enumerate() {
        if i > 0 {
            out.write_all(b",")?;
        }
        csv_field(out, c)?;
    }
    out.write_all(b"\r\n")
}

fn kind(n: &SceneNode) -> &'static str {
    match n.key.split_once(':').map_or(n.key.as_str(), |(k, _)| k) {
        "u" => "page",
        "d" => "folder",
        "m" => "more",
        "t" => "term",
        _ => "group",
    }
}

/// A node as the other file names it: its URL, else its key.
fn name(n: &SceneNode) -> &str {
    n.url.as_deref().unwrap_or(&n.key)
}

/// The nodes shown, one row each, in drawing order.
pub fn nodes(scene: &Scene, out: impl Write) -> io::Result<()> {
    let mut out = io::BufWriter::new(out);
    out.write_all(b"\xEF\xBB\xBF")?;
    if scene.view == ViewKind::Anchors {
        row(&mut out, &["Term", "Links"])?;
        for n in &scene.nodes {
            row(&mut out, &[&n.label, &n.count.to_string()])?;
        }
        return out.flush();
    }
    row(
        &mut out,
        &[
            "Node",
            "Kind",
            "URL",
            "Label",
            "Depth",
            "Response",
            "Link Score",
            "Inlinks",
            "Parent",
            "URLs",
            "Folded",
        ],
    )?;
    for n in &scene.nodes {
        let parent = n
            .parent
            .map(|p| name(&scene.nodes[p as usize]).to_string())
            .unwrap_or_default();
        let legend = scene
            .legend
            .iter()
            .find(|l| l.category == n.category)
            .map(|l| l.label);
        let response = legend.unwrap_or(n.category.label());
        row(
            &mut out,
            &[
                &n.key,
                kind(n),
                n.url.as_deref().unwrap_or(""),
                &n.label,
                &n.depth.to_string(),
                response,
                &n.score.map(|s| format!("{s:.2}")).unwrap_or_default(),
                &n.inlinks.map(|i| i.to_string()).unwrap_or_default(),
                &parent,
                &n.count.to_string(),
                &n.hidden.to_string(),
            ],
        )?;
    }
    out.flush()
}

/// Whether the scene has lines to list: tree branches or followed links.
pub fn has_links(scene: &Scene) -> bool {
    !scene.edges.is_empty() || scene.nodes.iter().any(|n| n.parent.is_some())
}

/// The lines shown: tree branches (parent to child) and followed links.
pub fn links(scene: &Scene, out: impl Write) -> io::Result<()> {
    let mut out = io::BufWriter::new(out);
    out.write_all(b"\xEF\xBB\xBF")?;
    row(&mut out, &["From", "To", "Kind"])?;
    for n in &scene.nodes {
        if let Some(p) = n.parent {
            row(&mut out, &[name(&scene.nodes[p as usize]), name(n), "tree"])?;
        }
    }
    for e in &scene.edges {
        row(
            &mut out,
            &[
                name(&scene.nodes[e.a as usize]),
                name(&scene.nodes[e.b as usize]),
                "link",
            ],
        )?;
    }
    out.flush()
}
