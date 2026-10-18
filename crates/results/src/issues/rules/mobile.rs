//! Mobile usability (spec 026). The viewport, zoom and plugin checks read
//! the HTML alone; the layout rules need a mobile-sized window and the
//! `mobile_measures` of a rendered crawl. Thresholds are the crawl's
//! accessibility settings (`a11y_setting`).

use crate::issues::{Category::Mobile as M, Priority::*, Rule, Scope, rule};
use crate::tabs::Need;

const VIEWPORT_STATE: &str = "(SELECT m.viewport_state FROM mobile_measures m WHERE m.url_id = u.id)";

pub const VIEWPORT_MISSING: &str =
    "EXISTS (SELECT 1 FROM mobile_measures m WHERE m.url_id = u.id AND m.viewport_state = 'missing')";
pub const VIEWPORT_NOT_DEVICE: &str = "EXISTS (SELECT 1 FROM mobile_measures m WHERE m.url_id = u.id AND m.viewport_state = 'not_device_width')";
pub const ZOOM_DISABLED: &str =
    "EXISTS (SELECT 1 FROM mobile_measures m WHERE m.url_id = u.id AND m.viewport_state = 'zoom_disabled')";
pub const PLUGINS: &str = "EXISTS (SELECT 1 FROM mobile_measures m WHERE m.url_id = u.id AND m.plugins > 0)";
pub const CONTENT_WIDER: &str = "EXISTS (SELECT 1 FROM mobile_measures m WHERE m.url_id = u.id AND m.measured = 'yes' \
     AND m.content_px - m.window_px > a11y_setting('overflow_px'))";
pub const SMALL_TEXT: &str = "EXISTS (SELECT 1 FROM mobile_measures m WHERE m.url_id = u.id AND m.measured = 'yes' \
     AND m.small_text_share > a11y_setting('small_text_share'))";
pub const SMALL_TARGETS: &str = "EXISTS (SELECT 1 FROM mobile_measures m WHERE m.url_id = u.id AND m.measured = 'yes' \
     AND m.small_targets > 0)";

pub const RULES: &[Rule] = &[
    rule(
        "mobile.viewport_missing",
        "Mobile: no viewport meta tag",
        M,
        Low,
        Scope::Pages,
        VIEWPORT_MISSING,
    )
    .needs(Need::AccessibilityStatic)
    .evidence(VIEWPORT_STATE)
    .explain(
        "Without a viewport tag phones show the page as a zoomed-out desktop page that visitors have to pinch and scroll.",
        "Add <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\"> to the head.",
    ),
    rule(
        "mobile.viewport_width",
        "Mobile: viewport is not the device width",
        M,
        Low,
        Scope::Pages,
        VIEWPORT_NOT_DEVICE,
    )
    .needs(Need::AccessibilityStatic)
    .evidence("(SELECT m.viewport FROM mobile_measures m WHERE m.url_id = u.id)")
    .explain(
        "A viewport with a fixed width, or without width=device-width, does not fit the page to the phone's screen.",
        "Set the viewport to width=device-width, initial-scale=1.",
    ),
    rule(
        "mobile.zoom_disabled",
        "Mobile: pinch zoom is disabled",
        M,
        High,
        Scope::Pages,
        ZOOM_DISABLED,
    )
    .needs(Need::AccessibilityStatic)
    .evidence("(SELECT m.viewport FROM mobile_measures m WHERE m.url_id = u.id)")
    .standards(&["WCAG 2.2 1.4.4"])
    .explain(
        "A viewport with user-scalable=no or a low maximum-scale stops people with low vision from zooming in.",
        "Remove user-scalable=no and maximum-scale from the viewport tag.",
    ),
    rule(
        "mobile.plugins",
        "Mobile: uses object, embed or applet",
        M,
        Low,
        Scope::Pages,
        PLUGINS,
    )
    .needs(Need::AccessibilityStatic)
    .evidence("(SELECT m.plugins || ' plugin elements' FROM mobile_measures m WHERE m.url_id = u.id)")
    .explain(
        "Plugin content such as Flash or Java applets does not run on phones, so the content is missing for those visitors.",
        "Replace the plugin content with HTML5 video, audio or images.",
    ),
    rule(
        "mobile.content_wider",
        "Mobile: content wider than the screen",
        M,
        Medium,
        Scope::Pages,
        CONTENT_WIDER,
    )
    .needs(Need::MobileLayout)
    .evidence("(SELECT CAST(m.content_px AS INTEGER) || ' px wide on a ' || CAST(m.window_px AS INTEGER) || ' px screen' FROM mobile_measures m WHERE m.url_id = u.id)")
    .explain(
        "Content that is wider than the screen makes visitors scroll sideways, which is hard to use on a phone.",
        "Find the widest element (shown in the page's Mobile section) and make it fit, with max-width: 100% or a responsive layout.",
    ),
    rule(
        "mobile.small_text",
        "Mobile: text too small",
        M,
        Medium,
        Scope::Pages,
        SMALL_TEXT,
    )
    .needs(Need::MobileLayout)
    .evidence("(SELECT CAST(m.small_text_share AS INTEGER) || '% of the text is below ' || a11y_setting('small_text_px') || ' px' FROM mobile_measures m WHERE m.url_id = u.id)")
    .explain(
        "Small text is hard to read on a phone without zooming in.",
        "Raise the base font size so body text is at least 16 px, and avoid fixed small sizes.",
    ),
    rule(
        "mobile.small_targets",
        "Mobile: tap targets too small",
        M,
        Medium,
        Scope::Pages,
        SMALL_TARGETS,
    )
    .needs(Need::MobileLayout)
    .evidence("(SELECT m.small_targets || ' targets under ' || a11y_setting('tap_target_px') || ' px' FROM mobile_measures m WHERE m.url_id = u.id)")
    .standards(&["WCAG 2.2 2.5.8"])
    .explain(
        "Links and buttons that are small or close together are hard to tap, and easy to miss for people with limited dexterity.",
        "Make interactive elements at least 24 by 24 CSS pixels, or leave enough space around them.",
    ),
];
