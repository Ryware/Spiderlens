//! The AI checks as rules (spec 015 FR-034, FR-038, FR-044; plan D11).
//!
//! Each check is a rule in the spec 009 rule shape (`RuleDef`) plus what the
//! AI score needs: its part, points and basis. `RuleDef` is a local copy of
//! 009's fields until that crate lands (task T060 swaps it in).

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Spec 009 FR-001 priority.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub enum Priority {
    High,
    Medium,
    Low,
    Opportunity,
}

/// Spec 009 FR-001 kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum RuleKind {
    PerPage,
    SiteWide,
}

/// The data a rule needs; missing data makes it "not available".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum RuleNeed {
    Base,
    Extraction,
    RawHtml,
    Robots,
    Rendering,
    SiteFiles,
}

/// The spec 009 rule shape (FR-001 of spec 009).
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct RuleDef {
    pub id: &'static str,
    pub name: &'static str,
    pub category: &'static str,
    pub priority: Priority,
    pub kind: RuleKind,
    pub needs: &'static [RuleNeed],
    pub explanation: &'static str,
    pub how_to_fix: &'static str,
}

/// The five parts of the page score (FR-034).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum Part {
    Access,
    Readable,
    Answer,
    Trust,
    Structured,
    /// Site checks (30% of the site score, FR-037).
    Site,
}

impl Part {
    /// Default maximum points.
    pub fn default_max(self) -> u32 {
        match self {
            Self::Access => 30,
            Self::Readable => 25,
            Self::Answer => 20,
            Self::Trust => 15,
            Self::Structured => 10,
            Self::Site => 30,
        }
    }

    pub const PAGE: [Part; 5] = [
        Self::Access,
        Self::Readable,
        Self::Answer,
        Self::Trust,
        Self::Structured,
    ];
}

/// What a check is based on (FR-038).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(export)]
pub enum Basis {
    VendorDoc {
        vendor: &'static str,
        url: &'static str,
    },
    Research {
        reference: &'static str,
        url: &'static str,
    },
    CommonPractice,
}

/// Which page types a check applies to (FR-019).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum AppliesTo {
    AllPages,
    Articles,
    Site,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct AeoRule {
    pub rule: RuleDef,
    pub part: Part,
    /// Points within the part at the default weights; 0 = shown, not scored.
    pub points: u32,
    pub applies_to: AppliesTo,
    pub basis: Basis,
    /// Bit in `aeo_pages.flags`, for checks with a tab filter.
    pub flag: Option<u8>,
}

const CAT: &str = "AI search";
const GOOGLE_AI: Basis = Basis::VendorDoc {
    vendor: "Google",
    url: "https://developers.google.com/search/docs/appearance/ai-features",
};
const OPENAI: Basis = Basis::VendorDoc {
    vendor: "OpenAI",
    url: "https://developers.openai.com/api/docs/bots",
};
const VERCEL: Basis = Basis::Research {
    reference: "Vercel and MERJ, The rise of the AI crawler (2024)",
    url: "https://vercel.com/blog/the-rise-of-the-ai-crawler",
};
const GEO: Basis = Basis::Research {
    reference: "Aggarwal et al., GEO: Generative Engine Optimization (KDD 2024)",
    url: "https://arxiv.org/html/2311.09735v3",
};
const SEER: Basis = Basis::Research {
    reference: "Seer Interactive, content recency and AI visibility (2026)",
    url: "https://www.seerinteractive.com/insights/study-content-recencys-impact-on-ai-visibility-in-2026",
};
const BING: Basis = Basis::VendorDoc {
    vendor: "Microsoft Bing",
    url: "https://www.bing.com/webmasters/help/webmaster-guidelines-30fba23a",
};
const SCHEMA_MATCH: Basis = Basis::VendorDoc {
    vendor: "Google",
    url: "https://developers.google.com/search/docs/appearance/structured-data/sd-policies",
};
const LLMS: Basis = Basis::Research {
    reference: "llms.txt proposal; Ahrefs llms.txt study (2026)",
    url: "https://ahrefs.com/blog/llmstxt-study/",
};

