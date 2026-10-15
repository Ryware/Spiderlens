//! Anchor text clouds (spec 022 US4, plan D6): the words, or whole
//! anchors, other pages use when they link to a page, or across the whole
//! site, sized by how many links use them.
//!
//! A term counts once per link: a link whose text says "shoes" twice is
//! one use, so the term's count is the number of links in its list
//! (FR-018). Image links count with their alt text.

use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

use rusqlite::{Connection, params_from_iter};
use serde::Serialize;
use ts_rs::TS;
use unicode_segmentation::UnicodeSegmentation;

use super::{Bounds, Category, Layout, Omitted, Scene, SceneNode, Terms, ViewKind, ViewState, now_ms};
use crate::Result;

/// Most terms in a cloud (FR-016).
pub const TERMS: usize = 100;
/// Font sizes in layout px, smallest and largest term.
pub const MIN_FONT: f32 = 12.0;
pub const MAX_FONT: f32 = 48.0;

const STOP_WORDS: [(&str, &str); 9] = [
    ("en", include_str!("stopwords/en.txt")),
    ("de", include_str!("stopwords/de.txt")),
    ("fr", include_str!("stopwords/fr.txt")),
    ("es", include_str!("stopwords/es.txt")),
    ("it", include_str!("stopwords/it.txt")),
    ("pt", include_str!("stopwords/pt.txt")),
    ("nl", include_str!("stopwords/nl.txt")),
    ("ru", include_str!("stopwords/ru.txt")),
    ("he", include_str!("stopwords/he.txt")),
];

/// The stop words for a language's primary subtag; English when we have
/// no list for it (or the page says no language).
fn stop_words(lang: &str) -> &'static HashSet<&'static str> {
    static LISTS: OnceLock<HashMap<&'static str, HashSet<&'static str>>> = OnceLock::new();
    let lists = LISTS.get_or_init(|| {
        STOP_WORDS
            .iter()
            .map(|(l, text)| {
                (
                    *l,
                    text.lines().map(str::trim).filter(|w| !w.is_empty()).collect(),
                )
            })
            .collect()
    });
    lists.get(lang).unwrap_or(&lists["en"])
}

/// `en-GB` → `en`.
fn primary(lang: &str) -> String {
    lang.split(['-', '_'])
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase()
}

/// Languages written without spaces between words: their terms are always
/// whole anchors (spec edge case).
fn no_spaces(lang: &str) -> bool {
    matches!(lang, "zh" | "ja" | "th" | "lo" | "km" | "my" | "bo")
}

/// How the terms of one cloud are made.
struct Terming {
    words: bool,
    stop: Option<&'static HashSet<&'static str>>,
}

impl Terming {
    /// The distinct terms of one anchor text, in first-seen order.
    fn terms(&self, text: &str) -> Vec<String> {
        let lower = text.to_lowercase();
        if !self.words {
            let whole = lower.split_whitespace().collect::<Vec<_>>().join(" ");
            return if whole.is_empty() { vec![] } else { vec![whole] };
        }
        let mut out: Vec<String> = Vec::new();
        for w in lower.unicode_words() {
            if self.stop.is_some_and(|s| s.contains(w)) || out.iter().any(|o| o == w) {
                continue;
            }
            out.push(w.to_string());
        }
        out
    }
}

/// One internal link to the cloud's page (or any internal page).
struct Row {
    from_id: i64,
    to_id: i64,
    /// Anchor text, or alt text for an image link; None when it has neither.
    text: Option<String>,
    image: bool,
    rel: Option<String>,
    nofollow: bool,
}

/// The cloud's page, from `state.focus` (`u:<id>`); None: the whole site.
fn target(state: &ViewState) -> Option<i64> {
    state
        .focus
        .as_deref()
        .and_then(|k| k.strip_prefix("u:"))
        .and_then(|id| id.parse().ok())
}

