//! Spec 009 acceptance tests: the first rule set on a real crawl of a
//! fixture site with a positive and a negative example per rule (SC-001).

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::time::Duration;

use spiderlens_config::{CrawlConfig, IssueSettings, ModeInput, Priority, RuleOverride, SpeedSettings};
use spiderlens_engine::crawl::{Control, Crawler};
use spiderlens_results::{Reader, ViewQuery, issues};
use spiderlens_test_fixtures::{Site, SiteBuilder};
use tokio::sync::watch;

const E: &str = "rules.test";

/// A page that passes every rule unless a field says otherwise.
struct P {
    title: Option<String>,
    desc: Option<String>,
    h1: Option<String>,
    lang: bool,
    canonical: bool,
    words: usize,
    head: String,
    body: String,
}

fn p(path: &str) -> P {
    P {
        title: Some(format!("{path} page about testing the rules")),
        desc: Some(format!(
            "Description of the {path} page, written long enough to pass the minimum length."
        )),
        h1: Some(format!("Heading for {path}")),
        lang: true,
        canonical: true,
        words: 240,
        head: String::new(),
        body: String::new(),
    }
}

impl P {
    fn html(&self, path: &str) -> String {
        let mut head = String::new();
        if let Some(t) = &self.title {
            head.push_str(&format!("<title>{t}</title>"));
        }
        if let Some(d) = &self.desc {
            head.push_str(&format!("<meta name=\"description\" content=\"{d}\">"));
        }
        if self.canonical {
            head.push_str(&format!("<link rel=\"canonical\" href=\"{path}\">"));
        }
        head.push_str(&self.head);
        let mut body = String::new();
        if let Some(h) = &self.h1 {
            body.push_str(&format!("<h1>{h}</h1><h2>Section of {path}</h2>"));
        }
        body.push_str(&format!(
            "<p>{}</p>",
            format!("Plain words about {path}. ").repeat(self.words / 4)
        ));
        body.push_str("<a href=\"/\">Home page</a>");
        body.push_str(&self.body);
        let lang = if self.lang { " lang=\"en\"" } else { "" };
        format!("<!doctype html><html{lang}><head>{head}</head><body>{body}</body></html>")
    }
}

