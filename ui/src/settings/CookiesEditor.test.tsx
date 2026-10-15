// Cookies sent with the crawl and the policy for cookies the site sets
// (spec 025 US3, FR-015..FR-019), checked through to what the crawl receives.
import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { App } from "../App";
import { fakeTransport } from "../testing/fakeTransport";
import { parseCookieLine } from "./CookiesEditor";

type T = ReturnType<typeof fakeTransport>;

const input = (label: string) => screen.getByLabelText(label) as HTMLInputElement;
const sw = (name: string) => screen.getByRole("switch", { name });
const start = () => screen.getByRole("button", { name: /Start$/ }) as HTMLButtonElement;
const nav = () => screen.queryByRole("navigation", { name: "Settings categories" });

async function openCookies() {
  const t = fakeTransport();
  render(<App transport={t} />);
  fireEvent.change(await screen.findByLabelText("Start URL"), { target: { value: "https://shop.example.com/" } });
  fireEvent.click(screen.getByRole("button", { name: "Configure" }));
  const n = await screen.findByRole("navigation", { name: "Settings categories" });
  fireEvent.click(within(n).getByRole("tab", { name: /^Sign-in and cookies/ }));
  await screen.findByRole("tabpanel", { name: "Sign-in and cookies" });
  return t;
}

async function startCrawl(t: T) {
  fireEvent.click(screen.getByRole("button", { name: /Done/ }));
  await waitFor(() => expect(nav()).toBeNull());
  await waitFor(() => expect(start().disabled).toBe(false));
  fireEvent.click(start());
  await waitFor(() => expect(t.crawls.started).toHaveLength(1));
  return t.crawls.started[0].config;
}

test("parse_cookie_line_splits_pairs_and_keeps_values_secret", () => {
  expect(parseCookieLine("Cookie: session=abc=1; theme=dark ;  ; bad", "example.com")).toEqual([
    { name: "session", value: { secret: true, value: "abc=1" }, domain: "example.com", path: "/", secure_only: false },
    { name: "theme", value: { secret: true, value: "dark" }, domain: "example.com", path: "/", secure_only: false },
  ]);
  expect(parseCookieLine("no pairs here", "example.com")).toEqual([]);
});

test("added_cookie_is_secret_for_the_start_host_and_reaches_the_crawl", async () => {
  const t = await openCookies();
  fireEvent.click(screen.getByRole("button", { name: /Add cookie/ }));
  expect(input("Cookie 1 domain").value).toBe("shop.example.com");
  expect(input("Cookie 1 value").type).toBe("password");
  fireEvent.change(input("Cookie 1 name"), { target: { value: "sid" } });
  fireEvent.change(input("Cookie 1 value"), { target: { value: "s3cret" } });
  fireEvent.change(input("Cookie 1 path"), { target: { value: "/account" } });
  fireEvent.click(sw("Cookie 1 https only"));
  await waitFor(() => expect(sw("Cookie 1 https only").getAttribute("aria-checked")).toBe("true"));

  fireEvent.click(screen.getByRole("button", { name: /Add cookie/ }));
  fireEvent.change(input("Cookie 2 name"), { target: { value: "lang" } });
  fireEvent.click(sw("Cookie 2 secret"));
  await waitFor(() => expect(input("Cookie 2 value").type).toBe("text"));
  fireEvent.change(input("Cookie 2 value"), { target: { value: "en" } });

  fireEvent.click(screen.getByRole("button", { name: /Add cookie/ }));
  fireEvent.click(screen.getByRole("button", { name: "Remove cookie 3" }));
  expect(screen.queryByLabelText("Cookie 3 name")).toBeNull();

  const config = await startCrawl(t);
  expect(config.access.cookies).toEqual([
    { name: "sid", value: { secret: true, value: "s3cret" }, domain: "shop.example.com", path: "/account", secure_only: true },
    { name: "lang", value: { plain: "en" }, domain: "shop.example.com", path: "/", secure_only: false },
  ]);
  expect(config.access.cookie_policy).toBe("ignore");
});

test("pasted_cookie_line_adds_rows_and_replaces_same_names", async () => {
  const t = await openCookies();
  fireEvent.click(screen.getByRole("button", { name: /Paste a Cookie line/ }));
  const add = screen.getByRole("button", { name: "Add cookies" }) as HTMLButtonElement;
  expect(add.disabled).toBe(true);
  fireEvent.change(input("Cookie line"), { target: { value: "Cookie: sid=one; cart=7" } });
  fireEvent.click(screen.getByRole("button", { name: "Add cookies" }));
  expect(input("Cookie 1 name").value).toBe("sid");
  expect(input("Cookie 2 name").value).toBe("cart");
  expect(screen.queryByLabelText("Cookie line")).toBeNull();

  fireEvent.click(screen.getByRole("button", { name: /Paste a Cookie line/ }));
  fireEvent.change(input("Cookie line"), { target: { value: "sid=two" } });
  fireEvent.click(screen.getByRole("button", { name: "Add cookies" }));
  expect(screen.queryByLabelText("Cookie 3 name")).toBeNull();

  const config = await startCrawl(t);
  expect(config.access.cookies.map((c) => [c.name, c.value])).toEqual([
    ["cart", { secret: true, value: "7" }],
    ["sid", { secret: true, value: "two" }],
  ]);
});

test("keep_policy_changes_the_help_and_reaches_the_crawl", async () => {
  const t = await openCookies();
  expect(screen.getByText(/not sent back, so every page is seen as a first visit/)).toBeTruthy();
  fireEvent.click(within(screen.getByRole("radiogroup", { name: "Cookies the site sets" })).getByRole("radio", { name: "Keep" }));
  expect(await screen.findByText(/Sent back for the rest of the crawl/)).toBeTruthy();
  const config = await startCrawl(t);
  expect(config.access.cookie_policy).toBe("keep");
});