/// Calls `f` for every link the cloud covers, in the order they were stored.
fn each_link(conn: &Connection, state: &ViewState, mut f: impl FnMut(Row)) -> Result<()> {
    let to = target(state);
    let mut sql = String::from(
        "SELECT l.from_id, l.to_id, l.text, l.alt, l.rel, (l.nofollow OR l.page_nofollow OR l.sponsored OR l.ugc) \
         FROM links l WHERE l.kind = 'anchor'",
    );
    // Site-wide, internal targets come from a table of ids in memory: a
    // join per link is most of the time on a big crawl.
    let internal: Vec<bool> = if to.is_some() {
        sql.push_str(" AND l.to_id = ?1");
        vec![]
    } else {
        sql.push_str(" AND l.to_id IS NOT NULL");
        let mut v = Vec::new();
        let mut stmt = conn.prepare("SELECT id FROM urls WHERE internal = 1")?;
        let mut q = stmt.query([])?;
        while let Some(r) = q.next()? {
            let id = r.get::<_, i64>(0)? as usize;
            if id >= v.len() {
                v.resize(id + 1, false);
            }
            v[id] = true;
        }
        v
    };
    if state.followed_only {
        sql.push_str(" AND NOT (l.nofollow OR l.page_nofollow OR l.sponsored OR l.ugc)");
    }
    // No ORDER BY: the file's own order (sources in crawl order) is
    // stable, and sorting 1M+ links would double the time.
    let mut stmt = conn.prepare(&sql)?;
    let mut q = stmt.query(params_from_iter(to))?;
    while let Some(r) = q.next()? {
        let to_id: i64 = r.get(1)?;
        if to.is_none() && !internal.get(to_id as usize).copied().unwrap_or(false) {
            continue;
        }
        let text = r
            .get_ref(2)?
            .as_str_or_null()
            .map_err(rusqlite::Error::from)?
            .filter(|t| !t.trim().is_empty());
        let alt = r
            .get_ref(3)?
            .as_str_or_null()
            .map_err(rusqlite::Error::from)?
            .filter(|a| !a.trim().is_empty());
        let image = text.is_none() && alt.is_some();
        f(Row {
            from_id: r.get(0)?,
            to_id,
            text: text.or(alt).map(str::to_string),
            image,
            rel: r.get(4)?,
            nofollow: r.get(5)?,
        });
    }
    Ok(())
}

/// The language whose stop words apply: the page's own, or the most
/// common one across the site.
fn language(conn: &Connection, to: Option<i64>) -> Result<String> {
    if let Some(id) = to {
        let lang: Option<String> = conn
            .query_row("SELECT lang FROM pages WHERE url_id = ?1", [id], |r| r.get(0))
            .or_else(|e| {
                if e == rusqlite::Error::QueryReturnedNoRows {
                    Ok(None)
                } else {
                    Err(e)
                }
            })?;
        return Ok(lang.map(|l| primary(&l)).unwrap_or_default());
    }
    let mut counts: HashMap<String, i64> = HashMap::new();
    let mut stmt = conn.prepare("SELECT lang, COUNT(*) FROM pages WHERE lang IS NOT NULL GROUP BY lang")?;
    let mut q = stmt.query([])?;
    while let Some(r) = q.next()? {
        *counts.entry(primary(&r.get::<_, String>(0)?)).or_default() += r.get::<_, i64>(1)?;
    }
    // Most pages first, then the name, so ties do not depend on hash order.
    Ok(counts
        .into_iter()
        .max_by(|a, b| a.1.cmp(&b.1).then(b.0.cmp(&a.0)))
        .map(|(l, _)| l)
        .unwrap_or_default())
}

fn terming(conn: &Connection, state: &ViewState) -> Result<(Terming, String)> {
    let lang = language(conn, target(state))?;
    let words = state.terms == Terms::Words && !no_spaces(&lang);
    let stop = (words && state.stop_words).then(|| stop_words(&lang));
    Ok((Terming { words, stop }, lang))
}

/// Advance of one character in em, by script: close enough to any UI font
/// that the drawn text, squeezed to this width with `maxWidth` or
/// `textLength`, never looks squashed (plan D6).
fn advance(c: char) -> f32 {
    match c {
        ' ' => 0.28,
        'i' | 'l' | 'j' | 't' | 'f' | 'r' | 'I' | '.' | ',' | ':' | ';' | '\'' | '!' | '|' => 0.32,
        'm' | 'w' | 'M' | 'W' => 0.86,
        'A'..='Z' => 0.68,
        '0'..='9' => 0.56,
        'a'..='z' => 0.55,
        '\u{0590}'..='\u{05FF}' => 0.58,
        '\u{0400}'..='\u{04FF}' => 0.62,
        '\u{1100}'..='\u{11FF}'
        | '\u{2E80}'..='\u{9FFF}'
        | '\u{AC00}'..='\u{D7AF}'
        | '\u{F900}'..='\u{FAFF}'
        | '\u{FF00}'..='\u{FFEF}' => 1.0,
        _ => 0.6,
    }
}

pub(crate) fn text_width(text: &str, font: f32) -> f32 {
    text.chars().map(advance).sum::<f32>() * font
}

/// Font size by the square root of the count, from [`MIN_FONT`] for the
/// least used term shown to [`MAX_FONT`] for the most used.
fn font(count: u32, min: u32, max: u32) -> f32 {
    if max == min {
        return 24.0;
    }
    let t = ((count as f32).sqrt() - (min as f32).sqrt()) / ((max as f32).sqrt() - (min as f32).sqrt());
    MIN_FONT + (MAX_FONT - MIN_FONT) * t
}

