//! A scene as an SVG file (spec 022 FR-024, plan D9), written by hand: the
//! same picture as the canvas, with a header naming the view, host and
//! crawl date, and the legend under it. The same scene and palette give
//! the same bytes, on every platform and from the CLI and the desktop
//! alike (SC-007).

use std::fmt::Write;

use super::palette::{Colour, Palette};
use super::{Category, ColourBy, Layout, Scene, SceneNode, ViewKind, thousands};

const MARGIN: f32 = 24.0;
const HEADER: f32 = 44.0;
const LINE: f32 = 18.0;
const LABEL_GAP: f32 = 6.0;
const LABEL_MAX: f32 = 260.0;
/// Labels drawn at most, as on screen (FR-022).
const LABELS: usize = 300;
pub const FONT: &str =
    "system-ui, -apple-system, 'Segoe UI', Roboto, Ubuntu, 'Noto Sans', 'Noto Sans Hebrew', sans-serif";

/// What the header says.
#[derive(Debug, Clone, Default)]
pub struct Header {
    pub host: String,
    /// `YYYY-MM-DD`, when the crawl started.
    pub date: String,
}

pub fn view_title(v: ViewKind) -> &'static str {
    match v {
        ViewKind::CrawlTree => "Crawl tree",
        ViewKind::DirectoryTree => "Directory tree",
        ViewKind::LinkGraph => "Link graph",
        ViewKind::Anchors => "Anchor text",
    }
}

/// A number as SVG writes it: one decimal, no `-0.0`.
fn n(v: f32) -> String {
    let s = format!("{:.1}", v);
    if s == "-0.0" { "0.0".into() } else { s }
}

fn esc(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => o.push_str("&amp;"),
            '<' => o.push_str("&lt;"),
            '>' => o.push_str("&gt;"),
            '"' => o.push_str("&quot;"),
            // Characters XML 1.0 does not allow.
            c if (c as u32) < 0x20 && !matches!(c, '\t' | '\n' | '\r') => o.push(' '),
            c => o.push(c),
        }
    }
    o
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Shape {
    Circle,
    Diamond,
    Triangle,
    Square,
    Ring,
    Dashed,
    Rounded,
}

/// Token and shape per category, as the canvas draws them (FR-005).
fn style(c: Category) -> (&'static str, Shape) {
    match c {
        Category::Ok => ("--viz-ok", Shape::Circle),
        Category::Redirect => ("--viz-redirect", Shape::Diamond),
        Category::ClientError => ("--viz-client-error", Shape::Triangle),
        Category::ServerError => ("--viz-server-error", Shape::Square),
        Category::Blocked => ("--viz-blocked", Shape::Ring),
        Category::NotCrawled | Category::Other => ("--viz-not-crawled", Shape::Dashed),
        Category::Folder => ("--viz-folder", Shape::Rounded),
        Category::Group => ("--viz-group", Shape::Ring),
    }
}

fn shape_path(d: &mut String, s: Shape, x: f32, y: f32, r: f32) {
    let _ = match s {
        Shape::Diamond => write!(
            d,
            "M{} {}L{} {}L{} {}L{} {}Z",
            n(x),
            n(y - r * 1.25),
            n(x + r * 1.25),
            n(y),
            n(x),
            n(y + r * 1.25),
            n(x - r * 1.25),
            n(y)
        ),
        Shape::Triangle => write!(
            d,
            "M{} {}L{} {}L{} {}Z",
            n(x),
            n(y - r * 1.2),
            n(x + r * 1.1),
            n(y + r * 0.8),
            n(x - r * 1.1),
            n(y + r * 0.8)
        ),
        Shape::Square => write!(
            d,
            "M{} {}h{}v{}h-{}Z",
            n(x - r * 0.9),
            n(y - r * 0.9),
            n(r * 1.8),
            n(r * 1.8),
            n(r * 1.8)
        ),
        Shape::Rounded => {
            let q = r * 0.4;
            write!(
                d,
                "M{} {}h{}a{q} {q} 0 0 1 {q} {q}v{}a{q} {q} 0 0 1 -{q} {q}h-{}a{q} {q} 0 0 1 -{q} -{q}v-{}a{q} {q} 0 0 1 {q} -{q}Z",
                n(x - r + q),
                n(y - r),
                n(2.0 * r - 2.0 * q),
                n(2.0 * r - 2.0 * q),
                n(2.0 * r - 2.0 * q),
                n(2.0 * r - 2.0 * q),
                q = n(q)
            )
        }
        _ => write!(
            d,
            "M{} {}a{r} {r} 0 1 0 {} 0a{r} {r} 0 1 0 -{} 0Z",
            n(x - r),
            n(y),
            n(2.0 * r),
            n(2.0 * r),
            r = n(r)
        ),
    };
}

