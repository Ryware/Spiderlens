# Research: AI Agents (spec 016)

**Date**: 2026-10-13. Sources were read on that date. This note feeds `/speckit-clarify` and `/speckit-plan`; the spec stays technology-neutral and the choices below are recommendations, not approved decisions.

## Decisions for the owner

Each has a default the spec already assumes.

| # | Question | Default in the spec | Why |
|---|---|---|---|
| D1 | Should SpiderLens call an LLM itself ("LLM scoring" in the backlog)? | **No.** The user's own assistant does model judgement through the MCP server. Built-in prompts with the user's key are a later spec. | Constitution II (local first, opt-in third parties) and "secrets never stored". The MCP route gives the same power with zero keys in SpiderLens. |
| D2 | Can assistants start crawls through MCP? | **Yes, as P3, off by default**, with an allow-list of profiles, never ignoring robots.txt. | Useful for automation (re-crawl after deploy), but it is network activity started by software. |
| D3 | "Test as AI agents" with other vendors' user agents? | **Opt-in, off, 20 pages per host**, labelled for sites the user controls. | Answers "does my WAF block agents?", the most common question, without spoofing during normal crawls. Same rule as the existing Googlebot presets (constitution IV). |
| D4 | Entry point probes on by default? | **On**, at most 8 requests per host plus one per linked card. | Cheap, off the fetch path, and the conventions are new and often broken. |
| D5 | Score: transparent weights or a learned model? | **Transparent sum of rule weights**, four parts. | A consultant must be able to defend every point. |
| D6 | MCP over the network (HTTP) as well as standard input and output? | **Standard input and output only.** | No open port means no DNS rebinding or local network exposure, and no server to secure (constitution II). |

## R1. What agents exist and how they behave (2026)

- Three kinds of AI visitors: training crawlers (GPTBot, ClaudeBot), search/answer crawlers (OAI-SearchBot, Claude-SearchBot, PerplexityBot) and **user-triggered agents** that act for a person (ChatGPT-User / ChatGPT agent, Claude-User, Google-Agent, Perplexity Comet). Spec 015 owns the registry; 016 only needs the purpose "acts for a user".
- **Google-Agent** was added to Google's fetcher docs on 2026-03-20. It is user-triggered and generally ignores robots.txt; ChatGPT-User and Claude-User respect it. Google is experimenting with Web Bot Auth signatures under the identity `https://agent.bot.goog`. ([SEJ](https://www.searchenginejournal.com/google-agent-the-webs-new-visitor-just-got-an-identity/), [ChatGPT agent record](https://knownagents.com/agents/chatgpt-agent))
- Many user-triggered agents drive a real browser, so JavaScript is less of a blocker for them than for crawlers, but forms without labels, icon-only buttons and click handlers without links still defeat them: agents read the accessibility tree. That is why the Acting part checks accessible names, not only raw HTML.

## R2. Discovery conventions an agent may look for

