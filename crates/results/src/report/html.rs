//! The report as one self-contained HTML file (spec 010 FR-013 to FR-017,
//! plan D6).
//!
//! No scripts and no external resource of any kind: styles are inline and
//! bars are inline SVG, so the file opens offline anywhere and sends
//! nothing. Every piece of crawl text goes through [`esc`]. Colours are the
//! light theme of the app's design tokens, read from `tokens.css` itself so
//! the report never drifts from the design system.

use std::fmt::Write;

use serde::Serialize;
use ts_rs::TS;

use super::{Block, Cell, Figure, Report, Section, SectionState};
use crate::issues::Priority;

/// The app's design tokens; the light theme is the first `:root` block.
const TOKENS: &str = include_str!("../../../../ui/src/styles/tokens.css");

pub const URLS_PER_ISSUE_DEFAULT: u32 = 100;
pub const URLS_PER_ISSUE_MAX: u32 = 10_000;

/// What the user sets for the report file (FR-015, FR-016).
#[derive(Debug, Clone, Serialize, serde::Deserialize, TS)]
#[ts(export)]
pub struct FileOptions {
    pub title: String,
    pub note: String,
    pub urls_per_issue: u32,
}

impl Default for FileOptions {
    fn default() -> Self {
        Self {
            title: String::new(),
            note: String::new(),
            urls_per_issue: URLS_PER_ISSUE_DEFAULT,
        }
    }
}

impl FileOptions {
    pub fn validate(&self) -> Result<(), String> {
        if self.urls_per_issue > URLS_PER_ISSUE_MAX {
            return Err(format!(
                "URLs per issue must be between 0 and {URLS_PER_ISSUE_MAX}"
            ));
        }
        if self.title.chars().count() > 200 {
            return Err("The report title can be at most 200 characters".into());
        }
        if self.note.chars().count() > 5_000 {
            return Err("The note can be at most 5,000 characters".into());
        }
        Ok(())
    }
}

/// The affected URLs of one issue, as many as the file shows, and the total.
#[derive(Debug, Clone, Default)]
pub struct IssueUrls {
    pub urls: Vec<(String, Option<String>)>,
    pub total: u64,
}

/// Escapes text for HTML element content and quoted attribute values.
pub fn esc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c => out.push(c),
        }
    }
    out
}

/// The light theme's custom properties, without anything that loads a
/// resource (FR-014).
pub fn light_tokens() -> String {
    let start = TOKENS.find(":root {").expect("tokens.css has a :root block");
    let body = &TOKENS[start + ":root {".len()..];
    let end = body.find("\n}").expect(":root block ends");
    body[..end]
        .lines()
        .map(str::trim)
        .filter(|l| l.starts_with("--") && !l.contains("url(") && !l.contains("@import"))
        .collect::<Vec<_>>()
        .join("\n")
}

