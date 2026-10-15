//! XML writing helpers and the structural validator used by tests on every
//! OS (research R6). CI also checks generated files with `xmllint` against
//! the official schemas on Linux.

pub const NS_SITEMAP: &str = "http://www.sitemaps.org/schemas/sitemap/0.9";
pub const NS_IMAGE: &str = "http://www.google.com/schemas/sitemap-image/1.1";
pub const NS_XHTML: &str = "http://www.w3.org/1999/xhtml";

/// Escapes text and attribute values (the five XML entities). The protocol
/// requires `&`, `'`, `"`, `>` and `<` in URLs to be escaped.
pub fn escape(s: &str, out: &mut String) {
    for ch in s.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            c => out.push(c),
        }
    }
}

/// Checks the structure the protocol schemas require: the root and its
/// namespace, the allowed children of each entry in order, a `<loc>` in
/// every entry, at most 50,000 entries and 50 MB, absolute http(s) URLs,
/// and well-formed XML. Returns the entry count, or what is wrong.
pub fn check_structure(xml: &[u8]) -> Result<usize, String> {
    use quick_xml::Reader;
    use quick_xml::events::Event;

    if xml.len() > 50 * 1024 * 1024 {
        return Err("larger than 50 MB".into());
    }
    let mut r = Reader::from_reader(xml);
    let mut buf = Vec::new();
    let mut stack: Vec<String> = Vec::new();
    let mut root: Option<String> = None;
    let mut entries = 0usize;
    // Children seen in the current entry, to check order and the required loc.
    let mut seen: Vec<String> = Vec::new();
    let mut text = String::new();
    loop {
        buf.clear();
        let ev = r
            .read_event_into(&mut buf)
            .map_err(|e| format!("not well-formed: {e}"))?;
        match ev {
            Event::Start(ref e) | Event::Empty(ref e) => {
                let empty = matches!(ev, Event::Empty(_));
                let name = e.name().as_ref().to_string();
                match stack.len() {
                    0 => {
                        if name != "urlset" && name != "sitemapindex" {
                            return Err(format!("root is <{name}>"));
                        }
                        let ns = e
                            .attributes()
                            .flatten()
                            .find(|a| a.key.as_ref() == "xmlns")
                            .map(|a| a.value.to_string());
                        if ns.as_deref() != Some(NS_SITEMAP) {
                            return Err("missing the sitemap namespace".into());
                        }
                        for a in e.attributes().flatten() {
                            let k = a.key.as_ref().to_string();
                            let v = a.value.to_string();
                            if (k == "xmlns:image" && v != NS_IMAGE) || (k == "xmlns:xhtml" && v != NS_XHTML)
                            {
                                return Err(format!("wrong namespace for {k}"));
                            }
                        }
                        root = Some(name.clone());
                    }
                    1 => {
                        let want = if root.as_deref() == Some("urlset") {
                            "url"
                        } else {
                            "sitemap"
                        };
                        if name != want {
                            return Err(format!("<{name}> inside <{}>", root.as_deref().unwrap_or("")));
                        }
                        entries += 1;
                        if entries > 50_000 {
                            return Err("more than 50,000 entries".into());
                        }
                        seen.clear();
                    }
                    2 => {
                        let order: &[&str] = if root.as_deref() == Some("urlset") {
                            &[
                                "loc",
                                "lastmod",
                                "changefreq",
                                "priority",
                                "image:image",
                                "xhtml:link",
                            ]
                        } else {
                            &["loc", "lastmod"]
                        };
                        let Some(pos) = order.iter().position(|o| *o == name) else {
                            return Err(format!("unexpected <{name}>"));
                        };
                        let repeatable = matches!(name.as_str(), "image:image" | "xhtml:link");
                        if let Some(last) = seen.last() {
                            let last_pos = order.iter().position(|o| o == last).unwrap_or(0);
                            if pos < last_pos || (pos == last_pos && !repeatable) {
                                return Err(format!("<{name}> out of order or repeated"));
                            }
                        } else if name != "loc" {
                            return Err("<loc> must come first".into());
                        }
                        if name == "xhtml:link" {
                            let attr = |k: &str| {
                                e.attributes()
                                    .flatten()
                                    .find(|a| a.key.as_ref() == k)
                                    .map(|a| a.value.to_string())
                            };
                            if attr("rel").as_deref() != Some("alternate") || attr("hreflang").is_none() {
                                return Err("xhtml:link needs rel=alternate and hreflang".into());
                            }
                            check_url(&attr("href").unwrap_or_default())?;
                        }
                        seen.push(name.clone());
                    }
                    3 if stack[2] == "image:image" => {
                        if name != "image:loc" {
                            return Err(format!("unexpected <{name}> in image:image"));
                        }
                    }
                    _ => return Err(format!("unexpected <{name}>")),
                }
                if !empty {
                    stack.push(name);
                }
                text.clear();
            }
            Event::Text(t) => text.push_str(&t.xml10_content()),
            Event::GeneralRef(g) => {
                let c = match &*g {
                    "amp" => '&',
                    "lt" => '<',
                    "gt" => '>',
                    "quot" => '"',
                    "apos" => '\'',
                    other => return Err(format!("unknown entity {other}")),
                };
                text.push(c);
            }
            Event::End(_) => {
                let name = stack.pop().unwrap_or_default();
                match name.as_str() {
                    "loc" | "image:loc" => check_url(text.trim())?,
                    "lastmod" if spiderlens_engine::crawl::sitemap::parse_lastmod(&text).is_none() => {
                        return Err(format!("invalid lastmod {text}"));
                    }
                    "priority" => {
                        let p: f32 = text
                            .trim()
                            .parse()
                            .map_err(|_| format!("invalid priority {text}"))?;
                        if !(0.0..=1.0).contains(&p) {
                            return Err(format!("priority {p} out of range"));
                        }
                    }
                    "url" | "sitemap" if seen.first().map(String::as_str) != Some("loc") => {
                        return Err("entry without <loc>".into());
                    }
                    _ => {}
                }
                text.clear();
            }
            Event::Eof => break,
            _ => {}
        }
    }
    if root.is_none() {
        return Err("empty document".into());
    }
    Ok(entries)
}

fn check_url(s: &str) -> Result<(), String> {
    match url::Url::parse(s) {
        Ok(u) if matches!(u.scheme(), "http" | "https") && s.len() < 2048 => Ok(()),
        _ => Err(format!("not an absolute http(s) URL under 2,048 characters: {s}")),
    }
}
