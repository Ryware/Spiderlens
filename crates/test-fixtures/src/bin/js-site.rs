//! The JavaScript fixture site for manual checks (spec 014 quickstart step 2).
//!
//! `js-site [--port 8710] [--app-pages 200]`
//!
//! Serves `js.test` and `cdn.test` on 127.0.0.1 only. A browser or crawler
//! has to map both names to 127.0.0.1, for example
//! `--host-resolver-rules="MAP js.test 127.0.0.1, MAP cdn.test 127.0.0.1"`
//! for Chrome, or `curl -H 'Host: js.test'`. `GET /__stats` shows what the
//! server saw.

use std::net::TcpListener;

use spiderlens_test_fixtures::js_site::{JS_HOST, JsSiteBuilder};

fn arg(name: &str) -> Option<u64> {
    let args: Vec<String> = std::env::args().collect();
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .and_then(|v| v.parse().ok())
}

fn main() {
    let port = arg("--port").unwrap_or(8710) as u16;
    let pages = arg("--app-pages").unwrap_or(200);
    let listener = TcpListener::bind(("127.0.0.1", port)).expect("bind 127.0.0.1");
    let site = JsSiteBuilder::new().app_pages(pages).start_on(listener);
    eprintln!(
        "js-site: {pages} client-rendered pages, {}",
        site.url(JS_HOST, "/app/0")
    );
    loop {
        std::thread::park();
    }
}
