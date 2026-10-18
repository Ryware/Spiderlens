//! A sitemap as the source of a List mode list (spec 011 FR-005, T014).
//!
//! The sitemap, and the files an index lists, are read before the crawl and
//! every page address becomes a list entry. The crawl itself is an ordinary
//! list crawl. Like the crawl's own sitemap reading, these requests do not
//! consult robots.txt: a sitemap is a file the site owner publishes for
//! crawlers.

use std::collections::{HashSet, VecDeque};
use std::path::PathBuf;

use spiderlens_config::{CrawlConfig, InternalHosts, ListEntry, ListInput, SubdomainMode, list_from_urls};
use url::Url;

use super::sitemap::{self, MAX_INDEX_DEPTH, Root};
use crate::CheckOptions;
use crate::net::NetSession;

/// Where the sitemap that becomes the list comes from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SitemapListSource {
    /// A sitemap or sitemap index on the web.
    Url(Url),
    /// A file on this computer. Relative addresses in it resolve against
    /// `base`, the address the file will have on the site.
    File { path: PathBuf, base: Option<Url> },
}

fn is_http(text: &str) -> bool {
    let lower = text.trim().to_ascii_lowercase();
    lower.starts_with("http://") || lower.starts_with("https://")
}

impl SitemapListSource {
    /// A web address or a file path, with the optional base address for a
    /// file. A base given with a web address is not needed and is ignored.
    pub fn new(location: &str, base_url: Option<&str>) -> Result<Self, String> {
        let location = location.trim();
        if location.is_empty() {
            return Err("Give the address or the file of a sitemap.".into());
        }
        if is_http(location) {
            return Url::parse(location)
                .map(Self::Url)
                .map_err(|e| format!("\"{location}\" is not a valid web address ({e})."));
        }
        let base = match base_url.map(str::trim).filter(|b| !b.is_empty()) {
            Some(b) if is_http(b) => Some(
                Url::parse(b).map_err(|e| format!("The base address \"{b}\" is not a valid web address ({e})."))?,
            ),
            Some(b) => return Err(format!("The base address \"{b}\" must start with http:// or https://.")),
            None => None,
        };
        Ok(Self::File {
            path: PathBuf::from(location),
            base,
        })
    }

    /// The command line form `<url-or-path>[=<base-url>]`. The text after
    /// the last `=` is a base address only when it is a web address and the
    /// text before it is a file, so a sitemap address with a query string
    /// stays whole.
    pub fn parse_arg(arg: &str) -> Result<Self, String> {
        let arg = arg.trim();
        if let Some(i) = arg.rfind('=') {
            let (left, right) = (&arg[..i], &arg[i + 1..]);
            if !left.is_empty() && !is_http(left) && is_http(right) {
                return Self::new(left, Some(right));
            }
        }
        Self::new(arg, None)
    }
}

/// One file waiting to be read.
struct Job {
    at: SitemapListSource,
    level: u32,
}

impl Job {
    fn location(&self) -> String {
        match &self.at {
            SitemapListSource::Url(u) => u.to_string(),
            SitemapListSource::File { path, .. } => format!("file:{}", path.display()),
        }
    }
}

/// Reads the sitemap, and for an index every file it lists (breadth first,
/// each file once, to the same depth the crawl follows), into a list. Fails
/// only when no file could be read; files that failed are noted on the list.
pub async fn list_from_sitemap(source: SitemapListSource, config: &CrawlConfig) -> Result<ListInput, String> {
    list_from_sitemap_resolving(source, config, &[]).await
}

