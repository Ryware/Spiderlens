use std::time::Duration;

use crate::BuildInfo;

/// Options for a single-page check. Defaults follow the cross-spec decisions
/// (10 s connect, 30 s read, 50 MB body, 10 redirect hops).
#[derive(Debug, Clone)]
pub struct CheckOptions {
    pub connect_timeout: Duration,
    pub read_timeout: Duration,
    pub max_body_bytes: u64,
    pub max_redirects: usize,
    pub user_agent: String,
}

impl Default for CheckOptions {
    fn default() -> Self {
        Self {
            connect_timeout: Duration::from_secs(10),
            read_timeout: Duration::from_secs(30),
            max_body_bytes: 50 * 1024 * 1024,
            max_redirects: 10,
            user_agent: BuildInfo::user_agent(),
        }
    }
}
