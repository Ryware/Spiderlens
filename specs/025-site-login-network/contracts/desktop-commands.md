# Contract: Desktop commands and events (Tauri)

| Command | Input | Output |
|---|---|---|
| `login_open` | `{ login_url, internal_hosts: string[], user_agent }` | `()`; opens the login window |
| `login_done` | `{}` | `{ sites: { domain, names: string[] }[], captured_at }`; values stay in Rust |
| `login_cancel` | `{}` | `()` |
| `login_clear` | `{}` | `()`; drops the captured session |
| `cookies_import` | `{ path }` or `{ cookie_line, domain }` | `{ imported, skipped_expired, skipped_external, skipped_malformed, cookies: SuppliedCookie[] }` (file imports become a login session: values stay in Rust, only names return) |
| `proxy_test` | `{ proxy: ProxySettings, probe_url }` | `{ ok: true, in_effect }` or `{ ok: false, problem: "unreachable" \| "rejected" \| "pac_unreadable" \| "pac_unsupported", detail }` |
| `crawl_add_sign_in` | `{ origin, user, password, method }` | `{ requeued: number }` |
| `crawl_resume` | existing, plus `secrets` keys from data-model.md and `skip_secrets: bool` | existing |
| `cookies_list` | `{ filter, sort, offset, limit }` | `{ rows: CookieRow[], total }` |
| `cookie_urls` | `{ name, domain, path, offset, limit }` | `{ rows: { url, status }[], total }` |

| Event | Payload |
|---|---|
| `crawl://auth-required` | `{ origin, schemes, supported }` |
| `crawl://auth-rejected` | `{ origin, method }` |
| `crawl://session-ended` | `{ since }` (the crawl is paused) |
| `login://navigated` | `{ url }` for the address strip |

The login webview has no capability entry, so it can call none of these.
