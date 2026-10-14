//! Every per-page AI check as one pure function of the page's facts
//! (spec 015 FR-009 to FR-033, plan D7, D10, D11).
//!
//! `evaluate` takes everything a page check reads ([`PageInput`]) and returns
//! one outcome per page rule, with evidence for the ones that failed. It
//! never touches the crawl file, so each acceptance scenario is a plain unit
//! test.

use jiff::ToSpan;
use jiff::civil::Date;
use serde::Serialize;
use serde_json::{Value, json};
use spiderlens_config::ai_bots::{AiBot, BotPurpose};
use ts_rs::TS;
use url::Url;

use crate::content::ContentFacts;
use crate::rules::{AeoRule, AppliesTo, Part, RULES};
use crate::score::Outcome;
use crate::structured::{Message, Severity, StructuredFacts, normalise};

/// FR-019 page classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum PageType {
    /// An article or answer page: answer readiness and trust checks apply.
    Article,
    Listing,
    Utility,
    Other,
}

impl PageType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Article => "article",
            Self::Listing => "listing",
            Self::Utility => "utility",
            Self::Other => "other",
        }
    }
}

/// Spec 014's stored comparison for a rendered page (plan D10); read by
/// interface only, `None` when the page was not rendered.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct RenderFacts {
    /// Share of the rendered text missing from the raw HTML, 0 to 1.
    pub dependence: f64,
    pub dep_headings: Option<f64>,
    pub dep_internal_links: Option<f64>,
    pub dep_structured: Option<f64>,
    pub raw_has_title: bool,
    pub raw_has_description: bool,
    pub raw_has_canonical: bool,
    pub raw_has_directives: bool,
}

/// Thresholds from the scoring profile (data-model.md, FR-040).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct Thresholds {
    pub direct_answer_words: u32,
    pub long_paragraph_words: u32,
    pub long_text_words: u32,
    pub stale_months: u32,
    pub date_agree_days: u32,
    pub js_dependence_pct: u32,
    pub blocked_cap: u32,
    /// Titles shared by at least this many pages are "not unique".
    pub shared_title_pages: u32,
    /// `max-snippet` values below this many characters restrict quoting.
    pub max_snippet_chars: u32,
}

impl Default for Thresholds {
    fn default() -> Self {
        Self {
            direct_answer_words: 60,
            long_paragraph_words: 150,
            long_text_words: 800,
            stale_months: 12,
            date_agree_days: 2,
            js_dependence_pct: 50,
            blocked_cap: 20,
            shared_title_pages: 3,
            max_snippet_chars: 160,
        }
    }
}

/// One stored robots directive (`directives` table).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Directive {
    pub directive: String,
    pub value: Option<String>,
    /// `all` or a crawler name, lower case.
    pub crawler: String,
    /// `meta` or `header`.
    pub source: String,
}

/// Which built-in crawlers serve which purpose, as bitmasks over `AiBot.index`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PurposeMasks {
    pub search: u64,
    pub training: u64,
    pub user_fetch: u64,
}

impl PurposeMasks {
    pub fn of(bots: &[AiBot]) -> Self {
        let mut m = Self::default();
        for b in bots.iter().filter(|b| !b.retired && !b.custom && b.index < 64) {
            let bit = 1u64 << b.index;
            match b.purpose {
                BotPurpose::Search => m.search |= bit,
                BotPurpose::Training => m.training |= bit,
                BotPurpose::UserFetch => m.user_fetch |= bit,
                BotPurpose::Other => {}
            }
        }
        m
    }
}

/// Everything the page checks read.
pub struct PageInput<'a> {
    pub url: &'a Url,
    /// The page's robots.txt verdicts are known (robots.txt fetched or missing).
    pub robots_known: bool,
    /// Built-in crawlers that may not fetch the page (bit = `AiBot.index`).
    pub blocked: u64,
    pub masks: PurposeMasks,
    pub bots: &'a [AiBot],
    pub noindex: bool,
    /// A `pages` row exists (spec 005 extraction ran).
    pub extracted: bool,
    pub title: Option<&'a str>,
    /// Pages in the crawl with the same title, this one included.
    pub title_pages: u32,
    pub h1: Option<&'a str>,
    pub h1_count: u32,
    pub directives: &'a [Directive],
    /// `None` when the page HTML was not stored (FR-025).
    pub content: Option<&'a ContentFacts>,
    pub structured: &'a StructuredFacts,
    pub sd_messages: &'a [Message],
    /// Structured data blocks stored for the page.
    pub sd_blocks: u32,
    pub render: Option<&'a RenderFacts>,
    /// The day the crawl ran, for staleness and future dates.
    pub today: Date,
    pub type_override: Option<PageType>,
}

