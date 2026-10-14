# CLI: External Data Integrations

All commands share the desktop app's keychain entries, connection list, usage ledger and cache (plan D2, D5, D6). `--json` prints the types from data-model.md.

## Connections

```
spiderlens integrations list [--json]
spiderlens integrations connect google [--client-id ID --client-secret SECRET] [--scope search-console|analytics|sheets ...]
spiderlens integrations connect <crux|pagespeed|ahrefs|majestic|moz> --label NAME     # key read from stdin, never from an argument
spiderlens integrations forget <provider> [--label NAME]
spiderlens integrations properties <search-console|analytics> [--account LABEL] [--json]
```

`connect google` opens the system browser; without one it prints the sign-in link and waits for the redirect (US8.2).

## Runs

```
spiderlens integrations fetch FILE --provider search-console [--property P] [--from DATE --to DATE | --last 28d] [--type web] [--fresh]
spiderlens integrations fetch FILE --provider url-inspection [--property P] [--urls indexable|all|FILTER] [--max N]
spiderlens integrations fetch FILE --provider analytics --property P --stream S [--last 28d] [--keep-query]
spiderlens integrations fetch FILE --provider field-data [--devices phone,desktop]
spiderlens integrations fetch FILE --provider lab-tests [--urls FILTER] [--strategy mobile|desktop|both]
spiderlens integrations fetch FILE --provider ahrefs|majestic|moz [--urls FILTER] [--metrics LIST] [--cap N] [--yes]
spiderlens integrations resume FILE [--run ID]
spiderlens integrations queries FILE URL [--json]
spiderlens integrations remove FILE --provider P|--all [--compact]
spiderlens integrations export-sheets FILE --tab TAB [--filter F] [--split|--truncate] [--looker]
spiderlens integrations cache clear [--provider P]
```

`fetch` prints the estimate first; link-metric runs need `--yes` or an interactive `y`. Progress goes to stderr; `--json` prints the final `RunProgress`.

## Exit codes

Existing codes unchanged. 0 finished; 2 usage; 4 run failed (provider error, no access); 6 local error (006); **new: paused for quota** (number taken at merge from the next free code, proposed 10), with the reset time printed.
