//! llms.txt validation and draft generation (spec 015 FR-045 to FR-047,
//! plan D9, research R2).
//!
//! The proposal at llmstxt.org: an H1 site name (the only required element),
//! an optional `>` summary, optional body text without headings, then H2
//! sections of Markdown link lists (`- [name](url): notes`). An H2 named
//! "Optional" marks links an agent can skip.

use serde::Serialize;
use ts_rs::TS;
use url::Url;

/// One problem with its 1-based line number (0 for the whole file).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct LlmsProblem {
    pub line: u32,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct LlmsLink {
    pub line: u32,
    pub section: String,
    pub name: String,
    /// As written.
    pub href: String,
    /// Resolved against the file's URL; `None` when it does not parse.
    pub url: Option<String>,
    pub optional: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct LlmsReport {
    pub title: Option<String>,
    pub summary: Option<String>,
    pub sections: Vec<String>,
    pub links: Vec<LlmsLink>,
    pub problems: Vec<LlmsProblem>,
}

impl LlmsReport {
    pub fn is_valid(&self) -> bool {
        self.problems.is_empty()
    }
}

/// Is a `Content-Type` acceptable for llms.txt (plain text or Markdown)?
pub fn acceptable_content_type(content_type: Option<&str>) -> bool {
    let Some(ct) = content_type else { return true };
    let mime = ct.split(';').next().unwrap_or("").trim().to_ascii_lowercase();
    matches!(
        mime.as_str(),
        "text/plain" | "text/markdown" | "text/x-markdown" | ""
    )
}

/// Checks `text`, served at `file_url`, against the llms.txt proposal.
pub fn validate(text: &str, file_url: &Url, content_type: Option<&str>) -> LlmsReport {
    let mut r = LlmsReport::default();
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    if !acceptable_content_type(content_type) {
        r.problems.push(LlmsProblem {
            line: 0,
            message: format!(
                "Served as {}; llms.txt should be plain text or Markdown",
                content_type.unwrap_or_default()
            ),
        });
    }
    let looks_html = text.trim_start().get(..15).is_some_and(|s| {
        let s = s.to_ascii_lowercase();
        s.starts_with("<!doctype") || s.starts_with("<html")
    });
    if looks_html {
        r.problems.push(LlmsProblem {
            line: 1,
            message: "The file is an HTML page, not Markdown".into(),
        });
        return r;
    }

    #[derive(PartialEq)]
    enum Stage {
        Start,
        Summary,
        Body,
        Sections,
    }
    let mut stage = Stage::Start;
    let mut section = String::new();
    for (i, raw) in text.lines().enumerate() {
        let line_no = u32::try_from(i + 1).unwrap_or(u32::MAX);
        let line = raw.trim_end();
        if line.trim().is_empty() {
            continue;
        }
        if stage == Stage::Start {
            match line.strip_prefix("# ") {
                Some(t) if !t.trim().is_empty() => {
                    r.title = Some(t.trim().to_string());
                    stage = Stage::Summary;
                }
                _ => {
                    r.problems.push(LlmsProblem {
                        line: line_no,
                        message: "The first line must be an H1 title (`# Site name`)".into(),
                    });
                    stage = Stage::Body;
                    // Re-read this line as body or section content below.
                    if let Some(name) = line.strip_prefix("## ") {
                        section = name.trim().to_string();
                        r.sections.push(section.clone());
                        stage = Stage::Sections;
                    }
                }
            }
            continue;
        }
        if let Some(name) = line.strip_prefix("## ") {
            section = name.trim().to_string();
            if section.is_empty() {
                r.problems.push(LlmsProblem {
                    line: line_no,
                    message: "Empty section heading".into(),
                });
            }
            r.sections.push(section.clone());
            stage = Stage::Sections;
            continue;
        }
        if line.starts_with('#') {
            let message = if line.starts_with("# ") {
                "Only one H1 title is allowed"
            } else if stage == Stage::Sections {
                "Sections use H2 headings (`## `); deeper headings are not part of llms.txt"
            } else {
                "Headings before the first section must be the H1 title"
            };
            r.problems.push(LlmsProblem {
                line: line_no,
                message: message.into(),
            });
            continue;
        }
        if stage == Stage::Summary {
            if let Some(q) = line.strip_prefix('>') {
                let q = q.trim();
                r.summary = Some(match r.summary.take() {
                    Some(prev) => format!("{prev} {q}"),
                    None => q.to_string(),
                });
                continue;
            }
            stage = Stage::Body;
        }
        if stage == Stage::Body {
            continue;
        }
        // Inside a section: every list item must be a link.
        let item = line.trim_start();
        if let Some(rest) = item.strip_prefix("- ").or_else(|| item.strip_prefix("* ")) {
            match parse_link(rest.trim()) {
                Some((name, href)) => {
                    let url = file_url
                        .join(href)
                        .ok()
                        .filter(|u| matches!(u.scheme(), "http" | "https"));
                    if url.is_none() {
                        r.problems.push(LlmsProblem {
                            line: line_no,
                            message: format!("Link target {href:?} is not a web address"),
                        });
                    }
                    r.links.push(LlmsLink {
                        line: line_no,
                        section: section.clone(),
                        name: name.to_string(),
                        href: href.to_string(),
                        url: url.map(String::from),
                        optional: section.eq_ignore_ascii_case("optional"),
                    });
                }
                None => r.problems.push(LlmsProblem {
                    line: line_no,
                    message: "List items in a section must be links: `- [name](url)` with optional `: notes`"
                        .into(),
                }),
            }
        }
    }
    if r.title.is_none() && !r.problems.iter().any(|p| p.message.starts_with("The first line")) {
        r.problems.push(LlmsProblem {
            line: 0,
            message: "The file is empty; an H1 title is required".into(),
        });
    }
    r
}

/// `[name](url)` at the start of a list item, then optional `: notes`.
fn parse_link(item: &str) -> Option<(&str, &str)> {
    let rest = item.strip_prefix('[')?;
    let close = rest.find("](")?;
    let name = &rest[..close];
    let after = &rest[close + 2..];
    let end = after.find(')')?;
    let href = after[..end].trim();
    let tail = after[end + 1..].trim_start();
    if name.trim().is_empty() || href.is_empty() || !(tail.is_empty() || tail.starts_with(':')) {
        return None;
    }
    Some((name.trim(), href))
}

/// One page offered to the draft generator (FR-047).
#[derive(Debug, Clone)]
pub struct DraftPage {
    pub url: Url,
    pub title: String,
    pub description: Option<String>,
    /// Internal links pointing at the page; more first.
    pub inlinks: u64,
}

/// Builds a draft llms.txt: H1 site name, summary, one H2 section per first
/// path segment (the home page and top-level pages under "Main pages"),
/// up to `max_links` links, most linked-to first. Deterministic.
pub fn draft(site_name: &str, summary: Option<&str>, pages: &[DraftPage], max_links: usize) -> String {
    let mut chosen: Vec<&DraftPage> = pages.iter().collect();
    chosen.sort_by(|a, b| {
        b.inlinks
            .cmp(&a.inlinks)
            .then_with(|| a.url.as_str().cmp(b.url.as_str()))
    });
    chosen.truncate(max_links);

    let mut sections: std::collections::BTreeMap<String, Vec<&DraftPage>> = Default::default();
    for p in chosen {
        let mut segs = p
            .url
            .path_segments()
            .into_iter()
            .flatten()
            .filter(|s| !s.is_empty());
        let first = segs.next();
        let key = match (first, segs.next()) {
            (Some(seg), Some(_)) => section_title(seg),
            _ => String::new(),
        };
        sections.entry(key).or_default().push(p);
    }

    let mut out = format!("# {}\n", one_line(site_name));
    if let Some(s) = summary.map(one_line).filter(|s| !s.is_empty()) {
        out.push_str(&format!("\n> {s}\n"));
    }
    for (key, list) in &sections {
        let title = if key.is_empty() {
            "Main pages"
        } else {
            key.as_str()
        };
        out.push_str(&format!("\n## {title}\n\n"));
        for p in list {
            let name = one_line(&p.title);
            let name = if name.is_empty() {
                p.url.as_str().to_string()
            } else {
                name.replace(['[', ']'], "")
            };
            match p.description.as_deref().map(one_line).filter(|d| !d.is_empty()) {
                Some(d) => out.push_str(&format!("- [{name}]({}): {d}\n", p.url)),
                None => out.push_str(&format!("- [{name}]({})\n", p.url)),
            }
        }
    }
    out
}

fn one_line(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// `getting-started` → `Getting started`.
fn section_title(segment: &str) -> String {
    let words = segment.replace(['-', '_'], " ");
    let mut c = words.chars();
    match c.next() {
        Some(f) => f.to_uppercase().chain(c).collect(),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> Url {
        Url::parse("https://a.test/llms.txt").unwrap()
    }

    fn check(text: &str) -> LlmsReport {
        validate(text, &base(), Some("text/plain; charset=utf-8"))
    }

    const VALID: &str = "# Acme\n\n> Tools for makers.\n> Since 1999.\n\nSome notes about the site.\n\n## Docs\n\n- [Start](/docs/start): First steps\n- [API](https://a.test/api)\n\n## Optional\n\n- [Blog](/blog)\n";

    #[test]
    fn valid_file() {
        let r = check(VALID);
        assert!(r.is_valid(), "{:?}", r.problems);
        assert_eq!(r.title.as_deref(), Some("Acme"));
        assert_eq!(r.summary.as_deref(), Some("Tools for makers. Since 1999."));
        assert_eq!(r.sections, ["Docs", "Optional"]);
        assert_eq!(r.links.len(), 3);
        assert_eq!(r.links[0].url.as_deref(), Some("https://a.test/docs/start"));
        assert_eq!(r.links[0].line, 10);
        assert!(!r.links[0].optional);
        assert!(r.links[2].optional);
    }

    #[test]
    fn bom_is_ignored() {
        assert!(check(&format!("\u{feff}{VALID}")).is_valid());
    }

    #[test]
    fn missing_title() {
        let r = check("## Docs\n\n- [Start](/docs/start)\n");
        assert_eq!(r.problems[0].line, 1);
        assert!(r.problems[0].message.contains("H1"));
        assert_eq!(r.links.len(), 1, "links are still listed");
    }

    #[test]
    fn empty_file() {
        let r = check("\n\n");
        assert_eq!(r.problems.len(), 1);
        assert_eq!(r.problems[0].line, 0);
    }

    #[test]
    fn html_page() {
        let r = validate(
            "<!DOCTYPE html><html><body>Not found</body></html>",
            &base(),
            Some("text/html"),
        );
        assert_eq!(r.problems.len(), 2);
        assert!(r.problems[1].message.contains("HTML page"));
    }

    #[test]
    fn bad_items_and_headings() {
        let r = check(
            "# A\n\n## Docs\n\n- Start here\n- [x](mailto:a@b.test)\n### Deep\n# Second\n- [y](/y) trailing\n",
        );
        let lines: Vec<_> = r.problems.iter().map(|p| p.line).collect();
        assert_eq!(lines, [5, 6, 7, 8, 9]);
        assert_eq!(r.links.len(), 1, "mailto is listed with its problem");
    }

    #[test]
    fn content_types() {
        assert!(acceptable_content_type(Some("text/markdown")));
        assert!(acceptable_content_type(None));
        assert!(!acceptable_content_type(Some("application/json")));
    }

    #[test]
    fn draft_groups_by_folder_and_validates() {
        let page = |u: &str, t: &str, d: Option<&str>, n| DraftPage {
            url: Url::parse(u).unwrap(),
            title: t.into(),
            description: d.map(Into::into),
            inlinks: n,
        };
        let pages = [
            page("https://a.test/", "Acme home", None, 50),
            page(
                "https://a.test/docs/start",
                "Getting  started",
                Some("First\nsteps"),
                20,
            ),
            page("https://a.test/docs/api", "API [v2]", None, 30),
            page("https://a.test/blog-posts/hello", "Hello", None, 1),
        ];
        let text = draft("Acme", Some("Tools for makers."), &pages, 3);
        assert_eq!(
            text,
            "# Acme\n\n> Tools for makers.\n\n## Main pages\n\n- [Acme home](https://a.test/)\n\n## Docs\n\n- [API v2](https://a.test/docs/api)\n- [Getting started](https://a.test/docs/start): First steps\n"
        );
        assert!(check(&text).is_valid());
    }
}
