// In-memory browser download for UI tests (spec 014 FR-045). The download,
// its resume and the reachability check are tested in Rust.

import type { BrowserFinished, BrowserProgress, BrowserStatus, Transport } from "../transport";

export interface FakeRender {
  status: BrowserStatus;
  /** Whether the download host answers; `network()` changes it and tells the dialog. */
  online: boolean;
  /** Jobs started by `renderBrowserInstall`, and those cancelled. */
  installs: number[];
  cancelled: number[];
  /** Paths given to `renderBrowserSetPath`, in order. */
  paths: (string | null)[];
  /** What the file picker returns. */
  pickedFile: string | null;
  watching: boolean;
  /** When true, starting a rendering crawl without a usable browser fails with `browser_unavailable:` as the app does. */
  requireBrowser: boolean;
  /** Sends a progress event for the running job. */
  progress(p: Partial<BrowserProgress>): void;
  /** Ends the running job; an error leaves nothing installed. */
  finish(error?: string | null): void;
  /** The download host is reachable or not (the 5 s check). */
  network(online: boolean): void;
}

type RenderApi = Pick<
  Transport,
  | "renderBrowserStatus"
  | "renderBrowserInstall"
  | "renderBrowserCancel"
  | "renderBrowserSetPath"
  | "renderBrowserNetworkWatch"
  | "pickBrowserFile"
  | "onRenderBrowserProgress"
  | "onRenderBrowserFinished"
  | "onRenderBrowserNetwork"
>;

const MISSING: BrowserStatus = {
  usable: null,
  problem: "JavaScript rendering needs a Chromium-based browser and none was found.",
  download: { version: "155.0.8059.39", bytes: 124_203_329 },
  user_path: null,
};

export function fakeRenderTransport(): { render: FakeRender; api: RenderApi } {
  const progressCbs = new Set<(p: BrowserProgress) => void>();
  const finishedCbs = new Set<(f: BrowserFinished) => void>();
  const networkCbs = new Set<(online: boolean) => void>();
  let job = 0;
  const render: FakeRender = {
    status: structuredClone(MISSING),
    online: true,
    installs: [],
    cancelled: [],
    paths: [],
    pickedFile: null,
    watching: false,
    requireBrowser: false,
    progress: (p) => {
      const event: BrowserProgress = { job, bytes_done: 0, bytes_total: 124_203_329, stage: "download", waiting: false, ...p };
      progressCbs.forEach((cb) => cb(event));
    },
    finish: (error = null) => {
      if (!error) {
        render.status = {
          ...render.status,
          usable: { name: "Chrome Headless Shell", version: "155.0.8059.39", source: "downloaded", path: "/data/browsers/chrome-headless-shell" },
          problem: null,
        };
      }
      const event: BrowserFinished = { job, status: structuredClone(render.status), error };
      finishedCbs.forEach((cb) => cb(event));
    },
    network: (online) => {
      render.online = online;
      networkCbs.forEach((cb) => cb(online));
    },
  };
  const api: RenderApi = {
    renderBrowserStatus: async () => structuredClone(render.status),
    renderBrowserInstall: async () => {
      job += 1;
      render.installs.push(job);
      return job;
    },
    renderBrowserCancel: async (id) => {
      render.cancelled.push(id);
    },
    renderBrowserSetPath: async (path) => {
      render.paths.push(path);
      if (path === null) return structuredClone(render.status);
      if (path.includes("nope")) return { ...structuredClone(render.status), usable: null, problem: `The browser at ${path} cannot be used: No browser at this path.` };
      render.status = {
        ...render.status,
        usable: { name: "Google Chrome", version: "155.0.8059.39", source: "user_path", path },
        problem: null,
        user_path: path,
      };
      return structuredClone(render.status);
    },
    renderBrowserNetworkWatch: async (on) => {
      render.watching = on;
      // The first answer comes at once, as the app's does.
      if (on) queueMicrotask(() => networkCbs.forEach((cb) => cb(render.online)));
    },
    pickBrowserFile: async () => render.pickedFile,
    onRenderBrowserProgress: async (cb) => {
      progressCbs.add(cb);
      return () => void progressCbs.delete(cb);
    },
    onRenderBrowserFinished: async (cb) => {
      finishedCbs.add(cb);
      return () => void finishedCbs.delete(cb);
    },
    onRenderBrowserNetwork: async (cb) => {
      networkCbs.add(cb);
      return () => void networkCbs.delete(cb);
    },
  };
  return { render, api };
}
