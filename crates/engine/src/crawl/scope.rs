//! The scope decision for one discovered URL (FR-007..FR-018, FR-030..FR-032,
//! FR-038; research R2).
//!
//! `classify` applies the FR-038 order up to "Nofollow". robots.txt and the
//! URL limit come next in that order but need crawl state, so the runner
//! applies them when the URL is about to be fetched. The scope tester calls
//! the same function, so it cannot disagree with the crawler.

use serde::Serialize;
use spiderlens_config::{
    CompiledRewrites, CrawlConfig, Decision, InternalHosts, LinkProblem, Mode, QueryParams, RobotsMode,
    UrlFilter, apply_query_rules, normalize_link,
};
use url::Url;

use super::reason::NotCrawledReason;
use crate::robots::{RobotsPolicy, RobotsVerdict};

/// Where a nofollow came from (FR-030).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Nofollow {
    /// `rel="nofollow"` on the link.
    Link,
    /// Meta robots or X-Robots-Tag `nofollow` on the linking page.
    Page,
}

/// How a URL was reached.
#[derive(Debug, Clone, Copy)]
pub struct Candidate {
    pub depth: u32,
    /// Start URL, its same-site redirect target, or a List mode URL. Include
    /// rules, the folder rule and depth limits do not apply (FR-013, FR-041).
    pub seed: bool,
    pub nofollow: Option<Nofollow>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "verdict", rename_all = "snake_case")]
pub enum Verdict {
    /// Queue for crawling. `via_nofollow` notes a nofollow link that the
    /// settings allowed (US5-3).
    Crawl { via_nofollow: bool },
    /// External: recorded, links never followed; `check` asks for a single
    /// status request (FR-010).
    External { check: bool },
    NotCrawled {
        reason: NotCrawledReason,
        detail: Option<String>,
    },
}

impl Verdict {
    fn not(reason: NotCrawledReason, detail: impl Into<Option<String>>) -> Self {
        Self::NotCrawled {
            reason,
            detail: detail.into(),
        }
    }
}

/// A scope settings snapshot, compiled once per crawl.
#[derive(Debug, Clone)]
pub struct ScopeRules {
    internal: InternalHosts,
    filter: UrlFilter,
    exclude: Vec<String>,
    /// Path prefix for "stay inside start folder", e.g. `/docs/`.
    folder: Option<String>,
    max_depth: Option<u32>,
    /// Most folders in a path and most query parameters (spec 002
    /// follow-up). Seeds are exempt.
    max_folder_depth: Option<u32>,
    max_query_params: Option<u32>,
    query: QueryParams,
    /// Regex rewrites of discovered links; empty means none.
    rewrite: CompiledRewrites,
    max_url_length: usize,
    check_external: bool,
    follow_internal_nofollow: bool,
    follow_external_nofollow: bool,
}

impl ScopeRules {
    /// `seeds` is the start URL in Spider mode or every listed URL in List
    /// mode. Fails only on invalid patterns, which validation reports first.
    pub fn new(config: &CrawlConfig, seeds: &[Url]) -> Result<Self, String> {
        let s = &config.scope;
        let filter = UrlFilter::new(&s.include, &s.exclude).map_err(|errors| {
            errors
                .iter()
                .map(|(_, e)| e.message.clone())
                .collect::<Vec<_>>()
                .join("; ")
        })?;
        let rewrite = CompiledRewrites::new(&s.rewrite)?;
        let first_host = seeds.first().and_then(Url::host_str).unwrap_or_default();
        let mut internal = InternalHosts::new(s.subdomains, first_host);
        for seed in seeds.iter().skip(1) {
            if let Some(h) = seed.host_str() {
                internal.add(h);
            }
        }
        let (folder, max_depth) = match config.mode {
            Mode::Spider => (
                s.stay_in_folder
                    .then(|| seeds.first().map(start_folder))
                    .flatten(),
                config.limits.max_depth,
            ),
            Mode::List => (None, Some(config.limits.list_depth)),
        };
        Ok(Self {
            internal,
            filter,
            exclude: s.exclude.clone(),
            folder,
            max_depth,
            max_folder_depth: config.limits.max_folder_depth,
            max_query_params: config.limits.max_query_params,
            query: s.query_params.clone(),
            rewrite,
            max_url_length: s.max_url_length as usize,
            check_external: s.check_external,
            follow_internal_nofollow: s.follow_internal_nofollow,
            follow_external_nofollow: s.follow_external_nofollow,
        })
    }

