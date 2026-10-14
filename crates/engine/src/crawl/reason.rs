//! Closed vocabularies stored in the crawl file (data-model.md).
//!
//! Each enum maps to a stable lowercase string. Adding a variant is a compile
//! error in every exhaustive `match`, so a new reason cannot be forgotten.

use serde::{Deserialize, Serialize};

macro_rules! string_enum {
    ($(#[$m:meta])* $name:ident { $($variant:ident = $text:literal),+ $(,)? }) => {
        $(#[$m])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
        #[serde(rename_all = "snake_case")]
        pub enum $name { $($variant),+ }

        impl $name {
            pub const ALL: &'static [Self] = &[$(Self::$variant),+];

            pub fn as_str(self) -> &'static str {
                match self { $(Self::$variant => $text),+ }
            }

            pub fn parse(s: &str) -> Option<Self> {
                match s { $($text => Some(Self::$variant),)+ _ => None }
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(self.as_str())
            }
        }
    };
}

string_enum! {
    /// Why a discovered URL was not crawled. Declared in the FR-038 order:
    /// when several apply, the earliest wins.
    NotCrawledReason {
        MalformedUrl = "malformed_url",
        UnsupportedScheme = "unsupported_scheme",
        External = "external",
        OutsideStartFolder = "outside_start_folder",
        Excluded = "excluded",
        NotIncluded = "not_included",
        OverDepthLimit = "over_depth_limit",
        Nofollow = "nofollow",
        BlockedByRobots = "blocked_by_robots",
        RobotsUnavailable = "robots_unavailable",
        OverUrlLimit = "over_url_limit",
        SitemapOnly = "sitemap_only",
        CrawlStopped = "crawl_stopped",
    }
}

impl NotCrawledReason {
    /// The label users see (spec FR-037).
    pub fn label(self) -> &'static str {
        match self {
            Self::MalformedUrl => "Malformed URL",
            Self::UnsupportedScheme => "Unsupported scheme",
            Self::External => "External",
            Self::OutsideStartFolder => "Outside start folder",
            Self::Excluded => "Excluded by rule",
            Self::NotIncluded => "Not matching include rules",
            Self::OverDepthLimit => "Over depth limit",
            Self::Nofollow => "Nofollow",
            Self::BlockedByRobots => "Blocked by robots.txt",
            Self::RobotsUnavailable => "Blocked: robots.txt unavailable",
            Self::OverUrlLimit => "Over URL limit",
            Self::SitemapOnly => "Found only in sitemap",
            Self::CrawlStopped => "Crawl stopped",
        }
    }

    /// Reasons that depend on how a URL was reached, not on the URL itself.
    /// A URL holding one of these is crawled if a better path turns up
    /// (FR-039, research R3). "Found only in sitemap" ends as soon as a
    /// link to the URL is found (spec 011 plan D5).
    pub fn can_upgrade(self) -> bool {
        matches!(self, Self::OverDepthLimit | Self::Nofollow | Self::SitemapOnly)
    }
}

string_enum! {
    /// How a URL was first discovered (FR-035).
    DiscoverySource {
        Start = "start",
        List = "list",
        Link = "link",
        Redirect = "redirect",
        Canonical = "canonical",
        Hreflang = "hreflang",
        Image = "image",
        Sitemap = "sitemap",
    }
}

string_enum! {
    /// What kind of reference a link is.
    LinkKind {
        Anchor = "anchor",
        Redirect = "redirect",
        Canonical = "canonical",
        Hreflang = "hreflang",
        Image = "image",
        MetaRefresh = "meta_refresh",
        Next = "next",
        Prev = "prev",
    }
}

impl LinkKind {
    pub fn source(self) -> DiscoverySource {
        match self {
            Self::Anchor => DiscoverySource::Link,
            Self::Redirect => DiscoverySource::Redirect,
            Self::Canonical => DiscoverySource::Canonical,
            Self::Hreflang => DiscoverySource::Hreflang,
            Self::Image => DiscoverySource::Image,
            Self::MetaRefresh => DiscoverySource::Redirect,
            Self::Next | Self::Prev => DiscoverySource::Link,
        }
    }
}

string_enum! {
    /// Where a URL is in its life cycle.
    UrlState {
        Queued = "queued",
        InFlight = "in_flight",
        Crawled = "crawled",
        NotCrawled = "not_crawled",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reasons_are_declared_in_fr_038_order() {
        let order: Vec<_> = NotCrawledReason::ALL.iter().map(|r| r.as_str()).collect();
        assert_eq!(
            order,
            [
                "malformed_url",
                "unsupported_scheme",
                "external",
                "outside_start_folder",
                "excluded",
                "not_included",
                "over_depth_limit",
                "nofollow",
                "blocked_by_robots",
                "robots_unavailable",
                "over_url_limit",
                "sitemap_only",
                "crawl_stopped",
            ]
        );
        assert!(NotCrawledReason::External < NotCrawledReason::Excluded);
    }

    #[test]
    fn strings_round_trip_and_match_serde() {
        for r in NotCrawledReason::ALL {
            assert_eq!(NotCrawledReason::parse(r.as_str()), Some(*r));
            assert_eq!(serde_json::to_value(r).unwrap(), r.as_str());
        }
    }
}
