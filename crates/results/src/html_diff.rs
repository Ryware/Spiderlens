//! Raw against rendered HTML (spec 014 FR-028, task T042).
//!
//! Both versions are normalised to one tag or text node per line before they
//! are compared, so the differences that remain are differences in content:
//! attribute order, quoting, whitespace and the layout of the markup do not
//! count. Long `<script>` and `<style>` bodies are folded to one line that
//! carries their size and a hash, so a script that is the same on both sides
//! costs one line and a changed one shows as one changed line.
//!
//! The tokenizer is deliberately small and does not repair markup: it shows
//! what the server sent, and what the browser serialised, as written.

use serde::Serialize;
use similar::{Algorithm, DiffTag, capture_diff_slices, group_diff_ops};
use ts_rs::TS;

/// Script and style bodies longer than this many bytes are folded.
const FOLD_OVER: usize = 160;
/// Context lines kept before a hunk, and around it in a unified diff.
const CONTEXT: usize = 2;

/// What a hunk is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum HunkKind {
    /// Only in the rendered HTML.
    Added,
    /// Only in the raw HTML.
    Removed,
    /// Lines on both sides that differ.
    Changed,
}

/// One difference between the normalised versions. Line numbers count from 0.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct DiffHunk {
    pub raw_start: u32,
    pub raw_lines: Vec<String>,
    pub rendered_start: u32,
    pub rendered_lines: Vec<String>,
    pub kind: HunkKind,
    /// Up to two unchanged lines before the hunk.
    pub context_before: Vec<String>,
}

/// The differences of one page, a window of them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct HtmlDiff {
    /// `max_hunks` hunks from `from_hunk`.
    pub hunks: Vec<DiffHunk>,
    /// All hunks of the page.
    pub total_hunks: u32,
    #[ts(type = "number")]
    pub raw_bytes: u64,
    #[ts(type = "number")]
    pub rendered_bytes: u64,
    /// Lines of each normalised version.
    pub raw_lines: u32,
    pub rendered_lines: u32,
}

