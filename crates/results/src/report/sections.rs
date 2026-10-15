//! The report's sections (plan D2). A section is a value in [`SECTIONS`];
//! later specs add a builder and one line. A builder that cannot run says
//! why instead of disappearing (FR-002).

use crate::issues::{IssuesSummary, Priority, catalogue};
use crate::reader::Reader;
use crate::tabs::{self, HTML_OK, Need};
use crate::view::UrlScope;
use crate::{Error, Result};

use super::{
    Bar, Block, Cell, Figure, Incomplete, IssueBlock, ReasonCount, Score, Section, SectionState, Target,
};

pub(crate) struct Ctx<'a> {
    pub running: bool,
    pub summary: &'a IssuesSummary,
    pub score: &'a Score,
}

pub struct SectionDef {
    pub id: &'static str,
    pub title: &'static str,
    pub need: Need,
    build: fn(&mut Reader, &Ctx) -> Result<Vec<Block>>,
}

/// Every section in report order (FR-003).
pub static SECTIONS: &[SectionDef] = &[
    SectionDef {
        id: "summary",
        title: "Summary",
        need: Need::Base,
        build: summary,
    },
    SectionDef {
        id: "completeness",
        title: "Crawl completeness",
        need: Need::Base,
        build: completeness,
    },
    SectionDef {
        id: "top_issues",
        title: "Top issues",
        need: Need::Base,
        build: top_issues,
    },
    SectionDef {
        id: "issues",
        title: "Issues by priority",
        need: Need::Base,
        build: issues,
    },
    SectionDef {
        id: "response_codes",
        title: "Response codes",
        need: Need::Base,
        build: response_codes,
    },
    SectionDef {
        id: "structure",
        title: "Site structure",
        need: Need::Base,
        build: structure,
    },
    SectionDef {
        id: "content",
        title: "Content",
        need: Need::Extraction,
        build: content,
    },
];

pub(crate) fn build(r: &mut Reader, def: &SectionDef, ctx: &Ctx) -> Section {
    let unavailable = |reason: String| Section {
        id: def.id.into(),
        title: def.title.into(),
        state: SectionState::Unavailable { reason },
        blocks: Vec::new(),
    };
    if !r.features.has(def.need) {
        return unavailable(def.need.unavailable_reason().into());
    }
    match (def.build)(r, ctx) {
        Ok(blocks) => Section {
            id: def.id.into(),
            title: def.title.into(),
            state: if ctx.running {
                SectionState::Provisional
            } else {
                SectionState::Available
            },
            blocks,
        },
        Err(e) => unavailable(format!("This section could not be built: {e}")),
    }
}

fn count(r: &Reader, sql: &str) -> Result<u64> {
    Ok(r.conn.query_row(sql, [], |row| row.get::<_, i64>(0))? as u64)
}

pub fn fmt_int(n: u64) -> String {
    crate::issues::fmt_num(n as f64)
}

pub fn fmt_duration(ms: u64) -> String {
    let s = ms / 1000;
    match (s / 3600, (s % 3600) / 60, s % 60) {
        (0, 0, s) => format!("{s} s"),
        (0, m, s) => format!("{m} min {s} s"),
        (h, m, _) => format!("{h} h {m} min"),
    }
}

/// Figures for some filters of a 007 tab, with the counts the tab shows
/// (FR-005) and a link to each (FR-004).
fn filter_figures(
    r: &mut Reader,
    tab: &str,
    ids: &[&str],
    scope: UrlScope,
    prefix: &str,
) -> Result<Vec<Figure>> {
    let t = tabs::tab(tab).ok_or_else(|| Error::invalid(format!("unknown tab {tab}")))?;
    let counts = r.counts(tab, scope)?;
    let mut out = Vec::new();
    for id in ids {
        let f = t
            .filter(id)
            .ok_or_else(|| Error::invalid(format!("unknown filter {tab}/{id}")))?;
        let Some(Some(n)) = counts.counts.get(*id) else {
            continue;
        };
        let label = counts
            .labels
            .get(*id)
            .cloned()
            .unwrap_or_else(|| f.label.to_string());
        out.push(Figure::new(
            format!("{prefix}{label}"),
            fmt_int(*n),
            Some(Target::scoped(tab, id, scope)),
        ));
    }
    Ok(out)
}