    /// Resolves and normalizes a link found on a page (FR-001..FR-006).
    pub fn normalize(&self, base: &Url, href: &str) -> Result<Url, LinkProblem> {
        if self.rewrite.is_empty() {
            return normalize_link(base, href, &self.query, self.max_url_length);
        }
        // The length limit applies to the rewritten URL, not the original.
        let url = normalize_link(base, href, &self.query, usize::MAX)?;
        let rewritten = self.rewrite.apply(url.as_str());
        let mut url = if rewritten == url.as_str() {
            url
        } else {
            Url::parse(&rewritten).map_err(|_| LinkProblem::Malformed {
                raw: href.to_string(),
                message: "rewrite gave an invalid URL".into(),
            })?
        };
        if !matches!(url.scheme(), "http" | "https") {
            return Err(LinkProblem::UnsupportedScheme {
                url: url.to_string(),
                scheme: url.scheme().to_string(),
            });
        }
        if url.host().is_none() {
            return Err(LinkProblem::Malformed {
                raw: href.to_string(),
                message: "rewrite gave an invalid URL".into(),
            });
        }
        url.set_fragment(None);
        apply_query_rules(&mut url, &self.query);
        let length = url.as_str().len();
        if length > self.max_url_length {
            return Err(LinkProblem::TooLong {
                url: url.to_string(),
                length,
            });
        }
        Ok(url)
    }

    /// Applies the query rules to a seed URL, which was normalized on input.
    pub fn normalize_seed(&self, url: &Url) -> Url {
        let mut u = url.clone();
        apply_query_rules(&mut u, &self.query);
        u
    }

    /// The internal hosts, for the cookie jar (spec 025).
    pub fn internal(&self) -> &InternalHosts {
        &self.internal
    }

    pub fn is_internal(&self, url: &Url) -> bool {
        url.host_str().is_some_and(|h| self.internal.is_internal(h))
    }

    /// The start URL redirected: a same-site target host becomes internal
    /// (spec 002 FR-003; research R12 item 1).
    pub fn adopt_start_redirect(&mut self, target: &Url) -> bool {
        target.host_str().is_some_and(|h| self.internal.adopt_redirect(h))
    }

    pub fn internal_hosts(&self) -> &[String] {
        self.internal.hosts()
    }

    /// Restores hosts adopted earlier in the crawl (on resume).
    pub fn restore_hosts(&mut self, hosts: &[String]) {
        for h in hosts {
            self.internal.add(h);
        }
    }

    /// The FR-038 order up to Nofollow. robots.txt and the URL limit are
    /// applied by the runner, in that order, after this.
    pub fn classify(&self, url: &Url, c: &Candidate) -> Verdict {
        if !c.seed && !self.is_internal(url) {
            let check = self.check_external && (c.nofollow.is_none() || self.follow_external_nofollow);
            return Verdict::External { check };
        }
        if !c.seed
            && let Some(folder) = &self.folder
            && !url.path().starts_with(folder.as_str())
        {
            return Verdict::not(NotCrawledReason::OutsideStartFolder, folder.clone());
        }
        match self.filter.decide(url.as_str()) {
            Decision::Excluded { exclude } => {
                return Verdict::not(NotCrawledReason::Excluded, self.exclude[exclude as usize].clone());
            }
            Decision::NotIncluded if !c.seed => return Verdict::not(NotCrawledReason::NotIncluded, None),
            _ => {}
        }
        if !c.seed {
            if let Some(max) = self.max_folder_depth
                && folder_depth(url) > max as usize
            {
                return Verdict::not(NotCrawledReason::OverFolderDepth, max.to_string());
            }
            if let Some(max) = self.max_query_params
                && query_params(url) > max as usize
            {
                return Verdict::not(NotCrawledReason::OverQueryLimit, max.to_string());
            }
        }
        if let Some(max) = self.max_depth
            && c.depth > max
            && !c.seed
        {
            return Verdict::not(NotCrawledReason::OverDepthLimit, max.to_string());
        }
        if let Some(source) = c.nofollow {
            if !self.follow_internal_nofollow {
                let detail = match source {
                    Nofollow::Link => "link",
                    Nofollow::Page => "page",
                };
                return Verdict::not(NotCrawledReason::Nofollow, detail.to_string());
            }
            return Verdict::Crawl { via_nofollow: true };
        }
        Verdict::Crawl { via_nofollow: false }
    }
}

