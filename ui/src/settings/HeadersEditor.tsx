import { Plus, X } from "lucide-react";
import type { CustomHeader } from "../generated/CustomHeader";
import { Button, Input, Switch } from "../components/ui";

export function HeadersEditor({ headers, onChange }: { headers: CustomHeader[]; onChange: (h: CustomHeader[]) => void }) {
  const update = (i: number, h: CustomHeader) => onChange(headers.map((x, j) => (j === i ? h : x)));
  return (
    <div className="flex w-full flex-col gap-2">
      {headers.map((h, i) => {
        const secret = "secret" in h.value;
        const value = "plain" in h.value ? h.value.plain : (h.value.value ?? "");
        return (
          <div className="flex flex-wrap items-center gap-2" key={i}>
            <Input aria-label={`Header ${i + 1} name`} placeholder="Name" mono className="w-40" value={h.name} onChange={(e) => update(i, { ...h, name: e.target.value })} />
            <Input
              aria-label={`Header ${i + 1} value`}
              placeholder={secret && !value ? "Re-enter secret value" : "Value"}
              type={secret ? "password" : "text"}
              mono
              className="min-w-0 flex-1"
              value={value}
              onChange={(e) => update(i, { ...h, value: secret ? { secret: true, value: e.target.value } : { plain: e.target.value } })}
            />
            <span className="flex items-center gap-1.5 text-sm text-fg-2">
              <Switch label={`Header ${i + 1} secret`} checked={secret} onChange={(s) => update(i, { ...h, value: s ? { secret: true, value } : { plain: value } })} />
              Secret
            </span>
            <span className="flex items-center gap-1.5 text-sm text-fg-2">
              <Switch label={`Header ${i + 1} all hosts`} checked={h.send_to === "all"} onChange={(all) => update(i, { ...h, send_to: all ? "all" : "internal" })} />
              All hosts
            </span>
            <Button variant="quiet" icon size="sm" aria-label={`Remove header ${i + 1}`} onClick={() => onChange(headers.filter((_, j) => j !== i))}>
              <X className="size-3.5" />
            </Button>
          </div>
        );
      })}
      <div>
        <Button size="sm" onClick={() => onChange([...headers, { name: "", value: { plain: "" }, send_to: "internal" }])}>
          <Plus className="size-3.5" /> Add header
        </Button>
      </div>
    </div>
  );
}
