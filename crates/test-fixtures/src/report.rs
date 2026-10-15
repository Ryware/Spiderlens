//! The report fixture site (spec 010 T004): known pages with and without
//! problems, response codes, a redirect chain, depths, and titles in
//! Hebrew and with markup in them.

use crate::{Site, SiteBuilder};

pub const HOST: &str = "report.test";

pub fn page(title: Option<&str>, links: &[&str]) -> String {
    let head = title.map_or(String::new(), |t| format!("<title>{t}</title>"));
    let body: String = links
        .iter()
        .map(|l| format!("<a href=\"{l}\">link to {l}</a>"))
        .collect();
    format!(
        "<!doctype html><html lang=\"en\"><head>{head}<meta name=\"description\" content=\"A description long enough to pass the minimum length check of the rule set.\"></head><body><h1>Heading</h1><p>{}</p>{body}</body></html>",
        "Words about this page. ".repeat(60)
    )
}

pub fn site() -> Site {
    let home = [
        "/a", "/b", "/c", "/notitle", "/dup1", "/dup2", "/gone", "/fail", "/r1", "/deep1", "/he", "/xss",
    ];
    SiteBuilder::new()
        .html(
            HOST,
            "/",
            &page(Some("Home page of the report fixture site"), &home),
        )
        .html(
            HOST,
            "/a",
            &page(Some("Page A of the report fixture site here"), &["/"]),
        )
        .html(
            HOST,
            "/b",
            &page(Some("Page B of the report fixture site here"), &["/"]),
        )
        .html(
            HOST,
            "/c",
            &page(Some("Page C of the report fixture site here"), &["/", "/gone"]),
        )
        .html(HOST, "/notitle", &page(None, &["/"]))
        .html(
            HOST,
            "/dup1",
            &page(Some("The same title on two pages of the site"), &["/"]),
        )
        .html(
            HOST,
            "/dup2",
            &page(Some("The same title on two pages of the site"), &["/"]),
        )
        .raw(HOST, "/gone", 404, "text/html", b"<title>Not found</title>")
        .raw(HOST, "/fail", 500, "text/html", b"oops")
        .redirect(HOST, "/r1", 301, "/r2")
        .redirect(HOST, "/r2", 301, "/a")
        .html(
            HOST,
            "/deep1",
            &page(Some("Deep page one of the report fixture"), &["/deep2"]),
        )
        .html(
            HOST,
            "/deep2",
            &page(Some("Deep page two of the report fixture"), &["/deep3"]),
        )
        .html(
            HOST,
            "/deep3",
            &page(Some("Deep page three of the report fixture"), &[]),
        )
        .html(
            HOST,
            "/he",
            &page(Some("דף בעברית עם כותרת באורך סביר לגמרי בדוח"), &["/"]),
        )
        .html(
            HOST,
            "/xss",
            &page(
                Some("&lt;/style&gt;&lt;script&gt;alert(1)&lt;/script&gt; title"),
                &["/"],
            ),
        )
        .start()
}
