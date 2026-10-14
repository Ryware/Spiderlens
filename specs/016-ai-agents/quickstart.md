# Quickstart: AI Agents (spec 016)

Manual checks that mirror the acceptance scenarios. Automated tests cover the same ground (tasks.md).

## 1. Agents fixture site

```bash
cargo run -p spiderlens-test-fixtures --bin agents-site   # serves 127.0.0.1:8720 and api.agents.test
```

Pages: `/search` (GET search form in raw HTML), `/contact-js` (form injected by script), `/unlabelled`, `/icons` (icon buttons with and without `aria-label`), `/fake-links` (`href="#"`, `javascript:`, `onclick` divs), `/captcha` (Turnstile widget), `/webmcp` (declarative tool, one field undescribed), `/injection` (text aimed at AI agents), Markdown alternates on half the pages. Well-known: valid AI Catalog linking an MCP server card and an A2A card, an invalid API catalog, an MCP endpoint that answers `server/discover`, a second one that answers 401.

## 2. Agent readiness (US2, US3, US4)

```bash
spiderlens crawl http://127.0.0.1:8720/ -o /tmp/agents.spiderlens
spiderlens agents summary /tmp/agents.spiderlens
spiderlens agents page /tmp/agents.spiderlens http://127.0.0.1:8720/unlabelled
```

Expect: entry points listed with the invalid API catalog flagged, the 401 endpoint as "requires sign-in"; `/contact-js` under "Form only after JavaScript" shown as not checked (no rendering); score with "N rules not checked". Re-run with `--render` (spec 014) and the form rule is checked. Open the file in the app: Agents tab filters show the same counts; the Agents summary shows the same score.

## 3. MCP server (US1)

```bash
spiderlens mcp --allow-read --folder /tmp --self-test
npx @modelcontextprotocol/inspector spiderlens mcp --allow-read --folder /tmp
```

In the inspector: `list_crawls`, `crawl_summary`, `query_pages` on tab `internal` filter `client_error`, page through with `next`, `page_details` for `/injection` (its text only under `crawled`), `page_html` in chunks. Try `start_crawl`: it is not listed.

With Claude Desktop: Settings › AI assistants in SpiderLens, turn on read access, copy the Claude Desktop snippet into its config, restart it, ask "What are the agent readiness problems in my latest SpiderLens crawl?". The activity list in SpiderLens shows the calls.

## 4. Assistants starting crawls (US6)

Turn on "Allow AI assistants to start crawls", allow the profile "Polite". Ask the assistant to crawl `http://127.0.0.1:8720/`. The crawl appears in the app's recent crawls marked "started by an AI assistant"; Stop in the app stops it. A profile that ignores robots.txt cannot be allowed.

## 5. Speed (SC-001, SC-004)

```bash
cargo run --release -p spiderlens-test-fixtures --bin bench-site -- --pages 100000 &
spiderlens crawl http://127.0.0.1:8700/p/0 --speed fast -o /tmp/bench.spiderlens   # compare with benchmarks.md baseline
```

Repeat while a script calls `query_pages` once per second through `spiderlens mcp`.
