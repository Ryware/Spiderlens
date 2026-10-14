# Contract: command line

## `spiderlens mcp`

```text
spiderlens mcp [--allow-read] [--folder DIR]... [--allow-crawl --profile NAME...] [--self-test]
```

- Without flags, access comes from `assistants.json` (the same file the desktop app writes).
- `--allow-read` turns read access on for this process only; `--folder` adds allowed folders for this process only. `--allow-crawl` requires at least one `--profile`.
- stdout is reserved for MCP messages; diagnostics on stderr. Exit 0 when the client closes stdin.
- The desktop executable accepts the same arguments after `mcp` (plan D8).

## `spiderlens agents`

```text
spiderlens agents summary FILE [--json]
spiderlens agents check FILE [--json]
spiderlens agents page FILE URL [--json]
```

- `summary`: the Agents summary (FR-018). Text output: crawl score and parts, top rules, entry points per host, not-checked list. `--json`: the same object as the MCP `agents_summary` tool, without the `crawled` envelope.
- `check`: re-runs collection (from stored HTML, if any rows are missing and HTML is stored) and evaluation on a saved crawl; prints the new summary. Opens the file for writing, so it is refused while another process holds the lock (spec 006: exit 2 with "in use by …").
- `page`: one page's agent findings, as `page_agent_findings`.

## `spiderlens crawl` flags (heading "Agents")

| Flag | Default | Maps to |
|---|---|---|
| `--no-agent-entry-points` | entry points on | `agent_settings.entry_points = false` |
| `--markdown-alternates-max N` | 50 | `markdown_alternates_max` |
| `--markdown-negotiation [N]` | off; sample 10 when given without N | `markdown_negotiation`, `markdown_negotiation_sample` |

The user agent probe flags are spec 015's.

## Exit codes

Unchanged from earlier specs. `agents check` on a file without agent data and without stored HTML still succeeds and reports the forms rules as not checked.
