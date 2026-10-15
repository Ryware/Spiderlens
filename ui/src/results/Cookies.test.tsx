// The cookie audit (spec 025 US5): the Cookies view with filters, sort and
// the URLs that set a cookie, and the cookies section of the detail pane.
// The queries themselves are tested in Rust (crates/results/tests/cookies.rs).
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { App } from "../App";
import { fakeReport, fakeTransport } from "../testing/fakeTransport";
import { fakeCookie, fakeUrls } from "../testing/fakeResults";
import { Inspector } from "./Inspector";

const table = () => screen.getByRole("table", { name: /^Cookies/ });
const names = () => within(table()).getAllByRole("row").slice(1).map((r) => within(r).getAllByRole("cell")[0].textContent);

function withCookies() {
  const t = fakeTransport(fakeUrls(10));
  t.results.features = ["base", "extraction", "cookies"];
  t.results.cookies = [
    fakeCookie("sid", [1, 2, 3, 4]),
    fakeCookie("_ga", [1, 2], { domain: "example.com", session: false, expires: Date.UTC(2027, 0, 31), http_only: false, secure: false, same_site: null }),
    fakeCookie("ad_id", [3], { domain: "ads.example.net", third_party: true, same_site: "none", session: false, expires: Date.UTC(2026, 11, 1) }),
  ];
  return t;
}

async function openCookies(t = withCookies()) {
  render(<App transport={t} />);
  fireEvent.change(await screen.findByLabelText("Start URL"), { target: { value: "example.com" } });
  await waitFor(() => expect((screen.getByRole("button", { name: /Start$/ }) as HTMLButtonElement).disabled).toBe(false));
  fireEvent.click(screen.getByRole("button", { name: /Start$/ }));
  await screen.findByRole("button", { name: /Pause/ });
  act(() => t.crawls.finish({ report: fakeReport("completed", 10) }));
  fireEvent.click(await screen.findByRole("button", { name: "Cookies" }));
  return t;
}

test("cookies_view_lists_cookies_by_urls_with_flags_and_counts", async () => {
  await openCookies();
  await waitFor(() => expect(names()).toEqual(["sid", "_ga", "ad_id"]));
  const ga = within(table()).getAllByRole("row")[2];
  expect(within(ga).getByText("Not Secure")).toBeTruthy();
  expect(within(ga).getByText("Script readable")).toBeTruthy();
  expect(within(ga).getByText("2027-01-31")).toBeTruthy();
  const strip = screen.getByRole("toolbar", { name: "Cookie filters" });
  await waitFor(() => expect(within(strip).getByRole("button", { name: /^Third party/ }).textContent).toBe("Third party1"));
  expect(within(strip).getByRole("button", { name: /^Session/ }).textContent).toBe("Session1");
  // No value is ever shown: the audit has none to show.
  expect(screen.queryByText(/value/i)).toBeNull();
});

test("filter_and_sort_change_the_rows", async () => {
  await openCookies();
  await waitFor(() => expect(names()).toHaveLength(3));
  fireEvent.click(screen.getByRole("button", { name: /^Missing HttpOnly/ }));
  await waitFor(() => expect(names()).toEqual(["_ga"]));
  fireEvent.click(screen.getByRole("button", { name: /^All/ }));
  await waitFor(() => expect(names()).toHaveLength(3));
  const header = within(table()).getByRole("columnheader", { name: /Name/ });
  fireEvent.click(within(header).getByRole("button"));
  await waitFor(() => expect(names()).toEqual(["_ga", "ad_id", "sid"]));
  expect(header.getAttribute("aria-sort")).toBe("ascending");
  fireEvent.click(within(header).getByRole("button"));
  await waitFor(() => expect(names()).toEqual(["sid", "ad_id", "_ga"]));
});

test("empty_filter_says_so", async () => {
  const t = withCookies();
  t.results.cookies = t.results.cookies.filter((c) => c.name === "sid");
  await openCookies(t);
  await waitFor(() => expect(names()).toEqual(["sid"]));
  fireEvent.click(screen.getByRole("button", { name: /^SameSite=None/ }));
  expect(await screen.findByText("No SameSite=None cookies")).toBeTruthy();
});

test("selecting_a_cookie_lists_the_urls_that_set_it_and_opens_one_in_details", async () => {
  await openCookies();
  await waitFor(() => expect(names()).toHaveLength(3));
  fireEvent.click(within(table()).getAllByRole("row")[1]);
  const urls = await screen.findByRole("region", { name: "URLs that set sid" });
  await waitFor(() => expect(within(urls).getAllByRole("button")).toHaveLength(4));
  expect(within(urls).getByText("(4)", { exact: false })).toBeTruthy();
  fireEvent.click(within(urls).getByText("https://example.com/page-2"));
  const details = screen.getByRole("complementary", { name: "Details" });
  await waitFor(() => expect(within(details).getByRole("list", { name: "Cookies set" })).toBeTruthy());
  const set = within(details).getByRole("list", { name: "Cookies set" });
  expect(within(set).getAllByRole("listitem").map((li) => li.querySelector(".font-mono")!.textContent)).toEqual(["sid", "ad_id"]);
});

test("file_without_cookie_data_explains_why", async () => {
  const t = withCookies();
  t.results.features = ["base"];
  await openCookies(t);
  expect(await screen.findByText("No cookie data")).toBeTruthy();
  expect(screen.getByText(/made before SpiderLens recorded the cookies/)).toBeTruthy();
});

test("inspector_lists_not_collected_cookies_for_old_files", async () => {
  const t = fakeTransport(fakeUrls(3));
  render(<Inspector transport={t} path="/crawls/old.spiderlens" id={1} />);
  const details = await screen.findByRole("complementary", { name: "Details" });
  await waitFor(() => expect(within(details).getByText(/Cookies\./)).toBeTruthy());
  expect(within(details).queryByRole("list", { name: "Cookies set" })).toBeNull();
});
