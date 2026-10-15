//! Custom search: count matches of a term in the HTML source or the
//! visible text, optionally only inside some elements (FR-006, FR-020,
//! FR-021, US2, US6).

use scraper::ElementRef;
use spiderlens_config::SearchTarget;

use super::{
    CSearch, DocView, MAX_CONTEXTS, RuleOutcome, Within, context, flags, outermost, visible_text, xpath,
};

pub(super) fn run(s: &CSearch, page: &DocView<'_>, tree: Option<&xpath::Tree<'_>>) -> RuleOutcome {
    let mut out = RuleOutcome {
        flags: flags::RUN,
        ..Default::default()
    };
    match &s.within {
        None => {
            let hay = match s.target {
                SearchTarget::Html => page.source,
                SearchTarget::Text => page.visible,
            };
            count(s, hay, &mut out);
        }
        Some(w) => {
            let ids = match w {
                Within::Css(sel) => page.doc.select(sel).map(|e| e.id()).collect(),
                Within::Xpath(expr) => match tree.expect("built when XPath rules exist").elements(expr) {
                    Ok(ids) => ids,
                    Err(msg) => {
                        out.flags |= flags::ERROR;
                        out.error = Some(msg);
                        return out;
                    }
                },
            };
            let ids = outermost(page.doc, ids);
            if ids.is_empty() {
                out.flags |= flags::ELEMENT_NOT_FOUND;
            }
            for id in ids {
                let el = ElementRef::wrap(page.doc.tree.get(id).expect("node of this document"))
                    .expect("selected nodes are elements");
                let hay = match s.target {
                    SearchTarget::Html => el.html(),
                    SearchTarget::Text => visible_text(el),
                };
                count(s, &hay, &mut out);
            }
        }
    }
    if (out.count > 0) != s.not {
        out.flags |= flags::MEETS;
    }
    if out.count == 0 {
        out.flags |= flags::NO_MATCH;
    }
    out
}

/// Non-overlapping matches, left to right (research R3); an empty match
/// counts once and the scan moves on, as regex engines do.
fn count(s: &CSearch, hay: &str, out: &mut RuleOutcome) {
    for m in s.re.find_iter(hay) {
        out.count += 1;
        if out.values.len() < MAX_CONTEXTS {
            out.values.push(context(hay, m.start(), m.end()));
        }
    }
}
