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

/// A page for the full checks (axe-core, spec 026 T045): the page at
/// `/full/<axe id>` must report `axe_id` as a violation, and the page at
/// `/full/<axe id>-ok` must not report it.
pub struct FullPage {
    pub path: &'static str,
    pub html: &'static str,
    pub axe_id: &'static str,
    /// Whether the page must report `axe_id` as a violation.
    pub violates: bool,
}

/// A whole document with a language, viewport, title, one main landmark and
/// one level-one heading around `$body`.
macro_rules! wrap {
    ($body:literal) => {
        concat!(
            "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\">",
            "<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">",
            "<title>Full check page</title></head><body><main><h1>Page</h1>",
            $body,
            "</main></body></html>"
        )
    };
}

macro_rules! full_pages {
    ($($id:literal => $fail:expr, $ok:expr;)*) => {
        pub const FULL_PAGES: &[FullPage] = &[$(
            FullPage { path: concat!("/full/", $id), html: $fail, axe_id: $id, violates: true },
            FullPage { path: concat!("/full/", $id, "-ok"), html: $ok, axe_id: $id, violates: false },
        )*];
    };
}

full_pages! {
    "image-alt" => wrap!(r#"<img src="/x.png">"#), wrap!(r#"<img src="/x.png" alt="A red shoe">"#);
    "label" => wrap!(r#"<input type="text" id="a">"#),
        wrap!(r#"<label for="a">Name</label><input type="text" id="a">"#);
    "button-name" => wrap!(r#"<button></button>"#), wrap!(r#"<button>Save</button>"#);
    "link-name" => wrap!(r#"<a href="/a"></a>"#), wrap!(r#"<a href="/a">Shoes</a>"#);
    "color-contrast" => wrap!(r#"<p style="color:#aaa;background:#fff">Pale text on white</p>"#),
        wrap!(r#"<p style="color:#000;background:#fff">Dark text on white</p>"#);
    "html-has-lang" => r#"<!doctype html><html><head><meta charset="utf-8">
        <meta name="viewport" content="width=device-width"><title>No language</title></head>
        <body><main><h1>Page</h1></main></body></html>"#,
        r#"<!doctype html><html lang="en"><head><meta charset="utf-8">
        <meta name="viewport" content="width=device-width"><title>Language</title></head>
        <body><main><h1>Page</h1></main></body></html>"#;
    "html-lang-valid" => r#"<!doctype html><html lang="english"><head><meta charset="utf-8">
        <meta name="viewport" content="width=device-width"><title>Bad language</title></head>
        <body><main><h1>Page</h1></main></body></html>"#,
        r#"<!doctype html><html lang="en-GB"><head><meta charset="utf-8">
        <meta name="viewport" content="width=device-width"><title>Good language</title></head>
        <body><main><h1>Page</h1></main></body></html>"#;
    "document-title" => r#"<!doctype html><html lang="en"><head><meta charset="utf-8">
        <meta name="viewport" content="width=device-width"></head>
        <body><main><h1>Page</h1></main></body></html>"#,
        r#"<!doctype html><html lang="en"><head><meta charset="utf-8">
        <meta name="viewport" content="width=device-width"><title>A title</title></head>
        <body><main><h1>Page</h1></main></body></html>"#;
    "duplicate-id-aria" => wrap!(r#"<div id="dup">A</div><div id="dup">B</div><button aria-labelledby="dup">Go</button>"#),
        wrap!(r#"<div id="one">A</div><button aria-labelledby="one">Go</button>"#);
    "aria-required-attr" => wrap!(r#"<div role="checkbox" tabindex="0">Option</div>"#),
        wrap!(r#"<div role="checkbox" tabindex="0" aria-checked="false">Option</div>"#);
    "aria-valid-attr" => wrap!(r#"<div aria-foo="x">Text</div>"#), wrap!(r#"<div aria-label="Text">Text</div>"#);
    "aria-roles" => wrap!(r#"<div role="madeup">Text</div>"#), wrap!(r#"<div role="note">Text</div>"#);
    "list" => wrap!(r#"<ul><div>Not an item</div></ul>"#), wrap!(r#"<ul><li>Item</li></ul>"#);
    "listitem" => wrap!(r#"<li>Orphan item</li>"#), wrap!(r#"<ul><li>Item</li></ul>"#);
    "definition-list" => wrap!(r#"<dl><p>Not a term</p></dl>"#),
        wrap!(r#"<dl><dt>Term</dt><dd>Definition</dd></dl>"#);
    "dlitem" => wrap!(r#"<dt>Term</dt><dd>Definition</dd>"#),
        wrap!(r#"<dl><dt>Term</dt><dd>Definition</dd></dl>"#);
    "th-has-data-cells" => wrap!(r#"<table><tr><th>A</th><th>B</th></tr></table>"#),
        wrap!(r#"<table><tr><th>A</th><th>B</th></tr><tr><td>1</td><td>2</td></tr></table>"#);
    "td-headers-attr" => wrap!(r#"<table><tr><th id="h1">H</th></tr><tr><td headers="nope">v</td></tr></table>"#),
        wrap!(r#"<table><tr><th id="h1">H</th></tr><tr><td headers="h1">v</td></tr></table>"#);
    "frame-title" => wrap!(r#"<iframe src="/frame"></iframe>"#),
        wrap!(r#"<iframe src="/frame" title="Store map"></iframe>"#);
    "input-image-alt" => wrap!(r#"<form><input type="image" src="/go.png"></form>"#),
        wrap!(r#"<form><input type="image" src="/go.png" alt="Search"></form>"#);
    "input-button-name" => wrap!(r#"<form><input type="button"></form>"#),
        wrap!(r#"<form><input type="button" value="Go"></form>"#);
    "select-name" => wrap!(r#"<select><option>One</option></select>"#),
        wrap!(r#"<label for="s">Pick</label><select id="s"><option>One</option></select>"#);
    "area-alt" => wrap!(r##"<img src="/m.png" usemap="#m" alt="Map"><map name="m"><area href="/n" shape="rect" coords="0,0,9,9"></map>"##),
        wrap!(r##"<img src="/m.png" usemap="#m" alt="Map"><map name="m"><area href="/n" alt="North" shape="rect" coords="0,0,9,9"></map>"##);
    "valid-lang" => wrap!(r#"<p lang="french">Bonjour</p>"#), wrap!(r#"<p lang="fr">Bonjour</p>"#);
    "meta-viewport" => r#"<!doctype html><html lang="en"><head><meta charset="utf-8">
        <meta name="viewport" content="width=device-width, user-scalable=no"><title>Zoom off</title></head>
        <body><main><h1>Page</h1></main></body></html>"#,
        r#"<!doctype html><html lang="en"><head><meta charset="utf-8">
        <meta name="viewport" content="width=device-width, initial-scale=1"><title>Zoom on</title></head>
        <body><main><h1>Page</h1></main></body></html>"#;
    "meta-refresh" => r#"<!doctype html><html lang="en"><head><meta charset="utf-8">
        <meta name="viewport" content="width=device-width">
        <meta http-equiv="refresh" content="3600; url=/next"><title>Refresh</title></head>
        <body><main><h1>Page</h1></main></body></html>"#,
        r#"<!doctype html><html lang="en"><head><meta charset="utf-8">
        <meta name="viewport" content="width=device-width"><title>No refresh</title></head>
        <body><main><h1>Page</h1></main></body></html>"#;
    "tabindex" => wrap!(r#"<div tabindex="3">Focusable</div>"#), wrap!(r#"<div tabindex="0">Focusable</div>"#);
    "landmark-one-main" => r#"<!doctype html><html lang="en"><head><meta charset="utf-8">
        <meta name="viewport" content="width=device-width"><title>No main</title></head>
        <body><h1>Page</h1><p>Text without a main landmark.</p></body></html>"#,
        r#"<!doctype html><html lang="en"><head><meta charset="utf-8">
        <meta name="viewport" content="width=device-width"><title>Main</title></head>
        <body><main><h1>Page</h1><p>Text inside the main landmark.</p></main></body></html>"#;
    "region" => r#"<!doctype html><html lang="en"><head><meta charset="utf-8">
        <meta name="viewport" content="width=device-width"><title>Loose</title></head>
        <body><main><h1>Page</h1></main><p>Text outside every landmark.</p></body></html>"#,
        r#"<!doctype html><html lang="en"><head><meta charset="utf-8">
        <meta name="viewport" content="width=device-width"><title>Inside</title></head>
        <body><main><h1>Page</h1><p>Text inside the landmark.</p></main></body></html>"#;
    "heading-order" => wrap!(r#"<h3>Skipped a level</h3>"#), wrap!(r#"<h2>Next level</h2>"#);
    "page-has-heading-one" => r#"<!doctype html><html lang="en"><head><meta charset="utf-8">
        <meta name="viewport" content="width=device-width"><title>No h1</title></head>
        <body><main><h2>Only a level two</h2></main></body></html>"#,
        r#"<!doctype html><html lang="en"><head><meta charset="utf-8">
        <meta name="viewport" content="width=device-width"><title>One h1</title></head>
        <body><main><h1>Page</h1></main></body></html>"#;
    "empty-heading" => wrap!(r#"<h2></h2>"#), wrap!(r#"<h2>With text</h2>"#);
    "nested-interactive" => wrap!(r#"<div role="button" tabindex="0"><a href="/x">Go</a></div>"#),
        wrap!(r#"<div role="button" tabindex="0">Go</div>"#);
    "aria-hidden-focus" => wrap!(r#"<div aria-hidden="true"><button>Go</button></div>"#),
        wrap!(r#"<div aria-hidden="true">Decoration</div>"#);
}

/// A page whose script adds an unlabelled input after the page loaded: only
/// the full checks, which run on the rendered page, can see it (`label`).
pub const SCRIPT_ADDED: FullPage = FullPage {
    path: "/full/script-added",
    html: wrap!(
        r#"<p>Loading</p><script>window.addEventListener('load', function () {
          document.querySelector('main').insertAdjacentHTML('beforeend', '<input type="text" id="late">');
        });</script>"#
    ),
    axe_id: "label",
    violates: true,
};

/// Text over a background image: the contrast cannot be decided, so the full
/// checks report `color-contrast` as needing a review, not as a violation.
pub const CONTRAST_OVER_IMAGE: FullPage = FullPage {
    path: "/full/contrast-over-image",
    html: wrap!(
        r#"<div style="background-image:url(data:image/gif;base64,R0lGODlhAQABAIAAAAAAAP///yH5BAEAAAAALAAAAAABAAEAAAIBRAA7);color:#777;width:300px;height:60px">Text over an image</div>"#
    ),
    axe_id: "color-contrast",
    violates: false,
};

/// A page for the mobile measures (spec 026). The mobile window is 411 px
/// wide; the default thresholds are 12 px text, 40 % small text and 24 px
/// tap targets.
pub struct MobilePage {
    pub path: &'static str,
    pub html: &'static str,
    pub content_wider: bool,
    pub small_text: bool,
    pub small_targets: bool,
}

macro_rules! mobile_doc {
    ($body:literal) => {
        concat!(
            "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\">",
            "<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">",
            "<title>Mobile page</title></head><body style=\"margin:0\">",
            $body,
            "</body></html>"
        )
    };
}

pub const MOBILE_PAGES: &[MobilePage] = &[
    MobilePage {
        path: "/mobile/content-wider",
        html: mobile_doc!(
            r#"<p style="font-size:16px">Intro text.</p>
            <div style="width:1400px;height:30px;background:#ccc;font-size:16px">A block far wider than the screen</div>"#
        ),
        content_wider: true,
        small_text: false,
        small_targets: false,
    },
    MobilePage {
        path: "/mobile/small-text",
        html: mobile_doc!(
            r#"<p style="font-size:8px">Tiny text one that goes on and on for a while so that it counts.</p>
            <p style="font-size:8px">Tiny text two that goes on and on for a while so that it counts.</p>
            <p style="font-size:8px">Tiny text three that goes on and on for a while so that it counts.</p>
            <p style="font-size:16px">Ok.</p>"#
        ),
        content_wider: false,
        small_text: true,
        small_targets: false,
    },
    MobilePage {
        path: "/mobile/small-targets",
        html: mobile_doc!(
            r#"<p style="font-size:16px">Links:</p>
            <a href="/a" style="display:inline-block;width:10px;height:10px;margin:2px;overflow:hidden;font-size:16px">a</a>
            <a href="/b" style="display:inline-block;width:10px;height:10px;margin:2px;overflow:hidden;font-size:16px">b</a>
            <a href="/c" style="display:inline-block;width:10px;height:10px;margin:2px;overflow:hidden;font-size:16px">c</a>
            <a href="/d" style="display:inline-block;width:10px;height:10px;margin:2px;overflow:hidden;font-size:16px">d</a>"#
        ),
        content_wider: false,
        small_text: false,
        small_targets: true,
    },
    MobilePage {
        path: "/mobile/ok",
        html: mobile_doc!(
            r#"<p style="font-size:16px">Plain readable text that fits the screen.</p>
            <a href="/a" style="display:block;width:200px;height:48px;font-size:16px">Large link</a>
            <a href="/b" style="display:block;width:200px;height:48px;margin-top:8px;font-size:16px">Other link</a>"#
        ),
        content_wider: false,
        small_text: false,
        small_targets: false,
    },
];