/// The verdict and stored text for a link that did not give a usable URL.
/// Self references (empty or `#fragment`) give `None`: they are not new URLs.
pub fn problem_verdict(problem: &LinkProblem) -> Option<(String, Verdict)> {
    match problem {
        LinkProblem::SelfReference => None,
        LinkProblem::Malformed { raw, message } => Some((
            raw.clone(),
            Verdict::not(NotCrawledReason::MalformedUrl, message.clone()),
        )),
        LinkProblem::UnsupportedScheme { url, scheme } => Some((
            url.clone(),
            Verdict::not(NotCrawledReason::UnsupportedScheme, scheme.clone()),
        )),
        LinkProblem::TooLong { url, length } => Some((
            url.clone(),
            Verdict::not(
                NotCrawledReason::MalformedUrl,
                format!("too long ({length} characters)"),
            ),
        )),
    }
}

/// How many folders a path has: `/` and `/page` have 0, `/a/` 1 and
/// `/a/b/page` 2.
fn folder_depth(url: &Url) -> usize {
    url.path().matches('/').count().saturating_sub(1)
}

/// How many query parameters a URL has: the non-empty `&` separated pairs.
fn query_params(url: &Url) -> usize {
    url.query()
        .map_or(0, |q| q.split('&').filter(|p| !p.is_empty()).count())
}

/// The folder of a start URL: its path up to and including the last `/`.
fn start_folder(url: &Url) -> String {
    let path = url.path();
    match path.rfind('/') {
        Some(i) => path[..=i].to_string(),
        None => "/".to_string(),
    }
}

/// The scope tester's answer for one URL (FR-016).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ScopeTest {
    pub input: String,
    /// The normalized URL, when the input is a usable URL.
    pub url: Option<String>,
    /// True when the crawler would request it as a page.
    pub would_crawl: bool,
    pub reason: Option<NotCrawledReason>,
    pub detail: Option<String>,
    /// Whether robots.txt was consulted for this host.
    pub robots_checked: bool,
}