#[derive(Clone, Copy)]
struct Rect {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
}

impl Rect {
    fn overlaps(&self, o: &Rect) -> bool {
        (self.x - o.x).abs() * 2.0 < self.w + o.w && (self.y - o.y).abs() * 2.0 < self.h + o.h
    }
}

/// Places boxes (width, height), biggest first, on an Archimedean spiral
/// out from the centre, each at the first spot where it overlaps nothing.
/// Positions are rounded to 0.1 px so every platform gives the same cloud.
fn spiral(boxes: &[(f32, f32)]) -> Vec<(f32, f32)> {
    let mut placed: Vec<Rect> = Vec::with_capacity(boxes.len());
    for &(w, h) in boxes {
        let mut t = 0.0f32;
        let mut spot = Rect { x: 0.0, y: 0.0, w, h };
        for _ in 0..40_000 {
            // Wider than tall, like a page.
            spot.x = (1.6 * 3.0 * t * t.cos() * 10.0).round() / 10.0;
            spot.y = (3.0 * t * t.sin() * 10.0).round() / 10.0;
            if !placed.iter().any(|p| p.overlaps(&spot)) {
                break;
            }
            t += 0.08;
        }
        placed.push(spot);
    }
    placed.iter().map(|r| (r.x, r.y)).collect()
}

/// Builds the cloud for `state.focus` (a page) or the whole site.
pub fn scene(conn: &Connection, state: &ViewState) -> Result<Scene> {
    let (terming, lang) = terming(conn, state)?;
    let mut counts: HashMap<String, u32> = HashMap::new();
    let (mut links, mut with_text) = (0u32, 0u32);
    each_link(conn, state, |row| {
        links += 1;
        if let Some(text) = &row.text {
            with_text += 1;
            for t in terming.terms(text) {
                *counts.entry(t).or_default() += 1;
            }
        }
    })?;
    let distinct = counts.len();
    let mut terms: Vec<(String, u32)> = counts.into_iter().collect();
    terms.sort_unstable_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    terms.truncate(TERMS);
    let (max, min) = (terms.first().map_or(0, |t| t.1), terms.last().map_or(0, |t| t.1));
    let boxes: Vec<(f32, f32)> = terms
        .iter()
        .map(|(t, c)| {
            let f = font(*c, min, max);
            (text_width(t, f) + 6.0, f * 1.15)
        })
        .collect();
    let at = spiral(&boxes);
    let nodes: Vec<SceneNode> = terms
        .iter()
        .enumerate()
        .map(|(i, (t, c))| SceneNode {
            key: format!("t:{t}"),
            label: t.clone(),
            url: None,
            url_id: None,
            x: at[i].0,
            y: at[i].1,
            r: boxes[i].1 / 2.0,
            width: Some(boxes[i].0),
            category: Category::Other,
            depth: i as u32,
            score: None,
            inlinks: None,
            count: *c,
            hidden: 0,
            expandable: false,
            expanded: false,
            parent: None,
        })
        .collect();
    let bounds = if nodes.is_empty() {
        Bounds {
            x: 0.0,
            y: 0.0,
            width: 0.0,
            height: 0.0,
        }
    } else {
        let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
        for n in &nodes {
            let w = n.width.unwrap_or(0.0) / 2.0;
            x0 = x0.min(n.x - w);
            x1 = x1.max(n.x + w);
            y0 = y0.min(n.y - n.r);
            y1 = y1.max(n.y + n.r);
        }
        Bounds {
            x: x0,
            y: y0,
            width: x1 - x0,
            height: y1 - y0,
        }
    };
    let page = target(state).is_some();
    let note = nodes.is_empty().then(|| {
        if links == 0 {
            if page {
                "No internal links point to this page.".to_string()
            } else {
                "No internal links in this crawl yet.".to_string()
            }
        } else if with_text == 0 {
            "None of these links has anchor text: they are image links without alt text, or empty links."
                .to_string()
        } else {
            "Every word of the anchor text is a stop word: turn Stop words off to see them.".to_string()
        }
    });
    let mut size_legend = format!("Size: links that use the term ({} with anchor text)", with_text);
    if state.terms == Terms::Words && !terming.words {
        size_legend.push_str(". Whole anchors: this language has no spaces between words");
    }
    Ok(Scene {
        view: ViewKind::Anchors,
        layout: Layout::Cloud,
        nodes,
        edges: vec![],
        directed: false,
        settled: true,
        legend: vec![],
        size_legend: Some(size_legend),
        omitted: (distinct > TERMS).then(|| Omitted {
            count: (distinct - TERMS) as u32,
            reason: "Only the most used terms are shown.".to_string(),
        }),
        bounds,
        total: links,
        snapshot_at: now_ms(),
        note,
        language: (!lang.is_empty()).then_some(lang),
    })
}

