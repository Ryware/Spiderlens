# Contract: desktop commands (Tauri)

All types are exported to `ui/src/generated` with ts-rs, as in spec 007.

| Command | Args | Returns | Notes |
|---|---|---|---|
| `agents_summary` | `{ handle }` | `AgentsSummary` | same data as the MCP tool, typed |
| `agents_page` | `{ handle, url_id }` | `PageAgentFindings` | for the inspector section |
| `agents_run_checks` | `{ handle }` | `AgentsSummary` | "Run agent checks" on an older or not-yet-evaluated file; refused while the file is locked by a crawl |
| `assistants_get` | `{}` | `AssistantSettings` + `snippets: [{client, label, text}]` + `executable` | snippets built from `current_exe()` |
| `assistants_set` | `{ settings: AssistantSettings }` | `AssistantSettings` | validates folders exist; profiles must exist and not ignore robots.txt |
| `assistants_test` | `{}` | `SelfTest` or error text | runs `<exe> mcp --self-test` with a 10 s timeout |
| `assistants_activity` | `{ limit }` | `[ActivityEntry]` | newest first |
| `assistants_activity_clear` | `{}` | `()` | |

The Agents tab needs no new command: it is a spec 007 tab (`view_open` with `tab: "agents"`).

## Fake transport

`ui/src/testing/fakeTransport.ts` gains handlers for every command above with fixture data covering: an evaluated crawl, a crawl with rules not checked, read access off and on, a failing self-test, and an empty activity log.