macro_rules! rule {
    ($id:literal, $name:literal, $prio:ident, $kind:ident, [$($need:ident),*], $part:ident, $points:literal,
     $applies:ident, $basis:expr, $flag:expr, $expl:literal, $fix:literal) => {
        AeoRule {
            rule: RuleDef {
                id: $id,
                name: $name,
                category: CAT,
                priority: Priority::$prio,
                kind: RuleKind::$kind,
                needs: &[$(RuleNeed::$need),*],
                explanation: $expl,
                how_to_fix: $fix,
            },
            part: Part::$part,
            points: $points,
            applies_to: AppliesTo::$applies,
            basis: $basis,
            flag: $flag,
        }
    };
}

/// Every AI check, in a stable order (ids never change meaning).
pub static RULES: &[AeoRule] = &[
    // Access (30)
    rule!(
        "ai.access.search_blocked",
        "Blocked for AI search crawlers",
        High,
        PerPage,
        [Robots],
        Access,
        12,
        AllPages,
        OPENAI,
        Some(0),
        "robots.txt stops at least one AI search crawler (for example OAI-SearchBot, Claude-SearchBot, PerplexityBot) from fetching this page, so it cannot be cited in that engine's answers.",
        "Allow the AI search crawlers in robots.txt. To keep content out of model training only, block the training crawlers (GPTBot, ClaudeBot, Google-Extended) instead."
    ),
    rule!(
        "ai.access.user_fetch_blocked",
        "Blocked for user-requested fetches",
        Medium,
        PerPage,
        [Robots],
        Access,
        6,
        AllPages,
        OPENAI,
        Some(2),
        "robots.txt names a crawler that fetches pages when a person asks an assistant to (for example ChatGPT-User). Some vendors say robots.txt may not apply to these fetches.",
        "Allow user-requested fetchers unless you want assistants to refuse to read the page."
    ),
    rule!(
        "ai.access.training_only_blocked",
        "Blocked for AI training only",
        Opportunity,
        PerPage,
        [Robots],
        Access,
        0,
        AllPages,
        OPENAI,
        Some(1),
        "Model training crawlers are blocked while AI search crawlers are allowed. This is a common, deliberate choice and costs no points.",
        "No action needed if this is intended."
    ),
    rule!(
        "ai.access.indexable",
        "Not indexable",
        High,
        PerPage,
        [Base],
        Access,
        6,
        AllPages,
        GOOGLE_AI,
        None,
        "Google's AI features only use pages that are indexed and eligible for a snippet; a noindex page cannot appear.",
        "Remove noindex if the page should appear in search and AI answers."
    ),
    rule!(
        "ai.access.snippet_restricted",
        "Snippet restricted",
        Medium,
        PerPage,
        [Extraction],
        Access,
        6,
        AllPages,
        GOOGLE_AI,
        Some(3),
        "nosnippet, max-snippet or noarchive limit what answer engines may quote from this page (Google AI Overviews and AI Mode, Copilot).",
        "Remove the restriction, or use data-nosnippet on the sections that must not be quoted."
    ),
    // Readable without JavaScript (25)
    rule!(
        "ai.readable.js_content",
        "Main content missing without JavaScript",
        High,
        PerPage,
        [Rendering],
        Readable,
        15,
        AllPages,
        VERCEL,
        Some(4),
        "Most of the page's text exists only after JavaScript runs. Most AI crawlers do not run JavaScript and see only the raw HTML.",
        "Render the main content on the server (server-side rendering or static generation)."
    ),
    rule!(
        "ai.readable.js_suspected",
        "Possibly built with JavaScript",
        Medium,
        PerPage,
        [RawHtml],
        Readable,
        15,
        AllPages,
        VERCEL,
        Some(5),
        "The raw HTML has very little visible text for its size and looks like an application shell. AI crawlers that do not run JavaScript may see an empty page.",
        "Run a crawl with JavaScript rendering to confirm, then render the main content on the server."
    ),
    rule!(
        "ai.readable.key_elements_raw",
        "Key elements only after JavaScript",
        Medium,
        PerPage,
        [Extraction],
        Readable,
        10,
        AllPages,
        VERCEL,
        None,
        "The title, main heading, meta description or structured data is missing from the raw HTML.",
        "Put the title, H1, meta description and structured data in the HTML the server sends."
    ),
    // Answer readiness (20)
    rule!(
        "ai.answer.title_topic",
        "Title or main heading does not state the topic",
        Medium,
        PerPage,
        [Extraction],
        Answer,
        5,
        AllPages,
        BING,
        None,
        "Answer engines match questions to pages by their title and main heading; this page's are missing, empty, generic or shared with many pages.",
        "Give the page a unique title and one H1 that name what it is about."
    ),
    rule!(
        "ai.answer.question_no_answer",
        "Question without a direct answer",
        Medium,
        PerPage,
        [RawHtml],
        Answer,
        5,
        Articles,
        Basis::CommonPractice,
        Some(6),
        "A heading asks a question but is not followed by a short paragraph that answers it, so the answer is hard to quote.",
        "Start the first paragraph after each question heading with the answer, in at most about 60 words, then add detail."
    ),
    rule!(
        "ai.answer.unstructured",
        "Long text without structure",
        Low,
        PerPage,
        [RawHtml],
        Answer,
        5,
        Articles,
        Basis::CommonPractice,
        Some(7),
        "A long page with no subheadings, lists or tables is hard to lift passages from.",
        "Break the text into sections with descriptive subheadings; use lists for steps and tables for comparisons."
    ),
    rule!(
        "ai.answer.no_facts",
        "No facts with sources",
        Low,
        PerPage,
        [RawHtml],
        Answer,
        5,
        Articles,
        GEO,
        Some(8),
        "The page states no figures, quotations or links to sources. In the GEO study, adding statistics, quotations and citations raised visibility in generative answers by about 30 to 40%.",
        "Back key claims with figures, attributed quotations and links to the sources."
    ),
    // Trust and freshness (15)
    rule!(
        "ai.trust.no_date",
        "No date",
        Medium,
        PerPage,
        [Extraction],
        Trust,
        4,
        Articles,
        SEER,
        Some(10),
        "The article shows no publication or update date, visibly or in structured data.",
        "Show the published and last updated dates near the title and add datePublished and dateModified to the structured data."
    ),
    rule!(
        "ai.trust.dates_disagree",
        "Dates disagree",
        Low,
        PerPage,
        [Extraction],
        Trust,
        3,
        Articles,
        SCHEMA_MATCH,
        Some(11),
        "The page's dates differ between the visible text, structured data and HTTP headers, or a date is in the future.",
        "Make every source state the same dates."
    ),
    rule!(
        "ai.trust.stale",
        "Not updated recently",
        Low,
        PerPage,
        [Extraction],
        Trust,
        4,
        Articles,
        SEER,
        Some(12),
        "The article has not been updated within the staleness threshold. In a 2026 study, 75% of pages cited by AI answers had been updated within a year.",
        "Review and update the content, and update dateModified when you do."
    ),
    rule!(
        "ai.trust.no_author",
        "No author",
        Low,
        PerPage,
        [RawHtml],
        Trust,
        4,
        Articles,
        Basis::CommonPractice,
        Some(13),
        "The article names no author, visibly or in structured data.",
        "Add a byline linked to an author page, and an author (Person) in the structured data."
    ),
    // Structured data (10)
    rule!(
        "ai.structured.errors",
        "Structured data errors",
        Medium,
        PerPage,
        [Extraction],
        Structured,
        4,
        AllPages,
        SCHEMA_MATCH,
        Some(14),
        "Structured data on the page cannot be parsed, uses unknown types or properties, or lacks required properties.",
        "Fix the listed errors; each message points to the value."
    ),
    rule!(
        "ai.structured.warnings",
        "Structured data warnings",
        Low,
        PerPage,
        [Extraction],
        Structured,
        2,
        AllPages,
        SCHEMA_MATCH,
        Some(15),
        "Structured data lacks recommended properties or uses values of the wrong kind.",
        "Add the recommended properties listed."
    ),
    rule!(
        "ai.structured.not_visible",
        "Structured data content not visible on the page",
        Medium,
        PerPage,
        [RawHtml],
        Structured,
        2,
        AllPages,
        SCHEMA_MATCH,
        Some(9),
        "FAQ or Q&A structured data contains questions or answers that are not in the visible text. Structured data must describe what the page shows.",
        "Show every question and answer on the page, or remove them from the structured data."
    ),
    rule!(
        "ai.structured.mismatch",
        "Structured data disagrees with the page",
        Low,
        PerPage,
        [Extraction],
        Structured,
        2,
        AllPages,
        SCHEMA_MATCH,
        None,
        "The headline or name in structured data differs from the title and main heading.",
        "Use the same headline in structured data as on the page."
    ),
    // Site checks (30)
    rule!(
        "ai.site.robots_unavailable",
        "robots.txt unavailable",
        High,
        SiteWide,
        [Robots],
        Site,
        6,
        Site,
        OPENAI,
        None,
        "robots.txt answered with a server error or could not be reached, so crawlers that follow the rules fetch nothing.",
        "Serve robots.txt with status 200, or 404 if you have no rules."
    ),
    rule!(
        "ai.site.search_access",
        "AI search crawlers blocked on the home page",
        High,
        SiteWide,
        [Robots],
        Site,
        10,
        Site,
        OPENAI,
        None,
        "At least one vendor's AI search crawler may not fetch the home page.",
        "Allow the AI search crawlers on the home page and main sections."
    ),
    rule!(
        "ai.site.entity",
        "No site entity",
        Medium,
        SiteWide,
        [Extraction],
        Site,
        6,
        Site,
        BING,
        None,
        "The site does not describe the organisation or person it belongs to (name, logo, official profiles) in structured data, or describes it inconsistently.",
        "Add Organization (or Person) and WebSite structured data to the home page with name, logo, url and sameAs links."
    ),
    rule!(
        "ai.site.about_contact",
        "No about or contact page",
        Low,
        SiteWide,
        [Base],
        Site,
        6,
        Site,
        Basis::CommonPractice,
        None,
        "No crawlable about page or contact page was found.",
        "Publish an about page and a contact page and link them from every page."
    ),
    rule!(
        "ai.site.llms_txt",
        "llms.txt missing or invalid",
        Opportunity,
        SiteWide,
        [SiteFiles],
        Site,
        2,
        Site,
        LLMS,
        None,
        "llms.txt is an optional file aimed at AI agents and coding tools. No major AI search vendor has said it reads it, and Google says no special AI files are needed.",
        "Optional: publish a valid llms.txt; SpiderLens can generate a draft."
    ),
];

