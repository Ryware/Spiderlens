// Sign-ins, the login window, cookies.txt import and the proxy (spec 025
// US1, US2, US4), checked through to what the crawl receives; and the
// sign-in form a running crawl shows when a site answers 401 (FR-004).
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { App } from "../App";
import { fakeTransport } from "../testing/fakeTransport";
import { describeProxy } from "./ProxyPanel";
import { loginGuess } from "./LoginPanel";
import { originOf } from "./SignInsEditor";

type T = ReturnType<typeof fakeTransport>;

const input = (label: string) => screen.getByLabelText(label) as HTMLInputElement;
const button = (name: RegExp | string) => screen.getByRole("button", { name }) as HTMLButtonElement;
const start = () => screen.getByRole("button", { name: /Start$/ }) as HTMLButtonElement;
const nav = () => screen.queryByRole("navigation", { name: "Settings categories" });

async function openAccess(t: T = fakeTransport()) {
  render(<App transport={t} />);
  fireEvent.change(await screen.findByLabelText("Start URL"), { target: { value: "https://staging.example.com/" } });
  fireEvent.click(screen.getByRole("button", { name: "Configure" }));
  const n = await screen.findByRole("navigation", { name: "Settings categories" });
  fireEvent.click(within(n).getByRole("tab", { name: /^Sign-in and cookies/ }));
  await screen.findByRole("tabpanel", { name: "Sign-in and cookies" });
  return t;
}

async function startCrawl(t: T) {
  fireEvent.click(screen.getByRole("button", { name: /^Done/ }));
  await waitFor(() => expect(nav()).toBeNull());
  await waitFor(() => expect(start().disabled).toBe(false));
  fireEvent.click(start());
  await waitFor(() => expect(t.crawls.started).toHaveLength(1));
  return t.crawls.started[0].config;
}

test("helpers_take_the_origin_and_describe_the_proxy", () => {
  expect(originOf("staging.example.com:8443/a")).toBe("https://staging.example.com:8443");
  expect(originOf("")).toBe("");
  expect(loginGuess("https://example.com/shop")).toBe("https://example.com/login");
  expect(loginGuess("")).toBe("");
  expect(describeProxy({ mode: "manual", kind: "socks5", host: "p.local", port: 1080, user: "ann", source: "manual", bypass: [] })).toBe("socks5 p.local:1080 as ann");
  expect(describeProxy({ mode: "system", kind: null, host: null, port: null, user: null, source: "none", bypass: [] })).toBe("The system has no proxy: direct connections.");
});

test("a_sign_in_for_the_start_site_reaches_the_crawl_with_its_password", async () => {
  const t = await openAccess();
  fireEvent.click(button(/Add sign-in/));
  expect(input("Sign-in 1 site").value).toBe("https://staging.example.com");
  expect(input("Sign-in 1 password").type).toBe("password");
  fireEvent.change(input("Sign-in 1 user"), { target: { value: "CORP\\ann" } });
  fireEvent.change(input("Sign-in 1 password"), { target: { value: "pw1" } });
  fireEvent.change(screen.getByLabelText("Sign-in 1 method"), { target: { value: "ntlm" } });
  const config = await startCrawl(t);
  expect(config.access.sign_ins).toEqual([{ origin: "https://staging.example.com", user: "CORP\\ann", password: { secret: true, value: "pw1" }, method: "ntlm" }]);
});

test("the_login_window_captures_cookie_names_and_the_crawl_keeps_the_login_page", async () => {
  const t = await openAccess();
  fireEvent.click(button(/Use a login page/));
  expect(input("Login page").value).toBe("https://staging.example.com/login");
  fireEvent.click(button(/Open login window/));
  await waitFor(() => expect(t.login.opened).toHaveLength(1));
  expect(t.login.opened[0]).toMatchObject({ loginUrl: "https://staging.example.com/login", internalHosts: ["staging.example.com"] });
  act(() => t.login.navigate("https://staging.example.com/account"));
  expect(screen.getByLabelText("Login window address").textContent).toBe("https://staging.example.com/account");
  fireEvent.click(button("Signed in"));
  const names = await screen.findByRole("list", { name: "Captured cookies" });
  expect(names.textContent).toBe("example.com: sid, csrf");
  // Sign-out links: the defaults, one more.
  const patterns = screen.getByLabelText("Never request links containing") as HTMLTextAreaElement;
  expect(patterns.value.split("\n")).toContain("logout");
  fireEvent.change(patterns, { target: { value: `${patterns.value}\nend-session` } });
  fireEvent.click(button("Add a check"));
  fireEvent.change(input("Check URL"), { target: { value: "https://staging.example.com/account" } });
  fireEvent.change(screen.getByLabelText("Signed out when"), { target: { value: "status" } });
  fireEvent.change(input("Signed out value"), { target: { value: "401" } });
  const config = await startCrawl(t);
  expect(config.access.login).toEqual({
    login_url: "https://staging.example.com/login",
    sign_out_patterns: ["logout", "log-out", "signout", "sign-out", "logoff", "log-off", "end-session"],
    check: { url: "https://staging.example.com/account", when: { status: 401 } },
  });
});