/// The node's colour: by category, or by depth for URL nodes.
fn fill(p: &Palette, colour: ColourBy, node: &SceneNode) -> String {
    let (token, _) = style(node.category);
    if colour == ColourBy::Depth && node.url_id.is_some() {
        let accent = 100.0f64.min(100.0 - node.depth as f64 * 16.0).max(20.0) / 100.0;
        return p.get("--accent").mix(p.get("--fg-3"), 1.0 - accent).hex();
    }
    p.hex(token)
}

/// A term's colour: the accent for the most used, fading to secondary text.
fn term_colour(p: &Palette, rank: usize, of: usize) -> Colour {
    let t = 0.7 * rank as f64 / (of.max(2) - 1) as f64;
    p.get("--accent-text").mix(p.get("--fg-2"), t)
}

/// Which nodes get a label, the same rule as the canvas at zoom 1.
fn labelled(nodes: &[SceneNode]) -> Vec<usize> {
    let mut out: Vec<usize> = (0..nodes.len()).filter(|&i| nodes[i].r >= 4.0).collect();
    out.sort_by(|&a, &b| {
        nodes[b]
            .r
            .total_cmp(&nodes[a].r)
            .then(nodes[a].depth.cmp(&nodes[b].depth))
            .then(a.cmp(&b))
    });
    out.truncate(LABELS);
    out
}

fn label_text(node: &SceneNode) -> String {
    if node.hidden > 0 && !node.key.starts_with("m:") {
        format!("{}  (+{})", node.label, node.hidden)
    } else {
        node.label.clone()
    }
}

/// The lines of text under the picture.
fn legend(scene: &Scene, colour: ColourBy) -> Vec<(Option<Category>, String)> {
    let mut out = Vec::new();
    if scene.view == ViewKind::Anchors {
        let what = if scene.nodes.is_empty() {
            scene.note.clone().unwrap_or_default()
        } else {
            "Words used in the anchor text of links, sized by the number of links".to_string()
        };
        out.push((None, what));
    } else if colour == ColourBy::Depth {
        out.push((
            None,
            "Colour: crawl depth, darker is closer to the start".to_string(),
        ));
    } else {
        for l in &scene.legend {
            out.push((
                Some(l.category),
                format!("{}  {}", l.label, thousands(l.count.into())),
            ));
        }
    }
    if let Some(s) = &scene.size_legend {
        out.push((None, s.clone()));
    }
    if let Some(o) = &scene.omitted {
        out.push((
            None,
            if scene.view == ViewKind::Anchors {
                format!(
                    "Showing {} of {} terms. {}",
                    scene.nodes.len(),
                    thousands(scene.nodes.len() as u64 + u64::from(o.count)),
                    o.reason
                )
            } else {
                format!(
                    "Showing {} of {} URLs. {}",
                    thousands(scene.total.saturating_sub(o.count).into()),
                    thousands(scene.total.into()),
                    o.reason
                )
            },
        ));
    }
    out
}

