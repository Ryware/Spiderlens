import { useState } from "react";
import { ClipboardPaste, Plus, X } from "lucide-react";
import type { SuppliedCookie } from "../generated/SuppliedCookie";
import { Button, Input, Switch, Textarea } from "../components/ui";

/** The host of the start URL, as the default cookie domain. */
export function hostOf(url: string): string {
  try {
    return new URL(url.includes("://") ? url : `https://${url}`).hostname;
  } catch {
    return "";
  }
}

/**
 * Splits a pasted `Cookie:` header ("a=1; b=2", with or without the
 * "Cookie:" prefix) into cookies for `domain`. Values are secret: they came
 * from a signed-in browser (spec 025 FR-015).
 */
export function parseCookieLine(line: string, domain: string): SuppliedCookie[] {
  const body = line.trim().replace(/^cookie\s*:/i, "");
  return body
    .split(";")
    .map((part) => part.trim())
    .filter((part) => part.includes("="))
    .map((part) => {
      const at = part.indexOf("=");
      return { name: part.slice(0, at).trim(), value: { secret: true as const, value: part.slice(at + 1).trim() }, domain, path: "/", secure_only: false };
    })
    .filter((c) => c.name !== "");
}

export function CookiesEditor({ cookies, defaultDomain, onChange }: { cookies: SuppliedCookie[]; defaultDomain: string; onChange: (c: SuppliedCookie[]) => void }) {
  const [pasting, setPasting] = useState(false);
  const [line, setLine] = useState("");
  const update = (i: number, c: SuppliedCookie) => onChange(cookies.map((x, j) => (j === i ? c : x)));
  const addLine = () => {
    const parsed = parseCookieLine(line, defaultDomain);
    // A pasted cookie replaces one with the same name, domain and path.
    const same = (a: SuppliedCookie, b: SuppliedCookie) => a.name === b.name && a.domain === b.domain && a.path === b.path;
    onChange([...cookies.filter((c) => !parsed.some((p) => same(p, c))), ...parsed]);
    setLine("");
    setPasting(false);
  };

  return (
    <div className="flex w-full flex-col gap-2">
      {cookies.map((c, i) => {
        const secret = "secret" in c.value;
        const value = "plain" in c.value ? c.value.plain : (c.value.value ?? "");
        const n = i + 1;
        return (
          <div className="flex flex-wrap items-center gap-2" key={i} data-cookie={c.name}>
            <Input aria-label={`Cookie ${n} name`} placeholder="Name" mono className="w-32" value={c.name} onChange={(e) => update(i, { ...c, name: e.target.value })} />
            <Input
              aria-label={`Cookie ${n} value`}
              placeholder={secret && !value ? "Re-enter secret value" : "Value"}
              type={secret ? "password" : "text"}
              mono
              className="min-w-0 flex-1"
              value={value}
              onChange={(e) => update(i, { ...c, value: secret ? { secret: true, value: e.target.value } : { plain: e.target.value } })}
            />
            <Input aria-label={`Cookie ${n} domain`} placeholder="example.com" mono className="w-36" value={c.domain} onChange={(e) => update(i, { ...c, domain: e.target.value })} />
            <Input aria-label={`Cookie ${n} path`} placeholder="/" mono className="w-20" value={c.path} onChange={(e) => update(i, { ...c, path: e.target.value })} />
            <span className="flex items-center gap-1.5 text-sm text-fg-2">
              <Switch label={`Cookie ${n} secret`} checked={secret} onChange={(s) => update(i, { ...c, value: s ? { secret: true, value } : { plain: value } })} />
              Secret
            </span>
            <span className="flex items-center gap-1.5 text-sm text-fg-2">
              <Switch label={`Cookie ${n} https only`} checked={c.secure_only} onChange={(v) => update(i, { ...c, secure_only: v })} />
              HTTPS only
            </span>
            <Button variant="quiet" icon size="sm" aria-label={`Remove cookie ${n}`} onClick={() => onChange(cookies.filter((_, j) => j !== i))}>
              <X className="size-3.5" />
            </Button>
          </div>
        );
      })}
      {pasting && (
        <div className="flex flex-col gap-2">
          <Textarea
            aria-label="Cookie line"
            rows={2}
            placeholder="session=abc123; theme=dark"
            value={line}
            onChange={(e) => setLine(e.target.value)}
          />
          <div className="flex gap-2">
            <Button size="sm" variant="primary" disabled={parseCookieLine(line, defaultDomain).length === 0} onClick={addLine}>
              Add cookies
            </Button>
            <Button size="sm" variant="quiet" onClick={() => (setLine(""), setPasting(false))}>
              Cancel
            </Button>
          </div>
        </div>
      )}
      <div className="flex gap-2">
        <Button size="sm" onClick={() => onChange([...cookies, { name: "", value: { secret: true, value: "" }, domain: defaultDomain, path: "/", secure_only: false }])}>
          <Plus className="size-3.5" /> Add cookie
        </Button>
        {!pasting && (
          <Button size="sm" onClick={() => setPasting(true)}>
            <ClipboardPaste className="size-3.5" /> Paste a Cookie line
          </Button>
        )}
      </div>
    </div>
  );
}