/// [`list_from_sitemap`] with host names pinned to addresses, for tests.
pub async fn list_from_sitemap_resolving(
    source: SitemapListSource,
    config: &CrawlConfig,
    resolve: &[(String, std::net::SocketAddr)],
) -> Result<ListInput, String> {
    let root_location = match &source {
        SitemapListSource::Url(u) => u.to_string(),
        SitemapListSource::File { path, .. } => path.display().to_string(),
    };
    let host = match &source {
        SitemapListSource::Url(u) => u.host_str(),
        SitemapListSource::File { base, .. } => base.as_ref().and_then(Url::host_str),
    }
    .unwrap_or("")
    .to_string();
    let opts = CheckOptions::from(config);
    let net = NetSession::new(
        &opts,
        &config.access,
        InternalHosts::new(SubdomainMode::ThisHost, &host),
        resolve,
    )
    .map_err(|e| e.message)?;
    let client = net.client();

    let mut todo: VecDeque<Job> = VecDeque::new();
    todo.push_back(Job { at: source, level: 0 });
    let mut seen: HashSet<String> = HashSet::new();
    let mut entries: Vec<ListEntry> = Vec::new();
    let mut problems: Vec<String> = Vec::new();
    let (mut files, mut indexes) = (0usize, 0usize);

    while let Some(job) = todo.pop_front() {
        let location = job.location();
        if !seen.insert(location.clone()) {
            continue;
        }
        let (bytes, base) = match &job.at {
            SitemapListSource::Url(url) => match sitemap::fetch(client, url).await {
                Ok(f) => (f.bytes, Some(f.final_url)),
                Err(e) => {
                    problems.push(format!("{location}: {}", e.message));
                    continue;
                }
            },
            SitemapListSource::File { path, base } => {
                let p = path.clone();
                let read = tokio::task::spawn_blocking(move || std::fs::read(p))
                    .await
                    .unwrap_or_else(|e| Err(std::io::Error::other(e)));
                match read {
                    Ok(b) => (b, base.clone()),
                    Err(e) => {
                        problems.push(format!("{location}: could not read the file ({e})"));
                        continue;
                    }
                }
            }
        };
        let mut found: Vec<sitemap::Entry> = Vec::new();
        let facts = match sitemap::read(&bytes, |e| found.push(e)) {
            Ok(f) => f,
            Err(e) => {
                let at = e.line.map(|l| format!(" (line {l})")).unwrap_or_default();
                problems.push(format!("{location}: {}{at}", e.message));
                // What was read before the error still counts.
                if !found.is_empty() {
                    push_urls(&mut entries, &location, base.as_ref(), &found);
                }
                continue;
            }
        };
        files += 1;
        if facts.over_limit {
            problems.push(format!(
                "{location}: more than {} entries or {} MB; only the first part was read",
                sitemap::MAX_ENTRIES,
                sitemap::MAX_SITEMAP_BYTES / (1024 * 1024)
            ));
        }
        match facts.root {
            Root::Urlset => push_urls(&mut entries, &location, base.as_ref(), &found),
            Root::Index => {
                indexes += 1;
                if job.level + 1 >= MAX_INDEX_DEPTH {
                    problems.push(format!(
                        "{location}: sitemap index nested deeper than {MAX_INDEX_DEPTH} levels; its files were not read"
                    ));
                    continue;
                }
                for e in &found {
                    let child = match &base {
                        Some(b) => b.join(&e.loc).ok(),
                        None => Url::parse(&e.loc).ok(),
                    };
                    if let Some(child) = child
                        && matches!(child.scheme(), "http" | "https")
                    {
                        todo.push_back(Job {
                            at: SitemapListSource::Url(child),
                            level: job.level + 1,
                        });
                    }
                }
            }
        }
    }

    if files == 0 && entries.is_empty() {
        return Err(problems
            .into_iter()
            .next()
            .unwrap_or_else(|| "No sitemap could be read.".into()));
    }
    let mut notices = vec![format!(
        "Read {files} sitemap file{}{}.",
        if files == 1 { "" } else { "s" },
        if indexes > 0 { format!(" ({indexes} index)") } else { String::new() }
    )];
    notices.extend(problems);
    let mut list = list_from_urls(entries, notices);
    list.sitemap = Some(root_location);
    Ok(list)
}

/// Adds a file's page addresses, resolved against the file's address.
fn push_urls(out: &mut Vec<ListEntry>, place: &str, base: Option<&Url>, found: &[sitemap::Entry]) {
    for e in found {
        // An empty address would otherwise resolve to the sitemap itself.
        let url = match base {
            Some(b) if !e.loc.trim().is_empty() => b.join(&e.loc).map(String::from).unwrap_or_else(|_| e.loc.clone()),
            _ => e.loc.clone(),
        };
        out.push(ListEntry {
            url,
            place: place.to_string(),
            position: u64::from(e.pos) + 1,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arg_with_base_for_a_file() {
        let s = SitemapListSource::parse_arg("/tmp/s.xml=https://a.test/").unwrap();
        match s {
            SitemapListSource::File { path, base } => {
                assert_eq!(path, PathBuf::from("/tmp/s.xml"));
                assert_eq!(base.unwrap().as_str(), "https://a.test/");
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn arg_url_with_query_stays_whole() {
        let s = SitemapListSource::parse_arg("https://a.test/s.xml?x=https://b.test/").unwrap();
        assert!(matches!(s, SitemapListSource::Url(u) if u.as_str() == "https://a.test/s.xml?x=https://b.test/"));
    }

    #[test]
    fn arg_file_with_equals_in_name_and_no_base() {
        let s = SitemapListSource::parse_arg("a=b.xml").unwrap();
        assert!(matches!(s, SitemapListSource::File { base: None, .. }));
    }

    #[test]
    fn empty_and_bad_base_are_refused() {
        assert!(SitemapListSource::new("  ", None).is_err());
        assert!(SitemapListSource::new("s.xml", Some("ftp://a.test")).is_err());
    }
}
