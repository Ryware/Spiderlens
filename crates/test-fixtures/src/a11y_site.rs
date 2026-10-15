//! Pages for the static accessibility checks of spec 026 (T001): one page
//! that fails each check and one that passes it, plus clean pages. Each page
//! lists exactly the findings it must produce (check id and element count),
//! so a test can compare the whole result, not only the check it is about.

/// One fixture page.
pub struct A11yPage {
    pub path: &'static str,
    pub html: &'static str,
    /// Expected findings: static check id and failing element count.
    /// Empty means the page must produce no finding at all.
    pub expect: &'static [(&'static str, u32)],
}

macro_rules! page {
    ($path:literal, [$($id:literal: $n:literal),*], $body:literal) => {
        A11yPage {
            path: $path,
            html: concat!(
                "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\">",
                "<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">",
                "<title>", $path, "</title></head><body><main>",
                $body,
                "</main></body></html>"
            ),
            expect: &[$(($id, $n)),*],
        }
    };
}

/// Pages with a full document of their own (language and viewport checks).
macro_rules! raw_page {
    ($path:literal, [$($id:literal: $n:literal),*], $html:literal) => {
        A11yPage { path: $path, html: $html, expect: &[$(($id, $n)),*] }
    };
}

pub const STATIC_PAGES: &[A11yPage] = &[
    // form_label
    page!("/static/form_label", ["form_label": 4], r#"
        <form>
          <input type="email" id="e">
          <input type="text">
          <select id="s"><option>One</option></select>
          <textarea></textarea>
          <input type="hidden" name="token">
          <input type="submit">
        </form>"#),
    page!(
        "/static/form_label-ok",
        [],
        r#"
        <form>
          <label for="e">Email</label><input type="email" id="e">
          <label>Name <input type="text"></label>
          <input type="search" aria-label="Search">
          <span id="lbl">Country</span><select aria-labelledby="lbl"><option>One</option></select>
          <textarea placeholder="Your message"></textarea>
          <input type="text" title="Phone">
          <input type="text" id="later"><label for="later">Later label</label>
          <div hidden><input type="text"></div>
        </form>"#
    ),
    // button_name
    page!("/static/button_name", ["button_name": 4], r#"
        <button></button>
        <button><span class="icon"></span></button>
        <input type="button">
        <button><svg><path d="M0 0"/></svg></button>"#),
    page!(
        "/static/button_name-ok",
        [],
        r#"
        <button>Save</button>
        <button aria-label="Close"><span class="icon"></span></button>
        <button><img src="/i.png" alt="Search"></button>
        <input type="button" value="Go">
        <button title="Menu"></button>
        <button><svg aria-label="Play"><path d="M0 0"/></svg></button>
        <button aria-hidden="true"></button>"#
    ),
    // link_name
    page!("/static/link_name", ["link_name": 3], r#"
        <a href="/a"></a>
        <a href="/b"><img src="/b.png"></a>
        <a href="/c"><img src="/c.png" alt=""></a>
        <a name="anchor-without-href"></a>"#),
    page!(
        "/static/link_name-ok",
        [],
        r#"
        <a href="/a">Shoes</a>
        <a href="/b"><img src="/b.png" alt="Boots"></a>
        <a href="/c" aria-label="Sandals"></a>
        <a href="/d" title="Slippers"></a>
        <span id="n">Socks</span><a href="/e" aria-labelledby="n"></a>"#
    ),
    // frame_title
    page!("/static/frame_title", ["frame_title": 2], r#"
        <iframe src="/map"></iframe>
        <iframe src="/video" title=" "></iframe>"#),
    page!(
        "/static/frame_title-ok",
        [],
        r#"
        <iframe src="/map" title="Store map"></iframe>
        <iframe src="/v" aria-label="Video"></iframe>
        <iframe src="/t" hidden></iframe>"#
    ),
    // lang_invalid
    raw_page!("/static/lang_invalid", ["lang_invalid": 1], r#"<!doctype html><html lang="english"><head>
        <meta name="viewport" content="width=device-width"><title>x</title></head><body>Hello</body></html>"#),
    raw_page!(
        "/static/lang_invalid-ok",
        [],
        r#"<!doctype html><html lang="en-GB"><head>
        <meta name="viewport" content="width=device-width"><title>x</title></head><body>Hello</body></html>"#
    ),
    // element_lang_invalid
    page!("/static/element_lang_invalid", ["element_lang_invalid": 2], r#"
        <p lang="french">Bonjour</p><p lang="en_US">Hi</p>"#),
    page!(
        "/static/element_lang_invalid-ok",
        [],
        r#"
        <p lang="fr">Bonjour</p><p lang="he">שלום</p><p lang="">Unknown</p><p lang="zh-Hant-TW">你好</p>"#
    ),
    // zoom_disabled
    raw_page!("/static/zoom_disabled", ["zoom_disabled": 1], r#"<!doctype html><html lang="en"><head>
        <meta name="viewport" content="width=device-width, user-scalable=no"><title>x</title></head><body>Hi</body></html>"#),
    raw_page!("/static/zoom_disabled-max", ["zoom_disabled": 1], r#"<!doctype html><html lang="en"><head>
        <meta name="viewport" content="width=device-width; maximum-scale=1.0"><title>x</title></head><body>Hi</body></html>"#),
    raw_page!(
        "/static/zoom_disabled-ok",
        [],
        r#"<!doctype html><html lang="en"><head>
        <meta name="viewport" content="width=device-width, initial-scale=1, maximum-scale=5"><title>x</title></head><body>Hi</body></html>"#
    ),
    // viewport_missing, viewport_width
    raw_page!("/static/viewport_missing", ["viewport_missing": 1], r#"<!doctype html><html lang="en"><head>
        <title>x</title></head><body>Hi</body></html>"#),
    raw_page!("/static/viewport_width", ["viewport_width": 1], r#"<!doctype html><html lang="en"><head>
        <meta name="viewport" content="width=1024"><title>x</title></head><body>Hi</body></html>"#),
    // tabindex_positive
    page!("/static/tabindex_positive", ["tabindex_positive": 2], r#"
        <a href="/a" tabindex="1">A</a><div tabindex="3">B</div>"#),
    page!(
        "/static/tabindex_positive-ok",
        [],
        r#"
        <a href="/a" tabindex="0">A</a><div tabindex="-1">B</div><div tabindex="x">C</div>"#
    ),
    // accesskey_duplicate
    page!("/static/accesskey_duplicate", ["accesskey_duplicate": 2], r#"
        <a href="/a" accesskey="s">Search</a><a href="/b" accesskey="S">Save</a><a href="/c" accesskey="h">Home</a>"#),
    page!(
        "/static/accesskey_duplicate-ok",
        [],
        r#"
        <a href="/a" accesskey="s">Search</a><a href="/b" accesskey="v">Save</a>"#
    ),
    // meta_refresh_delay
    raw_page!("/static/meta_refresh_delay", ["meta_refresh_delay": 1], r#"<!doctype html><html lang="en"><head>
        <meta name="viewport" content="width=device-width">
        <meta http-equiv="refresh" content="30; url=/next"><title>x</title></head><body>Hi</body></html>"#),
    raw_page!(
        "/static/meta_refresh_delay-ok",
        [],
        r#"<!doctype html><html lang="en"><head>
        <meta name="viewport" content="width=device-width">
        <meta http-equiv="refresh" content="0; url=/next"><title>x</title></head><body>Hi</body></html>"#
    ),
    // deprecated_element, plugin_element
    page!("/static/deprecated_element", ["deprecated_element": 2], r#"
        <marquee>Sale</marquee><blink>New</blink>"#),
    page!("/static/plugin_element", ["plugin_element": 2], r#"
        <object data="/movie.swf"></object><embed src="/game.swf">"#),
    // area_alt, input_image_alt
    page!("/static/area_alt", ["area_alt": 1], r##"
        <img src="/m.png" usemap="#m" alt="Map">
        <map name="m"><area href="/n" shape="rect" coords="0,0,9,9"><area href="/s" alt="South" shape="rect" coords="9,9,19,19"></map>"##),
    page!("/static/input_image_alt", ["input_image_alt": 1], r#"
        <form><input type="image" src="/go.png"><input type="image" src="/ok.png" alt="Search"></form>"#),
    // noscript content and hidden subtrees never count
    page!(
        "/static/noscript-ok",
        [],
        r#"
        <noscript><a href="/x"></a><input type="text"></noscript>
        <div aria-hidden="true"><a href="/y"></a></div>"#
    ),
    // several checks on one page
    page!("/static/mixed", ["button_name": 1, "form_label": 1, "link_name": 1], r#"
        <header><a href="/"><img src="/logo.png"></a></header>
        <form><input type="text" id="q"><button></button></form>"#),
    page!(
        "/clean/1",
        [],
        r#"
        <h1>Shoes</h1><p>Our range.</p>
        <nav><a href="/a">Boots</a> <a href="/b">Sandals</a></nav>
        <form><label for="q">Search</label><input id="q" type="search"><button>Go</button></form>
        <img src="/s.png" alt="A red shoe">"#
    ),
];

/// A page with `n` failing links, to test the node cap.
pub fn many_unnamed_links(n: usize) -> String {
    let mut body = String::from(
        "<!doctype html><html lang=\"en\"><head><meta name=\"viewport\" content=\"width=device-width\">\
         <title>many</title></head><body>",
    );
    for i in 0..n {
        body.push_str(&format!("<a href=\"/p/{i}\"></a>"));
    }
    body.push_str("</body></html>");
    body
}