/// Longest side of the picture at 1x, in px: a bigger scene is drawn
/// smaller, as fitting it to a window would (labels stay 12 px), so even a
/// 4x PNG of a 20,000-node tree stays under the size limit and renders in
/// seconds (SC-007).
pub const MAX_PICTURE: f32 = 2400.0;

/// `scene` zoomed by `k`: positions, radii and term boxes.
fn zoomed(scene: &Scene, k: f32) -> Scene {
    let mut z = scene.clone();
    for nd in &mut z.nodes {
        nd.x *= k;
        nd.y *= k;
        nd.r *= k;
        nd.width = nd.width.map(|w| w * k);
    }
    let b = &mut z.bounds;
    (b.x, b.y, b.width, b.height) = (b.x * k, b.y * k, b.width * k, b.height * k);
    z
}

/// The SVG of `scene` as drawn with `palette`.
pub fn svg(scene: &Scene, palette: &Palette, colour: ColourBy, header: &Header) -> String {
    let side = scene.bounds.width.max(scene.bounds.height);
    let fitted;
    let scene = if side > MAX_PICTURE {
        fitted = zoomed(scene, MAX_PICTURE / side);
        &fitted
    } else {
        scene
    };
    let nodes = &scene.nodes;
    let cloud = scene.layout == Layout::Cloud;
    let labels = if cloud { vec![] } else { labelled(nodes) };
    // Picture extent: the scene bounds and the labels to their right.
    let (mut x0, mut y0) = (scene.bounds.x, scene.bounds.y);
    let (mut x1, mut y1) = (x0 + scene.bounds.width, y0 + scene.bounds.height);
    for &i in &labels {
        let nd = &nodes[i];
        let w = super::anchors::text_width(&label_text(nd), 12.0).min(LABEL_MAX);
        x1 = x1.max(nd.x + nd.r + LABEL_GAP + w);
        y0 = y0.min(nd.y - 8.0);
        y1 = y1.max(nd.y + 8.0);
    }
    if nodes.is_empty() {
        (x0, y0, x1, y1) = (0.0, 0.0, 0.0, 0.0);
    }
    let lines = legend(scene, colour);
    let legend_h = lines.len() as f32 * LINE;
    let width = (x1 - x0 + 2.0 * MARGIN).max(560.0).ceil();
    let height = (HEADER + (y1 - y0) + MARGIN * 2.0 + legend_h + 8.0).ceil();
    // Centre a narrow picture.
    let ox = MARGIN - x0 + ((width - 2.0 * MARGIN) - (x1 - x0)).max(0.0) / 2.0;
    let oy = HEADER + MARGIN - y0;

    let mut s = String::with_capacity(64 * 1024 + nodes.len() * 64);
    let _ = write!(
        s,
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{w}\" height=\"{h}\" viewBox=\"0 0 {w} {h}\" font-family=\"{FONT}\">\n",
        w = n(width),
        h = n(height)
    );
    let title = view_title(scene.view);
    let place: Vec<&str> = [header.host.as_str(), header.date.as_str()]
        .into_iter()
        .filter(|p| !p.is_empty())
        .collect();
    let place = esc(&place.join(" · "));
    let _ = writeln!(
        s,
        "<title>{}{}{place}</title>",
        esc(title),
        if place.is_empty() { "" } else { " · " }
    );
    let _ = writeln!(
        s,
        "<rect width=\"100%\" height=\"100%\" fill=\"{}\"/>",
        palette.hex("--bg")
    );
    let _ = writeln!(
        s,
        "<g id=\"header\"><text x=\"{m}\" y=\"28\" font-size=\"16\" font-weight=\"600\" fill=\"{}\">{}</text><text x=\"{}\" y=\"28\" font-size=\"13\" text-anchor=\"end\" fill=\"{}\">{place}</text></g>",
        palette.hex("--fg"),
        esc(title),
        n(width - MARGIN),
        palette.hex("--fg-2"),
        m = n(MARGIN)
    );
    let _ = writeln!(
        s,
        "<g id=\"picture\" transform=\"translate({} {})\">",
        n(ox),
        n(oy)
    );
    let line = palette.hex("--viz-line");

    // Tree lines, then link graph lines and arrowheads.
    let mut d = String::new();
    for nd in nodes {
        let Some(p) = nd.parent else { continue };
        let p = &nodes[p as usize];
        if matches!(scene.layout, Layout::Radial | Layout::Force) {
            let _ = write!(d, "M{} {}L{} {}", n(p.x), n(p.y), n(nd.x), n(nd.y));
        } else {
            let mx = (p.x + nd.x) / 2.0;
            let _ = write!(
                d,
                "M{} {}C{} {} {} {} {} {}",
                n(p.x),
                n(p.y),
                n(mx),
                n(p.y),
                n(mx),
                n(nd.y),
                n(nd.x),
                n(nd.y)
            );
        }
    }
    let mut heads = String::new();
    for e in &scene.edges {
        let (a, b) = (&nodes[e.a as usize], &nodes[e.b as usize]);
        let _ = write!(d, "M{} {}L{} {}", n(a.x), n(a.y), n(b.x), n(b.y));
        if !scene.directed {
            continue;
        }
        let (dx, dy) = (b.x - a.x, b.y - a.y);
        let len = (dx * dx + dy * dy).sqrt();
        if len < b.r + 2.0 {
            continue;
        }
        let (ux, uy) = (dx / len, dy / len);
        let (tx, ty) = (b.x - ux * (b.r + 1.5), b.y - uy * (b.r + 1.5));
        let k = 5.0;
        let _ = write!(
            heads,
            "M{} {}L{} {}L{} {}Z",
            n(tx),
            n(ty),
            n(tx - ux * k - uy * k * 0.5),
            n(ty - uy * k + ux * k * 0.5),
            n(tx - ux * k + uy * k * 0.5),
            n(ty - uy * k - ux * k * 0.5)
        );
    }
    if !d.is_empty() {
        let _ = writeln!(
            s,
            "<path id=\"lines\" d=\"{d}\" fill=\"none\" stroke=\"{line}\" stroke-width=\"1\"/>"
        );
    }
    if !heads.is_empty() {
        let _ = writeln!(s, "<path id=\"arrows\" d=\"{heads}\" fill=\"{line}\"/>");
    }

    if cloud {
        let _ = writeln!(
            s,
            "<g id=\"terms\" text-anchor=\"middle\" dominant-baseline=\"central\">"
        );
        for (i, nd) in nodes.iter().enumerate() {
            let font = nd.r * 2.0 / 1.15;
            let _ = writeln!(
                s,
                "<text x=\"{}\" y=\"{}\" font-size=\"{}\" font-weight=\"{}\" fill=\"{}\" textLength=\"{}\" lengthAdjust=\"spacingAndGlyphs\">{}</text>",
                n(nd.x),
                n(nd.y),
                n(font),
                if i < 10 { 600 } else { 500 },
                term_colour(palette, i, nodes.len()).hex(),
                n((nd.width.unwrap_or(0.0) - 6.0).max(1.0)),
                esc(&nd.label)
            );
        }
        s.push_str("</g>\n");
    } else {
        // Nodes, one path per shape and colour, in first-seen order.
        let mut batches: Vec<(Shape, String, String)> = Vec::new();
        for nd in nodes {
            let (_, shape) = style(nd.category);
            let f = fill(palette, colour, nd);
            let at = match batches.iter().position(|(sh, c, _)| *sh == shape && *c == f) {
                Some(i) => i,
                None => {
                    batches.push((shape, f, String::new()));
                    batches.len() - 1
                }
            };
            shape_path(&mut batches[at].2, shape, nd.x, nd.y, nd.r);
        }
        let _ = writeln!(s, "<g id=\"nodes\">");
        for (shape, f, d) in &batches {
            match shape {
                Shape::Ring => {
                    let _ = writeln!(
                        s,
                        "<path d=\"{d}\" fill=\"none\" stroke=\"{f}\" stroke-width=\"2\"/>"
                    );
                }
                Shape::Dashed => {
                    let _ = writeln!(
                        s,
                        "<path d=\"{d}\" fill=\"none\" stroke=\"{f}\" stroke-width=\"2\" stroke-dasharray=\"3 2\"/>"
                    );
                }
                _ => {
                    let _ = writeln!(s, "<path d=\"{d}\" fill=\"{f}\"/>");
                }
            }
        }
        // Folded branches: a ring around the node.
        let mut folded = String::new();
        for nd in nodes
            .iter()
            .filter(|nd| nd.expandable && !nd.expanded && !nd.key.starts_with("m:"))
        {
            shape_path(&mut folded, Shape::Circle, nd.x, nd.y, nd.r + 3.0);
        }
        if !folded.is_empty() {
            let _ = writeln!(
                s,
                "<path d=\"{folded}\" fill=\"none\" stroke=\"{line}\" stroke-width=\"1\"/>"
            );
        }
        s.push_str("</g>\n");
        if !labels.is_empty() {
            let _ = writeln!(
                s,
                "<g id=\"labels\" font-size=\"12\" fill=\"{}\" stroke=\"{}\" stroke-width=\"3\" stroke-linejoin=\"round\" paint-order=\"stroke\" dominant-baseline=\"central\">",
                palette.hex("--fg-2"),
                palette.hex("--bg")
            );
            for &i in &labels {
                let nd = &nodes[i];
                let text = label_text(nd);
                let squeeze = if super::anchors::text_width(&text, 12.0) > LABEL_MAX {
                    format!(
                        " textLength=\"{}\" lengthAdjust=\"spacingAndGlyphs\"",
                        n(LABEL_MAX)
                    )
                } else {
                    String::new()
                };
                let _ = writeln!(
                    s,
                    "<text x=\"{}\" y=\"{}\"{squeeze}>{}</text>",
                    n(nd.x + nd.r + LABEL_GAP),
                    n(nd.y),
                    esc(&text)
                );
            }
            s.push_str("</g>\n");
        }
    }
    s.push_str("</g>\n");

    // Legend under the picture.
    let top = HEADER + (y1 - y0) + MARGIN * 2.0;
    let _ = writeln!(
        s,
        "<g id=\"legend\" font-size=\"12\" fill=\"{}\" dominant-baseline=\"central\">",
        palette.hex("--fg-2")
    );
    for (k, (cat, text)) in lines.iter().enumerate() {
        let y = top + k as f32 * LINE + LINE / 2.0;
        let mut x = MARGIN;
        if let Some(c) = cat {
            let (token, shape) = style(*c);
            let mut d = String::new();
            shape_path(
                &mut d,
                if shape == Shape::Dashed {
                    Shape::Circle
                } else {
                    shape
                },
                MARGIN + 5.0,
                y,
                4.0,
            );
            let hex = palette.hex(token);
            let _ = match shape {
                Shape::Ring => writeln!(
                    s,
                    "<path d=\"{d}\" fill=\"none\" stroke=\"{hex}\" stroke-width=\"1.5\"/>"
                ),
                Shape::Dashed => writeln!(
                    s,
                    "<path d=\"{d}\" fill=\"none\" stroke=\"{hex}\" stroke-width=\"1.5\" stroke-dasharray=\"2 1.5\"/>"
                ),
                _ => writeln!(s, "<path d=\"{d}\" fill=\"{hex}\"/>"),
            };
            x += 16.0;
        }
        let _ = writeln!(s, "<text x=\"{}\" y=\"{}\">{}</text>", n(x), n(y), esc(text));
    }
    s.push_str("</g>\n</svg>\n");
    s
}