fn summary(r: &mut Reader, ctx: &Ctx) -> Result<Vec<Block>> {
    let found = count(r, "SELECT COUNT(*) FROM urls")?;
    let crawled = count(r, "SELECT COUNT(*) FROM urls WHERE state = 'crawled'")?;
    let not_crawled = count(r, "SELECT COUNT(*) FROM urls WHERE state = 'not_crawled'")?;
    let queued = found - crawled - not_crawled;
    let failed = count(
        r,
        "SELECT COUNT(*) FROM urls WHERE state = 'crawled' AND error_class IS NOT NULL",
    )?;
    let pages = ctx.score.pages;
    let mut items = vec![
        Figure::new(
            "Health score",
            ctx.score
                .value
                .map_or_else(|| "Not available".to_string(), |v| v.to_string()),
            None,
        ),
        Figure::new("URLs found", fmt_int(found), None),
        Figure::new(
            "URLs crawled",
            fmt_int(crawled),
            Some(Target::tab("response-codes", "all")),
        ),
        Figure::new(
            "Pages scored",
            fmt_int(pages),
            Some(Target::tab("page-titles", "all")),
        ),
        Figure::new(
            "Failed (no response)",
            fmt_int(failed),
            Some(Target::tab("response-codes", "no_response")),
        ),
        Figure::new("Not crawled", fmt_int(not_crawled), None),
    ];
    if queued > 0 {
        items.push(Figure::new("Still queued", fmt_int(queued), None));
    }
    if r.features.has(Need::Timing) {
        let avg: Option<f64> = r.conn.query_row(
            "SELECT AVG(response_ms) FROM urls WHERE state = 'crawled' AND response_ms IS NOT NULL",
            [],
            |row| row.get(0),
        )?;
        if let Some(avg) = avg {
            items.push(Figure::new(
                "Average response time",
                format!("{} ms", avg.round() as u64),
                None,
            ));
        }
    }
    let active: u64 = r.meta("active_ms")?.and_then(|v| v.parse().ok()).unwrap_or(0);
    items.push(Figure::new("Crawl time", fmt_duration(active), None));
    Ok(vec![Block::Figures { items }])
}

fn completeness(r: &mut Reader, _ctx: &Ctx) -> Result<Vec<Block>> {
    let mut blocks = Vec::new();
    let mut rows = Vec::new();
    {
        let mut stmt = r.conn.prepare(
            "SELECT error_class, COUNT(*) FROM urls WHERE state = 'crawled' AND error_class IS NOT NULL \
             GROUP BY 1 ORDER BY 2 DESC, 1",
        )?;
        let mut q = stmt.query([])?;
        while let Some(row) = q.next()? {
            let class: String = row.get(0)?;
            let n: i64 = row.get(1)?;
            rows.push(vec![
                Cell {
                    text: crate::sql::error_class_label(&class).to_string(),
                    target: None,
                },
                Cell {
                    text: fmt_int(n as u64),
                    target: Some(Target::tab("response-codes", "no_response")),
                },
            ]);
        }
    }
    if rows.is_empty() {
        blocks.push(Block::Text {
            text: "Every crawled URL received a response.".into(),
        });
    } else {
        blocks.push(Block::Table {
            columns: vec!["Failed, by error class".into(), "URLs".into()],
            rows,
        });
    }
    let reasons = reasons(r)?;
    if !reasons.is_empty() {
        blocks.push(Block::Table {
            columns: vec!["Not crawled, by reason".into(), "URLs".into()],
            rows: reasons
                .into_iter()
                .map(|(code, label, n)| {
                    let target = match code.as_str() {
                        "blocked_by_robots" | "robots_unavailable" => {
                            Some(Target::tab("response-codes", "blocked_robots"))
                        }
                        "excluded" | "not_included" | "outside_start_folder" => {
                            Some(Target::tab("response-codes", "blocked_config"))
                        }
                        _ => None,
                    };
                    vec![
                        Cell {
                            text: label,
                            target: None,
                        },
                        Cell {
                            text: fmt_int(n),
                            target,
                        },
                    ]
                })
                .collect(),
        });
    }
    Ok(blocks)
}

/// Not-crawled URLs by stored reason: (code, label, count).
fn reasons(r: &Reader) -> Result<Vec<(String, String, u64)>> {
    let mut stmt = r.conn.prepare(
        "SELECT reason, COUNT(*) FROM urls WHERE state = 'not_crawled' AND reason IS NOT NULL \
         GROUP BY 1 ORDER BY 2 DESC, 1",
    )?;
    let mut out = Vec::new();
    let mut q = stmt.query([])?;
    while let Some(row) = q.next()? {
        let code: String = row.get(0)?;
        let n: i64 = row.get(1)?;
        let label = spiderlens_engine::crawl::NotCrawledReason::parse(&code)
            .map_or_else(|| code.clone(), |x| x.label().to_string());
        out.push((code, label, n as u64));
    }
    Ok(out)
}

