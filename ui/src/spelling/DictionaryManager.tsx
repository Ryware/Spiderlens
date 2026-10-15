// Installed dictionaries and those that can be downloaded, with the size and
// licence shown before anything is downloaded (spec 027 FR-D03, FR-D04).

import { useCallback, useEffect, useState } from "react";
import type { Dictionaries } from "../generated/Dictionaries";
import type { Downloadable } from "../generated/Downloadable";
import type { Transport } from "../transport";
import { Badge, Button, Input, Notice, Select } from "../components/ui";

const mb = (bytes: number) => `${(bytes / 1_048_576).toFixed(1)} MB`;

export function DictionaryManager({ transport }: { transport: Transport }) {
  const [dicts, setDicts] = useState<Dictionaries | null>(null);
  const [pick, setPick] = useState("");
  const [confirm, setConfirm] = useState<Downloadable | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [lang, setLang] = useState("");
  const [message, setMessage] = useState<{ kind: "info" | "bad"; text: string } | null>(null);

  const load = useCallback(() => transport.spellDictionaries().then(setDicts, (e) => setMessage({ kind: "bad", text: String(e) })), [transport]);
  useEffect(() => void load(), [load]);

  const run = async (what: string, job: () => Promise<unknown>, done: string) => {
    setBusy(what);
    setMessage(null);
    try {
      await job();
      setMessage({ kind: "info", text: done });
      await load();
    } catch (e) {
      setMessage({ kind: "bad", text: String(e) });
    } finally {
      setBusy(null);
    }
  };

  if (!dicts) return null;
  const chosen = dicts.available.find((d) => d.lang === pick) ?? null;
  return (
    <div className="flex w-full flex-col gap-2">
      <ul aria-label="Installed dictionaries" className="m-0 flex list-none flex-col divide-y divide-line rounded-md border border-line p-0">
        {dicts.installed.map((d) => (
          <li key={d.lang} className="flex items-center gap-2 px-2.5 py-1.5 text-sm">
            <span className="w-12 font-mono text-fg-2">{d.lang}</span>
            <span className="min-w-0 flex-1 truncate">{d.name}</span>
            <span className="text-fg-3 tabular">{mb(d.size)}</span>
            <span className="w-28 truncate text-fg-3">{d.licence}</span>
            {d.origin === "bundled" ? (
              <Badge tone="neutral">Built in</Badge>
            ) : (
              <Button size="sm" variant="quiet" aria-label={`Remove ${d.name}`} disabled={busy !== null} onClick={() => void run(d.lang, () => transport.spellRemove(d.lang), `Removed ${d.name}.`)}>
                Remove
              </Button>
            )}
          </li>
        ))}
      </ul>

      {dicts.available.length > 0 && (
        <div className="flex items-center gap-2">
          <Select
            aria-label="Dictionary to download"
            value={pick}
            onChange={(e) => {
              setPick(e.target.value);
              setConfirm(null);
            }}
            className="flex-1"
          >
            <option value="">Choose a language…</option>
            {dicts.available.map((d) => (
              <option key={d.lang} value={d.lang}>
                {d.name} ({d.lang})
              </option>
            ))}
          </Select>
          <Button size="sm" disabled={!chosen || busy !== null} onClick={() => setConfirm(chosen)}>
            Download…
          </Button>
        </div>
      )}
      {confirm && (
        <Notice kind="info">
          <span>
            {confirm.name} is {mb(confirm.size)} and licensed under {confirm.licence}. It is downloaded as a separate data file and checked against its published checksum.
          </span>
          <span className="ml-2 inline-flex gap-1">
            <Button
              size="sm"
              variant="primary"
              disabled={busy !== null}
              onClick={() => {
                const d = confirm;
                setConfirm(null);
                setPick("");
                void run(d.lang, () => transport.spellDownload(d.lang), `Installed ${d.name}.`);
              }}
            >
              Download
            </Button>
            <Button size="sm" variant="quiet" onClick={() => setConfirm(null)}>
              Cancel
            </Button>
          </span>
        </Notice>
      )}

      <div className="flex items-center gap-2">
        <Input aria-label="Language code for your dictionary" placeholder="Language code, e.g. de-CH" value={lang} onChange={(e) => setLang(e.target.value)} className="w-48" />
        <Button
          size="sm"
          variant="quiet"
          disabled={!lang.trim() || busy !== null}
          onClick={async () => {
            const files = await transport.pickDictionaryFiles();
            if (!files) return;
            const code = lang.trim();
            setLang("");
            await run(code, () => transport.spellAdd(code, files.aff, files.dic), `Installed ${code}.`);
          }}
        >
          Add from .aff and .dic files…
        </Button>
      </div>
      {busy && <span className="text-sm text-fg-3" role="status">Working on {busy}…</span>}
      {message && !busy && (
        <Notice kind={message.kind} role={message.kind === "bad" ? "alert" : "status"}>
          {message.text}
        </Notice>
      )}
    </div>
  );
}
