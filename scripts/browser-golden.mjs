// Writes crates/test-fixtures/malformed/golden.json: what headless Chromium
// sees in each malformed page (spec 005 plan D8, SC-002).
//
// The browser builds the tree; the fields are read from it with the same
// text rules SpiderLens uses (whitespace collapsed, script/style/template
// text skipped, block elements separate words), so the comparison tests
// tree construction, not text formatting. JavaScript is off, like the crawler.
//
// Run: node scripts/browser-golden.mjs   (needs Playwright with Chromium)

import { createRequire } from "node:module";
import { execSync } from "node:child_process";
import { readdirSync, writeFileSync } from "node:fs";
import { join, dirname } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const require = createRequire(import.meta.url);
let playwright;
try {
  playwright = require("playwright");
} catch {
  const root = execSync("npm root -g").toString().trim();
  playwright = require(join(root, "playwright"));
}

const here = dirname(fileURLToPath(import.meta.url));
const dir = join(here, "..", "crates", "test-fixtures", "malformed");
const files = readdirSync(dir).filter((f) => f.endsWith(".html")).sort();

const browser = await playwright.chromium.launch();
const context = await browser.newContext({ javaScriptEnabled: false });
const page = await context.newPage();
const golden = {};

for (const file of files) {
  await page.goto(pathToFileURL(join(dir, file)).href);
  golden[file] = await page.evaluate(() => {
    const HTML = "http://www.w3.org/1999/xhtml";
    const collapse = (s) => s.replace(/[\t\n\f\r ]+/g, " ").trim();
    const INLINE = new Set(
      "a abbr b bdi bdo cite code data dfn em font i kbd label mark q s samp small span strong sub sup time u var wbr".split(
        " ",
      ),
    );
    const SKIP = new Set(["script", "style", "template", "title"]);
    const text = (el) => {
      let out = "";
      const visit = (n) => {
        if (n.nodeType === Node.TEXT_NODE) {
          out += n.data;
          return;
        }
        if (n.nodeType !== Node.ELEMENT_NODE) return;
        const name = n.localName;
        const html = n.namespaceURI === HTML;
        if (html && SKIP.has(name)) return;
        const block = html && !INLINE.has(name);
        if (block) out += " ";
        for (const c of n.childNodes) visit(c);
        if (block) out += " ";
      };
      for (const c of el.childNodes) visit(c);
      return collapse(out);
    };
    const htmlEls = (sel) => [...document.querySelectorAll(sel)].filter((e) => e.namespaceURI === HTML);
    const title = htmlEls("title")[0];
    const desc = htmlEls("meta").find((m) => (m.getAttribute("name") || "").trim().toLowerCase() === "description");
    const relHas = (e, v) => (e.getAttribute("rel") || "").toLowerCase().split(/[\t\n\f\r ]+/).includes(v);
    return {
      title: title ? collapse(title.textContent) : null,
      description: desc && desc.hasAttribute("content") ? collapse(desc.getAttribute("content")) : null,
      headings: htmlEls("h1, h2, h3, h4, h5, h6").map((h) => [Number(h.localName[1]), text(h)]),
      canonical: htmlEls("link[href]")
        .filter((l) => relHas(l, "canonical"))
        .map((l) => l.getAttribute("href").trim()),
      links: htmlEls("a[href], area[href]").map((a) => a.getAttribute("href").trim()),
    };
  });
}

await browser.close();
writeFileSync(join(dir, "golden.json"), JSON.stringify(golden, null, 1) + "\n");
console.log(`wrote ${files.length} pages to golden.json`);
