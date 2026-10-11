use serde::Serialize;

/// Identity of this build. The CLI and the desktop app both read these
/// compile-time constants from the engine, so the same build always reports
/// the same values (FR-022, SC-008).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BuildInfo {
    pub product: &'static str,
    pub product_version: &'static str,
    pub engine_version: &'static str,
    pub build_id: &'static str,
    pub commit: Option<&'static str>,
}

impl BuildInfo {
    /// `0.1.0 (build abc1234)`, for `--version` flags.
    pub const VERSION_LINE: &'static str = concat!(
        env!("CARGO_PKG_VERSION"),
        " (build ",
        env!("SPIDERLENS_BUILD_ID"),
        ")"
    );

    pub const fn current() -> Self {
        let commit = env!("SPIDERLENS_COMMIT");
        Self {
            product: "SpiderLens",
            // The engine shares the workspace version, so both are the same
            // number today; they are separate fields so they can diverge later.
            product_version: env!("CARGO_PKG_VERSION"),
            engine_version: env!("CARGO_PKG_VERSION"),
            build_id: env!("SPIDERLENS_BUILD_ID"),
            commit: if commit.is_empty() { None } else { Some(commit) },
        }
    }

    /// Honest user agent sent with every request (principle IV, FR-016).
    pub fn user_agent() -> String {
        spiderlens_config::spiderlens_user_agent()
    }
}

impl std::fmt::Display for BuildInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} {} (engine {}, build {})",
            self.product, self.product_version, self.engine_version, self.build_id
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_id_is_never_empty() {
        let info = BuildInfo::current();
        assert!(!info.build_id.is_empty());
        assert_eq!(info.product, "SpiderLens");
    }

    #[test]
    fn build_id_matches_commit_when_known() {
        let info = BuildInfo::current();
        if let Some(commit) = info.commit {
            assert!(info.build_id.starts_with(&commit[..7]));
        }
    }

    #[test]
    fn user_agent_names_product_and_version() {
        let ua = BuildInfo::user_agent();
        assert!(ua.starts_with("SpiderLens/"));
        assert!(ua.contains(env!("CARGO_PKG_VERSION")));
    }
}
