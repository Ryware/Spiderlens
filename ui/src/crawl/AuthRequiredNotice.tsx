import { useState } from "react";
import type { AuthMethod } from "../generated/AuthMethod";
import { Button, Input, Notice, Select } from "../components/ui";
import { authMethods } from "../settings/SignInsEditor";

/**
 * A site answered 401 while the crawl runs (spec 025 FR-004): sign in here
 * and its pages are fetched again, without restarting the crawl.
 */
export function AuthRequiredNotice({
  origin,
  message,
  onSignIn,
  onDismiss,
}: {
  origin: string;
  message: string;
  /** Resolves to how many URLs were queued again. */
  onSignIn: (user: string, password: string, method: AuthMethod | null) => Promise<number | null>;
  onDismiss: () => void;
}) {
  const [user, setUser] = useState("");
  const [password, setPassword] = useState("");
  const [method, setMethod] = useState<AuthMethod>("any");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const submit = async () => {
    setBusy(true);
    setError(null);
    try {
      await onSignIn(user.trim(), password, method === "any" ? null : method);
    } catch (e) {
      setError(String(e));
      setBusy(false);
    }
  };
  return (
    <form
      aria-label={`Sign in to ${origin}`}
      className="flex flex-col gap-2 rounded-lg border border-line bg-surface px-3 py-2.5"
      onSubmit={(e) => {
        e.preventDefault();
        if (user.trim() && password) submit();
      }}
    >
      <Notice kind="warn">{message}</Notice>
      <span className="flex flex-wrap items-center gap-2">
        <Input aria-label="User" placeholder="User" className="w-40" autoComplete="off" value={user} onChange={(e) => setUser(e.target.value)} />
        <Input aria-label="Password" placeholder="Password" type="password" className="w-40" autoComplete="off" value={password} onChange={(e) => setPassword(e.target.value)} />
        <Select aria-label="Method" className="w-28" value={method} onChange={(e) => setMethod(e.target.value as AuthMethod)}>
          {authMethods.map((m) => (
            <option key={m.value} value={m.value}>
              {m.label}
            </option>
          ))}
        </Select>
        <Button type="submit" size="sm" variant="primary" disabled={busy || !user.trim() || !password}>
          Sign in and retry
        </Button>
        <Button type="button" size="sm" variant="quiet" onClick={onDismiss}>
          Not now
        </Button>
      </span>
      <p className="m-0 text-sm text-fg-3">The password is kept for this crawl only, never saved.</p>
      {error && (
        <Notice kind="bad" role="alert">
          {error}
        </Notice>
      )}
    </form>
  );
}
