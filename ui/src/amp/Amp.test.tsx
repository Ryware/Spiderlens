// Spec 027 part E in the window: the Check AMP setting, the AMP tab's
// core-subset note, and AMP checks in the details pane. The checks
// themselves are tested in Rust (crates/engine, crates/results/tests/amp.rs).
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { App } from "../App";
import type { AmpDetail } from "../generated/AmpDetail";
import type { UrlDetail } from "../generated/UrlDetail";
import { Inspector } from "../results/Inspector";
import { fakeReport, fakeTransport } from "../testing/fakeTransport";

const start = () => screen.getByRole("button", { name: /Start$/ }) as HTMLButtonElement;
const PATH = "/crawls/example.com.spiderlens";

test("check_amp_setting_reaches_the_crawl", async () => {
  const t = fakeTransport();
  render(<App transport={t} />);
  await screen.findByLabelText("Start URL");
  fireEvent.click(screen.getByRole("button", { name: "Configure" }));
  const nav = await screen.findByRole("navigation", { name: "Settings categories" });
  fireEvent.click(within(nav).getByRole("tab", { name: /^Page data/ }));
  const toggle = screen.getByRole("switch", { name: "Check AMP" });
  expect(toggle.getAttribute("aria-checked")).toBe("false");
  fireEvent.click(toggle);
  fireEvent.click(screen.getByRole("button", { name: /Done/ }));
  fireEvent.change(screen.getByLabelText("Start URL"), { target: { value: "example.com" } });
  await waitFor(() => expect(start().disabled).toBe(false));
  fireEvent.click(start());
  await waitFor(() => expect(t.crawls.started).toHaveLength(1));
  expect(t.crawls.started[0].config.extraction.check_amp).toBe(true);
});

test("amp_tab_says_it_is_a_core_subset_and_lists_failing_pages", async () => {
  const t = fakeTransport();
  t.results.features.push("amp");
  t.results.urls[2].ampFailures = 0;
  t.results.urls[3].ampFailures = 2;
  render(<App transport={t} />);
  fireEvent.change(await screen.findByLabelText("Start URL"), { target: { value: "example.com" } });
  await waitFor(() => expect(start().disabled).toBe(false));
  fireEvent.click(start());
  await screen.findByRole("button", { name: /Pause/ });
  act(() => t.crawls.finish({ report: fakeReport("completed", 30) }));
  fireEvent.click(await screen.findByRole("button", { name: "AMP" }));
  const tab = await screen.findByRole("region", { name: "AMP" });
  expect(within(tab).getByText(/core subset of AMP's rules, not the official AMP validator/)).toBeTruthy();
  await waitFor(() => expect(within(tab).getAllByRole("row")).toHaveLength(3));
  fireEvent.click(within(tab).getByRole("button", { name: /^Failing a check/ }));
  await waitFor(() => expect(within(tab).getAllByRole("row")).toHaveLength(2));
});

function withAmp(amp: AmpDetail | null) {
  const t = fakeTransport();
  const base = t.urlDetail;
  t.urlDetail = async (p, id): Promise<UrlDetail> => ({ ...(await base(p, id)), amp });
  return t;
}

const section = async (name: RegExp) => {
  const details = screen.getByRole("complementary", { name: "Details" });
  return (await within(details).findByRole("heading", { name })).closest("section") as HTMLElement;
};

test("details_list_failed_amp_checks_with_their_element", async () => {
  const t = withAmp({
    format: "website",
    paired: false,
    failures: [
      { check: "author_script", detail: "An inline script is not an AMP component.", pos: 14 },
      { check: "runtime", detail: "The AMP runtime script (cdn.ampproject.org/v0.js) is missing.", pos: null },
    ],
  });
  render(<Inspector transport={t} path={PATH} id={2} />);
  const s = within(await section(/^AMP \(2 failed\)/));
  expect(s.getByText("Author script")).toBeTruthy();
  expect(s.getByText("· element 14", { exact: false })).toBeTruthy();
  expect(s.getByText("Runtime script")).toBeTruthy();
  expect(s.getByText("The canonical page has no amphtml link back to this page.")).toBeTruthy();
  expect(s.getByText(/not the official AMP validator/)).toBeTruthy();
});

test("a_passing_amp_email_page_says_so", async () => {
  const t = withAmp({ format: "email", paired: null, failures: [] });
  render(<Inspector transport={t} path={PATH} id={2} />);
  const s = within(await section(/^AMP for email \(passes\)/));
  expect(s.getByText("Passes every check SpiderLens runs.")).toBeTruthy();
});

test("pages_that_are_not_amp_have_no_amp_section", async () => {
  render(<Inspector transport={withAmp(null)} path={PATH} id={2} />);
  await screen.findByRole("heading", { level: 2 });
  expect(screen.queryByRole("heading", { name: /^AMP/ })).toBeNull();
});
