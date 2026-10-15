// Crawl Analysis after a crawl (spec 023 FR-012, FR-013): it starts on its
// own, shows progress in the status bar, and can be cancelled or run again.
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { App } from "../App";
import { fakeReport, fakeTransport } from "../testing/fakeTransport";

const PATH = "/crawls/example.com.spiderlens";

async function crawl(t = fakeTransport()) {
  render(<App transport={t} />);
  fireEvent.change(await screen.findByLabelText("Start URL"), { target: { value: "example.com" } });
  const start = () => screen.getByRole("button", { name: /Start$/ }) as HTMLButtonElement;
  await waitFor(() => expect(start().disabled).toBe(false));
  fireEvent.click(start());
  await screen.findByRole("button", { name: /Pause/ });
  return t;
}

const bar = () => within(screen.getByRole("contentinfo", { name: "Crawl status" }));

test("analysis_runs_when_a_crawl_completes_and_shows_progress", async () => {
  const t = await crawl();
  expect(t.results.analyses).toEqual([]);
  act(() => t.crawls.finish({ report: fakeReport("completed", 7) }));
  await waitFor(() => expect(t.results.analyses).toEqual([PATH]));
  expect(await bar().findByText("Analysing: Link positions")).toBeTruthy();
  act(() => t.results.analysisProgress({ phase: "positions", done: 40, total: 100 }));
  expect(await bar().findByText("Analysing: Link positions 40%")).toBeTruthy();
  act(() => t.results.analysisProgress({ phase: "link_score", done: 1, total: 4 }));
  expect(await bar().findByText("Analysing: Link Score 25%")).toBeTruthy();
  act(() => t.results.analysisDone());
  expect(await bar().findByText("Link Score ready")).toBeTruthy();
  expect(bar().queryByRole("button", { name: "Cancel analysis" })).toBeNull();
});

test("cancel_stops_the_analysis_and_it_can_run_again", async () => {
  const t = await crawl();
  act(() => t.crawls.finish({ report: fakeReport("completed", 7) }));
  fireEvent.click(await bar().findByRole("button", { name: "Cancel analysis" }));
  await waitFor(() => expect(t.results.analysisCancels).toBe(1));
  act(() => t.results.analysisDone({ cancelled: true }));
  expect(await bar().findByText("Analysis cancelled")).toBeTruthy();
  fireEvent.click(bar().getByRole("button", { name: "Run analysis again" }));
  await waitFor(() => expect(t.results.analyses).toEqual([PATH, PATH]));
  expect(await bar().findByText("Analysing: Link positions")).toBeTruthy();
});

test("a_failed_analysis_says_so_with_the_reason", async () => {
  const t = await crawl();
  act(() => t.crawls.finish({ report: fakeReport("completed", 7) }));
  await waitFor(() => expect(t.results.analyses).toHaveLength(1));
  act(() => t.results.analysisDone({ error: "disk full" }));
  const failed = await bar().findByText("Analysis failed");
  expect(failed.closest("[data-analysis]")?.getAttribute("title")).toBe("disk full");
});

test("no_analysis_after_a_stopped_crawl", async () => {
  const t = await crawl();
  act(() => t.crawls.finish({ report: fakeReport("stopped", 3) }));
  await screen.findByRole("button", { name: /Start$/ });
  expect(t.results.analyses).toEqual([]);
});

test("auto_analyse_off_leaves_it_to_the_palette", async () => {
  const t = fakeTransport();
  const defaults = t.defaults.bind(t);
  t.defaults = async (builtin) => {
    const c = await defaults(builtin);
    return { ...c, links: { ...c.links, auto_analyse: false } };
  };
  await crawl(t);
  act(() => t.crawls.finish({ report: fakeReport("completed", 7) }));
  await screen.findByRole("button", { name: /Start$/ });
  expect(t.results.analyses).toEqual([]);
  fireEvent.keyDown(window, { key: "k", code: "KeyK", ctrlKey: true });
  const input = await screen.findByPlaceholderText("Type a command or a tab name");
  fireEvent.change(input, { target: { value: "Work out Link Score" } });
  fireEvent.keyDown(input, { key: "Enter" });
  await waitFor(() => expect(t.results.analyses).toEqual([PATH]));
});
