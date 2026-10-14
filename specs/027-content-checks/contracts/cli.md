# CLI Contract: Content Checks (spec 027)

Existing commands gain options; new commands are `serp` and `spell`. Exit codes follow the existing CLI (0 ok, 2 usage, 6 file errors from 006).

## Part A

```text
spiderlens serp <list.csv|-> [--device desktop|mobile|both] [--format table|csv|json] [--out FILE]
```
Reads URL, title, description rows (header optional, same parser as List mode). Prints chars, px and truncation per row. No network.

```text
spiderlens export <crawl> --tab page-titles      # gains title_px, title_trunc_desktop, title_trunc_mobile columns
spiderlens export <crawl> --proposals [--out FILE]
```

## Part B

```text
spiderlens export <crawl> --tab <id> [--segment NAME]
spiderlens segments <crawl> [--format table|json]   # segment, URL count
spiderlens crawl ... --profile P                      # segments come from the profile
```

## Part C

```text
spiderlens crawl <url> --crawl-pdfs [--no-follow-pdf-links] [--pdf-max-size 20MB] [--pdf-time-limit 5s]
spiderlens export <crawl> --tab pdfs
```

## Part D

```text
spiderlens spell <crawl> [--grammar] [--lang LANG] [--ignore WORD]... [--format table|csv|json] [--out FILE]
spiderlens spell dictionaries list|add <lang>|add-file <aff> <dic>|remove <lang>
spiderlens crawl <url> --spelling [--grammar]
```
`spell <crawl>` runs on stored text; exits 0 when done, also when errors are found (`issues --fail-on` from 009 is the gate).

## Part E

```text
spiderlens crawl <url> --check-amp
spiderlens export <crawl> --tab amp
```
