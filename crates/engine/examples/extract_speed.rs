//! Per-page extraction cost on a typical page (spec 020 T008, SC-004):
//! `cargo run --release -p spiderlens-engine --example extract_speed`

use spiderlens_engine::extract::{PageInput, extract_page};

fn main() {
    let mut html = String::from(
        "<!doctype html><html><head><title>Product</title><meta name=description content=\"A product\"></head><body><header><nav>",
    );
    for i in 0..60 {
        html.push_str(&format!("<a href=\"/c/{i}\">Category {i}</a> "));
    }
    html.push_str("</nav></header><main><h1>Hand made shoes</h1>");
    for p in 0..20 {
        html.push_str("<p>");
        for w in 0..40 {
            html.push_str(&format!("word{} ", (p * 40 + w) * 7919 % 5000));
        }
        html.push_str(&format!("<a href=\"/p/{p}\">more</a></p>"));
    }
    html.push_str("</main><aside>Related <a href=/r>items</a></aside><footer>© Shop · <a href=/privacy>Privacy</a></footer></body></html>");
    let url = url::Url::parse("https://shop.test/p/1").unwrap();
    let input = PageInput {
        url: &url,
        charset: Some("utf-8"),
        body: html.as_bytes(),
        truncated: false,
        robots_token: "SpiderLens",
        content: None,
        keep_text: false,
        a11y: false,
        hook: None,
    };
    for _ in 0..200 {
        extract_page(&input);
    }
    // Best of several rounds: the least disturbed by other work.
    let n: usize = std::env::var("N")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(3_000);
    let mut best = f64::MAX;
    for _ in 0..std::env::var("R").ok().and_then(|v| v.parse().ok()).unwrap_or(7) {
        let t = std::time::Instant::now();
        for _ in 0..n {
            std::hint::black_box(extract_page(&input));
        }
        best = best.min(t.elapsed().as_secs_f64() * 1e6 / n as f64);
    }
    let us = best;
    println!("{} KB page: {us:.1} µs per page on one thread", html.len() / 1024);
}
