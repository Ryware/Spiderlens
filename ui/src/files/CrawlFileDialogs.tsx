import { useState, type FormEvent } from "react";
import { CircleCheck, LoaderCircle } from "lucide-react";
import { Button, Input, Modal, Notice } from "../components/ui";
import type { CrawlInfo } from "../transport";
import { fileStem } from "./format";
import type { CrawlFile } from "./useCrawlFile";

/** The questions and messages of the crawl file flow, the Save As confirmation and the quit notice. */
export function CrawlFileDialogs({ file }: { file: CrawlFile }) {
  const p = file.prompt;
  const close = (o: boolean) => !o && file.dismiss();
  return (
    <>
      {p?.kind === "pause_first" && (
        <Modal open onOpenChange={close} title={`Pause the running crawl and open ${fileStem(p.path)}?`} description="The running crawl is kept and can be resumed later.">
          <Actions>
            <Button onClick={file.dismiss}>Cancel</Button>
            <Button variant="primary" onClick={file.confirmPause}>
              Pause and open
            </Button>
          </Actions>
        </Modal>
      )}

      {p?.kind === "damaged" && (
        <Modal open onOpenChange={close} title={p.failure.salvageable ? "This crawl file is damaged" : `Can't open ${fileStem(p.path)}`}>
          <Notice kind="bad">
            <span data-selectable>{p.failure.message}</span>
          </Notice>
          <p className="m-0 truncate font-code text-sm text-fg-3" title={p.path} data-selectable>
            {p.path}
          </p>
          {p.failure.salvageable && <p className="m-0 text-sm text-fg-2">Part of it can still be read. Recovering copies that part into a new crawl and leaves this file as it is.</p>}
          <Actions>
            <Button onClick={file.dismiss}>{p.failure.salvageable ? "Cancel" : "Close"}</Button>
            {p.failure.salvageable && (
              <Button variant="primary" onClick={() => void file.recover()}>
                Recover what can be read
              </Button>
            )}
          </Actions>
        </Modal>
      )}

      {p?.kind === "upgrade" && <UpgradePrompt key={p.info.path} info={p.info} onCancel={file.dismiss} onConfirm={(keep) => void file.confirmUpgrade(keep)} />}

      {p?.kind === "read_only" && (
        <Modal open onOpenChange={close} title="This file can't be changed here." description="Save a copy to resume it. The original stays as it is.">
          <p className="m-0 truncate font-code text-sm text-fg-3" title={p.info.path} data-selectable>
            {p.info.path}
          </p>
          <Actions>
            <Button onClick={file.dismiss}>Cancel</Button>
            <Button variant="primary" onClick={() => void file.saveCopyAndResume()}>
              Save As…
            </Button>
          </Actions>
        </Modal>
      )}

      {p?.kind === "secrets" && <SecretsPrompt key={p.info.path} info={p.info} onCancel={file.dismiss} onSubmit={(s) => void file.submitSecrets(s)} />}

      {p?.kind === "message" && (
        <Modal open onOpenChange={close} title={p.title}>
          <p className="m-0 text-sm text-fg-2" data-selectable>
            {p.body}
          </p>
          <Actions>
            <Button variant="primary" onClick={file.dismiss}>
              OK
            </Button>
          </Actions>
        </Modal>
      )}

      {file.flash && (
        <div role="status" className="fixed bottom-10 left-1/2 z-50 flex max-w-xl sheet-max-w -translate-x-1/2 items-center gap-2 rounded-lg bg-surface px-3 py-2 text-sm text-fg shadow-popover">
          <CircleCheck aria-hidden className="size-4 shrink-0 text-ok" />
          <span className="truncate" title={file.flash}>
            {file.flash}
          </span>
        </div>
      )}

      {file.quitting && (
        // Not a dismissable dialog: the app closes itself once the crawl is paused.
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-scrim">
          <div role="alertdialog" aria-modal="true" aria-label="Quitting" className="flex items-center gap-3 rounded-xl bg-surface px-5 py-4 text-base text-fg shadow-sheet">
            <LoaderCircle aria-hidden className="size-4 text-accent motion-safe:animate-spin" />
            Pausing the crawl so you can resume it later…
          </div>
        </div>
      )}
    </>
  );
}

function Actions({ children }: { children: React.ReactNode }) {
  return <div className="flex justify-end gap-2">{children}</div>;
}

function UpgradePrompt({ info, onCancel, onConfirm }: { info: CrawlInfo; onCancel: () => void; onConfirm: (keepCopy: boolean) => void }) {
  const [keep, setKeep] = useState(true);
  return (
    <Modal
      open
      onOpenChange={(o) => !o && onCancel()}
      title="Upgrade this crawl file?"
      description={`To resume ${info.name}, SpiderLens upgrades the file to the current format. Older SpiderLens versions may no longer open it.`}
    >
      <label className="flex items-center gap-2 text-sm text-fg">
        <input type="checkbox" className="size-3.5 accent-accent" checked={keep} onChange={(e) => setKeep(e.target.checked)} />
        Keep a copy of the original
      </label>
      <Actions>
        <Button onClick={onCancel}>Cancel</Button>
        <Button variant="primary" onClick={() => onConfirm(keep)}>
          Upgrade and resume
        </Button>
      </Actions>
    </Modal>
  );
}

function SecretsPrompt({ info, onCancel, onSubmit }: { info: CrawlInfo; onCancel: () => void; onSubmit: (secrets: Record<string, string>) => void }) {
  const [values, setValues] = useState<Record<string, string>>(() => Object.fromEntries(info.missing_secrets.map((n) => [n, ""])));
  const ready = info.missing_secrets.every((n) => values[n] !== "");
  const submit = (e: FormEvent) => {
    e.preventDefault();
    if (ready) onSubmit(values);
  };
  return (
    <Modal open onOpenChange={(o) => !o && onCancel()} title="Enter secrets to resume" description="Secret header values are never stored in the crawl file, so they are needed again.">
      <form className="flex flex-col gap-3" onSubmit={submit}>
        {info.missing_secrets.map((name, i) => (
          <label key={name} className="flex flex-col gap-1 text-sm">
            <span className="font-code text-fg-2">{name}</span>
            <Input type="password" autoFocus={i === 0} autoComplete="off" value={values[name]} onChange={(e) => setValues((v) => ({ ...v, [name]: e.target.value }))} />
          </label>
        ))}
        <Actions>
          <Button onClick={onCancel}>Cancel</Button>
          <Button type="submit" variant="primary" disabled={!ready}>
            Resume
          </Button>
        </Actions>
      </form>
    </Modal>
  );
}