/// What the crawl left undone (FR-007): URLs not crawled because of a
/// limit or a stop, still queued, or failed. Reasons that are the crawl's
/// own choices (external, excluded, nofollow) do not make it incomplete.
pub(crate) fn incomplete(r: &mut Reader) -> Result<Option<Incomplete>> {
    let mut reasons = Vec::new();
    for (code, label, n) in self::reasons(r)? {
        if matches!(
            code.as_str(),
            "over_url_limit" | "over_depth_limit" | "crawl_stopped" | "robots_unavailable"
        ) {
            reasons.push(ReasonCount { label, count: n });
        }
    }
    let queued = count(
        r,
        "SELECT COUNT(*) FROM urls WHERE state NOT IN ('crawled', 'not_crawled')",
    )?;
    if queued > 0 {
        reasons.push(ReasonCount {
            label: "Still queued".into(),
            count: queued,
        });
    }
    let failed = count(
        r,
        "SELECT COUNT(*) FROM urls WHERE state = 'crawled' AND error_class IS NOT NULL",
    )?;
    if failed > 0 {
        reasons.push(ReasonCount {
            label: "Failed (no response)".into(),
            count: failed,
        });
    }
    let status = r.meta("status")?.unwrap_or_default();
    if reasons.is_empty() && status != "stopped" && status != "paused" {
        return Ok(None);
    }
    if reasons.is_empty() {
        reasons.push(ReasonCount {
            label: format!("Crawl {status}"),
            count: 0,
        });
    }
    Ok(Some(Incomplete {
        not_crawled: reasons.iter().map(|x| x.count).sum(),
        reasons,
    }))
}

fn top_issues(_r: &mut Reader, ctx: &Ctx) -> Result<Vec<Block>> {
    let top: Vec<_> = ctx
        .score
        .issues
        .iter()
        .filter(|i| i.gain > 0.0 || i.share > 0.0)
        .take(10)
        .collect();
    if top.is_empty() {
        return Ok(vec![Block::Text {
            text: "No issue lowers the health score.".into(),
        }]);
    }
    Ok(vec![Block::Table {
        columns: vec![
            "Issue".into(),
            "Priority".into(),
            "Pages".into(),
            "Points if fixed".into(),
            "Share of points lost".into(),
        ],
        rows: top
            .into_iter()
            .map(|i| {
                vec![
                    Cell {
                        text: i.name.clone(),
                        target: Some(Target::issue(&i.id)),
                    },
                    Cell {
                        text: i.priority.label().into(),
                        target: None,
                    },
                    Cell {
                        text: fmt_int(i.pages),
                        target: Some(Target::issue(&i.id)),
                    },
                    Cell {
                        text: format!("{:.1}", i.gain),
                        target: None,
                    },
                    Cell {
                        text: format!("{:.1}", i.share),
                        target: None,
                    },
                ]
            })
            .collect(),
    }])
}

fn issues(_r: &mut Reader, ctx: &Ctx) -> Result<Vec<Block>> {
    let mut blocks = vec![Block::Figures {
        items: ctx
            .summary
            .by_priority
            .iter()
            .map(|p| {
                Figure::new(
                    p.priority.label(),
                    format!("{} issues, {} URLs", fmt_int(p.issues as u64), fmt_int(p.urls)),
                    None,
                )
            })
            .collect(),
    }];
    let gains: std::collections::HashMap<&str, f64> =
        ctx.score.issues.iter().map(|i| (i.id.as_str(), i.gain)).collect();
    for p in Priority::ALL {
        for i in ctx
            .summary
            .issues
            .iter()
            .filter(|i| i.priority == p && i.count.unwrap_or(0) > 0)
        {
            let Some(rule) = crate::issues::get(&i.id) else {
                continue;
            };
            blocks.push(Block::Issue {
                issue: IssueBlock {
                    id: i.id.clone(),
                    name: i.name.clone(),
                    category: rule.category.label().into(),
                    priority: i.priority,
                    count: i.count,
                    provisional: i.provisional,
                    unavailable: i.unavailable.clone(),
                    why: rule.why.into(),
                    fix: rule.fix.into(),
                    gain: gains.get(i.id.as_str()).copied().unwrap_or(0.0),
                },
            });
        }
    }
    let not_run: Vec<&str> = ctx
        .summary
        .issues
        .iter()
        .filter(|i| i.enabled && i.unavailable.is_some())
        .map(|i| i.name.as_str())
        .collect();
    if !not_run.is_empty() {
        blocks.push(Block::Text {
            text: format!(
                "Not checked on this crawl file, because the data they need was not collected: {}.",
                not_run.join(", ")
            ),
        });
    }
    let _ = catalogue();
    Ok(blocks)
}

