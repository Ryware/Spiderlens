import { Plus, X } from "lucide-react";
import type { AuthMethod } from "../generated/AuthMethod";
import type { SignIn } from "../generated/SignIn";
import { Button, Input, Select } from "../components/ui";

export const authMethods: { value: AuthMethod; label: string }[] = [
  { value: "any", label: "Any" },
  { value: "basic", label: "Basic" },
  { value: "digest", label: "Digest" },
  { value: "ntlm", label: "NTLM" },
];

/** The origin of the start URL (scheme, host and port), as the default for a new sign-in. */
export function originOf(url: string): string {
  try {
    return new URL(url.includes("://") ? url : `https://${url}`).origin;
  } catch {
    return "";
  }
}

/**
 * HTTP sign-ins, one per origin (spec 025 FR-001). Passwords are secret:
 * they are kept for this session only and never saved in a profile or crawl
 * file, so a reopened profile asks for them again.
 */
export function SignInsEditor({ signIns, defaultOrigin, onChange }: { signIns: SignIn[]; defaultOrigin: string; onChange: (s: SignIn[]) => void }) {
  const update = (i: number, s: SignIn) => onChange(signIns.map((x, j) => (j === i ? s : x)));
  return (
    <div className="flex w-full flex-col gap-2">
      {signIns.map((s, i) => {
        const n = i + 1;
        return (
          <div className="flex flex-wrap items-center gap-2" key={i} data-sign-in={s.origin}>
            <Input
              aria-label={`Sign-in ${n} site`}
              placeholder="https://staging.example.com"
              mono
              className="min-w-0 flex-1"
              value={s.origin}
              onChange={(e) => update(i, { ...s, origin: e.target.value })}
            />
            <Input aria-label={`Sign-in ${n} user`} placeholder="User" className="w-36" value={s.user} onChange={(e) => update(i, { ...s, user: e.target.value })} />
            <Input
              aria-label={`Sign-in ${n} password`}
              placeholder={s.password.value ? "Password" : "Enter password"}
              type="password"
              className="w-36"
              value={s.password.value ?? ""}
              onChange={(e) => update(i, { ...s, password: { secret: true, value: e.target.value } })}
            />
            <Select aria-label={`Sign-in ${n} method`} className="w-28" value={s.method} onChange={(e) => update(i, { ...s, method: e.target.value as AuthMethod })}>
              {authMethods.map((m) => (
                <option key={m.value} value={m.value}>
                  {m.label}
                </option>
              ))}
            </Select>
            <Button variant="quiet" icon size="sm" aria-label={`Remove sign-in ${n}`} onClick={() => onChange(signIns.filter((_, j) => j !== i))}>
              <X className="size-3.5" />
            </Button>
          </div>
        );
      })}
      <div>
        <Button
          size="sm"
          onClick={() => onChange([...signIns, { origin: signIns.some((s) => s.origin === defaultOrigin) ? "" : defaultOrigin, user: "", password: { secret: true, value: "" }, method: "any" }])}
        >
          <Plus className="size-3.5" /> Add sign-in
        </Button>
      </div>
    </div>
  );
}
