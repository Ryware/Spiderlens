//! Spec 016 User Story 6: assistants start crawls only when the user allows
//! it (SC-009). With crawling off the crawl tools are not listed and every
//! call is refused; with it on, only allowed profiles that respect
//! robots.txt start a crawl.

use serde_json::{Map, Value, json};
use spiderlens_config::{AppDirs, AssistantSettings, AssistantStore, CrawlConfig, ProfileStore, RobotsMode};
use spiderlens_mcp::{Overrides, ServeOptions, Server, activity, tool_list_with};

const OFF: &str = "Crawling through SpiderLens is off. Turn on Settings › AI assistants › Allow AI assistants to start crawls.";
const CRAWL_TOOLS: [&str; 5] = [
    "start_crawl",
    "crawl_progress",
    "pause_crawl",
    "resume_crawl",
    "stop_crawl",
];

struct Fixture {
    _dir: tempfile::TempDir,
    dirs: AppDirs,
}

fn fixture() -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let dirs = AppDirs {
        config: dir.path().join("config"),
        data: dir.path().join("data"),
    };
    let profiles = ProfileStore::open(dirs.config.join("profiles")).unwrap();
    profiles
        .save(
            "Polite",
            &CrawlConfig::default(),
            Some("https://example.test/"),
            false,
        )
        .unwrap();
    let mut rude = CrawlConfig::default();
    rude.robots.mode = RobotsMode::Ignore;
    profiles.save("Rude", &rude, None, false).unwrap();
    Fixture { _dir: dir, dirs }
}

fn settings(f: &Fixture, s: AssistantSettings) {
    AssistantStore::open(&f.dirs.config).unwrap().save(&s).unwrap();
}

fn server(f: &Fixture) -> Server {
    Server::new(ServeOptions {
        overrides: Overrides::default(),
        dirs: Some(f.dirs.clone()),
    })
}

fn args(v: Value) -> Map<String, Value> {
    v.as_object().unwrap().clone()
}

fn names(s: &Server) -> Vec<String> {
    s.tools().iter().map(|t| t.name.to_string()).collect()
}

#[test]
fn crawl_tools_are_absent_and_refused_while_crawling_is_off() {
    let f = fixture();
    // Reading on, crawling off: the state a user who only wants analysis has.
    settings(
        &f,
        AssistantSettings {
            allow_read: true,
            ..AssistantSettings::default()
        },
    );
    let s = server(&f);
    assert!(!s.crawl_access());
    let listed = names(&s);
    assert!(listed.iter().any(|n| n == "list_crawls"));
    for t in CRAWL_TOOLS {
        assert!(!listed.iter().any(|n| n == t), "{t} must not be listed");
        let e = s
            .call(
                t,
                &args(json!({"profile": "Polite", "crawl": "c_00000000"})),
                Some("test 1.0".into()),
            )
            .unwrap_err();
        assert!(e.refused, "{t}");
        assert_eq!(e.message, OFF);
    }
    // Nothing was created.
    assert!(!f.dirs.unsaved_crawls().exists());
    // The refusals are in the activity log.
    let log = activity::Log::new(&f.dirs.data).read(20);
    assert_eq!(log.iter().filter(|e| e.refused).count(), 5);
}

#[test]
fn the_crawl_tools_appear_when_crawling_is_turned_on_and_are_not_read_only() {
    let f = fixture();
    let s = server(&f);
    assert!(!names(&s).iter().any(|n| n == "start_crawl"));
    settings(
        &f,
        AssistantSettings {
            allow_crawl: true,
            allowed_profiles: vec!["Polite".into()],
            ..AssistantSettings::default()
        },
    );
    // The same server sees the change on its next list.
    assert!(s.crawl_access());
    let listed = names(&s);
    for t in CRAWL_TOOLS {
        assert!(listed.iter().any(|n| n == t), "{t} must be listed");
    }
    for t in tool_list_with(true)
        .iter()
        .filter(|t| CRAWL_TOOLS.contains(&&*t.name))
    {
        let a = t.annotations.as_ref().expect("annotations");
        assert_eq!(a.read_only_hint, Some(false));
        assert_eq!(a.destructive_hint, Some(false));
        assert_eq!(a.open_world_hint, Some(true));
    }
}

#[test]
fn only_allowed_profiles_that_respect_robots_start_a_crawl() {
    let f = fixture();
    settings(
        &f,
        AssistantSettings {
            allow_crawl: true,
            // "Rude" is allowed in the file, as if edited by hand: the
            // server checks again.
            allowed_profiles: vec!["Polite".into(), "Rude".into()],
            ..AssistantSettings::default()
        },
    );
    let s = server(&f);
    let call = |a: Value| s.call("start_crawl", &args(a), Some("test 1.0".into()));

    let e = call(json!({"profile": "Other", "start_url": "https://example.test/"})).unwrap_err();
    assert!(e.refused && e.message.contains("not allowed"));
    let e = call(json!({"profile": "Rude", "start_url": "https://example.test/"})).unwrap_err();
    assert!(e.refused && e.message.contains("ignores robots.txt"));

    // Bad requests are errors, not refusals, and create no file.
    let e = call(json!({"profile": "Polite", "start_url": "https://example.test/", "urls": ["https://example.test/a"]})).unwrap_err();
    assert!(!e.refused && e.message.contains("not both"));
    let too_many: Vec<String> = (0..10_001).map(|i| format!("https://example.test/{i}")).collect();
    let e = call(json!({"profile": "Polite", "urls": too_many})).unwrap_err();
    assert!(!e.refused && e.message.contains("at most 10000"));
    let e = call(json!({"profile": "Polite", "urls": ["", "  "]})).unwrap_err();
    assert!(!e.refused);
    assert!(!f.dirs.unsaved_crawls().exists());
}

#[test]
fn only_crawls_this_server_started_can_be_controlled() {
    let f = fixture();
    settings(
        &f,
        AssistantSettings {
            allow_crawl: true,
            allowed_profiles: vec!["Polite".into()],
            ..AssistantSettings::default()
        },
    );
    let s = server(&f);
    for t in ["crawl_progress", "pause_crawl", "resume_crawl", "stop_crawl"] {
        let e = s
            .call(t, &args(json!({"crawl": "c_12345678"})), None)
            .unwrap_err();
        assert!(
            !e.refused && e.message.contains("did not start this crawl"),
            "{t}"
        );
    }
    let e = s.call("crawl_progress", &args(json!({})), None).unwrap_err();
    assert!(e.message.contains("`crawl` is required"));
}