/// The rule with this id.
pub fn by_id(id: &str) -> Option<&'static AeoRule> {
    RULES.iter().find(|r| r.rule.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{BTreeMap, HashSet};

    #[test]
    fn ids_and_flags_are_unique() {
        let ids: HashSet<_> = RULES.iter().map(|r| r.rule.id).collect();
        assert_eq!(ids.len(), RULES.len());
        let flags: Vec<_> = RULES.iter().filter_map(|r| r.flag).collect();
        let unique: HashSet<_> = flags.iter().collect();
        assert_eq!(unique.len(), flags.len());
        assert!(flags.iter().all(|f| *f < 63));
    }

    #[test]
    fn points_fill_each_part() {
        // js_content and js_suspected are alternatives for the same 15 points.
        let mut sum: BTreeMap<Part, u32> = BTreeMap::new();
        for r in RULES.iter().filter(|r| r.rule.id != "ai.readable.js_suspected") {
            *sum.entry(r.part).or_default() += r.points;
        }
        for (part, total) in sum {
            assert_eq!(total, part.default_max(), "{part:?}");
        }
    }

    #[test]
    fn site_rules_are_site_wide() {
        for r in RULES {
            assert_eq!(
                r.part == Part::Site,
                r.rule.kind == RuleKind::SiteWide,
                "{}",
                r.rule.id
            );
            assert_eq!(
                r.part == Part::Site,
                r.applies_to == AppliesTo::Site,
                "{}",
                r.rule.id
            );
            assert!(r.rule.id.starts_with("ai."));
            if let Basis::VendorDoc { url, .. } | Basis::Research { url, .. } = r.basis {
                assert!(url.starts_with("https://"));
            }
        }
    }
}