| Convention | Where | Status (2026-10) | 016 action |
|---|---|---|---|
| `llms.txt`, `llms-full.txt` | `/llms.txt` | Community proposal. ~36k sites (May 2026); 97% of files got zero AI requests in May 2026; no major AI vendor commits to reading it; Google added an optional llms.txt audit to Lighthouse in May 2026. Common Crawl July 2026: only 49.9% follow the structure, 10.6% are robots.txt served at the wrong path. ([PPC Land](https://ppc.land/llms-txt-adoption-rises-8-8x-but-97-of-files-get-zero-ai-requests/), [Common Crawl](https://commoncrawl.org/blog/a-content-analysis-of-llms-txt-files-from-the-july-2026-crawl-archive)) | **Owned by 015.** 016 reads the result into Discovery. |
| AI Catalog | `/.well-known/ai-catalog.json` | Working draft, Linux Foundation; links or embeds MCP server cards, A2A agent cards and more. ([repo](https://github.com/Agent-Card/ai-catalog)) | Fetch, parse, follow linked cards. |
| MCP Server Cards | Any URI, `<endpoint>/server-card` recommended; listed from the AI Catalog | SEP-2127 **Final**, extensions track, created 2026-01-21. Cards carry identity, endpoints, protocol versions; deliberately no tool list. ([SEP-2127](https://modelcontextprotocol.io/seps/2127-mcp-server-cards)) | Parse; one discovery request to the endpoint; never call tools. |
| A2A Agent Card | `/.well-known/agent-card.json` | A2A protocol, widely implemented by agent frameworks. | Parse if linked or present. |
| API catalog | `/.well-known/api-catalog` | RFC 9727 (2025). | Fetch, parse linkset. |
| Service descriptions | `Link: rel="service-desc"` / `service-doc` | RFC 8631. | Read from stored headers and `<link>` elements. |
| Markdown for agents | `<link rel="alternate" type="text/markdown">`, or `Accept: text/markdown` | Cloudflare shipped on-the-fly conversion 2026-02-12 (`content-type: text/markdown`, `x-markdown-tokens`, `content-signal`). ([Cloudflare](https://developers.cloudflare.com/changelog/2026-02-12-markdown-for-agents)) | Alternates checked by default; negotiation sample opt-in. |
| MCP DNS discovery | DNS TXT | Individual IETF drafts only ([draft-morrison](https://datatracker.ietf.org/doc/html/draft-morrison-mcp-dns-discovery-01)). | Not checked (too early, no DNS in scope). |

## R3. Acting on a page

- **WebMCP** lets a page expose tools to an in-browser agent. Imperative API `document.modelContext.registerTool()` (renamed from `navigator.modelContext` in the 2026-09-30 draft) runs in script and is invisible to a crawler without instrumenting the renderer. The **declarative API** puts attributes on forms: `toolname`, `tooldescription`, `toolautosubmit` on `<form>`, `toolparamdescription` on fields; Chrome origin trial, docs updated 2026-09-25 (Chrome 156). W3C Community Group draft, not standards track. ([Chrome declarative API](https://developer.chrome.com/docs/ai/webmcp/declarative-api), [reality check](https://studiomeyer.io/es/blog/webmcp-reality-check-may-2026.md)) → 016 checks the declarative attributes only.
- **Schema.org actions** (`potentialAction`, `SearchAction`, `OrderAction`, `ReserveAction`) are read from spec 005's stored structured data.
- **Accessible names and labels** follow the HTML accessible name computation; this is what agents reading the accessibility tree see.
- **Challenges**: reCAPTCHA, hCaptcha, Cloudflare Turnstile and similar widgets are detected by signature (script source, element class) from a versioned list.

## R4. Agent identity and usage signals (context, owned elsewhere)

- **Web Bot Auth**: IETF WG draft `draft-ietf-webbotauth-httpsig-protocol-00` (2026-09-01): `Signature`, `Signature-Input`, `Signature-Agent` headers, keys at `/.well-known/http-message-signatures-directory`. Relevant to sites verifying agents; SpiderLens signing its own requests is out of scope (it would need a published key). ([draft](https://datatracker.ietf.org/doc/html/draft-ietf-webbotauth-httpsig-protocol))
- **AIPREF** (`Content-Usage` header and robots.txt rule): IETF drafts, attach -05 (2026-08-19), vocab -08 (2026-09-14), no deployment beyond demos. **Content-Signal** is Cloudflare's deployed equivalent. Both are usage preferences, owned by spec 015. ([Lumar notes](https://agentic-readiness.lumar.io/docs/access/ai-pref), [attach draft](https://datatracker.ietf.org/doc/draft-ietf-aipref-attach/))

## R5. SpiderLens's own MCP server

- **Transport**: standard input and output, started by the assistant (`spiderlens mcp`). All mainstream desktop clients (Claude Desktop, Claude Code, Cursor, ChatGPT desktop) support it. No port is opened.
- **Protocol version**: MCP 2026-07-28 makes the core stateless (no `initialize` handshake or session id), requires tool input schemas to be JSON Schema 2020-12, adds `ttlMs` on `tools/list`, and deprecates roots, logging and sampling. A read-only stdio server is affected mostly by the handshake change. The plan should target 2026-07-28 and keep accepting the previous version while clients migrate. ([summary](https://www.developersdigest.tech/blog/mcp-2026-07-28-breaking-changes)) *(secondary source; confirm against the spec during `/speckit-plan`)*
- **Rust options** (to compare in the plan): the official Rust SDK `rmcp`, or a small hand-written JSON-RPC loop over stdio on top of `crates/results` (the server needs perhaps 12 tools, all read-only). Principle VIII favours whichever adds less.
- **Reuse**: all queries go through `crates/results`, which already backs the desktop table and the CLI, so parity (SC-006) is structural rather than tested-in.
- **Prompt injection**: crawled text is attacker-controlled by definition. Keep it out of tool descriptions, return it in a `content` field marked untrusted, and keep the server read-only by default so a hijacked assistant can at worst read crawl data the user already allowed.

## R6. Competitors

- Lumar has an "agentic readiness" documentation site that tracks conventions with maturity levels but only scores ratified ones ([docs](https://agentic-readiness.lumar.io/docs)). Online "agent readiness checkers" (LLM Pulse, SEOJuice, WebsiteRating) check one URL for llms.txt, robots rules and schema. None found that checks forms and actions across a whole crawl, compares raw and rendered, or exposes crawl data to the user's assistant locally. Screaming Frog has an MCP-style integration only through its API connectors, not over a crawl file *(unverified)*.

## R7. Starting score weights (to tune in `/speckit-clarify`)

Part weights: Access 30, Reading 25, Acting 30, Discovery 15. Discovery is lowest because adoption is low and no major agent depends on it yet (R2).

| Part | Example rules (priority) |
|---|---|
| Access | Agents disallowed by robots.txt (High, from 015); agent sample blocked or challenged (High, opt-in); pages with a bot challenge (High, from 014) |
| Reading | Main content only after JavaScript (High, from 014); no Markdown alternate (Opportunity) |
| Acting | Form only after JavaScript (High); form fields without labels (Medium); buttons without accessible names (Medium); actions without a link (Medium); action behind a challenge (Medium); declared agent tool missing description (Low); no `SearchAction` on a site with search (Opportunity) |
| Discovery | Broken AI Catalog or card (Medium); MCP endpoint in a card does not answer (Medium); no entry points at all (Opportunity); `llms.txt` problems (from 015) |
