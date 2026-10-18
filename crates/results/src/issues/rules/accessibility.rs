//! Accessibility (spec 026). One rule per static check, then the generic
//! rules for what the full checks report. Every condition counts only what
//! falls inside the crawl's WCAG target (`wcag_in_target`, FR-020).
//!
//! Spec 009's missing alt, missing language and heading order rules are
//! unchanged; the viewport, zoom and plugin checks are in `mobile.rs`.

use crate::issues::{Category::Accessibility as A, Priority::*, Rule, Scope, rule};
use crate::tabs::Need;

/// Pages where the static check `$id` found something inside the target.
macro_rules! static_failed {
    ($id:literal) => {
        concat!(
            "EXISTS (SELECT 1 FROM a11y_results a WHERE a.url_id = u.id AND a.check_id = 'static.",
            $id,
            "' AND a.kind = 'violation' AND wcag_in_target(a.wcag))"
        )
    };
}

/// The number of failing elements of the static check `$id`.
macro_rules! static_elements {
    ($id:literal) => {
        concat!(
            "(SELECT a.nodes_total FROM a11y_results a WHERE a.url_id = u.id AND a.check_id = 'static.",
            $id,
            "' AND a.kind = 'violation' LIMIT 1) || ' elements'"
        )
    };
}

pub const RULES: &[Rule] = &[
    rule(
        "accessibility.form_label",
        "Accessibility: form field without a label",
        A,
        High,
        Scope::Pages,
        static_failed!("form_label"),
    )
    .needs(Need::AccessibilityStatic)
    .evidence(static_elements!("form_label"))
    .standards(&["WCAG 2.2 1.3.1", "WCAG 2.2 4.1.2"])
    .explain(
        "A form field without a label has no name for screen readers, so people using them cannot tell what to enter.",
        "Give every field a visible label tied to it with for and id, or a clear aria-label.",
    ),
    rule(
        "accessibility.button_name",
        "Accessibility: button without a name",
        A,
        High,
        Scope::Pages,
        static_failed!("button_name"),
    )
    .needs(Need::AccessibilityStatic)
    .evidence(static_elements!("button_name"))
    .standards(&["WCAG 2.2 4.1.2"])
    .explain(
        "A button with no text or label is announced only as \"button\", so people using screen readers cannot tell what it does.",
        "Put text inside the button, or give it an aria-label that says what it does.",
    ),
    rule(
        "accessibility.link_name",
        "Accessibility: link without a name",
        A,
        Medium,
        Scope::Pages,
        static_failed!("link_name"),
    )
    .needs(Need::AccessibilityStatic)
    .evidence(static_elements!("link_name"))
    .standards(&["WCAG 2.2 2.4.4", "WCAG 2.2 4.1.2"])
    .explain(
        "A link with no text, such as an icon or an image without alt text, is read out only as \"link\", so its purpose is lost.",
        "Give the link text, or alt text on its image, or an aria-label that says where it goes.",
    ),
    rule(
        "accessibility.frame_title",
        "Accessibility: frame without a title",
        A,
        Medium,
        Scope::Pages,
        static_failed!("frame_title"),
    )
    .needs(Need::AccessibilityStatic)
    .evidence(static_elements!("frame_title"))
    .standards(&["WCAG 2.2 4.1.2"])
    .explain(
        "Screen readers list frames by their title; without one people cannot tell what an embedded frame holds.",
        "Add a short title attribute to every iframe and frame that says what it contains.",
    ),
    rule(
        "accessibility.lang_invalid",
        "Accessibility: invalid page language",
        A,
        Medium,
        Scope::Pages,
        static_failed!("lang_invalid"),
    )
    .needs(Need::AccessibilityStatic)
    .evidence(static_elements!("lang_invalid"))
    .standards(&["WCAG 2.2 3.1.1"])
    .explain(
        "A language code that is not valid makes screen readers pick the wrong voice and pronunciation.",
        "Set the lang attribute on the html element to a valid code such as en or pt-BR.",
    ),
    rule(
        "accessibility.element_lang_invalid",
        "Accessibility: invalid language on an element",
        A,
        Medium,
        Scope::Pages,
        static_failed!("element_lang_invalid"),
    )
    .needs(Need::AccessibilityStatic)
    .evidence(static_elements!("element_lang_invalid"))
    .standards(&["WCAG 2.2 3.1.2"])
    .explain(
        "A lang attribute that is not a valid code on part of the page makes screen readers read that part with the wrong voice.",
        "Correct the lang attribute on those elements to a valid code, or remove it.",
    ),
    rule(
        "accessibility.tabindex_positive",
        "Accessibility: positive tabindex",
        A,
        Opportunity,
        Scope::Pages,
        static_failed!("tabindex_positive"),
    )
    .needs(Need::AccessibilityStatic)
    .evidence(static_elements!("tabindex_positive"))
    .explain(
        "A tabindex above zero changes the keyboard order away from the reading order, which confuses people who navigate with the keyboard.",
        "Use tabindex=\"0\" or none, and put elements in the HTML in the order people should reach them.",
    ),
    rule(
        "accessibility.accesskey_duplicate",
        "Accessibility: duplicate accesskey",
        A,
        Opportunity,
        Scope::Pages,
        static_failed!("accesskey_duplicate"),
    )
    .needs(Need::AccessibilityStatic)
    .evidence(static_elements!("accesskey_duplicate"))
    .explain(
        "When two elements share an accesskey only one of them can be reached with it, so the shortcut is unreliable.",
        "Give every accesskey on the page a different value, or remove the attributes.",
    ),
    rule(
        "accessibility.meta_refresh_delay",
        "Accessibility: timed page refresh",
        A,
        High,
        Scope::Pages,
        static_failed!("meta_refresh_delay"),
    )
    .needs(Need::AccessibilityStatic)
    .evidence(static_elements!("meta_refresh_delay"))
    .standards(&["WCAG 2.2 2.2.1"])
    .explain(
        "A page that reloads or redirects after a delay takes the page away before some people have finished reading it.",
        "Remove the delayed meta refresh. Use a server redirect, or let the visitor choose when to continue.",
    ),
    rule(
        "accessibility.deprecated_element",
        "Accessibility: blinking or scrolling element",
        A,
        Medium,
        Scope::Pages,
        static_failed!("deprecated_element"),
    )
    .needs(Need::AccessibilityStatic)
    .evidence(static_elements!("deprecated_element"))
    .standards(&["WCAG 2.2 2.2.2"])
    .explain(
        "The blink and marquee elements move content that people cannot pause, which is hard to read and can cause discomfort.",
        "Replace them with static content, or with an animation the visitor can pause.",
    ),
    rule(
        "accessibility.area_alt",
        "Accessibility: image map area without alt text",
        A,
        High,
        Scope::Pages,
        static_failed!("area_alt"),
    )
    .needs(Need::AccessibilityStatic)
    .evidence(static_elements!("area_alt"))
    .standards(&["WCAG 2.2 1.1.1", "WCAG 2.2 2.4.4"])
    .explain(
        "A clickable area in an image map without alt text has no name, so screen readers cannot say where it leads.",
        "Add an alt attribute to every area that says where it goes.",
    ),
    rule(
        "accessibility.input_image_alt",
        "Accessibility: image button without alt text",
        A,
        High,
        Scope::Pages,
        static_failed!("input_image_alt"),
    )
    .needs(Need::AccessibilityStatic)
    .evidence(static_elements!("input_image_alt"))
    .standards(&["WCAG 2.2 1.1.1"])
    .explain(
        "An image used as a submit button without alt text has no name, so screen readers cannot say what it does.",
        "Add an alt attribute to the input that says what the button does, such as \"Search\".",
    ),
    rule(
        "accessibility.full_violation",
        "Accessibility: violations found by the full checks",
        A,
        Medium,
        Scope::Pages,
        "EXISTS (SELECT 1 FROM a11y_results a WHERE a.url_id = u.id AND a.source = 'full' AND a.kind = 'violation' AND wcag_in_target(a.wcag))",
    )
    .needs(Need::AccessibilityFull)
    .evidence(concat!(
        "(SELECT COUNT(DISTINCT a.check_id) || ' checks failed, on ' || SUM(a.nodes_total) || ' elements' ",
        "FROM a11y_results a WHERE a.url_id = u.id AND a.source = 'full' AND a.kind = 'violation' AND wcag_in_target(a.wcag))"
    ))
    .standards(&["WCAG 2.2"])
    .explain(
        "The full checks run on the rendered page and found elements that fail an accessibility rule, such as low contrast or missing names. Automated checks find only part of the problems.",
        "Open the page's Accessibility section to see each check, the failing elements and a link to the rule's documentation.",
    ),
    rule(
        "accessibility.needs_review",
        "Accessibility: results that need a human review",
        A,
        Opportunity,
        Scope::Pages,
        "EXISTS (SELECT 1 FROM a11y_results a WHERE a.url_id = u.id AND a.kind = 'needs_review')",
    )
    .needs(Need::AccessibilityFull)
    .evidence("(SELECT COUNT(DISTINCT a.check_id) || ' checks to review' FROM a11y_results a WHERE a.url_id = u.id AND a.kind = 'needs_review')")
    .explain(
        "The checks could not decide these results by themselves. A person has to look at the elements to see whether they are a problem.",
        "Open the page's Accessibility section, review each listed element and fix those that fail.",
    ),
];
