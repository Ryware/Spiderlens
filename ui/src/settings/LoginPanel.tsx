import { useEffect, useState } from "react";
import { FileUp, LogIn, X } from "lucide-react";
import type { CheckCondition } from "../generated/CheckCondition";
import type { Issue } from "../generated/Issue";
import type { LoginSettings } from "../generated/LoginSettings";
import type { LoginCaptured, Transport } from "../transport";
import { Button, Input, Notice, Select, Textarea } from "../components/ui";
import { Row } from "./fields";

/** The sign-out patterns a new login starts with (the engine's DEFAULT_SIGN_OUT_PATTERNS). */
export const defaultSignOutPatterns = ["logout", "log-out", "signout", "sign-out", "logoff", "log-off"];

type ConditionKind = "redirects_to" | "status" | "contains";

const conditionKind = (c: CheckCondition): ConditionKind => ("redirects_to" in c ? "redirects_to" : "status" in c ? "status" : "contains");
const conditionValue = (c: CheckCondition): string => ("redirects_to" in c ? c.redirects_to : "status" in c ? String(c.status) : c.contains);

function condition(kind: ConditionKind, value: string): CheckCondition {
  if (kind === "status") return { status: Number(value) || 0 };
  return kind === "redirects_to" ? { redirects_to: value } : { contains: value };
}

/** "https://example.com/login" for a start URL on example.com; blank without one. */
export function loginGuess(startUrl: string): string {
  try {
    return startUrl.trim() ? new URL("/login", startUrl.includes("://") ? startUrl : `https://${startUrl}`).href : "";
  } catch {
    return "";
  }
}

/**
 * The login page (spec 025 US2): the person signs in to the site in a
 * private window, and the crawl uses the cookies it left. Only the settings
 * are saved; the cookies stay in memory, and only their names are shown.
 */
