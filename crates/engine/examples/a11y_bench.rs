//! Spec 026 T011: the cost of the static accessibility checks inside page
//! extraction. Extraction is only part of a crawl's work per page, so the
//! crawl slows down by at most this share (SC-003 allows 3%).
//!
//! `cargo run --release -p spiderlens-engine --example a11y_bench`

use std::hint::black_box;
use std::time::{Duration, Instant};

use spiderlens_engine::extract::{PageInput, extract_page};
use url::Url;

/// The 100,000-page benchmark site's page shape (bench-site.rs).
fn bench_page(n: u64) -> String {
    let mut h = format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"><title>Page {n}</title>\
         <meta name=\"description\" content=\"Page {n} of the benchmark\"></head><body><nav>"
    );
    for i in 0..20 {
        h.push_str(&format!("<a href=\"/p/{i}\">Section {i}</a> "));
    }
    h.push_str(&format!("</nav><main><h1>Page {n}</h1>"));
    for _ in 0..8 {
        h.push_str("<p>Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod tempor.</p>");
    }
    h.push_str("<ul>");
    for c in 0..10 {
        h.push_str(&format!("<li><a href=\"/p/{}\">Child {c}</a></li>", n * 10 + c));
    }
    h.push_str("</ul></main></body></html>");
    h
}

/// A heavier, more typical commercial page: header, forms, images, cards.
fn rich_page() -> String {
    let mut h = String::from(
        "<!doctype html><html lang=\"en\"><head><meta name=\"viewport\" content=\"width=device-width\">\
         <title>Shop</title></head><body><header><a href=\"/\"><img src=\"/logo.svg\" alt=\"Home\"></a>\
         <form><label for=\"q\">Search</label><input id=\"q\" type=\"search\"><button>Go</button></form></header><main>",
    );
    for i in 0..60 {
        h.push_str(&format!(
            "<div class=\"card\"><a href=\"/p/{i}\"><img src=\"/i/{i}.webp\" alt=\"Product {i}\" width=\"200\" height=\"200\"></a>\
             <h3><a href=\"/p/{i}\">Product {i}</a></h3><p class=\"price\"><span>$</span>{i}.99</p>\
             <button aria-label=\"Add product {i} to cart\"><svg><path d=\"M0 0\"/></svg></button></div>"
        ));
    }
    h.push_str("</main><footer><form><input type=\"email\" placeholder=\"Email\"><button>Join</button></form></footer></body></html>");
    h
}

fn time(pages: &[String], a11y: bool, rounds: usize) -> Duration {
    let url = Url::parse("https://example.test/").unwrap();
    let started = Instant::now();
    for _ in 0..rounds {
        for p in pages {
            black_box(extract_page(&PageInput {
                url: &url,
                charset: None,
                body: p.as_bytes(),
                truncated: false,
                robots_token: "SpiderLens",
                content: None,
                a11y,
                hook: None,
            }));
        }
    }
    started.elapsed()
}

fn main() {
    for (name, pages) in [
        ("bench-site pages", (0..200).map(bench_page).collect::<Vec<_>>()),
        ("rich pages", vec![rich_page(); 50]),
    ] {
        time(&pages, true, 3);
        // Alternate off and on, 15 pairs; the fastest run of each is the
        // least disturbed by other work on the machine.
        let (mut off, mut on) = (Duration::MAX, Duration::MAX);
        for _ in 0..15 {
            off = off.min(time(&pages, false, 10));
            on = on.min(time(&pages, true, 10));
        }
        let per = |d: Duration| d.as_secs_f64() * 1e6 / (10.0 * pages.len() as f64);
        println!(
            "{name}: off {:.1} µs/page, on {:.1} µs/page, extra {:+.1}% of extraction",
            per(off),
            per(on),
            (on.as_secs_f64() / off.as_secs_f64() - 1.0) * 100.0
        );
    }
}