const STYLE: &str = r#"
*{box-sizing:border-box}
html{background:var(--bg);color:var(--fg);font:14px/1.5 var(--font-ui)}
body{margin:0 auto;max-width:960px;padding:32px 24px}
h1{font-size:26px;margin:0 0 4px}
h2{font-size:18px;margin:32px 0 12px;padding-bottom:6px;border-bottom:1px solid var(--line);break-after:avoid}
h3{font-size:15px;margin:16px 0 4px;break-after:avoid}
p{margin:6px 0}
.meta{color:var(--fg-2);font-size:12px}
.note{background:var(--surface-2);border-radius:var(--radius-lg);padding:12px 16px;margin:16px 0;white-space:pre-wrap}
.banner{border:1px solid var(--warn);background:var(--warn-soft);border-radius:var(--radius-lg);padding:10px 14px;margin:16px 0}
.score{display:flex;gap:24px;align-items:center;margin:20px 0;break-inside:avoid}
.dial{font-size:48px;font-weight:700;line-height:1;min-width:110px;text-align:center;padding:16px;border-radius:var(--radius-xl);border:2px solid currentColor}
.good{color:var(--ok)}.fair{color:var(--warn)}.poor{color:var(--bad)}
.figs{display:grid;grid-template-columns:repeat(auto-fill,minmax(200px,1fr));gap:8px;margin:8px 0;break-inside:avoid}
.fig{background:var(--surface);border:1px solid var(--line);border-radius:var(--radius-md);padding:8px 12px}
.fig b{display:block;font-size:18px}
.fig span{color:var(--fg-2);font-size:12px}
table{border-collapse:collapse;width:100%;margin:8px 0;font-size:13px}
thead{display:table-header-group}
th,td{text-align:start;padding:5px 8px;border-bottom:1px solid var(--line);vertical-align:top}
th{color:var(--fg-2);font-weight:600}
td.n{text-align:end;font-variant-numeric:tabular-nums}
tr{break-inside:avoid}
.issue{border:1px solid var(--line);border-radius:var(--radius-md);padding:10px 14px;margin:10px 0;background:var(--surface)}
.badge{display:inline-block;font-size:11px;font-weight:600;padding:1px 8px;border-radius:var(--radius-full);border:1px solid currentColor;margin-inline-end:8px}
.p-high{color:var(--bad)}.p-medium{color:var(--warn)}.p-low{color:var(--info)}.p-opportunity{color:var(--ok)}
.urls{font-family:var(--font-code);font-size:12px;margin:6px 0 0;padding-inline-start:18px;overflow-wrap:anywhere}
.muted{color:var(--fg-3)}
svg.bars{width:100%}
svg.bars rect{fill:var(--accent)}
svg.bars text{fill:var(--fg);font-size:12px}
@page{margin:16mm 14mm}
@media print{html{background:var(--surface)}body{padding:0;max-width:none}.fig,.issue{break-inside:avoid}}
"#;

fn band(v: u8) -> (&'static str, &'static str) {
    match v {
        90..=100 => ("good", "Good"),
        70..=89 => ("fair", "Needs work"),
        _ => ("poor", "Poor"),
    }
}

/// Renders `report` with the affected URLs `urls_of` returns per issue id.
pub fn render(report: &Report, opts: &FileOptions, mut urls_of: impl FnMut(&str) -> IssueUrls) -> String {
    let mut h = String::with_capacity(64 * 1024);
    let title = if opts.title.trim().is_empty() {
        match &report.meta.start_url {
            Some(u) => format!("Crawl report: {u}"),
            None => "Crawl report".to_string(),
        }
    } else {
        opts.title.trim().to_string()
    };
    let _ = write!(
        h,
        "<!doctype html>\n<html lang=\"en\"><head><meta charset=\"utf-8\">\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\
         <meta name=\"generator\" content=\"SpiderLens {}\">\
         <title>{}</title><style>:root{{{}}}{}</style></head><body>",
        esc(&report.meta.app_version),
        esc(&title),
        light_tokens(),
        STYLE
    );
    let _ = write!(h, "<h1 dir=\"auto\">{}</h1>", esc(&title));
    let m = &report.meta;
    let _ = write!(
        h,
        "<p class=\"meta\">{}Crawl {} · {} · SpiderLens {} · rule set v{} · score formula v{}</p>",
        m.start_url
            .as_ref()
            .map(|u| format!("<span dir=\"ltr\">{}</span> · ", esc(u)))
            .unwrap_or_default(),
        esc(&m.status),
        esc(&super::sections::fmt_duration(m.active_ms)),
        esc(&m.app_version),
        m.rules_version,
        m.score_version,
    );
    if report.provisional {
        h.push_str("<div class=\"banner\"><b>Provisional.</b> The crawl was still running when this report was made; figures can change.</div>");
    }
    if let Some(inc) = &report.incomplete {
        let reasons: Vec<String> = inc
            .reasons
            .iter()
            .map(|r| format!("{} ({})", esc(&r.label), super::sections::fmt_int(r.count)))
            .collect();
        let _ = write!(
            h,
            "<div class=\"banner\"><b>Incomplete crawl.</b> Not everything found was crawled: {}. The score is partial.</div>",
            reasons.join(", ")
        );
    }
    if m.custom_issue_settings {
        h.push_str("<p class=\"meta\">Custom issue settings were used: some rules are turned off or have changed priorities or thresholds.</p>");
    }
    if !opts.note.trim().is_empty() {
        let _ = write!(
            h,
            "<div class=\"note\" dir=\"auto\">{}</div>",
            esc(opts.note.trim())
        );
    }
    score(&mut h, report);
    for s in &report.sections {
        section(&mut h, s, opts, &mut urls_of);
    }
    h.push_str("</body></html>\n");
    h
}

