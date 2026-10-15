// In-memory site login and proxy commands for UI tests (spec 025). The
// login window, cookie capture and proxy checks are tested in Rust.

import type { AuthMethod } from "../generated/AuthMethod";
import type { ProxySettings } from "../generated/ProxySettings";
import type { LoginCaptured, ProxyTestResult, Transport } from "../transport";

export interface FakeLogin {
  /** Whether the login window is open. */
  open: boolean;
  opened: { loginUrl: string; internalHosts: string[]; userAgent: string | null }[];
  /** What Done captures; tests replace it. */
  capture: LoginCaptured;
  session: LoginCaptured | null;
  /** The file the cookies.txt picker returns. */
  cookiesFile: string | null;
  signIns: { id: number; origin: string; user: string; password: string; method: AuthMethod | null }[];
  /** How many URLs a sign-in re-queues. */
  requeued: number;
  proxyTests: { proxy: ProxySettings; probeUrl: string }[];
  /** What the proxy Test button answers; tests replace it. */
  proxyResult: ProxyTestResult;
  navigate(url: string): void;
  close(): void;
}

type LoginApi = Pick<
  Transport,
  | "loginOpen"
  | "loginDone"
  | "loginCancel"
  | "loginClear"
  | "loginStatus"
  | "onLoginNavigated"
  | "onLoginClosed"
  | "pickCookiesFile"
  | "cookiesImport"
  | "crawlAddSignIn"
  | "proxyTest"
>;

export function fakeLoginTransport(): { login: FakeLogin; api: LoginApi } {
  const navigated = new Set<(url: string) => void>();
  const closed = new Set<() => void>();
  const login: FakeLogin = {
    open: false,
    opened: [],
    capture: { sites: [{ domain: "example.com", names: ["sid", "csrf"] }], captured_at: 1_760_000_000_000, login_url: "https://example.com/login" },
    session: null,
    cookiesFile: null,
    signIns: [],
    requeued: 3,
    proxyTests: [],
    proxyResult: {
      ok: true,
      in_effect: { mode: "manual", kind: "http", host: "proxy.local", port: 8080, user: null, source: "manual", bypass: [] },
      problem: null,
      message: null,
    },
    navigate: (url) => navigated.forEach((cb) => cb(url)),
    close: () => {
      login.open = false;
      closed.forEach((cb) => cb());
    },
  };
  const api: LoginApi = {
    loginOpen: async (loginUrl, internalHosts, userAgent) => {
      if (!/^https?:\/\//i.test(loginUrl.trim())) throw new Error("The login page must start with http:// or https://.");
      login.open = true;
      login.opened.push({ loginUrl, internalHosts, userAgent });
    },
    loginDone: async () => {
      if (!login.open) throw new Error("The login window is not open.");
      login.open = false;
      login.session = structuredClone(login.capture);
      return structuredClone(login.capture);
    },
    loginCancel: async () => {
      login.open = false;
    },
    loginClear: async () => {
      login.session = null;
    },
    loginStatus: async () => (login.session ? structuredClone(login.session) : null),
    onLoginNavigated: async (cb) => {
      navigated.add(cb);
      return () => void navigated.delete(cb);
    },
    onLoginClosed: async (cb) => {
      closed.add(cb);
      return () => void closed.delete(cb);
    },
    pickCookiesFile: async () => login.cookiesFile,
    cookiesImport: async (path) => {
      const captured: LoginCaptured = { sites: [{ domain: "example.com", names: ["sid"] }], captured_at: 1_760_000_000_000, login_url: path.split(/[\\/]/).pop()! };
      login.session = captured;
      return { imported: 1, skipped_expired: 2, skipped_external: 1, skipped_malformed: 0, captured };
    },
    crawlAddSignIn: async (id, origin, user, password, method) => {
      login.signIns.push({ id, origin, user, password, method });
      return login.requeued;
    },
    proxyTest: async (proxy, probeUrl) => {
      login.proxyTests.push({ proxy: structuredClone(proxy), probeUrl });
      return structuredClone(login.proxyResult);
    },
  };
  return { login, api };
}