/// What `evaluate` found for one page.
#[derive(Debug, Clone)]
pub struct PageEval {
    pub page_type: PageType,
    pub page_type_reason: String,
    /// One per page rule, in `RULES` order.
    pub outcomes: Vec<(&'static AeoRule, Outcome)>,
    /// Evidence for failed and not-available checks, in `RULES` order.
    pub evidence: Vec<(&'static str, Value)>,
    pub flags: u64,
    /// All AI search crawlers are blocked (the cap of FR-035 applies).
    pub blocked_for_all_search: bool,
    pub published: Option<String>,
    pub modified: Option<String>,
    pub dates_agree: Option<bool>,
    pub author: Option<String>,
    /// `raw`, or `rendered` when spec 014 values were used.
    pub version_used: &'static str,
}

const ARTICLE_TYPES: &[&str] = &[
    "Article",
    "NewsArticle",
    "BlogPosting",
    "TechArticle",
    "Report",
    "ScholarlyArticle",
    "FAQPage",
    "QAPage",
    "HowTo",
    "Recipe",
];
const LISTING_TYPES: &[&str] = &["CollectionPage", "ItemList", "SearchResultsPage"];
const UTILITY_TYPES: &[&str] = &["ContactPage", "CheckoutPage", "ProfilePage"];
const UTILITY_WORDS: &[&str] = &[
    "login",
    "log-in",
    "signin",
    "sign-in",
    "signup",
    "sign-up",
    "register",
    "account",
    "cart",
    "basket",
    "checkout",
    "search",
    "contact",
    "contact-us",
    "privacy",
    "privacy-policy",
    "terms",
    "terms-of-service",
    "legal",
    "cookies",
    "cookie-policy",
    "imprint",
    "impressum",
    "accessibility",
];
const LISTING_WORDS: &[&str] = &[
    "category",
    "categories",
    "tag",
    "tags",
    "archive",
    "page",
    "author",
];
const GENERIC_TITLES: &[&str] = &[
    "home",
    "home page",
    "homepage",
    "untitled",
    "untitled document",
    "index",
    "page",
    "new page",
    "welcome",
    "document",
    "default",
    "title",
];

/// FR-019: the page type and why.
pub fn classify(input: &PageInput<'_>) -> (PageType, String) {
    if let Some(t) = input.type_override {
        return (t, "set by a URL pattern in settings".into());
    }
    let types = &input.structured.types;
    let has = |list: &[&str]| types.iter().find(|t| list.contains(&t.as_str())).cloned();
    if let Some(t) = has(UTILITY_TYPES) {
        return (PageType::Utility, format!("structured data type {t}"));
    }
    if let Some(t) = has(ARTICLE_TYPES) {
        return (PageType::Article, format!("structured data type {t}"));
    }
    if let Some(t) = has(LISTING_TYPES) {
        return (PageType::Listing, format!("structured data type {t}"));
    }
    let segments: Vec<String> = input
        .url
        .path_segments()
        .map(|s| {
            s.filter(|s| !s.is_empty())
                .map(|s| s.to_ascii_lowercase())
                .collect()
        })
        .unwrap_or_default();
    if let Some(s) = segments.iter().find(|s| {
        let stem = s.split('.').next().unwrap_or(s);
        UTILITY_WORDS.contains(&stem)
    }) {
        return (PageType::Utility, format!("URL contains /{s}"));
    }
    if segments.is_empty() {
        return (PageType::Listing, "home page".into());
    }
    if let Some(s) = segments.iter().find(|s| LISTING_WORDS.contains(&s.as_str())) {
        return (PageType::Listing, format!("URL contains /{s}"));
    }
    match input.content {
        Some(c) if c.main_words >= 300 && c.paragraphs >= 3 => (
            PageType::Article,
            format!(
                "{} words of running text in {} paragraphs",
                c.main_words, c.paragraphs
            ),
        ),
        Some(c) if c.lists > 0 && c.paragraphs < 3 => {
            (PageType::Listing, "mostly lists, little running text".into())
        }
        Some(c) => (
            PageType::Other,
            format!("only {} words of running text", c.main_words),
        ),
        None => (PageType::Other, "page HTML not stored".into()),
    }
}

/// The checks for one HTML page.
pub fn evaluate(input: &PageInput<'_>, th: &Thresholds) -> PageEval {
    let (page_type, page_type_reason) = classify(input);
    let dates = choose_dates(input, th);
    let mut ev = PageEval {
        page_type,
        page_type_reason,
        outcomes: Vec::new(),
        evidence: Vec::new(),
        flags: 0,
        blocked_for_all_search: input.robots_known
            && input.masks.search != 0
            && input.blocked & input.masks.search == input.masks.search,
        published: dates.published.clone(),
        modified: dates.modified.clone(),
        dates_agree: dates.agree,
        author: author(input),
        version_used: if input.render.is_some() { "rendered" } else { "raw" },
    };
    for rule in RULES.iter().filter(|r| r.part != Part::Site) {
        let (outcome, evidence) = if rule.applies_to == AppliesTo::Articles && page_type != PageType::Article
        {
            (Outcome::NotApplicable, None)
        } else {
            check(rule.rule.id, input, th, &dates)
        };
        if outcome == Outcome::Fail
            && let Some(bit) = rule.flag
        {
            ev.flags |= 1 << bit;
        }
        if let Some(e) = evidence
            && matches!(outcome, Outcome::Fail | Outcome::NotAvailable)
        {
            ev.evidence.push((rule.rule.id, e));
        }
        ev.outcomes.push((rule, outcome));
    }
    ev
}

type Checked = (Outcome, Option<Value>);

fn not_available(reason: &str) -> Checked {
    (Outcome::NotAvailable, Some(json!({ "reason": reason })))
}

const NO_HTML: &str = "page HTML not stored";

fn pass_or_fail(fail: bool, evidence: Value) -> Checked {
    if fail {
        (Outcome::Fail, Some(evidence))
    } else {
        (Outcome::Pass, None)
    }
}

fn check(id: &str, p: &PageInput<'_>, th: &Thresholds, dates: &Dates) -> Checked {
    match id {
        "ai.access.search_blocked" => access(p, p.masks.search),
        "ai.access.user_fetch_blocked" => access(p, p.masks.user_fetch),
        "ai.access.training_only_blocked" => {
            if !p.robots_known {
                return not_available("robots.txt was not fetched");
            }
            let training = p.blocked & p.masks.training;
            pass_or_fail(
                training != 0 && p.blocked & p.masks.search == 0,
                json!({ "crawlers": names(p.bots, training) }),
            )
        }
        "ai.access.indexable" => pass_or_fail(p.noindex, json!({ "directive": "noindex" })),
        "ai.access.snippet_restricted" => {
            if !p.extracted {
                return not_available("page was not extracted");
            }
            let found = snippet_limits(p.directives, th);
            pass_or_fail(!found.is_empty(), json!({ "directives": found }))
        }
        "ai.readable.js_content" => match p.render {
            Some(r) => {
                let pct = (r.dependence * 100.0).round() as i64;
                pass_or_fail(
                    pct >= i64::from(th.js_dependence_pct),
                    json!({ "dependence_pct": pct, "threshold_pct": th.js_dependence_pct }),
                )
            }
            // The raw-HTML heuristic below covers pages that were not rendered (FR-018).
            None => (Outcome::NotApplicable, None),
        },
        "ai.readable.js_suspected" => {
            if p.render.is_some() {
                return (Outcome::NotApplicable, None);
            }
            let Some(c) = p.content else {
                return not_available(NO_HTML);
            };
            let js = &c.js;
            pass_or_fail(
                js.suspected(),
                json!({
                    "basis": "raw HTML only",
                    "body_words": js.body_words,
                    "html_bytes": js.html_bytes,
                    "script_bytes": js.script_bytes,
                    "external_scripts": js.external_scripts,
                    "empty_container": js.empty_container,
                    "framework_markers": js.framework_markers,
                }),
            )
        }
        "ai.readable.key_elements_raw" => match p.render {
            Some(r) => {
                let mut missing = Vec::new();
                if !r.raw_has_title {
                    missing.push("title");
                }
                if r.dep_headings.is_some_and(|d| d >= 0.5) {
                    missing.push("main heading");
                }
                if !r.raw_has_description {
                    missing.push("meta description");
                }
                if r.dep_structured.is_some_and(|d| d > 0.0) {
                    missing.push("structured data");
                }
                pass_or_fail(!missing.is_empty(), json!({ "missing_from_raw_html": missing }))
            }
            None => not_available("needs a crawl with JavaScript rendering"),
        },
        "ai.answer.title_topic" => {
            if !p.extracted {
                return not_available("page was not extracted");
            }
            let title = p.title.map(str::trim).unwrap_or("");
            let mut problems = Vec::new();
            if title.is_empty() {
                problems.push("no title".to_string());
            } else if GENERIC_TITLES.contains(&normalise(title).as_str()) {
                problems.push(format!("generic title \"{title}\""));
            } else if p.title_pages >= th.shared_title_pages {
                problems.push(format!("title shared by {} pages", p.title_pages));
            }
            if p.h1_count == 0 || p.h1.is_none_or(|h| h.trim().is_empty()) {
                problems.push("no main heading (H1)".to_string());
            }
            pass_or_fail(!problems.is_empty(), json!({ "problems": problems }))
        }
        "ai.answer.question_no_answer" => {
            let Some(c) = p.content else {
                return not_available(NO_HTML);
            };
            if c.questions.is_empty() {
                return (Outcome::NotApplicable, None);
            }
            let bad: Vec<Value> = c
                .questions
                .iter()
                .filter(|q| q.answer.is_none() || q.answer_words > th.direct_answer_words)
                .map(|q| {
                    json!({
                        "heading": q.text,
                        "followed_by": q.followed_by,
                        "answer_words": q.answer.as_ref().map(|_| q.answer_words),
                    })
                })
                .collect();
            pass_or_fail(
                !bad.is_empty(),
                json!({ "questions": bad, "max_words": th.direct_answer_words }),
            )
        }
        "ai.answer.unstructured" => {
            let Some(c) = p.content else {
                return not_available(NO_HTML);
            };
            let long = c.main_words >= th.long_text_words;
            let per_thousand = (c.subheadings * 1000).checked_div(c.main_words).unwrap_or(0);
            pass_or_fail(
                long && per_thousand < 1 && c.lists == 0 && c.tables == 0,
                json!({
                    "words": c.main_words,
                    "subheadings": c.subheadings,
                    "lists": c.lists,
                    "tables": c.tables,
                    "longest_paragraph": c.longest_paragraph,
                }),
            )
        }
        "ai.answer.no_facts" => {
            let Some(c) = p.content else {
                return not_available(NO_HTML);
            };
            pass_or_fail(
                c.figures + c.quotations + c.citations == 0,
                json!({ "figures": 0, "quotations": 0, "citations": 0 }),
            )
        }
        "ai.trust.no_date" => {
            if p.content.is_none() && dates.published.is_none() && dates.modified.is_none() {
                return not_available(NO_HTML);
            }
            pass_or_fail(
                dates.published.is_none() && dates.modified.is_none(),
                json!({ "looked_in": ["structured data", "time elements", "visible text near the title"] }),
            )
        }
        "ai.trust.dates_disagree" => {
            if dates.agree.is_none() && dates.future.is_empty() {
                return (Outcome::NotApplicable, None);
            }
            pass_or_fail(
                dates.agree == Some(false) || !dates.future.is_empty(),
                json!({ "dates": dates.found, "future": dates.future, "tolerance_days": th.date_agree_days }),
            )
        }
        "ai.trust.stale" => {
            let Some(last) = dates.modified.as_ref().or(dates.published.as_ref()) else {
                return (Outcome::NotApplicable, None);
            };
            let Some(d) = parse(last) else {
                return (Outcome::NotApplicable, None);
            };
            let Ok(limit) = p.today.checked_sub(i64::from(th.stale_months).months()) else {
                return (Outcome::NotApplicable, None);
            };
            pass_or_fail(d < limit, json!({ "last_date": last, "months": th.stale_months }))
        }
        "ai.trust.no_author" => {
            if p.structured.author.is_some() {
                return (Outcome::Pass, None);
            }
            let Some(c) = p.content else {
                return not_available(NO_HTML);
            };
            pass_or_fail(
                c.byline.is_none(),
                json!({ "looked_in": ["structured data author", "rel=author", "byline"] }),
            )
        }
        "ai.structured.errors" | "ai.structured.warnings" => {
            if !p.extracted {
                return not_available("page was not extracted");
            }
            if p.sd_blocks == 0 {
                return (Outcome::NotApplicable, None);
            }
            let sev = if id.ends_with("errors") {
                Severity::Error
            } else {
                Severity::Warning
            };
            let msgs: Vec<&Message> = p.sd_messages.iter().filter(|m| m.severity == sev).collect();
            pass_or_fail(!msgs.is_empty(), json!({ "messages": msgs }))
        }
        "ai.structured.not_visible" => {
            if p.structured.qa.is_empty() {
                return (Outcome::NotApplicable, None);
            }
            let Some(c) = p.content else {
                return not_available(NO_HTML);
            };
            let missing: Vec<&str> = p
                .structured
                .qa
                .iter()
                .filter(|(q, a)| !visible(&c.visible_text, q) || !visible(&c.visible_text, a))
                .map(|(q, _)| q.as_str())
                .collect();
            pass_or_fail(!missing.is_empty(), json!({ "questions": missing }))
        }
        "ai.structured.mismatch" => {
            let Some(headline) = p.structured.headline.as_deref() else {
                return (Outcome::NotApplicable, None);
            };
            let h = normalise(headline);
            let matches = |s: Option<&str>| {
                s.map(normalise)
                    .is_some_and(|s| !s.is_empty() && (s.contains(&h) || h.contains(&s)))
            };
            pass_or_fail(
                !h.is_empty() && !matches(p.title) && !matches(p.h1),
                json!({ "headline": headline, "title": p.title, "h1": p.h1 }),
            )
        }
        other => unreachable!("page rule without a check: {other}"),
    }
}

fn access(p: &PageInput<'_>, mask: u64) -> Checked {
    if !p.robots_known {
        return not_available("robots.txt was not fetched");
    }
    let hit = p.blocked & mask;
    pass_or_fail(hit != 0, json!({ "crawlers": names(p.bots, hit) }))
}

fn names(bots: &[AiBot], mask: u64) -> Vec<&str> {
    bots.iter()
        .filter(|b| !b.custom && b.index < 64 && mask & (1 << b.index) != 0)
        .map(|b| b.name.as_str())
        .collect()
}

/// Crawlers whose directives also govern answer engines.
fn governs_answers(crawler: &str) -> bool {
    matches!(crawler, "all" | "robots" | "googlebot" | "bingbot" | "msnbot")
}

/// FR-013 restrictions on quoting.
fn snippet_limits(directives: &[Directive], th: &Thresholds) -> Vec<Value> {
    let mut out = Vec::new();
    for d in directives.iter().filter(|d| governs_answers(&d.crawler)) {
        let name = d.directive.to_ascii_lowercase();
        let restricts = match name.as_str() {
            "nosnippet" | "noarchive" | "nocache" => true,
            "max-snippet" => d
                .value
                .as_deref()
                .and_then(|v| v.trim().parse::<i64>().ok())
                .is_some_and(|n| n >= 0 && n < i64::from(th.max_snippet_chars)),
            _ => false,
        };
        if restricts {
            out.push(json!({
                "directive": name,
                "value": d.value,
                "crawler": d.crawler,
                "source": d.source,
                "meaning": crate::signals::directive_meaning(&name),
            }));
        }
    }
    out
}

fn visible(page: &str, s: &str) -> bool {
    let n = normalise(s);
    if n.is_empty() {
        return true;
    }
    // Long answers: the opening is enough to show the text is on the page.
    let head: String = n.split(' ').take(12).collect::<Vec<_>>().join(" ");
    page.contains(&head)
}

fn author(p: &PageInput<'_>) -> Option<String> {
    p.structured
        .author
        .clone()
        .or_else(|| p.content.and_then(|c| c.byline.clone()))
}

fn parse(s: &str) -> Option<Date> {
    Date::strptime("%Y-%m-%d", s.get(..10)?).ok()
}

#[derive(Debug, Default)]
struct Dates {
    published: Option<String>,
    modified: Option<String>,
    /// `None` when fewer than two sources state the same kind of date.
    agree: Option<bool>,
    found: Vec<Value>,
    future: Vec<String>,
}

/// FR-026, FR-027: the dates the page states and whether they agree.
fn choose_dates(p: &PageInput<'_>, th: &Thresholds) -> Dates {
    let sd = p.structured;
    let mut d = Dates::default();
    let (mut vis_pub, mut vis_mod) = (None, None);
    if let Some(c) = p.content {
        for f in &c.dates {
            let slot = if f.modified { &mut vis_mod } else { &mut vis_pub };
            if slot.is_none() {
                *slot = Some(f.date.clone());
            }
            d.found
                .push(json!({ "date": f.date, "source": f.source, "modified": f.modified }));
        }
    }
    if let Some(x) = &sd.published {
        d.found
            .push(json!({ "date": x, "source": "structured data datePublished", "modified": false }));
    }
    if let Some(x) = &sd.modified {
        d.found
            .push(json!({ "date": x, "source": "structured data dateModified", "modified": true }));
    }
    d.published = sd.published.clone().or(vis_pub.clone());
    d.modified = sd.modified.clone().or(vis_mod.clone());
    let close = |a: &str, b: &str| match (parse(a), parse(b)) {
        (Some(a), Some(b)) => (a - b).get_days().unsigned_abs() <= th.date_agree_days,
        _ => true,
    };
    let mut compared = false;
    let mut agree = true;
    for (s, v) in [(&sd.published, &vis_pub), (&sd.modified, &vis_mod)] {
        if let (Some(s), Some(v)) = (s, v) {
            compared = true;
            agree &= close(s, v);
        }
    }
    if let (Some(pubd), Some(modd)) = (
        d.published.as_deref().and_then(parse),
        d.modified.as_deref().and_then(parse),
    ) && modd < pubd
    {
        compared = true;
        agree = false;
    }
    if compared {
        d.agree = Some(agree);
    }
    let mut future: Vec<String> = d
        .found
        .iter()
        .filter_map(|f| f["date"].as_str())
        .filter(|s| parse(s).is_some_and(|x| x > p.today))
        .map(str::to_string)
        .collect();
    future.sort();
    future.dedup();
    d.future = future;
    d
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::{DateFound, JsEvidence, QuestionHeading};
    use crate::structured::Entity;

    fn bots() -> &'static [AiBot] {
        spiderlens_config::ai_bots::all()
    }

    fn bit(token: &str) -> u64 {
        1 << bots().iter().find(|b| b.token == token).unwrap().index
    }

    fn content() -> ContentFacts {
        ContentFacts {
            main_source: "main",
            main_words: 900,
            paragraphs: 9,
            subheadings: 3,
            lists: 1,
            figures: 2,
            citations: 1,
            byline: Some("Dana Levi".into()),
            dates: vec![DateFound {
                date: "2026-06-01".into(),
                source: "time",
                modified: false,
            }],
            visible_text: "how long does it take about two weeks".into(),
            js: JsEvidence {
                html_bytes: 20_000,
                body_words: 900,
                ..Default::default()
            },
            ..Default::default()
        }
    }

    struct Fixture {
        url: Url,
        content: Option<ContentFacts>,
        sd: StructuredFacts,
        msgs: Vec<Message>,
        directives: Vec<Directive>,
        render: Option<RenderFacts>,
        blocked: u64,
        robots_known: bool,
        title: Option<&'static str>,
    }

    impl Fixture {
        fn article() -> Self {
            Self {
                url: Url::parse("https://a.test/blog/how-long").unwrap(),
                content: Some(content()),
                sd: StructuredFacts {
                    types: vec!["BlogPosting".into()],
                    published: Some("2026-06-01".into()),
                    headline: Some("How long does it take".into()),
                    ..Default::default()
                },
                msgs: vec![],
                directives: vec![],
                render: None,
                blocked: 0,
                robots_known: true,
                title: Some("How long does it take | Site"),
            }
        }

        fn run(&self) -> PageEval {
            let input = PageInput {
                url: &self.url,
                robots_known: self.robots_known,
                blocked: self.blocked,
                masks: PurposeMasks::of(bots()),
                bots: bots(),
                noindex: false,
                extracted: true,
                title: self.title,
                title_pages: 1,
                h1: Some("How long does it take"),
                h1_count: 1,
                directives: &self.directives,
                content: self.content.as_ref(),
                structured: &self.sd,
                sd_messages: &self.msgs,
                sd_blocks: 1,
                render: self.render.as_ref(),
                today: Date::constant(2026, 10, 13),
                type_override: None,
            };
            evaluate(&input, &Thresholds::default())
        }
    }

    fn outcome(e: &PageEval, id: &str) -> Outcome {
        e.outcomes.iter().find(|(r, _)| r.rule.id == id).unwrap().1
    }

    #[test]
    fn a_good_article_passes_every_scored_check() {
        let e = Fixture::article().run();
        assert_eq!(e.page_type, PageType::Article);
        assert_eq!(e.page_type_reason, "structured data type BlogPosting");
        let fails: Vec<_> = e
            .outcomes
            .iter()
            .filter(|(_, o)| *o == Outcome::Fail)
            .map(|(r, _)| r.rule.id)
            .collect();
        assert!(fails.is_empty(), "{fails:?}");
        assert_eq!(e.flags, 0);
        assert_eq!(e.version_used, "raw");
        assert_eq!(e.published.as_deref(), Some("2026-06-01"));
        assert_eq!(e.author.as_deref(), Some("Dana Levi"));
        // Every page rule has exactly one outcome, in RULES order.
        let page_rules = RULES.iter().filter(|r| r.part != Part::Site).count();
        assert_eq!(e.outcomes.len(), page_rules);
    }

    #[test]
    fn search_crawler_block_fails_and_sets_flag_us1() {
        let mut f = Fixture::article();
        f.blocked = bit("oai-searchbot");
        let e = f.run();
        assert_eq!(outcome(&e, "ai.access.search_blocked"), Outcome::Fail);
        assert_ne!(e.flags & 1, 0);
        assert!(!e.blocked_for_all_search);
        let (_, ev) = e
            .evidence
            .iter()
            .find(|(id, _)| *id == "ai.access.search_blocked")
            .unwrap();
        assert_eq!(ev["crawlers"], json!(["OAI-SearchBot"]));
    }

    #[test]
    fn every_search_crawler_blocked_triggers_the_cap() {
        let mut f = Fixture::article();
        f.blocked = PurposeMasks::of(bots()).search;
        assert!(f.run().blocked_for_all_search);
    }

    #[test]
    fn training_only_block_costs_nothing_but_is_flagged() {
        let mut f = Fixture::article();
        f.blocked = bit("gptbot");
        let e = f.run();
        assert_eq!(outcome(&e, "ai.access.training_only_blocked"), Outcome::Fail);
        assert_eq!(outcome(&e, "ai.access.search_blocked"), Outcome::Pass);
        assert_ne!(e.flags & (1 << 1), 0);
    }

    #[test]
    fn unknown_robots_makes_access_not_available() {
        let mut f = Fixture::article();
        f.robots_known = false;
        let e = f.run();
        assert_eq!(outcome(&e, "ai.access.search_blocked"), Outcome::NotAvailable);
        assert!(!e.blocked_for_all_search);
    }

    #[test]
    fn snippet_restrictions_us2() {
        let mut f = Fixture::article();
        f.directives = vec![
            Directive {
                directive: "max-snippet".into(),
                value: Some("50".into()),
                crawler: "googlebot".into(),
                source: "meta".into(),
            },
            Directive {
                directive: "nosnippet".into(),
                value: None,
                crawler: "otherbot".into(),
                source: "header".into(),
            },
        ];
        let e = f.run();
        assert_eq!(outcome(&e, "ai.access.snippet_restricted"), Outcome::Fail);
        let (_, ev) = e
            .evidence
            .iter()
            .find(|(id, _)| *id == "ai.access.snippet_restricted")
            .unwrap();
        assert_eq!(ev["directives"].as_array().unwrap().len(), 1);
        f.directives[0].value = Some("-1".into());
        assert_eq!(outcome(&f.run(), "ai.access.snippet_restricted"), Outcome::Pass);
    }

    #[test]
    fn js_checks_use_render_facts_when_present_us3() {
        let mut f = Fixture::article();
        let c = f.content.as_mut().unwrap();
        c.js = JsEvidence {
            html_bytes: 40_000,
            script_bytes: 30_000,
            body_words: 3,
            empty_container: Some("#root".into()),
            ..Default::default()
        };
        let e = f.run();
        assert_eq!(outcome(&e, "ai.readable.js_suspected"), Outcome::Fail);
        assert_eq!(outcome(&e, "ai.readable.js_content"), Outcome::NotApplicable);
        assert_eq!(outcome(&e, "ai.readable.key_elements_raw"), Outcome::NotAvailable);
        assert_ne!(e.flags & (1 << 5), 0);

        f.render = Some(RenderFacts {
            dependence: 0.8,
            raw_has_title: true,
            raw_has_description: true,
            dep_headings: Some(1.0),
            ..Default::default()
        });
        let e = f.run();
        assert_eq!(e.version_used, "rendered");
        assert_eq!(outcome(&e, "ai.readable.js_content"), Outcome::Fail);
        assert_eq!(outcome(&e, "ai.readable.js_suspected"), Outcome::NotApplicable);
        assert_eq!(outcome(&e, "ai.readable.key_elements_raw"), Outcome::Fail);
        assert_ne!(e.flags & (1 << 4), 0);
    }

    #[test]
    fn missing_html_is_not_available_not_failed_fr025() {
        let mut f = Fixture::article();
        f.content = None;
        let e = f.run();
        for id in [
            "ai.answer.question_no_answer",
            "ai.answer.unstructured",
            "ai.answer.no_facts",
            "ai.readable.js_suspected",
        ] {
            assert_eq!(outcome(&e, id), Outcome::NotAvailable, "{id}");
        }
        // Structured data still gives a date.
        assert_eq!(outcome(&e, "ai.trust.no_date"), Outcome::Pass);
        let (_, ev) = e
            .evidence
            .iter()
            .find(|(id, _)| *id == "ai.answer.no_facts")
            .unwrap();
        assert_eq!(ev["reason"], "page HTML not stored");
    }

    #[test]
    fn question_without_direct_answer_us4() {
        let mut f = Fixture::article();
        f.content.as_mut().unwrap().questions = vec![
            QuestionHeading {
                level: 2,
                text: "How long does it take?".into(),
                answer: Some("About two weeks.".into()),
                answer_words: 3,
                followed_by: "paragraph",
            },
            QuestionHeading {
                level: 2,
                text: "What does it cost?".into(),
                answer: None,
                answer_words: 0,
                followed_by: "media",
            },
        ];
        let e = f.run();
        assert_eq!(outcome(&e, "ai.answer.question_no_answer"), Outcome::Fail);
        let (_, ev) = e
            .evidence
            .iter()
            .find(|(id, _)| *id == "ai.answer.question_no_answer")
            .unwrap();
        assert_eq!(ev["questions"][0]["heading"], "What does it cost?");
        assert_eq!(ev["questions"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn long_text_without_structure() {
        let mut f = Fixture::article();
        let c = f.content.as_mut().unwrap();
        c.main_words = 2_000;
        c.subheadings = 1;
        c.lists = 0;
        assert_eq!(outcome(&f.run(), "ai.answer.unstructured"), Outcome::Fail);
        f.content.as_mut().unwrap().subheadings = 4;
        assert_eq!(outcome(&f.run(), "ai.answer.unstructured"), Outcome::Pass);
    }

    #[test]
    fn dates_disagree_future_and_stale_us6() {
        let mut f = Fixture::article();
        f.sd.published = Some("2026-05-01".into());
        let e = f.run();
        assert_eq!(outcome(&e, "ai.trust.dates_disagree"), Outcome::Fail);
        assert_eq!(e.dates_agree, Some(false));

        let mut f = Fixture::article();
        f.sd.modified = Some("2027-01-01".into());
        assert_eq!(outcome(&f.run(), "ai.trust.dates_disagree"), Outcome::Fail);

        let mut f = Fixture::article();
        f.sd.published = Some("2025-01-01".into());
        f.content.as_mut().unwrap().dates.clear();
        let e = f.run();
        assert_eq!(outcome(&e, "ai.trust.stale"), Outcome::Fail);
        assert_eq!(outcome(&e, "ai.trust.dates_disagree"), Outcome::NotApplicable);
        assert_ne!(e.flags & (1 << 12), 0);

        let mut f = Fixture::article();
        f.sd.published = None;
        f.content.as_mut().unwrap().dates.clear();
        let e = f.run();
        assert_eq!(outcome(&e, "ai.trust.no_date"), Outcome::Fail);
        assert_eq!(outcome(&e, "ai.trust.stale"), Outcome::NotApplicable);
    }

    #[test]
    fn author_from_byline_or_structured_data() {
        let mut f = Fixture::article();
        f.content.as_mut().unwrap().byline = None;
        assert_eq!(outcome(&f.run(), "ai.trust.no_author"), Outcome::Fail);
        f.sd.author = Some("Dana".into());
        assert_eq!(outcome(&f.run(), "ai.trust.no_author"), Outcome::Pass);
    }

    #[test]
    fn faq_not_visible_and_headline_mismatch() {
        let mut f = Fixture::article();
        f.sd.qa = vec![
            ("How long does it take".into(), "About two weeks".into()),
            ("Is it free".into(), "Yes".into()),
        ];
        f.sd.headline = Some("Something else entirely".into());
        let e = f.run();
        assert_eq!(outcome(&e, "ai.structured.not_visible"), Outcome::Fail);
        let (_, ev) = e
            .evidence
            .iter()
            .find(|(id, _)| *id == "ai.structured.not_visible")
            .unwrap();
        assert_eq!(ev["questions"], json!(["Is it free"]));
        assert_eq!(outcome(&e, "ai.structured.mismatch"), Outcome::Fail);
    }

    #[test]
    fn structured_messages_split_by_severity() {
        let mut f = Fixture::article();
        f.msgs = vec![Message {
            severity: Severity::Warning,
            block: 0,
            pointer: "/image".into(),
            message: "recommended property image is missing".into(),
        }];
        let e = f.run();
        assert_eq!(outcome(&e, "ai.structured.errors"), Outcome::Pass);
        assert_eq!(outcome(&e, "ai.structured.warnings"), Outcome::Fail);
    }

    #[test]
    fn generic_title_fails_topic_check() {
        let mut f = Fixture::article();
        f.title = Some("Home");
        assert_eq!(outcome(&f.run(), "ai.answer.title_topic"), Outcome::Fail);
    }

    #[test]
    fn classification_and_article_only_checks_fr019() {
        let mut f = Fixture::article();
        f.sd = StructuredFacts::default();
        f.url = Url::parse("https://a.test/contact").unwrap();
        let e = f.run();
        assert_eq!(e.page_type, PageType::Utility);
        assert_eq!(e.page_type_reason, "URL contains /contact");
        assert_eq!(outcome(&e, "ai.trust.no_date"), Outcome::NotApplicable);
        assert_eq!(outcome(&e, "ai.access.search_blocked"), Outcome::Pass);

        f.url = Url::parse("https://a.test/").unwrap();
        assert_eq!(f.run().page_type, PageType::Listing);

        f.url = Url::parse("https://a.test/guide").unwrap();
        let e = f.run();
        assert_eq!(e.page_type, PageType::Article);
        assert!(e.page_type_reason.contains("900 words"));

        f.sd.org = Some(Entity::default());
        f.sd.types = vec!["ContactPage".into()];
        assert_eq!(f.run().page_type, PageType::Utility);
    }
}