fn response_codes(r: &mut Reader, _ctx: &Ctx) -> Result<Vec<Block>> {
    const IDS: [&str; 6] = [
        "success",
        "redirection",
        "client_error",
        "server_error",
        "no_response",
        "redirect_chain",
    ];
    let t = tabs::tab("response-codes").expect("response codes tab");
    let internal = r.counts("response-codes", UrlScope::Internal)?;
    let external = r.counts("response-codes", UrlScope::External)?;
    let rows = IDS
        .iter()
        .map(|id| {
            let f = t.filter(id).expect("filter");
            let n = |c: &crate::FilterCounts| c.counts.get(*id).copied().flatten().unwrap_or(0);
            vec![
                Cell {
                    text: f.label.clone().into(),
                    target: None,
                },
                Cell {
                    text: fmt_int(n(&internal)),
                    target: Some(Target::scoped("response-codes", id, UrlScope::Internal)),
                },
                Cell {
                    text: fmt_int(n(&external)),
                    target: Some(Target::scoped("response-codes", id, UrlScope::External)),
                },
            ]
        })
        .collect();
    Ok(vec![Block::Table {
        columns: vec!["Response".into(), "Internal".into(), "External".into()],
        rows,
    }])
}

fn structure(r: &mut Reader, _ctx: &Ctx) -> Result<Vec<Block>> {
    let mut depths: Vec<(i64, u64)> = Vec::new();
    {
        let mut stmt = r.conn.prepare(&format!(
            "SELECT u.depth, COUNT(*) FROM urls u WHERE {HTML_OK} GROUP BY 1 ORDER BY 1"
        ))?;
        let mut q = stmt.query([])?;
        while let Some(row) = q.next()? {
            depths.push((row.get(0)?, row.get::<_, i64>(1)? as u64));
        }
    }
    let max = depths.iter().map(|d| d.1).max().unwrap_or(0).max(1);
    let mut blocks = vec![Block::Bars {
        items: depths
            .iter()
            .map(|(d, n)| Bar {
                label: format!("Depth {d}"),
                value: *n,
                share: *n as f64 / max as f64,
                target: None,
            })
            .collect(),
    }];
    let one_inlink = count(
        r,
        &format!(
            "SELECT COUNT(*) FROM urls u WHERE {HTML_OK} AND u.depth > 0 AND \
             (SELECT COUNT(DISTINCT l.from_id) FROM links l WHERE l.to_id = u.id AND l.kind = 'anchor' AND l.from_id <> u.id) <= 1"
        ),
    )?;
    blocks.push(Block::Figures {
        items: vec![
            Figure::new(
                "Deepest level",
                depths.last().map_or(0, |d| d.0).to_string(),
                None,
            ),
            Figure::new("Pages with one inlink or none", fmt_int(one_inlink), None),
        ],
    });
    let mut deepest = Vec::new();
    {
        let mut stmt = r.conn.prepare(&format!(
            "SELECT u.url, u.depth FROM urls u WHERE {HTML_OK} ORDER BY u.depth DESC, u.id LIMIT 10"
        ))?;
        let mut q = stmt.query([])?;
        while let Some(row) = q.next()? {
            let url: String = row.get(0)?;
            let d: i64 = row.get(1)?;
            deepest.push(vec![
                Cell {
                    text: url,
                    target: None,
                },
                Cell {
                    text: d.to_string(),
                    target: None,
                },
            ]);
        }
    }
    if !deepest.is_empty() {
        blocks.push(Block::Table {
            columns: vec!["Deepest pages".into(), "Depth".into()],
            rows: deepest,
        });
    }
    Ok(blocks)
}

fn content(r: &mut Reader, ctx: &Ctx) -> Result<Vec<Block>> {
    let mut items = filter_figures(
        r,
        "page-titles",
        &["missing", "duplicate", "over_60"],
        UrlScope::All,
        "Titles: ",
    )?;
    items.extend(filter_figures(
        r,
        "meta-description",
        &["missing", "duplicate", "over_155"],
        UrlScope::All,
        "Descriptions: ",
    )?);
    items.extend(filter_figures(
        r,
        "h1",
        &["missing", "duplicate", "multiple"],
        UrlScope::All,
        "H1: ",
    )?);
    for id in ["content.duplicate", "content.thin"] {
        if let Some(i) = ctx.summary.issues.iter().find(|i| i.id == id)
            && let Some(n) = i.count
        {
            items.push(Figure::new(i.name.clone(), fmt_int(n), Some(Target::issue(id))));
        }
    }
    Ok(vec![Block::Figures { items }])
}