fn score(h: &mut String, report: &Report) {
    let s = &report.score;
    h.push_str("<h2>Health score</h2><div class=\"score\">");
    match s.value {
        Some(v) => {
            let (class, word) = band(v);
            let _ = write!(
                h,
                "<div class=\"dial {class}\" aria-label=\"Health score {v} of 100\">{v}</div><div><b>{word}</b>{}<p>{} pages scored. Each page loses points for its worst issue: High 1, Medium 0.5, Low 0.2, Opportunity 0.</p></div>",
                if s.partial || report.incomplete.is_some() {
                    " <span class=\"muted\">(partial)</span>"
                } else {
                    ""
                },
                super::sections::fmt_int(s.pages)
            );
        }
        None => {
            let _ = write!(
                h,
                "<div class=\"dial muted\">–</div><p>{}</p>",
                esc(s.unavailable.as_deref().unwrap_or("Not available"))
            );
        }
    }
    h.push_str("</div>");
    if s.value.is_some() {
        h.push_str(
            "<table><thead><tr><th>Priority</th><th>Pages</th><th>Points lost</th></tr></thead><tbody>",
        );
        for p in &s.by_priority {
            let _ = write!(
                h,
                "<tr><td>{}</td><td class=\"n\">{}</td><td class=\"n\">{:.1}</td></tr>",
                p.priority.label(),
                super::sections::fmt_int(p.pages),
                p.points
            );
        }
        h.push_str("</tbody></table>");
    }
    if !s.not_run.is_empty() {
        let _ = write!(
            h,
            "<p class=\"muted\">Not checked on this crawl file: {}.</p>",
            esc(&s.not_run.join(", "))
        );
    }
}

fn prio_class(p: Priority) -> &'static str {
    match p {
        Priority::High => "p-high",
        Priority::Medium => "p-medium",
        Priority::Low => "p-low",
        Priority::Opportunity => "p-opportunity",
    }
}

