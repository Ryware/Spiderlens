# Quickstart: accessibility and mobile checks

How to see each user story working on the local fixture site. Nothing here touches a live site.

1. **Serve the fixture**: `cargo run -p spiderlens-test-fixtures --bin a11y-site` (127.0.0.1:8720). Each page's path names the check it fails (`/static/form-label`, `/full/color-contrast`, `/mobile/content-wider`, `/clean/1` ...).
2. **Static checks, no browser (US2)**: `spiderlens crawl http://127.0.0.1:8720/ -o /tmp/a11y.sqlite` (static is the default), then `spiderlens accessibility /tmp/a11y.sqlite`. Expect one violation group per `/static/*` page, nothing on `/clean/*`, and the line "Full checks need JavaScript rendering".
3. **Full checks (US1)**: add `--render javascript --accessibility full`. Expect the `/full/*` violations with impact and WCAG criteria, "needs review" for `/full/contrast-over-image`, and the engine version in `spiderlens info /tmp/a11y.sqlite`.
4. **Mobile (US3)**: same crawl with `--user-agent googlebot_smartphone` (mobile window). `spiderlens accessibility /tmp/a11y.sqlite --mobile` lists the `/mobile/*` findings with measured widths and shares. With a desktop user agent the layout columns read "Not measured: rendered at desktop size".
5. **One page (US4)**: open the file in the desktop app, Accessibility tab, select `/full/color-contrast`: the inspector shows three elements with ratio 2.85 against 4.5.
6. **Existing crawl (US5)**: crawl with `--accessibility off`, then `spiderlens accessibility run /tmp/a11y.sqlite --static`; the result equals step 2.
7. **CI (US6)**: `spiderlens issues /tmp/a11y.sqlite --fail-on high; echo $?` prints the issues exit code from spec 009.
8. **Speed**: `bench-site --pages 100000` with `--accessibility off` and `static`, compared in `benchmarks.md` (SC-002, SC-003).
