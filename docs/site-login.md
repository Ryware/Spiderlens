# Signing in, cookies and proxy

SpiderLens can crawl pages that sit behind a password, and it can reach the web through a proxy. This page covers each option in the desktop app and on the command line.

## What it is for

Use these options to crawl:

- a staging site behind a browser password prompt,
- a members area behind a login form,
- a site that you can only reach through a company proxy.

By default a crawl is anonymous and connects directly. You switch these options on in Settings, under "Sign-in and network", or with command line flags.

## Warning: use a test account

A crawl requests every link a signed-in user can see. If the site changes data through plain links (delete, approve, send), the crawl will follow them. Sign in with a test account that has no rights to real data. SpiderLens shows a one-time warning the first time you start a signed-in crawl.

## HTTP sign-ins

An HTTP sign-in is the password prompt your browser shows before a page loads. SpiderLens supports three methods:

- Basic
- Digest
- NTLM. Enter the user as `DOMAIN\user`.

In Settings, add a row with the site, user, password and method. Leave the method on its default to let SpiderLens use whatever the site asks for.

The password is sent to the site you list, and to no other site.

### http:// sites

On an `http://` site the password travels without encryption. The command line refuses this unless you add `--allow-insecure-auth`. Use it only on a network you trust.

### A 401 during a crawl

If a page answers 401 and you have no sign-in for that site, the crawl log shows one line, for example "basic.test asks for a sign-in (Basic)". The pages show as 401 in the Response codes tab. In the desktop app a notice offers "Sign in and retry". Enter the user and password, and the crawl signs in and fetches those pages again. The password is kept for this crawl only.

### Command line

```sh
export SPIDERLENS_PASSWORD_STAGING_EXAMPLE_COM=secret
spiderlens crawl https://staging.example.com/ \
  --auth https://staging.example.com=ann

# restrict the method
spiderlens crawl https://staging.example.com/ \
  --auth https://staging.example.com=ann --auth-method https://staging.example.com=digest

# an http:// site
spiderlens crawl http://staging.example.com/ \
  --auth http://staging.example.com=ann --allow-insecure-auth
```

`--auth` can be repeated, one per site. If the environment variable is not set and a terminal is available, SpiderLens asks for the password.

## The login window

Use the login window for sites with a login form. It is a desktop feature.

1. Open Settings, "Sign-in and network", "Login page".
2. Enter the address of the login page and choose "Open login window".
3. Sign in in the window that opens, as you would in a browser.
4. Choose "Signed in".

SpiderLens then shows the names of the cookies it captured, for example "2 cookies for login.test". It shows names only, never values. The cookies are kept in memory for this session and are never saved. Choose "Sign in again" to replace them.

### Sign-out links

A crawl that is signed in must not click "Log out". SpiderLens does not request links that contain these words:

`logout`, `log-out`, `signout`, `sign-out`, `logoff`, `log-off`

The check ignores case and looks at the path and the query. These links appear in the results as not crawled, with the reason "could end the session". You can add your own patterns under "Never request links containing", one per line, or remove the defaults.

### Signed-out check

Sessions can expire during a long crawl. Under "Notice when signed out", enter a page that only a signed-in user can open, and say what means signed out:

- Redirects to a given address,
- Answers a given status,
- Page contains a given text (for example "Sign in").

SpiderLens fetches that page every so often. When it says you are signed out, the crawl pauses and tells you. Sign in again in the login window, then choose Resume. The crawl fetches again the pages it fetched since the last successful check, so no page is kept from the signed-out period.

## Cookies from your browser (cookies.txt)

You can also sign in with a browser and hand the session to SpiderLens.

1. Sign in to the site in your browser.
2. Export the site's cookies in the Netscape `cookies.txt` format. Browser extensions that export `cookies.txt` do this.
3. In the desktop app, choose "Import cookies.txt" under "Login page". On the command line, use `--cookies-file`.

SpiderLens keeps only cookies for the crawl's own site and its subdomains. It skips expired cookies and cookies for other sites, and the desktop app shows how many it imported and skipped. The file is read for this run only. Its contents are not stored in the crawl file.

Treat a `cookies.txt` file like a password. Delete it when you are done.

