//! What the user pastes into an assistant's configuration to add SpiderLens
//! (spec 016 FR-020, plan D8). The command is this computer's SpiderLens
//! executable with `mcp`, so nothing else needs installing.

use std::path::Path;

use serde::Serialize;
use serde_json::json;
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, PartialEq, Eq, TS)]
#[ts(export, rename = "AssistantSnippet")]
pub struct Snippet {
    pub client: &'static str,
    pub label: &'static str,
    /// Where the text goes, in a few words.
    pub hint: &'static str,
    pub text: String,
}

/// Snippets for the assistants people use most, for this executable.
pub fn snippets(exe: &Path) -> Vec<Snippet> {
    let exe = exe.to_string_lossy();
    let config = serde_json::to_string_pretty(&json!({
        "mcpServers": { "spiderlens": { "command": exe, "args": ["mcp"] } }
    }))
    .expect("snippet serializes");
    vec![
        Snippet {
            client: "claude_desktop",
            label: "Claude Desktop",
            hint: "Settings › Developer › Edit Config, then restart Claude",
            text: config.clone(),
        },
        Snippet {
            client: "claude_code",
            label: "Claude Code",
            hint: "Run in a terminal",
            text: format!("claude mcp add spiderlens -- {} mcp", shell_quote(&exe)),
        },
        Snippet {
            client: "cursor",
            label: "Cursor",
            hint: "Settings › MCP › Add new global MCP server",
            text: config,
        },
    ]
}

/// Quotes a path for a shell command line when it needs it.
fn shell_quote(s: &str) -> String {
    if s.chars()
        .all(|c| c.is_ascii_alphanumeric() || "/._-:\\".contains(c))
    {
        s.to_string()
    } else {
        format!("\"{}\"", s.replace('"', "\\\""))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snippets_run_this_executable_with_mcp() {
        let s = snippets(Path::new(
            "/Applications/SpiderLens.app/Contents/MacOS/spiderlens-desktop",
        ));
        assert_eq!(s.len(), 3);
        let v: serde_json::Value = serde_json::from_str(&s[0].text).unwrap();
        assert_eq!(v["mcpServers"]["spiderlens"]["args"][0], "mcp");
        assert_eq!(
            s[1].text,
            "claude mcp add spiderlens -- /Applications/SpiderLens.app/Contents/MacOS/spiderlens-desktop mcp"
        );
    }

    #[test]
    fn paths_with_spaces_are_quoted() {
        let s = snippets(Path::new(r"C:\Program Files\SpiderLens\SpiderLens.exe"));
        assert_eq!(
            s[1].text,
            r#"claude mcp add spiderlens -- "C:\Program Files\SpiderLens\SpiderLens.exe" mcp"#
        );
        let v: serde_json::Value = serde_json::from_str(&s[2].text).unwrap();
        assert_eq!(
            v["mcpServers"]["spiderlens"]["command"],
            r"C:\Program Files\SpiderLens\SpiderLens.exe"
        );
    }
}