fn section(h: &mut String, s: &Section, opts: &FileOptions, urls_of: &mut impl FnMut(&str) -> IssueUrls) {
    let _ = write!(h, "<section id=\"{}\"><h2>{}</h2>", esc(&s.id), esc(&s.title));
    match &s.state {
        SectionState::Unavailable { reason } => {
            let _ = write!(
                h,
                "<p class=\"muted\">Not available: {}</p></section>",
                esc(reason)
            );
            return;
        }
        SectionState::Provisional => {
            h.push_str("<p class=\"muted\">Provisional: the crawl was still running.</p>")
        }
        SectionState::Available => {}
    }
    for b in &s.blocks {
        match b {
            Block::Figures { items } => figures(h, items),
            Block::Table { columns, rows } => table(h, columns, rows),
            Block::Bars { items } => {
                let row = 22;
                let height = row * items.len().max(1);
                let _ = write!(
                    h,
                    "<svg class=\"bars\" viewBox=\"0 0 600 {height}\" role=\"img\" aria-label=\"{}\">",
                    esc(&s.title)
                );
                for (i, b) in items.iter().enumerate() {
                    let y = i * row;
                    let w = (b.share.clamp(0.0, 1.0) * 380.0).max(1.0);
                    let _ = write!(
                        h,
                        "<text x=\"0\" y=\"{}\">{}</text><rect x=\"110\" y=\"{}\" width=\"{w:.1}\" height=\"14\" rx=\"3\"/><text x=\"{:.1}\" y=\"{}\">{}</text>",
                        y + 15,
                        esc(&b.label),
                        y + 4,
                        116.0 + w,
                        y + 15,
                        super::sections::fmt_int(b.value)
                    );
                }
                h.push_str("</svg>");
            }
            Block::Text { text } => {
                let _ = write!(h, "<p dir=\"auto\">{}</p>", esc(text));
            }
            Block::Issue { issue } => {
                let _ = write!(
                    h,
                    "<div class=\"issue\"><h3 dir=\"auto\"><span class=\"badge {}\">{}</span>{}</h3><p class=\"meta\">{} · {} URLs{}{}</p><p><b>Why it matters.</b> {}</p><p><b>How to fix.</b> {}</p>",
                    prio_class(issue.priority),
                    issue.priority.label(),
                    esc(&issue.name),
                    esc(&issue.category),
                    issue.count.map_or_else(|| "–".into(), super::sections::fmt_int),
                    if issue.gain > 0.0 {
                        format!(" · {:.1} points if fixed", issue.gain)
                    } else {
                        String::new()
                    },
                    if issue.provisional { " · provisional" } else { "" },
                    esc(&issue.why),
                    esc(&issue.fix),
                );
                if opts.urls_per_issue > 0 {
                    let list = urls_of(&issue.id);
                    if !list.urls.is_empty() {
                        h.push_str("<ol class=\"urls\">");
                        for (u, ev) in &list.urls {
                            let _ = write!(
                                h,
                                "<li><span dir=\"ltr\">{}</span>{}</li>",
                                esc(u),
                                ev.as_ref()
                                    .filter(|e| !e.is_empty())
                                    .map(|e| format!(
                                        " <span class=\"muted\" dir=\"auto\">({})</span>",
                                        esc(e)
                                    ))
                                    .unwrap_or_default()
                            );
                        }
                        h.push_str("</ol>");
                        let shown = list.urls.len() as u64;
                        if list.total > shown {
                            let _ = write!(
                                h,
                                "<p class=\"muted\">And {} more. The full list is in the issues export.</p>",
                                super::sections::fmt_int(list.total - shown)
                            );
                        }
                    }
                }
                h.push_str("</div>");
            }
        }
    }
    h.push_str("</section>");
}

fn figures(h: &mut String, items: &[Figure]) {
    h.push_str("<div class=\"figs\">");
    for f in items {
        let _ = write!(
            h,
            "<div class=\"fig\"><b dir=\"auto\">{}</b><span dir=\"auto\">{}</span></div>",
            esc(&f.value),
            esc(&f.label)
        );
    }
    h.push_str("</div>");
}

fn table(h: &mut String, columns: &[String], rows: &[Vec<Cell>]) {
    h.push_str("<table><thead><tr>");
    for c in columns {
        let _ = write!(h, "<th>{}</th>", esc(c));
    }
    h.push_str("</tr></thead><tbody>");
    for r in rows {
        h.push_str("<tr>");
        for c in r {
            let numeric = c
                .text
                .chars()
                .all(|ch| ch.is_ascii_digit() || ch == ',' || ch == '.');
            let _ = write!(
                h,
                "<td{} dir=\"auto\">{}</td>",
                if numeric && !c.text.is_empty() {
                    " class=\"n\""
                } else {
                    ""
                },
                esc(&c.text)
            );
        }
        h.push_str("</tr>");
    }
    h.push_str("</tbody></table>");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_everything_markup_could_hide_in() {
        assert_eq!(
            esc("<script>alert('x')</script> & \"q\""),
            "&lt;script&gt;alert(&#39;x&#39;)&lt;/script&gt; &amp; &quot;q&quot;"
        );
    }

    #[test]
    fn light_tokens_are_plain_custom_properties() {
        let t = light_tokens();
        assert!(t.contains("--bg:"));
        assert!(t.contains("--fg:"));
        assert!(!t.contains("url("));
        assert!(t.lines().all(|l| l.starts_with("--")));
    }

    #[test]
    fn options_are_validated() {
        assert!(FileOptions::default().validate().is_ok());
        let o = FileOptions {
            urls_per_issue: URLS_PER_ISSUE_MAX + 1,
            ..FileOptions::default()
        };
        assert!(o.validate().is_err());
    }
}
