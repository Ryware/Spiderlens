# CLI Contract: configuration

## Shared crawl options

Accepted by `spiderlens config show|validate` now, and by `spiderlens crawl`
from spec 003. Precedence: built-in defaults, then `--profile`, then flags.

| Flag | Setting |
| --- | --- |
| `--profile <FILE>` | Exported profile file |
| `--list <FILE>` | List mode input (text or CSV); otherwise Spider mode with `<URL>` |
| `--max-urls <N>` / `--max-depth <N>` | limits |
| `--include <REGEX>` / `--exclude <REGEX>` | repeatable |
| `--all-subdomains` / `--no-external-check` / `--follow-nofollow` | scope |
| `--user-agent <PRESET>` / `--user-agent-string <S> --robots-token <T>` | identity |
| `--header 'Name: value'` / `--secret-header 'Name: value'` / `--header-all-hosts` | headers |
| `--speed polite\|fast` / `--concurrency <N>` / `--per-host <N>` / `--rate <N>` / `--no-adaptive` | speed |
| `--robots respect\|ignore-report\|ignore` / `--ignore-crawl-delay` | robots |
| `--connect-timeout <S>` / `--read-timeout <S>` / `--max-redirects <N>` / `--max-size <MB>` | network |
| `--attempts <N>` / `--max-wait <S>` / `--no-retry-5xx` / `--no-retry-429` | retry |

`spiderlens check` accepts the identity, robots-token and network options.

## Commands

| Command | Output | Exit |
| --- | --- | --- |
| `spiderlens config show [URL] [options] [--json]` | resolved configuration and issues | 0, or 2 if any error issue |
| `spiderlens config validate [URL] [options]` | issues only | 0 valid, 2 invalid |
| `spiderlens config defaults` | built-in defaults as a profile file | 0 |
| `spiderlens config test-patterns --include … --exclude … <URL>...` | per URL: crawled or excluded, deciding pattern | 0 |

Warnings print to stderr as `warning: <path>: <message>` and do not change
the exit code.