fn site() -> Site {
    let mut pages: Vec<(&str, P)> = Vec::new();
    let mut add = |path: &'static str, f: &dyn Fn(&mut P)| {
        let mut x = p(path);
        f(&mut x);
        pages.push((path, x));
    };
    add("/good", &|_| {});
    add("/notitle", &|x| x.title = None);
    add("/longtitle", &|x| {
        x.title = Some("A title that is clearly longer than sixty characters in total length".into())
    });
    add("/two-titles", &|x| x.head = "<title>Second title</title>".into());
    add("/no-h1", &|x| x.h1 = None);
    add("/two-h1", &|x| x.body = "<h1>Another top heading</h1>".into());
    add("/skip-heading", &|x| x.body = "<h4>Jumped from H2</h4>".into());
    add("/hard-read", &|x| {
        x.body = format!(
            "<p>{}</p>",
            "Notwithstanding considerable methodological heterogeneity, investigators characterised \
             organisational interdependencies comprehensively "
                .repeat(12)
        );
    });
    add("/title-in-body", &|x| {
        x.title = None;
        x.body = "<title>/title-in-body page about testing the rules</title>".into();
    });
    add("/thin", &|x| x.words = 8);
    add("/nolang", &|x| x.lang = false);
    add("/nodesc", &|x| x.desc = None);
    add("/noindex", &|x| {
        x.head = "<meta name=\"robots\" content=\"noindex\">".into()
    });
    add("/conflict", &|x| {
        x.head = "<meta name=\"robots\" content=\"index, follow\">".into()
    });
    add("/canon-other", &|x| {
        x.canonical = false;
        x.head = "<link rel=\"canonical\" href=\"/good\">".into();
    });
    add("/canon-bad", &|x| {
        x.canonical = false;
        x.head = "<link rel=\"canonical\" href=\"/missing\">".into();
    });
    add("/no-canon", &|x| x.canonical = false);
    add("/img-page", &|x| {
        x.body = "<img src=\"/a.png\"><img src=\"/b.png\" alt=\"Fine\" width=\"1\" height=\"1\">\
                  <img src=\"/gone.png\" alt=\"Gone\" width=\"1\" height=\"1\">"
            .into()
    });
    add("/hreflang-a", &|x| {
        x.head = "<link rel=\"alternate\" hreflang=\"en\" href=\"/hreflang-a\">\
                  <link rel=\"alternate\" hreflang=\"de\" href=\"/hreflang-b\">\
                  <link rel=\"alternate\" hreflang=\"en-UK\" href=\"/good\">"
            .into()
    });
    add("/hreflang-b", &|_| {});
    add("/json", &|x| {
        x.head = "<script type=\"application/ld+json\">{\"@type\": \"Thing\",}</script>".into()
    });
    add("/weak", &|x| x.body = "<a href=\"/good\">click here</a>".into());
    add("/broken", &|x| {
        x.body = "<a href=\"/missing\">Missing page</a><a href=\"/error\">Error page</a>".into()
    });
    add("/redir-link", &|x| {
        x.body = "<a href=\"/old\">Old page</a><a href=\"/c1\">Chain start</a>".into()
    });
    add("/nofollow-link", &|x| {
        x.body = "<a rel=\"nofollow\" href=\"/good\">Good page</a>".into()
    });
    add("/dup-a", &|x| {
        x.title = Some("Duplicate title for testing pages".into());
        x.desc = Some("The same description on two pages, long enough to pass the minimum.".into());
        x.h1 = Some("Same heading".into());
    });
    add("/dup-b", &|x| {
        x.title = Some("Duplicate title for testing pages".into());
        x.desc = Some("The same description on two pages, long enough to pass the minimum.".into());
        x.h1 = Some("Same heading".into());
    });
    add("/Upper_Case", &|_| {});

    let links: String = pages
        .iter()
        .map(|(path, _)| format!("<a href=\"{path}\">{path} link</a>"))
        .collect();
    let mut home = p("/");
    home.body = links;
    let mut b = SiteBuilder::new().html(E, "/", &home.html("/"));
    for (path, x) in &pages {
        b = b.html(E, path, &x.html(path));
    }
    b.header(E, "/conflict", "X-Robots-Tag", "noindex")
        .raw(E, "/missing", 404, "text/html", b"<title>Not found</title>")
        .raw(E, "/error", 500, "text/html", b"oops")
        .redirect(E, "/old", 301, "/good")
        .redirect(E, "/c1", 301, "/c2")
        .redirect(E, "/c2", 301, "/good")
        .raw(E, "/a.png", 200, "image/png", b"\x89PNG")
        .raw(E, "/b.png", 200, "image/png", b"\x89PNG")
        .raw(E, "/gone.png", 404, "text/plain", b"gone")
        .start()
}

struct Crawled {
    _dir: tempfile::TempDir,
    path: PathBuf,
    site: Site,
}

async fn crawl() -> Crawled {
    let site = site();
    let c = CrawlConfig {
        speed: SpeedSettings::FAST,
        ..CrawlConfig::default()
    };
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("rules.spiderlens");
    let input = ModeInput::Spider {
        start_url: site.url(E, "/"),
    };
    let crawler = Crawler::create(&path, c, &input)
        .unwrap()
        .with_resolve(site.resolve())
        .with_retry_base(Duration::from_millis(10));
    let (_tx, rx) = watch::channel(Control::Run);
    tokio::time::timeout(Duration::from_secs(60), crawler.run(rx, None))
        .await
        .expect("crawl finishes")
        .expect("crawl succeeds");
    Crawled {
        _dir: dir,
        path,
        site,
    }
}

/// The paths a rule flags, from its issue tab (the rows the app shows).
fn flagged(r: &mut Reader, site: &Site, id: &str) -> BTreeSet<String> {
    let q = ViewQuery {
        tab: format!("issue:{id}"),
        filter: "all".into(),
        scope: Default::default(),
        search: String::new(),
        sort: None,
    };
    let page = r.open_view(&q, 1000).unwrap();
    let base = site.url(E, "");
    page.rows
        .iter()
        .map(|row| {
            let u = row[1].as_str().unwrap().to_string();
            u.strip_prefix(base.trim_end_matches('/'))
                .map_or(u.clone(), str::to_string)
        })
        .collect()
}

