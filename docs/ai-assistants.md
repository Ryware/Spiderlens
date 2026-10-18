# AI assistants

SpiderLens can let AI assistants on your computer (Claude Desktop, Claude Code,
Cursor and any other MCP client) read the crawls you made. It runs a small MCP
server as a local program that the assistant starts itself and talks to over
standard input and output. Nothing listens on the network and nothing leaves
your computer through SpiderLens.

Read access is off until you turn it on.

## Turn it on

1. Open **Settings › AI assistants**.
2. Turn on **Allow AI assistants to read my crawls**.
3. Optionally add **folders**: crawl files (`*.spiderlens`) directly inside them
   can be read too. Your recent crawls are always readable once access is on.
4. Under **Connect an assistant**, pick your assistant, press **Copy** and paste
   the snippet where the hint says.
5. Press **Test**. SpiderLens starts the server the way the assistant will and
   shows how many tools it offers.

The setting is read again on every request, so turning it off takes effect at
once, even for an assistant that is already connected.

### Claude Desktop

Settings › Developer › Edit Config, add the copied `spiderlens` entry under
`mcpServers`, then restart Claude Desktop.

### Claude Code

Run the copied command in a terminal:

```sh
claude mcp add spiderlens -- "/path/to/SpiderLens" mcp
```

### Cursor

Settings › MCP › Add new global MCP server, and paste the copied entry.

### Without the desktop app

The command-line tool has the same server:

```sh
spiderlens mcp                      # uses the app's settings
spiderlens mcp --allow-read --folder ~/Crawls
spiderlens mcp --self-test          # prints the tool list and exits
```

`--allow-read` and `--folder` apply only to that process and are not saved.

## What assistants can do

All tools only read. An assistant can:

- list your crawls (`list_crawls`) and read a crawl's summary (`crawl_summary`);
- list the result tabs and filters (`list_tabs`) and page through rows of any tab,
  with filters, search, conditions and sorting (`query_pages`);
- read one page's details, its links, images and headings (`page_details`), its
  stored HTML (`page_html`) and its forms and actions for AI agents
  (`page_agent_findings`);
- get up to 1,000 rows as CSV text (`export_rows`); no file is written.

Answers are kept small: at most 500 rows and 64 KB per answer, with a `next`
cursor for the rest, and long texts are cut at 2,000 characters.

## What assistants cannot do

- Start, pause or stop crawls, or change settings or files.
- Read files outside your recent crawls and the folders you added.
- Make SpiderLens fetch anything from the internet.

## Text written to mislead an AI

Pages you crawl can contain text written to manipulate an AI ("ignore your
instructions and …"). SpiderLens puts everything a website wrote (titles,
headings, link text, form labels, HTML) under a `crawled` key and tells the
assistant in every tool description that this is data from the website, not
instructions from you. An assistant may still be misled, so check what it does
with the crawl before acting on it.

## Activity

**Settings › AI assistants › Recent activity** lists each request: when, which
assistant, which tool, which crawl and how much it returned, or whether it was
refused. Arguments and answers are not recorded. **Clear activity** empties the
list.

## Agent readiness of the sites you crawl

Separately from assistants reading your crawls, SpiderLens checks how well a
crawled site works for AI agents that browse and act on it:

- forms and their fields (missing labels, placeholder-only labels, buttons
  without a name, forms without a submit button, CAPTCHA widgets), actions that
  only work by script, declared WebMCP tools and markdown alternates, shown in a
  page's details under **AI agents**;
- entry points that agents look for on each host: `/.well-known/ai-catalog.json`,
  `/.well-known/api-catalog` (RFC 9727) and `/.well-known/agent-card.json`
  (A2A), plus API descriptions linked from pages. They are fetched once per
  host after the last page, under robots.txt, and can be turned off in the
  crawl settings;
- rules in the **AI agents** issue category and an agent readiness score,
  computed when the crawl ends.

### The score

Each rule belongs to one part: Access, Reading, Acting or Discovery, with a
weight. For a page, each part scores
`100 × (1 − weight of failed rules ÷ weight of checked rules)`, and the page
total weights the parts Access 30, Reading 25, Acting 30 and Discovery 15,
counting only parts that had a checked rule. The crawl score is the mean of
its internal HTML pages that returned 200, with site-wide rules (entry points,
llms.txt) applied once to Discovery. Rules whose data the crawl did not collect
are not counted and are listed as not checked.