test("closing_the_login_window_or_cancelling_keeps_nothing", async () => {
  const t = await openAccess();
  fireEvent.click(button(/Use a login page/));
  fireEvent.click(button(/Open login window/));
  await screen.findByRole("button", { name: "Signed in" });
  act(() => t.login.close());
  await waitFor(() => expect(screen.queryByRole("button", { name: "Signed in" })).toBeNull());
  expect(screen.getByText("Not signed in yet.")).toBeTruthy();
  expect(t.login.session).toBeNull();
});

test("importing_cookies_txt_reports_what_was_skipped", async () => {
  const t = fakeTransport();
  t.login.cookiesFile = "/Users/ann/Downloads/cookies.txt";
  await openAccess(t);
  fireEvent.click(button(/Import cookies.txt/));
  await screen.findByText("Imported 1 cookie; skipped 2 expired, 1 for other sites, 0 unreadable.");
  expect(screen.getByRole("list", { name: "Captured cookies" }).textContent).toBe("example.com: sid");
  // Forget session clears it here and in the app.
  fireEvent.click(button("Forget session"));
  await screen.findByText("Not signed in yet.");
  expect(t.login.session).toBeNull();
});

test("a_manual_proxy_is_tested_and_reaches_the_crawl", async () => {
  const t = await openAccess();
  const proxy = screen.getByRole("radiogroup", { name: "Proxy" });
  fireEvent.click(within(proxy).getByRole("radio", { name: "Manual" }));
  expect(input("Proxy port").value).toBe("8080");
  fireEvent.change(input("Proxy host"), { target: { value: "proxy.corp.local" } });
  fireEvent.change(input("User"), { target: { value: "ann" } });
  fireEvent.change(input("Password"), { target: { value: "pp" } });
  fireEvent.change(screen.getByLabelText("Reach directly"), { target: { value: "localhost\n*.corp.local" } });
  fireEvent.click(button("Test"));
  await screen.findByText(/Works: http proxy.local:8080/);
  expect(t.login.proxyTests[0].probeUrl).toBe("https://staging.example.com/");

  t.login.proxyResult = { ok: false, in_effect: null, problem: "rejected", message: "The proxy refused the user name or password." };
  fireEvent.click(button("Test"));
  expect((await screen.findByRole("alert")).textContent).toBe("The proxy refused the user name or password.");

  const config = await startCrawl(t);
  expect(config.access.proxy).toEqual({
    mode: "manual",
    kind: "http",
    host: "proxy.corp.local",
    port: 8080,
    user: "ann",
    password: { secret: true, value: "pp" },
    bypass: ["localhost", "*.corp.local"],
  });
});

test("a_401_during_the_crawl_offers_a_sign_in_and_retries_the_site", async () => {
  const t = fakeTransport();
  render(<App transport={t} />);
  fireEvent.change(await screen.findByLabelText("Start URL"), { target: { value: "example.com" } });
  await waitFor(() => expect(start().disabled).toBe(false));
  fireEvent.click(start());
  await screen.findByRole("button", { name: /Pause/ });
  act(() =>
    t.crawls.notice({
      kind: "auth_required",
      host: "https://example.com",
      message: "example.com asks for a sign-in (Basic). Add one for https://example.com and its pages are fetched again.",
    }),
  );
  const form = await screen.findByRole("form", { name: "Sign in to https://example.com" });
  const signIn = within(form).getByRole("button", { name: "Sign in and retry" }) as HTMLButtonElement;
  expect(signIn.disabled).toBe(true);
  fireEvent.change(within(form).getByLabelText("User"), { target: { value: "ann" } });
  fireEvent.change(within(form).getByLabelText("Password"), { target: { value: "pw" } });
  fireEvent.click(signIn);
  await waitFor(() => expect(t.login.signIns).toEqual([{ id: 1, origin: "https://example.com", user: "ann", password: "pw", method: null }]));
  await waitFor(() => expect(screen.queryByRole("form", { name: "Sign in to https://example.com" })).toBeNull());

  // "Not now" leaves the 401s as they are.
  act(() => t.crawls.notice({ kind: "auth_required", host: "https://admin.example.com", message: "admin.example.com asks for a sign-in (Digest)." }));
  const second = await screen.findByRole("form", { name: "Sign in to https://admin.example.com" });
  fireEvent.click(within(second).getByRole("button", { name: "Not now" }));
  await waitFor(() => expect(screen.queryByRole("form", { name: "Sign in to https://admin.example.com" })).toBeNull());
  expect(t.login.signIns).toHaveLength(1);
});
