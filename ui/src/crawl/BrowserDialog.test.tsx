// Spec 014 FR-045 in the app: consent before the download, the raw fallback,
// errors, and the offline, restored and lost-mid-download states (guide
// section 7). The download itself is tested in Rust.

import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { App } from "../App";
import { BrowserDialog } from "./BrowserDialog";
import { fakeTransport } from "../testing/fakeTransport";

function setup() {
  const t = fakeTransport();
  const calls = { ready: 0, raw: 0, closed: 0 };
  render(
    <BrowserDialog
      transport={t}
      message="JavaScript rendering needs a Chromium-based browser and none was found."
      onReady={() => void calls.ready++}
      onRaw={() => void calls.raw++}
      onClose={() => void calls.closed++}
    />,
  );
  return { t, calls };
}

const download = () => screen.getByRole("button", { name: "Download and crawl" });

describe("BrowserDialog", () => {
  it("shows the size and waits for the click before downloading", async () => {
    const { t } = setup();
    await screen.findByText(/about 124 MB/);
    expect(t.render.installs).toEqual([]);
    await waitFor(() => expect(download()).toBeEnabled());
    fireEvent.click(download());
    await waitFor(() => expect(t.render.installs).toEqual([1]));
  });

  it("shows progress, then starts the crawl when the browser is installed", async () => {
    const { t, calls } = setup();
    await waitFor(() => expect(download()).toBeEnabled());
    fireEvent.click(download());
    await waitFor(() => expect(t.render.installs).toHaveLength(1));
    act(() => t.render.progress({ bytes_done: 58_944_974 }));
    expect(await screen.findByText("Downloading the browser: 59 of 124 MB")).toBeInTheDocument();
    act(() => t.render.finish());
    await waitFor(() => expect(calls.ready).toBe(1));
  });

  it("crawls raw HTML instead without downloading", async () => {
    const { t, calls } = setup();
    fireEvent.click(await screen.findByRole("button", { name: "Crawl raw HTML instead" }));
    expect(calls.raw).toBe(1);
    expect(t.render.installs).toEqual([]);
  });

  it("shows a failed download and lets the person try again", async () => {
    const { t, calls } = setup();
    await waitFor(() => expect(download()).toBeEnabled());
    fireEvent.click(download());
    await waitFor(() => expect(t.render.installs).toHaveLength(1));
    act(() => t.render.finish("The downloaded browser does not match its expected checksum and was removed."));
    expect(await screen.findByRole("alert")).toHaveTextContent("does not match its expected checksum");
    expect(calls.ready).toBe(0);
    expect(download()).toBeEnabled();
  });

  it("offline: says so, disables the download and keeps the raw fallback", async () => {
    const { t, calls } = setup();
    await waitFor(() => expect(t.render.watching).toBe(true));
    act(() => t.render.network(false));
    expect(await screen.findByText("No internet connection: the browser can't be downloaded now")).toBeInTheDocument();
    expect(download()).toBeDisabled();
    fireEvent.click(screen.getByRole("button", { name: "Crawl raw HTML instead" }));
    expect(calls.raw).toBe(1);
  });

  it("restored: says so and enables the download without starting it", async () => {
    const { t } = setup();
    await waitFor(() => expect(t.render.watching).toBe(true));
    act(() => t.render.network(false));
    await screen.findByText(/No internet connection/);
    act(() => t.render.network(true));
    expect(await screen.findByText("Connection restored")).toBeInTheDocument();
    expect(download()).toBeEnabled();
    expect(t.render.installs).toEqual([]);
  });

  it("lost mid-download: waits to resume, keeps the progress and does not fail", async () => {
    const { t, calls } = setup();
    await waitFor(() => expect(download()).toBeEnabled());
    fireEvent.click(download());
    await waitFor(() => expect(t.render.installs).toHaveLength(1));
    act(() => t.render.progress({ bytes_done: 30_000_000 }));
    act(() => t.render.progress({ bytes_done: 30_000_000, waiting: true }));
    expect(await screen.findByText("Connection lost, waiting to resume")).toBeInTheDocument();
    expect(screen.queryByRole("alert")).toBeNull();
    act(() => t.render.progress({ bytes_done: 80_000_000 }));
    expect(await screen.findByText("Downloading the browser: 80 of 124 MB")).toBeInTheDocument();
    act(() => t.render.finish());
    await waitFor(() => expect(calls.ready).toBe(1));
  });

  it("cancels a running download", async () => {
    const { t } = setup();
    await waitFor(() => expect(download()).toBeEnabled());
    fireEvent.click(download());
    await waitFor(() => expect(t.render.installs).toHaveLength(1));
    fireEvent.click(await screen.findByRole("button", { name: "Cancel download" }));
    await waitFor(() => expect(t.render.cancelled).toEqual([1]));
    expect(download()).toBeEnabled();
  });

  it("uses a browser the person chose, and explains one that cannot be used", async () => {
    const { t, calls } = setup();
    fireEvent.click(await screen.findByRole("button", { name: "Change browser" }));
    const input = screen.getByLabelText("Browser path");
    fireEvent.change(input, { target: { value: "/opt/nope/chrome" } });
    fireEvent.click(screen.getByRole("button", { name: "Use this browser" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("No browser at this path.");
    expect(calls.ready).toBe(0);
    fireEvent.change(input, { target: { value: "/usr/bin/google-chrome" } });
    fireEvent.click(screen.getByRole("button", { name: "Use this browser" }));
    await waitFor(() => expect(calls.ready).toBe(1));
    expect(t.render.paths).toEqual(["/opt/nope/chrome", "/usr/bin/google-chrome"]);
  });
});

describe("starting a rendering crawl without a browser", () => {
  async function startRendering() {
    const t = fakeTransport();
    t.render.requireBrowser = true;
    render(<App transport={t} />);
    await screen.findByLabelText("Start URL");
    fireEvent.click(screen.getByRole("button", { name: "Configure" }));
    const nav = await screen.findByRole("navigation", { name: "Settings categories" });
    fireEvent.click(screen.getAllByRole("tab").find((x) => /^JavaScript rendering/.test(x.textContent ?? ""))!);
    expect(nav).toBeTruthy();
    fireEvent.click(await screen.findByRole("radio", { name: "JavaScript rendering" }));
    fireEvent.click(screen.getByRole("button", { name: /Done/ }));
    fireEvent.change(screen.getByLabelText("Start URL"), { target: { value: "example.com" } });
    const start = () => screen.getByRole("button", { name: /Start$/ }) as HTMLButtonElement;
    await waitFor(() => expect(start().disabled).toBe(false));
    fireEvent.click(start());
    await screen.findByText("JavaScript rendering needs a browser");
    return t;
  }

  it("opens the dialog, and raw HTML starts the crawl with rendering off", async () => {
    const t = await startRendering();
    expect(t.crawls.started).toHaveLength(0);
    fireEvent.click(screen.getByRole("button", { name: "Crawl raw HTML instead" }));
    await waitFor(() => expect(t.crawls.started).toHaveLength(1));
    expect(t.crawls.started[0].config.rendering.mode).toBe("raw");
    expect(t.render.installs).toEqual([]);
  });

  it("starts the rendering crawl once the browser is installed", async () => {
    const t = await startRendering();
    await waitFor(() => expect(screen.getByRole("button", { name: "Download and crawl" })).toBeEnabled());
    fireEvent.click(screen.getByRole("button", { name: "Download and crawl" }));
    await waitFor(() => expect(t.render.installs).toHaveLength(1));
    act(() => t.render.finish());
    await waitFor(() => expect(t.crawls.started).toHaveLength(1));
    expect(t.crawls.started[0].config.rendering.mode).toBe("javascript");
  });
});
