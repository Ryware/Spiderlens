# Quickstart: AEO and GEO Checks (spec 015)

Scenarios to check by hand on the AI fixture site (`cargo run -p spiderlens-test-fixtures --bin fixture-site -- ai`) once the code lands. Automated tests cover the same scenarios (tasks.md).

1. **Access table (US1)**: crawl `http://127.0.0.1:<port>/robots-gptbot/`. Open AI search › Overview. GPTBot is blocked on 100% with `Disallow: /` on line 2; OAI-SearchBot, ChatGPT-User and ClaudeBot are allowed. The OpenAI summary reads "training blocked, search allowed, user fetches allowed".
2. **Google-Extended (US1.4)**: crawl `/robots-google-extended/`. The explanation says Search and AI Overviews are unaffected, with Googlebot's verdict beside it.
3. **Readable without JavaScript (US2)**: crawl `/spa/` without rendering: pages appear under "Possibly built with JavaScript" with evidence. With rendering (after spec 014 code), the same pages appear under "Main content missing without JavaScript" with the percentage.
4. **Answer readiness (US3)**: open `/articles/shipping` in the detail pane: "How long does shipping take?" shows a direct answer; `/articles/no-answer` lists the heading without one.
5. **Trust (US4)**: `/articles/dates-disagree` shows both dates with their sources.
6. **llms.txt (US5)**: host `/llms-broken/` lists a 404 link and a redirected link with line numbers. "Generate llms.txt" produces a draft for review.
7. **Scores (US6)**: the overview shows the site score, five parts and top fixes; every lost point in a page explanation names its check and basis.
8. **CLI (US7)**: `spiderlens aeo run crawl.sqlite --fail-under 90` prints the same score as the app and exits with the quality-gate code.
9. **Speed (SC-001, SC-002)**: `cargo bench`-style run per tasks T070 and T071.
