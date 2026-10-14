# Contract: CLI

All new flags belong to `spiderlens crawl` (and `spiderlens page` where they make sense for one URL). Defaults equal the desktop defaults.

| Flag | Meaning |
|---|---|
| `--auth ORIGIN=USER` | adds a sign-in; password from `SPIDERLENS_PASSWORD_<HOST>` or a terminal prompt. Repeatable |
| `--auth-method ORIGIN=basic\|digest\|ntlm` | restricts a sign-in's method |
| `--allow-insecure-auth` | needed when a sign-in uses `http://` |
| `--cookie NAME@DOMAIN[/PATH]` | adds a cookie; value from `SPIDERLENS_COOKIE_<NAME>` or prompt |
| `--cookie-plain NAME=VALUE@DOMAIN[/PATH]` | adds a non-secret cookie (consent flags, A/B variants) |
| `--cookies-file PATH` | imports a Netscape `cookies.txt`; treated as a login session (memory only, never stored) |
| `--cookie-policy ignore\|keep` | default `ignore` |
| `--sign-out-pattern TEXT` | adds a pattern; `--no-default-sign-out-patterns` clears the defaults |
| `--signed-out-check URL --when redirects-to:TEXT\|status:N\|contains:TEXT` | session expiry check |
| `--proxy URL` | `http://`, `https://` or `socks5://host:port`, user in the URL (`http://ann@proxy:8080`); password from `SPIDERLENS_PROXY_PASSWORD` or prompt |
| `--proxy-system` | use the system proxy settings |
| `--no-proxy-for HOSTS` | comma-separated bypass list |
| `--proxy-test` | check the proxy and exit (0 ok, 2 with the problem printed) |
| `--resume-without-secrets` | resume and drop missing secrets, recorded in the crawl's history |

Secrets are never accepted as plain arguments. Without a terminal (scheduled runs) a missing secret ends the command with the `SECRETS_MISSING` exit code and one line per key naming its environment variable. `--header-secret NAME=VALUE` keeps working; `SPIDERLENS_HEADER_<NAME>` is the new environment alternative.

New command:

```text
spiderlens cookies FILE [--filter session|persistent|third-party|missing-secure|missing-httponly|samesite-none] [--url URL] [--format table|csv|json]
```

Prints the distinct cookies (or the cookies one URL set) without values. `spiderlens info FILE` gains lines for sign-ins (origin and user), login used, cookie policy and proxy in effect, and `spiderlens export --tab cookies` writes the CSV.