fn set(paths: &[&str]) -> BTreeSet<String> {
    paths.iter().map(|s| s.to_string()).collect()
}

#[tokio::test(flavor = "multi_thread")]
async fn sc001_first_rule_set_flags_exactly_the_expected_urls() {
    let c = crawl().await;
    let mut r = Reader::open(&c.path).unwrap();
    let summary = r.issues_summary(false).unwrap();
    let counts: BTreeMap<String, Option<u64>> =
        summary.issues.iter().map(|i| (i.id.clone(), i.count)).collect();
    assert_eq!(counts.len(), issues::catalogue().len());

    let expect: &[(&str, &[&str])] = &[
        ("response.internal_4xx", &["/missing", "/gone.png"]),
        ("response.internal_5xx", &["/error"]),
        ("response.internal_redirect", &["/old", "/c1", "/c2"]),
        ("response.redirect_chain", &["/c1"]),
        ("links.broken_internal", &["/broken"]),
        ("links.to_redirect", &["/redir-link"]),
        ("links.nofollow_internal", &["/nofollow-link"]),
        ("links.weak_anchor", &["/weak"]),
        ("title.missing", &["/notitle"]),
        ("title.duplicate", &["/dup-a", "/dup-b"]),
        ("title.over", &["/longtitle"]),
        ("title.multiple", &["/two-titles"]),
        ("meta.missing", &["/nodesc"]),
        ("meta.duplicate", &["/dup-a", "/dup-b"]),
        ("h1.missing", &["/no-h1"]),
        ("h1.duplicate", &["/dup-a", "/dup-b"]),
        ("h1.multiple", &["/two-h1"]),
        ("headings.non_sequential", &["/skip-heading"]),
        ("title.outside_head", &["/title-in-body"]),
        ("content.readability", &["/hard-read"]),
        ("content.thin", &["/thin"]),
        ("content.missing_lang", &["/nolang"]),
        ("canonical.canonicalised", &["/canon-other", "/canon-bad"]),
        ("canonical.missing", &["/no-canon"]),
        ("canonical.bad_target", &["/canon-bad"]),
        ("directives.noindex", &["/noindex", "/conflict"]),
        ("directives.conflicting", &["/conflict"]),
        ("hreflang.no_return", &["/hreflang-a"]),
        ("hreflang.invalid_code", &["/hreflang-a"]),
        ("hreflang.no_x_default", &["/hreflang-a"]),
        ("structured.invalid_json", &["/json"]),
        ("image.missing_alt", &["/a.png"]),
        ("image.missing_size", &["/a.png"]),
        ("image.broken", &["/gone.png"]),
        ("url.uppercase", &["/Upper_Case"]),
        ("url.underscores", &["/Upper_Case"]),
    ];
    let mut wrong = Vec::new();
    for (id, paths) in expect {
        let got = flagged(&mut r, &c.site, id);
        if got != set(paths) || counts[*id] != Some(paths.len() as u64) {
            wrong.push(format!("{id}: got {got:?} (count {:?})", counts[*id]));
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
    // The clean page only has what a plain-HTTP fixture server cannot avoid.
    let q = ViewQuery {
        tab: "internal".into(),
        filter: "html".into(),
        scope: Default::default(),
        search: "/good".into(),
        sort: None,
    };
    let good = r
        .open_view(&q, 10)
        .unwrap()
        .rows
        .into_iter()
        .find(|row| row[1].as_str().unwrap().ends_with("/good"))
        .unwrap();
    let ids: BTreeSet<String> = r
        .url_issues(good[0].as_i64().unwrap())
        .unwrap()
        .into_iter()
        .map(|i| i.id)
        .collect();
    assert_eq!(ids, set(&["security.http_page", "security.no_nosniff"]));

}

#[tokio::test(flavor = "multi_thread")]
async fn sc007_filters_and_rules_list_the_same_urls() {
    let c = crawl().await;
    let mut r = Reader::open(&c.path).unwrap();
    let pairs = [
        ("page-titles", "missing", "title.missing"),
        ("page-titles", "duplicate", "title.duplicate"),
        ("page-titles", "over_60", "title.over"),
        ("meta-description", "missing", "meta.missing"),
        ("h1", "multiple", "h1.multiple"),
        ("url", "uppercase", "url.uppercase"),
        ("canonicals", "missing", "canonical.missing"),
        ("images", "missing_alt", "image.missing_alt"),
    ];
    for (tab, filter, rule) in pairs {
        let q = ViewQuery {
            tab: tab.into(),
            filter: filter.into(),
            scope: Default::default(),
            search: String::new(),
            sort: None,
        };
        let base = c.site.url(E, "");
        let rows: BTreeSet<String> = r
            .open_view(&q, 1000)
            .unwrap()
            .rows
            .iter()
            .map(|row| {
                row[1]
                    .as_str()
                    .unwrap()
                    .strip_prefix(base.trim_end_matches('/'))
                    .unwrap()
                    .to_string()
            })
            .collect();
        assert_eq!(rows, flagged(&mut r, &c.site, rule), "{tab}/{filter} vs {rule}");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn us2_a_page_lists_its_issues_with_evidence() {
    let c = crawl().await;
    let mut r = Reader::open(&c.path).unwrap();
    let q = ViewQuery {
        tab: "issue:title.over".into(),
        filter: "all".into(),
        scope: Default::default(),
        search: String::new(),
        sort: None,
    };
    let row = r.open_view(&q, 10).unwrap().rows.remove(0);
    let id = row[0].as_i64().unwrap();
    assert_eq!(row[2].as_str(), Some("68 characters"));
    let mine = r.url_issues(id).unwrap();
    let over = mine.iter().find(|i| i.id == "title.over").unwrap();
    assert_eq!(over.name, "Page title: over 60 characters");
    assert_eq!(over.evidence.as_deref(), Some("68 characters"));
    assert!(mine.windows(2).all(|w| w[0].priority <= w[1].priority));
}

#[tokio::test(flavor = "multi_thread")]
async fn us4_settings_change_thresholds_priorities_and_switch_rules_off() {
    let c = crawl().await;
    let mut r = Reader::open(&c.path).unwrap();
    let count = |r: &mut Reader, id: &str| {
        r.issues_summary(false)
            .unwrap()
            .issues
            .into_iter()
            .find(|i| i.id == id)
            .unwrap()
    };
    assert_eq!(count(&mut r, "title.over").count, Some(1));

    let mut s = IssueSettings::default();
    s.rules.insert(
        "title.over".into(),
        RuleOverride {
            threshold: Some(70.0),
            ..Default::default()
        },
    );
    s.rules.insert(
        "canonical.missing".into(),
        RuleOverride {
            enabled: Some(false),
            ..Default::default()
        },
    );
    s.rules.insert(
        "h1.multiple".into(),
        RuleOverride {
            priority: Some(Priority::High),
            ..Default::default()
        },
    );
    assert!(r.set_issue_settings(&s).is_empty());
    let t = count(&mut r, "title.over");
    assert_eq!(
        (t.count, t.name.as_str()),
        (Some(0), "Page title: over 70 characters")
    );
    assert_eq!(count(&mut r, "canonical.missing").count, None);
    // FR-005: the Page titles filter follows the same threshold.
    let fc = r.counts("page-titles", Default::default()).unwrap();
    assert_eq!(fc.labels["over_60"], "Over 70 characters");
    assert_eq!(fc.counts["over_60"], Some(0));
    assert_eq!(count(&mut r, "h1.multiple").priority, Priority::High);

    // US4 scenario 5: an invalid value is rejected and the previous one stays.
    let mut bad = s.clone();
    bad.rules.insert(
        "title.over".into(),
        RuleOverride {
            threshold: Some(-1.0),
            ..Default::default()
        },
    );
    assert!(r.set_issue_settings(&bad).iter().any(|n| n.error));
    assert_eq!(count(&mut r, "title.over").name, "Page title: over 70 characters");
}
