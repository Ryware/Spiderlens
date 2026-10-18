# Accessibility and mobile checks

SpiderLens checks every page it crawls for common accessibility problems and for mobile friendliness. Nothing is sent to a server. All checks run on your machine.

> Automated checks find only part of the accessibility problems. A page without violations is not necessarily accessible.

SpiderLens shows this note wherever it shows accessibility results: the tab, the details pane, exports and the command line summary. Use the results as a start. A person still needs to test with a keyboard and a screen reader.

## Levels

Set the level in Settings > Accessibility.

| Level | What it does |
| --- | --- |
| Off | No accessibility checks. Mobile checks follow their own switch. |
| Static checks | Reads the page's HTML. Adds no requests and little time. This is the default. |
| Full checks | Runs an accessibility engine inside the rendered page. Needs JavaScript rendering. |

If you choose Full checks while rendering is off, SpiderLens says so and runs static checks instead. The notice has a button that opens the Rendering settings.

## What static checks cover

Static checks read the HTML as the server sent it. They find things such as:

- Images without alt text.
- Form fields without a label.
- Links and buttons without a name.
- A missing page language or page title.
- Skipped heading levels.
- A missing viewport tag.

They cannot see colours, layout or anything a script adds later.

## What full checks add

Full checks run [axe-core](https://github.com/dequelabs/axe-core) in the rendered page. They see the page as a visitor does. They also check:

- Colour contrast.
- ARIA roles and attributes.
- Hidden or covered elements.
- Content added by scripts.

Full checks stop after the time per page you set (1 to 30 seconds, 5 by default). A page that runs out of time is marked "Timed out". Where a full check and a static check find the same problem, SpiderLens keeps the full one.

## WCAG target

Pick the version (2.0, 2.1 or 2.2) and the level (A, AA or AAA). The default is WCAG 2.2 level AA. Counts and the "Checks failed" column follow this target. Changing the target does not run the checks again.

Best practice advice is not part of WCAG. It is on by default. Turn it off to see only WCAG results.

Each result has an impact: critical, serious, moderate or minor. Results that need a person to judge are listed as "Needs review".

## Mobile checks

The Mobile tab shows, for each page:

- The viewport tag and whether it is usable.
- Content width against window width, to find sideways scrolling.
- The share of small text.
- Links and buttons that are too small to tap.

You can change what counts as small text, the share that flags a page, the tap target size and the overflow allowance in the same settings page. Width, text and tap target checks need a rendered page.

## Run the checks on a saved crawl

Open a crawl, then open the command palette (Ctrl+K or Cmd+K) and choose "Run accessibility checks". SpiderLens asks first, because earlier static results are replaced. It reads the stored HTML and sends no requests. When it finishes it shows how many pages it checked, how many had findings and how many had no stored HTML.

Crawls made without stored HTML cannot be checked this way.

## Command line

```sh
spiderlens crawl example.com --accessibility static
spiderlens crawl example.com --accessibility full --render javascript --wcag 2.2-aa
spiderlens accessibility example.spiderlens
spiderlens accessibility example.spiderlens --wcag 2.1-aa
spiderlens accessibility example.spiderlens --needs-review
spiderlens accessibility example.spiderlens --summary --format csv
spiderlens accessibility example.spiderlens --mobile
spiderlens accessibility run example.spiderlens --static
```

Crawl flags:

| Flag | Meaning |
| --- | --- |
| `--accessibility off\|static\|full` | The level. Default `static`. |
| `--wcag 2.0-a ... 2.2-aaa` | The WCAG target. Default `2.2-aa`. |
| `--no-a11y-best-practice` | Leave out best practice advice. |
| `--no-a11y-mobile` | Skip the mobile checks. |
| `--a11y-budget <seconds>` | Time per page for full checks, 1 to 30. |

`spiderlens accessibility <file>` options:

| Option | Meaning |
| --- | --- |
| `--wcag <target>` | Count against another target without checking again. |
| `--needs-review` | Show "needs review" results instead of violations. |
| `--format text\|csv\|json` | CSV and JSON have one row per page, check and element. |
| `--summary` | One row per check. |
| `--mobile` | The Mobile tab's rows. |

The command exits with 0 when it printed, 1 when the file cannot be read and 2 for wrong usage. To fail a build on violations, use `spiderlens issues <file> --fail-on high`.

`spiderlens accessibility run <file> --static` checks an existing crawl again. It asks first unless you pass `--yes`. It exits with 2 for files without stored HTML and for files another process has open for writing. `--full` on an existing crawl is not built yet.

## Credits

Full checks use [axe-core](https://github.com/dequelabs/axe-core) by Deque Systems, licensed under the Mozilla Public License 2.0 (MPL-2.0). SpiderLens bundles axe-core unchanged. Its source and license are at the link above.
