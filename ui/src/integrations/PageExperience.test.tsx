// Page experience (spec 024 US3): saving one Google API key, turning on field
// data and lab tests, the estimate shown before a run, running, pause at the
// daily allowance, resume, and removal. The provider calls are tested in Rust.
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { App } from "../App";
import { fakeReport, fakeTransport } from "../testing/fakeTransport";
import { formatDuration } from "./PageExperiencePanel";

async function openIntegrations() {
  const t = fakeTransport();
  render(<App transport={t} />);
  fireEvent.change(await screen.findByLabelText("Start URL"), { target: { value: "example.com" } });
  await waitFor(() => expect((screen.getByRole("button", { name: /Start$/ }) as HTMLButtonElement).disabled).toBe(false));
  fireEvent.click(screen.getByRole("button", { name: /Start$/ }));
  await screen.findByRole("button", { name: /Pause/ });
  act(() => t.crawls.finish({ report: fakeReport("completed", 30) }));
  fireEvent.click(await screen.findByRole("button", { name: "Integrations" }));
  await screen.findByRole("heading", { name: "Integrations" });
  return t;
}

const source = (id: string) => screen.getByRole("list", { name: "Data sources" }).querySelector(`[data-provider="${id}"]`) as HTMLElement;

async function ready() {
  const t = await openIntegrations();
  const form = await screen.findByRole("form", { name: "Save a Google API key" });
  fireEvent.change(within(form).getByLabelText("API key"), { target: { value: "AIza-test" } });
  fireEvent.click(within(form).getByRole("button", { name: "Save key to keychain" }));
  await screen.findByRole("list", { name: "Saved API keys" });
  fireEvent.click(within(source("field_data")).getByRole("switch"));
  fireEvent.click(within(source("lab_tests")).getByRole("switch"));
  return t;
}

test("formatDuration_reads_naturally", () => {
  expect(formatDuration(45)).toBe("45 s");
  expect(formatDuration(600)).toBe("10 min");
  expect(formatDuration(8340)).toBe("2 h 19 min");
});

test("us3_sources_need_turning_on_and_a_key_before_a_run", async () => {
  await openIntegrations();
  expect(screen.getByText(/Turn on Chrome UX Report/)).toBeTruthy();
  expect(screen.getByText(/Turn on PageSpeed Insights/)).toBeTruthy();
  expect(screen.queryByRole("button", { name: "Fetch field data" })).toBeNull();
});

test("us3_one_key_serves_both_and_the_estimate_shows_before_a_run", async () => {
  const t = await ready();
  expect(t.integrations.calls).toContain("save google key Google API key");
  const fetch = await screen.findByRole("button", { name: "Fetch field data" });
  const estimate = await screen.findByRole("status", { name: "Field data estimate" });
  // 300 pages on two devices.
  await waitFor(() => expect(estimate.textContent).toContain("600 requests for 300 pages"));
  expect(estimate.textContent).toContain("140 a minute");
  expect((await screen.findByRole("status", { name: "Lab test estimate" })).textContent).toContain("50 requests for 50 pages");

  fireEvent.click(screen.getByLabelText("Desktop"));
  await waitFor(() => expect(screen.getByRole("status", { name: "Field data estimate" }).textContent).toContain("300 requests"));
  fireEvent.click(fetch);
  await screen.findByRole("button", { name: /Pause/ });
  expect(t.integrations.fieldRequests[0].devices).toEqual(["phone"]);
  act(() => t.integrations.finishRun());
  const runs = await screen.findByRole("list", { name: "Page experience runs" });
  await waitFor(() => expect(runs.textContent).toContain("Finished"));
  expect(runs.textContent).toContain("300 of 300 pages");
});

test("us3_lab_tests_run_for_the_chosen_device_and_a_quota_stop_can_be_resumed", async () => {
  const t = await ready();
  await screen.findByRole("button", { name: "Run lab tests" });
  fireEvent.change(screen.getByLabelText("Device"), { target: { value: "both" } });
  await waitFor(() => expect(screen.getByRole("status", { name: "Lab test estimate" }).textContent).toContain("100 requests"));
  t.integrations.quotaNext = true;
  fireEvent.click(screen.getByRole("button", { name: "Run lab tests" }));
  await screen.findByRole("button", { name: /Pause/ });
  expect(t.integrations.labRequests[0].strategy).toBe("both");
  act(() => t.integrations.finishRun());
  const resume = await screen.findByRole("button", { name: /Resume lab test run/ });
  expect(screen.getByRole("list", { name: "Page experience runs" }).textContent).toContain("Paused: daily allowance used");
  fireEvent.click(resume);
  await screen.findByRole("button", { name: /Pause/ });
  act(() => t.integrations.finishRun());
  await waitFor(() => expect(screen.getByRole("list", { name: "Page experience runs" }).textContent).toContain("Finished"));
});

test("fr_011_page_experience_data_can_be_removed_from_the_crawl", async () => {
  const t = await ready();
  fireEvent.click(await screen.findByRole("button", { name: "Fetch field data" }));
  await screen.findByRole("button", { name: /Pause/ });
  act(() => t.integrations.finishRun());
  fireEvent.click(await screen.findByRole("button", { name: "Remove field data from this crawl" }));
  await waitFor(() => expect(t.integrations.calls.some((c) => c.startsWith("remove field_data"))).toBe(true));
  await waitFor(() => expect(screen.queryByRole("button", { name: "Remove field data from this crawl" })).toBeNull());
});