fn fnv(s: &str) -> u64 {
    let mut h = 0xcbf2_9ce4_8422_2325u64;
    for b in s.as_bytes() {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

fn collapse(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// A tag's attributes sorted by name, values with whitespace collapsed.
fn attributes(src: &str) -> Vec<(String, Option<String>)> {
    let b = src.as_bytes();
    let mut i = 0;
    let mut out = Vec::new();
    while i < b.len() {
        while i < b.len() && (b[i].is_ascii_whitespace() || b[i] == b'/') {
            i += 1;
        }
        let start = i;
        while i < b.len() && !b[i].is_ascii_whitespace() && b[i] != b'=' && b[i] != b'/' {
            i += 1;
        }
        if start == i {
            // A stray character such as a lone '='.
            i += 1;
            continue;
        }
        let name = src[start..i].to_ascii_lowercase();
        while i < b.len() && b[i].is_ascii_whitespace() {
            i += 1;
        }
        let mut value = None;
        if i < b.len() && b[i] == b'=' {
            i += 1;
            while i < b.len() && b[i].is_ascii_whitespace() {
                i += 1;
            }
            if i < b.len() && (b[i] == b'"' || b[i] == b'\'') {
                let quote = b[i];
                let from = i + 1;
                i = from;
                while i < b.len() && b[i] != quote {
                    i += 1;
                }
                value = Some(collapse(&src[from..i]));
                i = (i + 1).min(b.len());
            } else {
                let from = i;
                while i < b.len() && !b[i].is_ascii_whitespace() {
                    i += 1;
                }
                value = Some(collapse(&src[from..i]));
            }
        }
        // `defer` and `defer=""` are the same attribute.
        out.push((name, value.filter(|v| !v.is_empty())));
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

/// The end of a tag that starts at `from` (just after the `<`): the index
/// of its `>`, skipping over quoted values.
fn tag_end(s: &str, from: usize) -> Option<usize> {
    let b = s.as_bytes();
    let mut i = from;
    let mut quote = 0u8;
    while i < b.len() {
        let c = b[i];
        if quote != 0 {
            if c == quote {
                quote = 0;
            }
        } else if c == b'"' || c == b'\'' {
            quote = c;
        } else if c == b'>' {
            return Some(i);
        }
        i += 1;
    }
    None
}

/// Finds `</name` (case-insensitive) at or after `from`.
fn close_at(s: &str, from: usize, name: &str) -> Option<usize> {
    let lower = s[from..].to_ascii_lowercase();
    lower.find(&format!("</{name}")).map(|i| from + i)
}

/// The normalised lines of an HTML document.
pub fn normalise(html: &str) -> Vec<String> {
    let mut lines = Vec::new();
    let b = html.as_bytes();
    let mut i = 0;
    let mut text_from = 0;
    let flush_text = |lines: &mut Vec<String>, from: usize, to: usize| {
        if from < to {
            let t = collapse(&html[from..to]);
            if !t.is_empty() {
                lines.push(t);
            }
        }
    };
    while i < b.len() {
        if b[i] != b'<' {
            i += 1;
            continue;
        }
        let rest = &html[i..];
        if rest.starts_with("<!--") {
            flush_text(&mut lines, text_from, i);
            let end = rest.find("-->").map_or(html.len(), |e| i + e + 3);
            lines.push(collapse(&html[i..end]));
            i = end;
            text_from = i;
        } else if rest.starts_with("<!") || rest.starts_with("<?") {
            flush_text(&mut lines, text_from, i);
            let end = tag_end(html, i + 1).map_or(html.len(), |e| e + 1);
            let mut line = collapse(&html[i..end]);
            if line.get(..9).is_some_and(|p| p.eq_ignore_ascii_case("<!doctype")) {
                line = line.to_ascii_lowercase();
            }
            lines.push(line);
            i = end;
            text_from = i;
        } else if rest.starts_with("</") && rest.as_bytes().get(2).is_some_and(u8::is_ascii_alphabetic) {
            flush_text(&mut lines, text_from, i);
            let close = tag_end(html, i + 2);
            let end = close.map_or(html.len(), |e| e + 1);
            let name: String = html[i + 2..close.unwrap_or(html.len())]
                .split(|c: char| c.is_whitespace() || c == '>')
                .next()
                .unwrap_or_default()
                .to_ascii_lowercase();
            lines.push(format!("</{name}>"));
            i = end;
            text_from = i;
        } else if rest.as_bytes().get(1).is_some_and(u8::is_ascii_alphabetic) {
            flush_text(&mut lines, text_from, i);
            let end = tag_end(html, i + 1);
            let inner_end = end.unwrap_or(html.len());
            let inner = &html[i + 1..inner_end];
            let name_len = inner
                .find(|c: char| c.is_whitespace() || c == '/')
                .unwrap_or(inner.len());
            let name = inner[..name_len].to_ascii_lowercase();
            let mut line = format!("<{name}");
            for (key, value) in attributes(&inner[name_len..]) {
                line.push(' ');
                line.push_str(&key);
                if let Some(v) = value {
                    line.push_str("=\"");
                    line.push_str(&v);
                    line.push('"');
                }
            }
            line.push('>');
            lines.push(line);
            i = end.map_or(html.len(), |e| e + 1);
            text_from = i;
            if name == "script" || name == "style" {
                let close = close_at(html, i, &name).unwrap_or(html.len());
                let body = html[i..close].trim();
                if body.len() > FOLD_OVER {
                    lines.push(format!("/* {} bytes, hash {:016x} */", body.len(), fnv(body)));
                } else if !body.is_empty() {
                    lines.push(collapse(body));
                }
                i = close;
                text_from = i;
            }
        } else {
            i += 1;
        }
    }
    flush_text(&mut lines, text_from, html.len());
    lines
}

/// The hunks between two normalised versions, all of them.
fn hunks_of(raw: &[String], rendered: &[String]) -> Vec<DiffHunk> {
    capture_diff_slices(Algorithm::Myers, raw, rendered)
        .into_iter()
        .filter(|op| op.tag() != DiffTag::Equal)
        .map(|op| {
            let (old, new) = (op.old_range(), op.new_range());
            DiffHunk {
                raw_start: old.start as u32,
                raw_lines: raw[old.clone()].to_vec(),
                rendered_start: new.start as u32,
                rendered_lines: rendered[new].to_vec(),
                kind: match op.tag() {
                    DiffTag::Delete => HunkKind::Removed,
                    DiffTag::Insert => HunkKind::Added,
                    _ => HunkKind::Changed,
                },
                context_before: raw[old.start.saturating_sub(CONTEXT)..old.start].to_vec(),
            }
        })
        .collect()
}

/// Compares two HTML documents: `max_hunks` hunks from hunk `from_hunk`.
pub fn diff(raw: &str, rendered: &str, from_hunk: u32, max_hunks: u32) -> HtmlDiff {
    let (a, b) = (normalise(raw), normalise(rendered));
    let all = hunks_of(&a, &b);
    HtmlDiff {
        total_hunks: all.len() as u32,
        hunks: all
            .into_iter()
            .skip(from_hunk as usize)
            .take(max_hunks as usize)
            .collect(),
        raw_bytes: raw.len() as u64,
        rendered_bytes: rendered.len() as u64,
        raw_lines: a.len() as u32,
        rendered_lines: b.len() as u32,
    }
}

/// The differences as a unified diff of the normalised versions, with
/// `--- raw` and `+++ rendered` headers. Empty when there are none.
pub fn unified(raw: &str, rendered: &str) -> String {
    let (a, b) = (normalise(raw), normalise(rendered));
    let ops = capture_diff_slices(Algorithm::Myers, &a, &b);
    let groups = group_diff_ops(ops, CONTEXT);
    if groups.is_empty() {
        return String::new();
    }
    let mut out = String::from("--- raw\n+++ rendered\n");
    for group in groups {
        let (Some(first), Some(last)) = (group.first(), group.last()) else {
            continue;
        };
        let (old_start, old_end) = (first.old_range().start, last.old_range().end);
        let (new_start, new_end) = (first.new_range().start, last.new_range().end);
        out.push_str(&format!(
            "@@ -{},{} +{},{} @@\n",
            old_start + 1,
            old_end - old_start,
            new_start + 1,
            new_end - new_start
        ));
        for op in &group {
            if matches!(op.tag(), DiffTag::Equal) {
                for line in &a[op.old_range()] {
                    out.push_str(&format!(" {line}\n"));
                }
                continue;
            }
            for line in &a[op.old_range()] {
                out.push_str(&format!("-{line}\n"));
            }
            for line in &b[op.new_range()] {
                out.push_str(&format!("+{line}\n"));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAGE: &str = "<!DOCTYPE html><html><head><title>T</title></head>\
        <body><h1>Hello</h1><p>Some   text</p><div class=\"x\" id=\"y\">D</div></body></html>";

    #[test]
    fn identical_pages_have_no_hunks() {
        let d = diff(PAGE, PAGE, 0, 100);
        assert_eq!(d.total_hunks, 0);
        assert!(d.hunks.is_empty());
        assert_eq!(unified(PAGE, PAGE), "");
    }

    #[test]
    fn attribute_order_alone_is_not_a_difference() {
        let other = PAGE.replace("class=\"x\" id=\"y\"", "id=y   class='x'");
        assert_ne!(PAGE, other);
        assert_eq!(diff(PAGE, &other, 0, 100).total_hunks, 0);
    }

    #[test]
    fn layout_alone_is_not_a_difference() {
        let spaced = PAGE.replace("><", ">\n  <").replace("Some   text", "Some text");
        assert_eq!(diff(PAGE, &spaced, 0, 100).total_hunks, 0);
    }

    #[test]
    fn one_inserted_anchor_is_one_hunk() {
        let more = PAGE.replace("</body>", "<a href=\"/b\">B</a></body>");
        let d = diff(PAGE, &more, 0, 100);
        assert_eq!(d.total_hunks, 1);
        let h = &d.hunks[0];
        assert_eq!(h.kind, HunkKind::Added);
        assert_eq!(h.rendered_lines, ["<a href=\"/b\">", "B", "</a>"]);
        assert!(h.raw_lines.is_empty());
        let u = unified(PAGE, &more);
        assert!(u.starts_with("--- raw\n+++ rendered\n@@ "), "{u}");
        assert!(u.contains("+<a href=\"/b\">\n"), "{u}");
    }

    #[test]
    fn a_changed_title_is_a_changed_hunk() {
        let d = diff(
            PAGE,
            &PAGE.replace("<title>T</title>", "<title>New</title>"),
            0,
            100,
        );
        assert_eq!(d.total_hunks, 1);
        assert_eq!(d.hunks[0].kind, HunkKind::Changed);
        assert_eq!(d.hunks[0].raw_lines, ["T"]);
        assert_eq!(d.hunks[0].rendered_lines, ["New"]);
    }

    #[test]
    fn long_scripts_fold_to_one_line() {
        let long = "x".repeat(500);
        let html = format!("<script>{long}</script>");
        let lines = normalise(&html);
        assert_eq!(lines.len(), 3);
        assert!(lines[1].starts_with("/* 500 bytes, hash "), "{lines:?}");
        // Short ones stay.
        assert_eq!(normalise("<script>var a = 1;</script>")[1], "var a = 1;");
        // A script that contains markup is still one body.
        let tricky = format!("<script>var s = '<b>{long}</b>';</script><p>after</p>");
        assert_eq!(normalise(&tricky).last().unwrap(), "</p>");
    }

    #[test]
    fn windows_of_hunks() {
        let a = "<p>1</p><hr><p>2</p><hr><p>3</p>";
        let b = "<p>one</p><hr><p>2</p><hr><p>three</p>";
        let all = diff(a, b, 0, 10);
        assert_eq!(all.total_hunks, 2);
        let second = diff(a, b, 1, 10);
        assert_eq!(second.total_hunks, 2);
        assert_eq!(second.hunks, all.hunks[1..]);
        assert_eq!(diff(a, b, 0, 1).hunks.len(), 1);
    }
}
