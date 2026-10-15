//! The near duplicate fixture site (spec 020 T012): groups of pages with
//! known similarity of their main content, shared site chrome, a large
//! "related products" block, short pages, non-indexable copies and a
//! template group.

use crate::{Site, SiteBuilder};

pub const HOST: &str = "dup.test";

/// Deterministic filler words: `n` words unique to `seed`.
pub fn words(seed: &str, n: usize) -> Vec<String> {
    (0..n).map(|i| format!("{seed}{i}")).collect()
}

/// `base` with `change` words replaced, spread through the text.
pub fn variant(base: &[String], change: usize, tag: &str) -> Vec<String> {
    let mut v = base.to_vec();
    for k in 0..change {
        let at = (k * 7919 + 13) % v.len();
        v[at] = format!("{tag}{k}");
    }
    v
}

/// A page with site chrome around `main` and optional extra head tags.
pub fn page(main: &[String], chrome: &str, head: &str, extra_body: &str) -> String {
    let paragraphs: String = main
        .chunks(60)
        .map(|c| format!("<p>{}</p>", c.join(" ")))
        .collect();
    format!(
        "<!doctype html><html><head><title>Page</title>{head}</head><body>\
         <header><a href=\"/\">Logo</a> {chrome}</header><nav>{chrome} <a href=\"/\">Home</a></nav>\
         <main>{paragraphs}{extra_body}</main><footer>{chrome} footer</footer></body></html>"
    )
}

/// Paths and what the tests expect of them.
pub struct Expect;

impl Expect {
    /// Same main content, different chrome: 100%, exact.
    pub const EXACT: [&'static str; 2] = ["/shoe-red", "/shoe-blue"];
    /// One sentence changed: 90 to 99%.
    pub const CLOSE: &'static str = "/shoe-green";
    /// Around 85% to the shoe pages: under 90, joins at 80.
    pub const EIGHTY_FIVE: &'static str = "/shoe-old";
    /// Unrelated content, same chrome.
    pub const UNRELATED: &'static str = "/about";
    /// Different 60-word content plus a shared 1,500-word related block.
    pub const RELATED: [&'static str; 2] = ["/bag-a", "/bag-b"];
    /// Under the 50-word minimum.
    pub const SHORT: &'static str = "/short";
    pub const NOINDEX: &'static str = "/shoe-print";
    pub const CANONICAL: &'static str = "/shoe-red-amp";
    /// Template group size.
    pub const CITIES: usize = 12;
}

/// Builds the site. Every page is linked from `/`.
pub fn site() -> Site {
    let shoe = words("shoe", 600);
    let chrome_a = words("chromea", 300).join(" ");
    let chrome_b = words("chromeb", 300).join(" ");
    let related: String = format!(
        "<div class=\"related-products\">{}</div>",
        words("related", 1500).join(" ")
    );
    let city = words("city", 300);
    let mut links: Vec<String> = Vec::new();
    let mut b = SiteBuilder::new();
    let mut add = |b: SiteBuilder, path: &str, html: String| {
        links.push(path.to_string());
        b.html(HOST, path, &html)
    };
    b = add(b, "/shoe-red", page(&shoe, &chrome_a, "", ""));
    b = add(b, "/shoe-blue", page(&shoe, &chrome_b, "", ""));
    b = add(
        b,
        "/shoe-green",
        page(&variant(&shoe, 3, "green"), &chrome_a, "", ""),
    );
    b = add(
        b,
        "/shoe-old",
        page(&variant(&shoe, 10, "old"), &chrome_a, "", ""),
    );
    b = add(b, "/about", page(&words("about", 400), &chrome_a, "", ""));
    b = add(b, "/bag-a", page(&words("baga", 60), &chrome_a, "", &related));
    b = add(b, "/bag-b", page(&words("bagb", 60), &chrome_a, "", &related));
    b = add(b, "/short", page(&words("short", 20), &chrome_a, "", ""));
    b = add(
        b,
        "/shoe-print",
        page(&shoe, &chrome_a, "<meta name=\"robots\" content=\"noindex\">", ""),
    );
    b = add(
        b,
        "/shoe-red-amp",
        page(
            &shoe,
            &chrome_a,
            "<link rel=\"canonical\" href=\"/shoe-red\">",
            "",
        ),
    );
    for i in 0..Expect::CITIES {
        let mut v = city.clone();
        v[150] = format!("cityname{i}");
        b = add(b, &format!("/city/{i}"), page(&v, &chrome_a, "", ""));
    }
    let home: String = links.iter().map(|l| format!("<a href=\"{l}\">x</a>")).collect();
    b.html(
        HOST,
        "/",
        &format!("<!doctype html><title>Home</title><body>{home}</body>"),
    )
    .start()
}