/// One link that uses a term (FR-018).
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct TermLink {
    #[ts(type = "number")]
    pub from_id: i64,
    pub from_url: String,
    #[ts(type = "number")]
    pub to_id: i64,
    pub to_url: String,
    /// The full anchor text, or the alt text of an image link.
    pub text: String,
    pub image: bool,
    pub rel: Option<String>,
    /// nofollow, sponsored or ugc on the link, or nofollow on its page.
    pub nofollow: bool,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct TermLinks {
    /// Links using the term; the same as the term's count in the cloud.
    pub total: u32,
    /// The first `limit` of them, in source page order.
    pub links: Vec<TermLink>,
}

/// The links whose anchor text has `term`, under the same rules as the
/// cloud, so `total` is the number the cloud shows.
pub fn term_links(conn: &Connection, state: &ViewState, term: &str, limit: usize) -> Result<TermLinks> {
    let (terming, _) = terming(conn, state)?;
    let mut hits: Vec<Row> = Vec::new();
    let mut total = 0u32;
    each_link(conn, state, |row| {
        if let Some(text) = &row.text
            && terming.terms(text).iter().any(|t| t == term)
        {
            total += 1;
            if hits.len() < limit {
                hits.push(row);
            }
        }
    })?;
    let mut url = conn.prepare_cached("SELECT url FROM urls WHERE id = ?1")?;
    let mut name = |id: i64| -> Result<String> { Ok(url.query_row([id], |r| r.get(0))?) };
    let mut links = Vec::with_capacity(hits.len());
    for h in hits {
        links.push(TermLink {
            from_url: name(h.from_id)?,
            to_url: name(h.to_id)?,
            from_id: h.from_id,
            to_id: h.to_id,
            text: h.text.unwrap_or_default(),
            image: h.image,
            rel: h.rel,
            nofollow: h.nofollow,
        });
    }
    Ok(TermLinks { total, links })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(stop: bool, lang: &str) -> Terming {
        Terming {
            words: true,
            stop: stop.then(|| stop_words(lang)),
        }
    }

    #[test]
    fn words_drop_stop_words_and_count_once_per_link() {
        let t = words(true, "en");
        assert_eq!(
            t.terms("The best Running shoes, running fast"),
            ["best", "running", "shoes", "fast"]
        );
        assert_eq!(words(false, "en").terms("The shoes"), ["the", "shoes"]);
        let he = words(true, "he");
        assert_eq!(he.terms("נעלי ריצה של החנות"), ["נעלי", "ריצה", "החנות"]);
        // An unknown language falls back to English stop words.
        assert_eq!(words(true, "xx").terms("click here"), ["click"]);
    }

    #[test]
    fn whole_anchors_collapse_space_and_case() {
        let t = Terming {
            words: false,
            stop: None,
        };
        assert_eq!(t.terms("  Running\n  Shoes "), ["running shoes"]);
        assert!(t.terms("   ").is_empty());
    }

    #[test]
    fn languages_and_widths() {
        assert_eq!(primary("en-GB"), "en");
        assert_eq!(primary("pt_BR"), "pt");
        assert!(no_spaces("ja") && !no_spaces("he"));
        assert!(text_width("中文", 10.0) > text_width("ab", 10.0));
        assert_eq!(font(5, 5, 5), 24.0);
        assert_eq!(font(1, 1, 100), MIN_FONT);
        assert_eq!(font(100, 1, 100), MAX_FONT);
    }

    #[test]
    fn spiral_places_without_overlap_and_the_same_every_time() {
        let boxes: Vec<(f32, f32)> = (0..100)
            .map(|i| (40.0 + (i % 7) as f32 * 15.0, 14.0 + (i % 3) as f32 * 8.0))
            .collect();
        let a = spiral(&boxes);
        assert_eq!(a, spiral(&boxes));
        assert_eq!(a[0], (0.0, 0.0));
        for i in 0..a.len() {
            for j in 0..i {
                let ri = Rect {
                    x: a[i].0,
                    y: a[i].1,
                    w: boxes[i].0,
                    h: boxes[i].1,
                };
                let rj = Rect {
                    x: a[j].0,
                    y: a[j].1,
                    w: boxes[j].0,
                    h: boxes[j].1,
                };
                assert!(!ri.overlaps(&rj), "{i} overlaps {j}");
            }
        }
    }
}
