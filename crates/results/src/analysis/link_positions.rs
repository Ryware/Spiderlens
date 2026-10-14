//! Link positions (spec 023 US4, plan D8): where on its page each body link
//! sits, worked out after the crawl from the stored HTML.
//!
//! Each page is parsed again with the same decoding and walk as extraction,
//! so the n-th reference found is the link stored with `pos = n`. A link is
//! classified by its ancestors: the user's rules first, in order, then the
//! nearest landmark element (`nav`, `header`, `aside`, `footer` and their
//! roles), Content otherwise. Pages are processed on every core in batches
//! read by id, and written in id order, so the result does not depend on
//! thread timing.

use ego_tree::NodeId;
use rusqlite::{Connection, OptionalExtension, params};
use scraper::{ElementRef, Html, Selector};
use serde::Serialize;
use spiderlens_config::{Position, PositionRule};
use spiderlens_engine::crawl::LinkKind;
use spiderlens_engine::extract::{PageInput, Ref, RefSource, body, parse_page};
use ts_rs::TS;
use url::Url;

/// Pages read from the file per round.
const BATCH: usize = 512;

/// Stored for a link on a page whose HTML was not kept.
pub const UNKNOWN: u8 = 0;

/// The user's rules, compiled once per run.
pub struct Rules(Vec<(Selector, Position)>);

impl Rules {
    /// Rules with an invalid selector are skipped; Settings rejects them
    /// before they get here.
    pub fn compile(rules: &[PositionRule]) -> Self {
        Self(
            rules
                .iter()
                .filter_map(|r| Selector::parse(&r.selector).ok().map(|s| (s, r.position)))
                .collect(),
        )
    }
}

/// One classified link.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Classified {
    pub pos: u32,
    pub position: Position,
    /// Index of the custom rule that matched; None = built-in rule.
    pub rule: Option<u32>,
}

/// What a run looked at.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Stats {
    pub pages: u64,
    pub links: u64,
    /// Internal HTML pages with no stored HTML: their links are Unknown.
    pub pages_without_html: u64,
}

/// Classifies every body link (`a` and `area` with an href) of one page.
pub fn classify_page(url: &Url, encoding: Option<&str>, html: &[u8], rules: &Rules) -> Vec<Classified> {
    links_of(url, encoding, html, rules, |pos, _, position, rule| Classified {
        pos,
        position,
        rule,
    })
}

/// Parses one page and maps each body link, with its `pos`, position and
/// matching rule.
fn links_of<T>(
    url: &Url,
    encoding: Option<&str>,
    html: &[u8],
    rules: &Rules,
    f: impl Fn(u32, &Ref, Position, Option<u32>) -> T,
) -> Vec<T> {
    let input = PageInput {
        url,
        charset: encoding,
        body: html,
        truncated: false,
        robots_token: "",
        // Positions only need the links, not the accessibility checks.
        a11y: false,
    };
    let Some(page) = parse_page(&input) else {
        return Vec::new();
    };
    page.refs
        .iter()
        .zip(&page.nodes)
        .enumerate()
        .filter(|(_, (r, _))| r.kind == LinkKind::Anchor && r.source == RefSource::Html)
        .map(|(pos, (r, &node))| {
            let (position, rule) = classify(&page.doc, node, rules);
            f(pos as u32, r, position, rule)
        })
        .collect()
}

fn classify(doc: &Html, node: NodeId, rules: &Rules) -> (Position, Option<u32>) {
    let Some(link) = doc.tree.get(node).and_then(ElementRef::wrap) else {
        return (Position::Content, None);
    };
    // The link and its ancestors, innermost first.
    let chain: Vec<ElementRef<'_>> = std::iter::once(link)
        .chain(link.ancestors().filter_map(ElementRef::wrap))
        .collect();
    for (i, (selector, position)) in rules.0.iter().enumerate() {
        if chain.iter().any(|e| selector.matches(e)) {
            return (*position, Some(i as u32));
        }
    }
    for (i, e) in chain.iter().enumerate() {
        if let Some(p) = landmark(e, &chain[i + 1..]) {
            return (p, None);
        }
    }
    (Position::Content, None)
}

/// The landmark an element is, if any (HTML-AAM): a `header` or `footer`
/// inside sectioning content is not the page's banner or content info.
fn landmark(e: &ElementRef<'_>, outer: &[ElementRef<'_>]) -> Option<Position> {
    let el = e.value();
    let role = el.attr("role").map(|r| r.trim().to_ascii_lowercase());
    match role.as_deref() {
        Some("navigation") => return Some(Position::Navigation),
        Some("banner") => return Some(Position::Header),
        Some("complementary") => return Some(Position::Sidebar),
        Some("contentinfo") => return Some(Position::Footer),
        Some("main") => return Some(Position::Content),
        _ => {}
    }
    let sectioned = || {
        outer
            .iter()
            .any(|o| matches!(o.value().name(), "article" | "aside" | "main" | "nav" | "section"))
    };
    match el.name() {
        "nav" => Some(Position::Navigation),
        "aside" => Some(Position::Sidebar),
        "main" | "article" => Some(Position::Content),
        "header" if !sectioned() => Some(Position::Header),
        "footer" if !sectioned() => Some(Position::Footer),
        _ => None,
    }
}