/// Explains what the crawler would do with `input` if the start page linked
/// to it. `robots` is the host's policy when it has been fetched.
pub fn test_url(
    rules: &ScopeRules,
    start: &Url,
    input: &str,
    robots: Option<&RobotsPolicy>,
    robots_mode: RobotsMode,
) -> ScopeTest {
    let mut out = ScopeTest {
        input: input.to_string(),
        url: None,
        would_crawl: false,
        reason: None,
        detail: None,
        robots_checked: false,
    };
    let url = match rules.normalize(start, input) {
        Ok(u) => u,
        Err(LinkProblem::SelfReference) => rules.normalize_seed(start),
        Err(p) => {
            if let Some((stored, Verdict::NotCrawled { reason, detail })) = problem_verdict(&p) {
                out.url = Some(stored);
                out.reason = Some(reason);
                out.detail = detail;
            }
            return out;
        }
    };
    out.url = Some(url.to_string());
    let is_start = url == rules.normalize_seed(start);
    let candidate = Candidate {
        depth: u32::from(!is_start),
        seed: is_start,
        nofollow: None,
    };
    match rules.classify(&url, &candidate) {
        Verdict::External { .. } => {
            out.reason = Some(NotCrawledReason::External);
            return out;
        }
        Verdict::NotCrawled { reason, detail } => {
            out.reason = Some(reason);
            out.detail = detail;
            return out;
        }
        Verdict::Crawl { .. } => {}
    }
    if robots_mode == RobotsMode::Respect
        && let Some(policy) = robots
    {
        out.robots_checked = true;
        if let RobotsVerdict::Disallowed(rule) = policy.check(&url) {
            out.reason = Some(if policy.is_disallow_all() {
                NotCrawledReason::RobotsUnavailable
            } else {
                NotCrawledReason::BlockedByRobots
            });
            out.detail = Some(rule);
            return out;
        }
    }
    out.would_crawl = true;
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use spiderlens_config::{QueryMode, SubdomainMode};

    fn url(s: &str) -> Url {
        Url::parse(s).unwrap()
    }

    fn rules(edit: impl FnOnce(&mut CrawlConfig)) -> ScopeRules {
        let mut c = CrawlConfig::default();
        edit(&mut c);
        ScopeRules::new(&c, &[url("https://example.com/docs/")]).unwrap()
    }

    fn link(depth: u32) -> Candidate {
        Candidate {
            depth,
            seed: false,
            nofollow: None,
        }
    }

    fn reason(v: Verdict) -> NotCrawledReason {
        match v {
            Verdict::NotCrawled { reason, .. } => reason,
            Verdict::External { .. } => NotCrawledReason::External,
            other => panic!("expected not crawled, got {other:?}"),
        }
    }

    #[test]
    fn www_is_internal_and_subdomains_depend_on_setting() {
        let r = rules(|_| {});
        assert_eq!(
            r.classify(&url("https://www.example.com/about"), &link(1)),
            Verdict::Crawl { via_nofollow: false }
        );
        assert_eq!(
            r.classify(&url("https://blog.example.com/"), &link(1)),
            Verdict::External { check: true }
        );
        let all = rules(|c| c.scope.subdomains = SubdomainMode::AllSubdomains);
        assert!(matches!(
            all.classify(&url("https://blog.example.com/"), &link(1)),
            Verdict::Crawl { .. }
        ));
    }

    #[test]
    fn exclude_wins_and_names_pattern_include_spares_seeds() {
        let r = rules(|c| {
            c.scope.include = vec!["/blog/".into()];
            c.scope.exclude = vec![r"\?sort=".into()];
        });
        assert_eq!(
            r.classify(&url("https://example.com/blog/?sort=a"), &link(1)),
            Verdict::NotCrawled {
                reason: NotCrawledReason::Excluded,
                detail: Some(r"\?sort=".into())
            }
        );
        assert_eq!(
            reason(r.classify(&url("https://example.com/shop/"), &link(1))),
            NotCrawledReason::NotIncluded
        );
        let seed = Candidate {
            depth: 0,
            seed: true,
            nofollow: None,
        };
        assert!(matches!(
            r.classify(&url("https://example.com/"), &seed),
            Verdict::Crawl { .. }
        ));
    }

    #[test]
    fn depth_folder_and_nofollow() {
        let r = rules(|c| c.limits.max_depth = Some(2));
        assert!(matches!(
            r.classify(&url("https://example.com/a"), &link(2)),
            Verdict::Crawl { .. }
        ));
        assert_eq!(
            reason(r.classify(&url("https://example.com/a"), &link(3))),
            NotCrawledReason::OverDepthLimit
        );

        let folder = rules(|c| c.scope.stay_in_folder = true);
        assert_eq!(
            reason(folder.classify(&url("https://example.com/blog/"), &link(1))),
            NotCrawledReason::OutsideStartFolder
        );
        assert!(matches!(
            folder.classify(&url("https://example.com/docs/x"), &link(1)),
            Verdict::Crawl { .. }
        ));

        let nf = Candidate {
            depth: 1,
            seed: false,
            nofollow: Some(Nofollow::Link),
        };
        assert_eq!(
            reason(r.classify(&url("https://example.com/x"), &nf)),
            NotCrawledReason::Nofollow
        );
        assert_eq!(
            r.classify(&url("https://other.test/"), &nf),
            Verdict::External { check: false },
            "external nofollow links are not checked by default"
        );
        let follow = rules(|c| c.scope.follow_internal_nofollow = true);
        assert_eq!(
            follow.classify(&url("https://example.com/x"), &nf),
            Verdict::Crawl { via_nofollow: true }
        );
    }

    #[test]
    fn folder_depth_and_query_limits_spare_seeds() {
        let r = rules(|c| {
            c.limits.max_folder_depth = Some(1);
            c.limits.max_query_params = Some(1);
        });
        for ok in [
            "https://example.com/",
            "https://example.com/page",
            "https://example.com/a/",
            "https://example.com/a/page?x=1",
            "https://example.com/a/page?x=1&",
        ] {
            assert!(
                matches!(r.classify(&url(ok), &link(1)), Verdict::Crawl { .. }),
                "{ok}"
            );
        }
        assert_eq!(
            r.classify(&url("https://example.com/a/b/page"), &link(1)),
            Verdict::NotCrawled {
                reason: NotCrawledReason::OverFolderDepth,
                detail: Some("1".into())
            }
        );
        assert_eq!(
            reason(r.classify(&url("https://example.com/a/b/"), &link(1))),
            NotCrawledReason::OverFolderDepth
        );
        assert_eq!(
            r.classify(&url("https://example.com/p?a=1&b=2"), &link(1)),
            Verdict::NotCrawled {
                reason: NotCrawledReason::OverQueryLimit,
                detail: Some("1".into())
            }
        );
        let seed = Candidate {
            depth: 0,
            seed: true,
            nofollow: None,
        };
        assert!(matches!(
            r.classify(&url("https://example.com/a/b/c/?a=1&b=2"), &seed),
            Verdict::Crawl { .. }
        ));
        // Zero means no folders and no query strings.
        let none = rules(|c| {
            c.limits.max_folder_depth = Some(0);
            c.limits.max_query_params = Some(0);
        });
        assert!(matches!(
            none.classify(&url("https://example.com/page"), &link(1)),
            Verdict::Crawl { .. }
        ));
        assert_eq!(
            reason(none.classify(&url("https://example.com/a/page"), &link(1))),
            NotCrawledReason::OverFolderDepth
        );
        assert_eq!(
            reason(none.classify(&url("https://example.com/page?x"), &link(1))),
            NotCrawledReason::OverQueryLimit
        );
    }

    #[test]
    fn rewrites_apply_to_links_but_not_seeds() {
        use spiderlens_config::RewriteRule;
        let r = rules(|c| {
            c.scope.rewrite = vec![
                RewriteRule {
                    pattern: r"/old/(\w+)".into(),
                    replace: "/new/$1".into(),
                },
                RewriteRule {
                    pattern: r"\?utm=\w+$".into(),
                    replace: String::new(),
                },
            ];
        });
        let base = url("https://example.com/docs/");
        assert_eq!(
            r.normalize(&base, "/old/page?utm=x").unwrap().as_str(),
            "https://example.com/new/page"
        );
        assert_eq!(
            r.normalize(&base, "/plain").unwrap().as_str(),
            "https://example.com/plain"
        );
        assert_eq!(
            r.normalize_seed(&url("https://example.com/old/page")).as_str(),
            "https://example.com/old/page"
        );
        let bad = rules(|c| {
            c.scope.rewrite = vec![RewriteRule {
                pattern: r"^https://example\.com/x$".into(),
                replace: "::::".into(),
            }];
        });
        assert!(matches!(
            bad.normalize(&base, "/x"),
            Err(LinkProblem::Malformed { .. })
        ));
        let invalid = {
            let mut c = CrawlConfig::default();
            c.scope.rewrite = vec![RewriteRule {
                pattern: "(".into(),
                replace: String::new(),
            }];
            ScopeRules::new(&c, &[url("https://example.com/")])
        };
        assert!(invalid.is_err());
    }

    #[test]
    fn earlier_reasons_win() {
        // Outside the folder and excluded and too deep: the folder comes first.
        let r = rules(|c| {
            c.scope.stay_in_folder = true;
            c.scope.exclude = vec!["shop".into()];
            c.limits.max_depth = Some(1);
        });
        assert_eq!(
            reason(r.classify(&url("https://example.com/shop/x"), &link(5))),
            NotCrawledReason::OutsideStartFolder
        );
        // External comes before everything that is not a link problem.
        assert_eq!(
            reason(r.classify(&url("https://shop.test/x"), &link(5))),
            NotCrawledReason::External
        );
    }

    #[test]
    fn link_problems_map_to_reasons() {
        let r = rules(|c| c.scope.query_params.mode = QueryMode::RemoveAll);
        let base = url("https://example.com/p");
        let (stored, v) = problem_verdict(&r.normalize(&base, "mailto:a@b.test").unwrap_err()).unwrap();
        assert_eq!(stored, "mailto:a@b.test");
        assert_eq!(reason(v), NotCrawledReason::UnsupportedScheme);
        assert!(problem_verdict(&r.normalize(&base, "#x").unwrap_err()).is_none());
        assert_eq!(
            r.normalize(&base, "/a?x=1").unwrap().as_str(),
            "https://example.com/a"
        );
    }

    #[test]
    fn list_mode_hosts_are_internal_and_depth_defaults_to_zero() {
        let c = CrawlConfig {
            mode: Mode::List,
            ..CrawlConfig::default()
        };
        let r = ScopeRules::new(&c, &[url("https://a.test/1"), url("https://b.test/2")]).unwrap();
        assert!(r.is_internal(&url("https://b.test/x")));
        assert_eq!(
            reason(r.classify(&url("https://a.test/x"), &link(1))),
            NotCrawledReason::OverDepthLimit
        );
    }

    #[test]
    fn tester_reports_robots_rule() {
        let r = rules(|c| c.scope.exclude = vec!["nothing-matches".into()]);
        let policy =
            crate::robots::RobotsTxt::parse(b"User-agent: *\nDisallow: /private/\n").for_agent("SpiderLens");
        let start = url("https://example.com/");
        let t = test_url(
            &r,
            &start,
            "https://example.com/private/x",
            Some(&policy),
            RobotsMode::Respect,
        );
        assert!(!t.would_crawl);
        assert_eq!(t.reason, Some(NotCrawledReason::BlockedByRobots));
        assert_eq!(t.detail.as_deref(), Some("Disallow: /private/"));
        let t = test_url(&r, &start, "/ok", Some(&policy), RobotsMode::Respect);
        assert!(t.would_crawl && t.robots_checked);
        let t = test_url(&r, &start, "/ok", None, RobotsMode::Respect);
        assert!(t.would_crawl && !t.robots_checked);
    }
}