export function LoginPanel({
  login,
  startUrl,
  internalHosts,
  userAgent,
  transport,
  issues,
  onChange,
}: {
  login: LoginSettings | null;
  startUrl: string;
  /** The crawl's own hosts: only their cookies are kept. */
  internalHosts: string[];
  userAgent: string | null;
  transport: Transport;
  issues: (prefix: string) => Issue[];
  onChange: (l: LoginSettings | null) => void;
}) {
  const [captured, setCaptured] = useState<LoginCaptured | null>(null);
  const [windowOpen, setWindowOpen] = useState(false);
  const [address, setAddress] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [imported, setImported] = useState<string | null>(null);

  useEffect(() => {
    let live = true;
    transport.loginStatus().then((c) => live && setCaptured(c));
    const offs = [
      transport.onLoginNavigated((url) => setAddress(url)),
      transport.onLoginClosed(() => {
        setWindowOpen(false);
        setAddress(null);
      }),
    ];
    return () => {
      live = false;
      offs.forEach((o) => o.then((off) => off()));
    };
  }, [transport]);

  const run = async (call: () => Promise<void>) => {
    setError(null);
    try {
      await call();
    } catch (e) {
      setError(String(e));
    }
  };
  const openWindow = (url: string) =>
    run(async () => {
      await transport.loginOpen(url, internalHosts, userAgent);
      setWindowOpen(true);
      setImported(null);
    });
  const done = () =>
    run(async () => {
      setCaptured(await transport.loginDone());
      setWindowOpen(false);
      setAddress(null);
    });
  const cancel = () =>
    run(async () => {
      await transport.loginCancel();
      setWindowOpen(false);
      setAddress(null);
    });
  const clear = () =>
    run(async () => {
      await transport.loginClear();
      setCaptured(null);
      setImported(null);
    });
  const importFile = () =>
    run(async () => {
      const path = await transport.pickCookiesFile();
      if (!path) return;
      const r = await transport.cookiesImport(path, internalHosts);
      const skipped = r.skipped_expired + r.skipped_external + r.skipped_malformed;
      setImported(
        r.imported === 0
          ? "No cookie in this file is for this site."
          : `Imported ${r.imported} cookie${r.imported === 1 ? "" : "s"}${skipped ? `; skipped ${r.skipped_expired} expired, ${r.skipped_external} for other sites, ${r.skipped_malformed} unreadable` : ""}.`,
      );
      if (r.captured) {
        setCaptured(r.captured);
        if (!login) onChange({ login_url: r.captured.login_url, sign_out_patterns: defaultSignOutPatterns, check: null });
      }
    });

  if (!login) {
    return (
      <Row label="Login page" help="Sign in to the site in a private window; the crawl then sees the pages a signed-in visitor sees.">
        {() => (
          <span className="flex gap-2">
            <Button size="sm" onClick={() => onChange({ login_url: loginGuess(startUrl), sign_out_patterns: defaultSignOutPatterns, check: null })}>
              <LogIn className="size-3.5" /> Use a login page
            </Button>
            <Button size="sm" onClick={importFile}>
              <FileUp className="size-3.5" /> Import cookies.txt
            </Button>
          </span>
        )}
      </Row>
    );
  }

  const check = login.check;
  return (
    <>
      <Row label="Login page" issues={issues("access.login.login_url")}>
        {(id) => (
          <span className="flex items-center gap-2">
            <Input id={id} mono className="w-64" placeholder="https://example.com/login" value={login.login_url} onChange={(e) => onChange({ ...login, login_url: e.target.value })} />
            <Button variant="quiet" icon size="sm" aria-label="Remove login page" onClick={() => (clear(), onChange(null))}>
              <X className="size-3.5" />
            </Button>
          </span>
        )}
      </Row>
      <Row
        label="Session"
        stacked
        help={windowOpen ? "Sign in in the window that opened, then choose Signed in. Only the names of the cookies are shown here." : "Kept in memory for this session only, never saved."}
      >
        {() => (
          <div className="flex flex-col gap-2" aria-label="Login session">
            {windowOpen ? (
              <>
                {address && (
                  <p className="m-0 truncate font-code text-sm text-fg-2" aria-label="Login window address" data-selectable>
                    {address}
                  </p>
                )}
                <span className="flex gap-2">
                  <Button size="sm" variant="primary" onClick={done}>
                    Signed in
                  </Button>
                  <Button size="sm" variant="quiet" onClick={cancel}>
                    Cancel
                  </Button>
                </span>
              </>
            ) : (
              <>
                {captured ? (
                  <ul aria-label="Captured cookies" className="m-0 flex list-none flex-col gap-0.5 p-0 text-sm">
                    {captured.sites.map((s) => (
                      <li key={s.domain}>
                        <span className="font-code">{s.domain}</span>: {s.names.join(", ")}
                      </li>
                    ))}
                  </ul>
                ) : (
                  <p className="m-0 text-sm text-fg-3">Not signed in yet.</p>
                )}
                <span className="flex gap-2">
                  <Button size="sm" disabled={!login.login_url.trim()} onClick={() => openWindow(login.login_url)}>
                    <LogIn className="size-3.5" /> {captured ? "Sign in again" : "Open login window"}
                  </Button>
                  <Button size="sm" onClick={importFile}>
                    <FileUp className="size-3.5" /> Import cookies.txt
                  </Button>
                  {captured && (
                    <Button size="sm" variant="quiet" onClick={clear}>
                      Forget session
                    </Button>
                  )}
                </span>
              </>
            )}
            {imported && <Notice kind="info">{imported}</Notice>}
            {error && (
              <Notice kind="bad" role="alert">
                {error}
              </Notice>
            )}
          </div>
        )}
      </Row>
      <Row
        label="Never request links containing"
        stacked
        help="One per line, compared with the path and query ignoring case. These links would end the session; they are listed as not crawled."
        issues={issues("access.login.sign_out_patterns")}
      >
        {(id) => (
          <Textarea
            id={id}
            rows={3}
            value={login.sign_out_patterns.join("\n")}
            onChange={(e) => onChange({ ...login, sign_out_patterns: e.target.value.split("\n").map((l) => l.trim()).filter(Boolean) })}
          />
        )}
      </Row>
      <Row
        label="Notice when signed out"
        stacked
        help="Checked every so often while the crawl runs; when it says signed out, the crawl pauses so you can sign in again."
        issues={issues("access.login.check")}
      >
        {(id) =>
          check ? (
            <span className="flex flex-wrap items-center gap-2">
              <Input id={id} aria-label="Check URL" mono className="w-56" placeholder="https://example.com/account" value={check.url} onChange={(e) => onChange({ ...login, check: { ...check, url: e.target.value } })} />
              <Select
                aria-label="Signed out when"
                className="w-40"
                value={conditionKind(check.when)}
                onChange={(e) => onChange({ ...login, check: { ...check, when: condition(e.target.value as ConditionKind, conditionValue(check.when)) } })}
              >
                <option value="redirects_to">Redirects to</option>
                <option value="status">Answers status</option>
                <option value="contains">Page contains</option>
              </Select>
              <Input
                aria-label="Signed out value"
                className="w-36"
                value={conditionValue(check.when)}
                onChange={(e) => onChange({ ...login, check: { ...check, when: condition(conditionKind(check.when), e.target.value) } })}
              />
              <Button variant="quiet" icon size="sm" aria-label="Remove check" onClick={() => onChange({ ...login, check: null })}>
                <X className="size-3.5" />
              </Button>
            </span>
          ) : (
            // The row label would rename the button; it keeps its own name.
            <span id={id}>
              <Button size="sm" onClick={() => onChange({ ...login, check: { url: "", when: { redirects_to: "/login" } } })}>
                Add a check
              </Button>
            </span>
          )
        }
      </Row>
    </>
  );
}