struct Page {
    id: i64,
    url: String,
    encoding: Option<String>,
    size: i64,
    body: Option<Vec<u8>>,
}

/// The columns [`Page::from_row`] reads, from `urls u`, `pages p` and
/// `bodies b`.
const PAGE_COLUMNS: &str = "u.id, u.url, p.encoding, b.codec, b.size, b.body";

impl Page {
    fn from_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Self> {
        let codec: Option<String> = r.get(3)?;
        Ok(Self {
            id: r.get(0)?,
            url: r.get(1)?,
            encoding: r.get(2)?,
            size: r.get::<_, Option<i64>>(4)?.unwrap_or(0),
            body: if codec.as_deref() == Some(body::CODEC) {
                r.get(5)?
            } else {
                None
            },
        })
    }

    /// The decompressed HTML; None when not stored or unreadable.
    fn html(&self) -> Option<(Url, Vec<u8>)> {
        let url = Url::parse(&self.url).ok()?;
        let html = body::decompress(self.body.as_ref()?, self.size as usize).ok()?;
        Some((url, html))
    }
}

/// Builds `temp.link_positions` (data model) for every internal HTML page.
pub fn build(conn: &Connection, rules: &[PositionRule]) -> rusqlite::Result<Stats> {
    let rules = Rules::compile(rules);
    conn.execute_batch(
        "DROP TABLE IF EXISTS temp.link_positions;
         CREATE TEMP TABLE link_positions (
           from_id INTEGER NOT NULL, pos INTEGER NOT NULL, position INTEGER NOT NULL, rule INTEGER,
           PRIMARY KEY (from_id, pos)) WITHOUT ROWID;",
    )?;
    let threads = std::thread::available_parallelism().map_or(1, usize::from);
    let mut stats = Stats::default();
    // NOT INDEXED: walk by id; the state index would rescan every batch.
    let mut select = conn.prepare(&format!(
        "SELECT {PAGE_COLUMNS}
         FROM urls u NOT INDEXED LEFT JOIN pages p ON p.url_id = u.id LEFT JOIN bodies b ON b.url_id = u.id
         WHERE {} AND u.id > ?1 ORDER BY u.id LIMIT {BATCH}",
        crate::tabs::HTML_OK
    ))?;
    let mut last = 0i64;
    // Batch k is parsed on the worker threads while this thread writes
    // batch k - 1.
    let mut done: Option<(Vec<Page>, Vec<Vec<Classified>>)> = None;
    loop {
        let pages = select
            .query_map([last], Page::from_row)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        if let Some(end) = pages.last() {
            last = end.id;
        }
        let chunk = pages.len().div_ceil(threads).max(1);
        let results = std::thread::scope(|s| -> rusqlite::Result<Vec<Vec<Classified>>> {
            let handles: Vec<_> = pages
                .chunks(chunk)
                .map(|part| s.spawn(|| part.iter().map(|p| page_positions(p, &rules)).collect::<Vec<_>>()))
                .collect();
            if let Some((pages, results)) = done.take() {
                write(conn, &pages, &results, &mut stats)?;
            }
            Ok(handles
                .into_iter()
                .flat_map(|h| h.join().expect("classifier threads do not panic"))
                .collect())
        })?;
        if pages.is_empty() {
            break;
        }
        done = Some((pages, results));
    }
    Ok(stats)
}

fn write(
    conn: &Connection,
    pages: &[Page],
    results: &[Vec<Classified>],
    stats: &mut Stats,
) -> rusqlite::Result<()> {
    let tx = conn.unchecked_transaction()?;
    {
        let mut ins =
            tx.prepare_cached("INSERT OR IGNORE INTO temp.link_positions VALUES (?1, ?2, ?3, ?4)")?;
        for (page, links) in pages.iter().zip(results) {
            stats.pages += 1;
            if page.body.is_none() {
                stats.pages_without_html += 1;
            }
            for c in links {
                stats.links += 1;
                ins.execute(params![page.id, c.pos, c.position.code(), c.rule])?;
            }
        }
    }
    tx.commit()
}

fn page_positions(p: &Page, rules: &Rules) -> Vec<Classified> {
    let Some((url, html)) = p.html() else {
        return Vec::new();
    };
    // A parser panic on one page must not take the analysis down.
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        classify_page(&url, p.encoding.as_deref(), &html, rules)
    }))
    .unwrap_or_default()
}

