//! "Possible bot challenge" (spec 014 FR-044, plan D5).
//!
//! A rendered page is flagged when it looks like a security check or
//! interstitial page instead of the content the raw HTML had: a known
//! challenge title or a challenge script or form marker, on a short page.
//! A page that merely mentions "captcha", or a long page with a captcha
//! widget on a contact form, is not flagged.

use super::PageData;

/// Titles of challenge and interstitial pages, compared lowercased and
/// trimmed. A title equal to one of these, or starting with it, matches.
const TITLES: &[&str] = &[
    "just a moment",
    "attention required!",
    "checking your browser",
    "please wait while we verify",
    "verify you are human",
    "are you a robot",
    "human verification",
    "security check",
    "one more step",
    "pardon our interruption",
    "ddos-guard",
    "access denied",
    "request blocked",
    "please enable js and disable any ad blocker",
];

/// Script, form and element markers that only challenge pages carry.
const STRONG: &[&str] = &[
    "/cdn-cgi/challenge-platform/",
    "_cf_chl_opt",
    "cf-browser-verification",
    "id=\"challenge-form\"",
    "captcha-delivery.com",
    "_Incapsula_Resource",
    "px-captcha",
    "sec-if-cpt-container",
    "/.well-known/sgcaptcha/",
    "ddos-guard.net/",
];

/// Captcha widgets that also appear on ordinary forms.
const WEAK: &[&str] = &["g-recaptcha", "h-captcha", "cf-turnstile"];

/// Challenge pages are short. Longer pages are treated as content.
const SHORT_PAGE_WORDS: u32 = 300;

/// Whether the rendered page looks like a bot challenge instead of content.
pub fn is_bot_challenge(rendered_html: &str, rendered: &PageData, raw: &PageData) -> bool {
    if rendered.word_count >= SHORT_PAGE_WORDS {
        return false;
    }
    let title = rendered
        .title()
        .map(|t| t.trim().to_lowercase())
        .unwrap_or_default();
    let title_match = !title.is_empty() && TITLES.iter().any(|t| title.starts_with(t));
    if title_match || STRONG.iter().any(|m| rendered_html.contains(m)) {
        return true;
    }
    // A captcha widget alone counts only when rendering lost most of the text.
    WEAK.iter().any(|m| rendered_html.contains(m))
        && u64::from(rendered.word_count) * 2 < u64::from(raw.word_count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extract::{PageInput, extract_page};
    use url::Url;

    fn data(html: &str) -> PageData {
        let url = Url::parse("https://shop.test/p/1").unwrap();
        extract_page(&PageInput {
            url: &url,
            charset: None,
            body: html.as_bytes(),
            truncated: false,
            robots_token: "SpiderLens",
            content: None,
            keep_text: false,
            a11y: false,
            hook: None,
        })
    }

    fn check(rendered: &str, raw: &str) -> bool {
        is_bot_challenge(rendered, &data(rendered), &data(raw))
    }

    fn article(words: usize, extra: &str) -> String {
        let body = "word ".repeat(words);
        format!("<html><head><title>Product</title></head><body><p>{body}</p>{extra}</body></html>")
    }

    const CLOUDFLARE: &str = r#"<!DOCTYPE html><html lang="en-US"><head><title>Just a moment...</title>
        <meta http-equiv="refresh" content="390"></head><body><div class="main-wrapper" role="main">
        <div class="main-content"><h1 class="zone-name-title h1">shop.test</h1>
        <h2 class="h2" id="challenge-running">Checking if the site connection is secure</h2>
        <noscript><div id="challenge-error-title">Enable JavaScript and cookies to continue</div></noscript>
        </div></div><script>(function(){window._cf_chl_opt={cvId: '3',cZone: "shop.test"};
        var cpo = document.createElement('script');cpo.src = '/cdn-cgi/challenge-platform/h/b/orchestrate/chl_page/v1';
        document.getElementsByTagName('head')[0].appendChild(cpo);}());</script></body></html>"#;

    const CLOUDFLARE_BLOCK: &str = r#"<html><head><title>Attention Required! | Cloudflare</title></head>
        <body><div id="cf-wrapper"><h1>Sorry, you have been blocked</h1>
        <h2>You are unable to access shop.test</h2></div></body></html>"#;

    const DATADOME: &str = r#"<html><head><title>shop.test</title></head><body>
        <p>Please enable JS and disable any ad blocker</p>
        <script>var dd={'rt':'c','cid':'AHrlqAAAAAMA','hsh':'2211F522','t':'fe','host':'geo.captcha-delivery.com'}</script>
        <script src="https://ct.captcha-delivery.com/c.js"></script></body></html>"#;

    const PERIMETERX: &str = r#"<html><head><title>Access to this page has been denied</title></head>
        <body><div id="px-captcha"></div><p>Press and hold to confirm you are a human.</p></body></html>"#;

    #[test]
    fn saved_challenge_pages_are_flagged() {
        let raw = article(800, "");
        assert!(check(CLOUDFLARE, &raw));
        assert!(check(CLOUDFLARE_BLOCK, &raw));
        assert!(check(DATADOME, &raw));
        assert!(check(PERIMETERX, &raw));
        // Also when the raw HTML was the challenge too.
        assert!(check(CLOUDFLARE, CLOUDFLARE));
    }

    #[test]
    fn pages_that_mention_captcha_are_not_flagged() {
        let blog = "<html><head><title>How CAPTCHA works</title></head><body>\
                    <h1>What is a captcha?</h1><p>A captcha is a test that tells humans and bots apart. \
                    Google reCAPTCHA and hCaptcha are common.</p></body></html>";
        assert!(!check(blog, blog));
        let normal = article(500, "");
        assert!(!check(&normal, &normal));
    }

    #[test]
    fn a_captcha_on_a_form_is_not_a_challenge() {
        let contact = article(
            120,
            r#"<form><div class="g-recaptcha" data-sitekey="x"></div><button>Send</button></form>"#,
        );
        // Same amount of text raw and rendered: an ordinary contact form.
        assert!(!check(&contact, &contact));
        // A widget on a long page is never a challenge.
        let long = article(900, r#"<div class="cf-turnstile"></div>"#);
        assert!(!check(&long, &long));
        // But a widget that replaced most of the raw content is.
        let rendered = article(20, r#"<div class="h-captcha"></div>"#);
        assert!(check(&rendered, &article(400, "")));
    }

    #[test]
    fn a_long_page_with_a_challenge_title_word_is_content() {
        let page = format!(
            "<html><head><title>Security check list for shops</title></head><body><p>{}</p></body></html>",
            "word ".repeat(600)
        );
        assert!(!check(&page, &page));
    }
}
