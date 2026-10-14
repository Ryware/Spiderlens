# Quickstart: Site Login, Cookies and Proxy

Manual checks with the local fixtures (all on 127.0.0.1, nothing on the internet).

1. `cargo run -p spiderlens-test-fixtures --bin auth-site` serves `basic.test`, `digest.test` and `ntlm.test` on port 8720 (user `ann`, password `pw`).
   `SPIDERLENS_PASSWORD_BASIC_TEST_8720=pw spiderlens crawl http://basic.test:8720/ --resolve basic.test=127.0.0.1 --auth http://basic.test:8720=ann --allow-insecure-auth -o /tmp/a.spiderlens` ends with every page 200. Without `--auth`: one "basic.test asks for a sign-in (Basic)" line and 401 rows.
2. `cargo run -p spiderlens-test-fixtures --bin login-site` serves a login form on port 8721. In the desktop app: Settings › Sign-in and network › Login page, URL `http://login.test:8721/login`, Open login window, sign in as `ann`/`pw`, Done: "2 cookies for login.test". Crawl: members pages 200, `/logout` listed as "not crawled: could end the session".
3. Same site with `?expire_after=40`: with a signed-out check (`/account`, contains "Sign in") the crawl pauses with "Your session on login.test ended"; Sign in again, Resume, the crawl finishes.
4. `cargo run -p spiderlens-test-fixtures --bin proxy` starts an HTTP proxy on 8730 and SOCKS5 on 8731 that print each request. `--proxy http://127.0.0.1:8730` sends everything through it; stop it and `--proxy-test` prints "proxy unreachable".
5. Cookie audit: crawl `cookie.test` from the cookie fixture, then `spiderlens cookies /tmp/c.spiderlens --filter missing-httponly`. `grep -c fixture-secret /tmp/c.spiderlens` prints 0.