```sh
spiderlens crawl https://members.example.com/ --cookies-file ./cookies.txt

# with a sign-out pattern and a signed-out check
spiderlens crawl https://members.example.com/ --cookies-file ./cookies.txt \
  --sign-out-pattern "/leave" \
  --signed-out-check https://members.example.com/account --when contains:"Sign in"
```

`--when` takes `redirects-to:TEXT`, `status:N` or `contains:TEXT`. Use `--no-default-sign-out-patterns` to drop the built-in patterns.

You can also set single cookies, for example a consent cookie or a test variant, in Settings, or with `--cookie NAME@DOMAIN` (the value comes from `SPIDERLENS_COOKIE_<NAME>`) and `--cookie-plain NAME=VALUE@DOMAIN` for values that are not secret.

### Cookie policy

The policy controls what happens to cookies the site sets during the crawl.

- Ignore (default): SpiderLens drops them, as a search engine does.
- Keep: SpiderLens keeps them in memory and sends them back where the cookie rules allow.

Choose Keep if a page redirects in a loop until the visitor returns its cookie.

```sh
spiderlens crawl https://example.com/ --cookie-policy keep
```

## Proxy

Under "Sign-in and network", "Proxy", choose a mode:

- None: connect directly. This is the default.
- System: use your computer's proxy settings, read when the crawl starts.
- Manual: enter the type, host, port, and an optional user and password.

Manual proxies can be HTTP, HTTPS or SOCKS5. Every request goes through the proxy, including robots.txt and sitemaps.

### System mode

SpiderLens reads, in this order of source:

1. the proxy environment variables (for example `HTTPS_PROXY`, `NO_PROXY`),
2. the operating system settings on macOS, Windows and GNOME, including an automatic configuration script (PAC) address.

Automatic discovery without an address (WPAD) is not supported. If your network needs it, enter the PAC address in the system settings, or use Manual mode.

### Bypass list

"Reach directly" lists hosts that skip the proxy, one per line. You can use a name, a wildcard or a range: `localhost`, `*.example.com`, `10.0.0.0/8`.

### Test button

"Test" requests the start URL's robots.txt through the proxy and shows the result or the problem.

### Command line

```sh
# manual proxy, user in the address, password from the environment
export SPIDERLENS_PROXY_PASSWORD=secret
spiderlens crawl https://example.com/ --proxy http://ann@proxy.example.com:8080 \
  --no-proxy-for "localhost,*.internal.example.com"

# SOCKS5
spiderlens crawl https://example.com/ --proxy socks5://127.0.0.1:1080

# system settings
spiderlens crawl https://example.com/ --proxy-system

# check the proxy and exit
spiderlens crawl https://example.com/ --proxy http://proxy.example.com:8080 --proxy-test
```

`--proxy` and `--proxy-system` cannot be used together. Do not put the password in the address; SpiderLens refuses it.

`--proxy-test` exits with 0 when the proxy works and prints "proxy ok". It exits with 2 and prints the problem when it does not, for example "proxy unreachable".

## Passwords and secrets

SpiderLens never stores:

- site passwords,
- proxy passwords,
- login-window cookies, imported cookies, or cookie values marked secret.

Crawl files and profiles keep only the sites, user names, cookie policy and proxy address. When you resume a crawl or open a profile, SpiderLens asks again for what is missing, before it sends any request.

On the command line, secrets are never arguments. SpiderLens reads them from the environment, or asks on a terminal:

| Secret | Environment variable |
|---|---|
| Site password | `SPIDERLENS_PASSWORD_<HOST>` |
| Proxy password | `SPIDERLENS_PROXY_PASSWORD` |
| Secret cookie | `SPIDERLENS_COOKIE_<NAME>` |

In the names, letters are upper case and every other character becomes `_`. Include the port when the site has one. For `https://staging.example.com` the variable is `SPIDERLENS_PASSWORD_STAGING_EXAMPLE_COM`. For `http://basic.test:8720` it is `SPIDERLENS_PASSWORD_BASIC_TEST_8720`.

Use the same variables when you resume a crawl:

```sh
export SPIDERLENS_PASSWORD_STAGING_EXAMPLE_COM=secret
spiderlens crawl --resume site.spiderlens --cookies-file ./cookies.txt
```

Without a terminal (a scheduled run), a missing secret ends the command with an error that lists each missing variable by name. To resume without the missing secrets, add `--resume-without-secrets`; the crawl history records that.