/// One link in the rules tester (FR-022).
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct TestedLink {
    pub pos: u32,
    /// The link as written.
    pub raw: String,
    pub text: Option<String>,
    pub position: Position,
    /// Index of the custom rule that matched; null = built-in rule.
    pub rule: Option<u32>,
}

/// Runs `rules` on one stored page without writing anything (FR-022).
/// None when the page's HTML was not stored.
pub fn test_page(
    conn: &Connection,
    url_id: i64,
    rules: &[PositionRule],
) -> rusqlite::Result<Option<Vec<TestedLink>>> {
    let page = conn
        .query_row(
            &format!(
                "SELECT {PAGE_COLUMNS} FROM urls u
                 JOIN bodies b ON b.url_id = u.id LEFT JOIN pages p ON p.url_id = u.id WHERE u.id = ?1"
            ),
            [url_id],
            Page::from_row,
        )
        .optional()?;
    let Some((url, html)) = page.as_ref().and_then(Page::html) else {
        return Ok(None);
    };
    let encoding = page.and_then(|p| p.encoding);
    Ok(Some(links_of(
        &url,
        encoding.as_deref(),
        &html,
        &Rules::compile(rules),
        |pos, r, position, rule| TestedLink {
            pos,
            raw: r.raw.clone(),
            text: r.text.clone(),
            position,
            rule,
        },
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn positions(html: &str, rules: &[PositionRule]) -> Vec<(Position, Option<u32>)> {
        let url = Url::parse("https://example.test/").unwrap();
        classify_page(&url, Some("utf-8"), html.as_bytes(), &Rules::compile(rules))
            .into_iter()
            .map(|c| (c.position, c.rule))
            .collect()
    }

    const PAGE: &str = r#"<!doctype html><html><head><link rel="canonical" href="/"></head><body>
        <header><a href="/h">h</a><nav><a href="/n">n</a></nav></header>
        <div role="navigation"><a href="/rn">rn</a></div>
        <main><article><header><a href="/ah">ah</a></header><p><a href="/c">c</a></p>
          <footer><a href="/af">af</a></footer></article></main>
        <aside><a href="/s">s</a></aside>
        <div class="mega-menu"><ul><li><a href="/m">m</a></li></ul></div>
        <map><area href="/area" alt="x"></map>
        <footer><a href="/f">f</a><img src="/i.png"></footer>
        </body></html>"#;

    #[test]
    fn built_in_landmarks() {
        use Position::*;
        assert_eq!(
            positions(PAGE, &[]),
            vec![
                (Header, None),
                (Navigation, None),
                (Navigation, None),
                (Content, None), // a header inside an article is not the banner
                (Content, None),
                (Content, None),
                (Sidebar, None),
                (Content, None),
                (Content, None),
                (Footer, None),
            ]
        );
    }

    #[test]
    fn custom_rules_come_first_in_order() {
        let rules = [
            PositionRule {
                selector: ".mega-menu".into(),
                position: Position::Navigation,
            },
            PositionRule {
                selector: "article".into(),
                position: Position::Sidebar,
            },
            PositionRule {
                selector: "a[href='/c']".into(),
                position: Position::Footer,
            },
        ];
        let got = positions(PAGE, &rules);
        assert_eq!(got[7], (Position::Navigation, Some(0)));
        // Rule order decides, not which element is nearer.
        assert_eq!(got[4], (Position::Sidebar, Some(1)));
        assert_eq!(got[0], (Position::Header, None));
    }

    #[test]
    fn pos_matches_extraction_on_malformed_pages() {
        // Unclosed tags, a stray </nav>, links in noscript and a table
        // the parser reorders: positions must line up with the stored refs.
        let html = r#"<nav><a href="/1">1<a href="/2">2</nav></a><table><a href="/t">t</a><tr><td>
            <footer><a href="/3">3</table><noscript><a href="/ns">ns</a></noscript><svg><a href="/svg">s</a></svg>"#;
        let url = Url::parse("https://example.test/").unwrap();
        let data = spiderlens_engine::extract::extract_page(&PageInput {
            url: &url,
            charset: Some("utf-8"),
            body: html.as_bytes(),
            truncated: false,
            robots_token: "",
            a11y: false,
        });
        let anchors: Vec<u32> = data
            .refs
            .iter()
            .enumerate()
            .filter(|(_, r)| r.kind == LinkKind::Anchor)
            .map(|(i, _)| i as u32)
            .collect();
        let got = classify_page(&url, Some("utf-8"), html.as_bytes(), &Rules::compile(&[]));
        assert_eq!(got.iter().map(|c| c.pos).collect::<Vec<_>>(), anchors);
        assert_eq!(got[0].position, Position::Navigation);
    }

    #[test]
    fn binary_or_empty_bodies_have_no_links() {
        let url = Url::parse("https://example.test/").unwrap();
        assert!(classify_page(&url, None, b"", &Rules::compile(&[])).is_empty());
    }
}
