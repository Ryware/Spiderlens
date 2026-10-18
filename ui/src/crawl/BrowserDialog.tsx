import { useEffect, useRef, useState } from "react";
import { Bar, Button, Input, Modal, Notice } from "../components/ui";
import type { BrowserProgress, BrowserStatus, Transport } from "../transport";

const mb = (bytes: number) => Math.round(bytes / 1_000_000);

/** One line for what a download is doing now. */
export function progressText(p: BrowserProgress): string {
  if (p.waiting) return "Connection lost, waiting to resume";
  if (p.stage === "verify") return "Checking the download";
  if (p.stage === "unpack") return "Unpacking the browser";
  return p.bytes_total > 0 ? `Downloading the browser: ${mb(p.bytes_done)} of ${mb(p.bytes_total)} MB` : "Downloading the browser";
}

/**
 * Asks before JavaScript rendering gets its browser (spec 014 FR-045): the
 * download is never started without the click. Offline, the download button
 * is disabled and "Crawl raw HTML instead" stays; the dialog checks the
 * connection every 5 s (from Rust) and says when it is back.
 */
export function BrowserDialog({
  transport,
  message,
  onReady,
  onRaw,
  startLabel = "Download and crawl",
  onClose,
}: {
  transport: Transport;
  /** The engine's words for why no browser is usable. */
  message: string;
  /** A browser is usable now: start the crawl again with rendering. */
  onReady: () => void;
  /** Switch rendering off and start the crawl. Left out where there is no raw alternative (rendering an existing crawl). */
  onRaw?: () => void;
  /** The download button's words. */
  startLabel?: string;
  onClose: () => void;
}) {
  const [status, setStatus] = useState<BrowserStatus | null>(null);
  const [online, setOnline] = useState<boolean | null>(null);
  const [restored, setRestored] = useState(false);
  const [job, setJob] = useState<number | null>(null);
  const [progress, setProgress] = useState<BrowserProgress | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [choosing, setChoosing] = useState(false);
  const [path, setPath] = useState("");
  const jobRef = useRef<number | null>(null);
  const wasOffline = useRef(false);
  const ready = useRef(onReady);
  ready.current = onReady;

  useEffect(() => {
    let alive = true;
    void transport.renderBrowserStatus().then((s) => alive && setStatus(s));
    const offs = [
      transport.onRenderBrowserNetwork((now) => {
        if (!alive) return;
        if (!now) wasOffline.current = true;
        else if (wasOffline.current) {
          wasOffline.current = false;
          setRestored(true);
        }
        setOnline(now);
      }),
      transport.onRenderBrowserProgress((p) => {
        if (alive && p.job === jobRef.current) setProgress(p);
      }),
      transport.onRenderBrowserFinished((f) => {
        if (!alive || f.job !== jobRef.current) return;
        jobRef.current = null;
        setJob(null);
        setProgress(null);
        setStatus(f.status);
        if (f.error) setError(f.error);
        else ready.current();
      }),
    ];
    void transport.renderBrowserNetworkWatch(true);
    return () => {
      alive = false;
      void transport.renderBrowserNetworkWatch(false);
      offs.forEach((o) => void o.then((off) => off()));
    };
  }, [transport]);

  const download = status?.download ?? null;
  const offline = online === false;

  const install = async () => {
    setError(null);
    setRestored(false);
    try {
      const id = await transport.renderBrowserInstall();
      jobRef.current = id;
      setJob(id);
      setProgress(null);
    } catch (e) {
      setError(String(e));
    }
  };

  const cancel = async () => {
    const id = jobRef.current;
    if (id === null) return;
    jobRef.current = null;
    setJob(null);
    setProgress(null);
    await transport.renderBrowserCancel(id);
  };

  const close = async () => {
    await cancel();
    onClose();
  };

  const useBrowser = async () => {
    setError(null);
    try {
      const s = await transport.renderBrowserSetPath(path.trim() || null);
      setStatus(s);
      if (s.usable) onReady();
      else if (s.problem) setError(s.problem);
    } catch (e) {
      setError(String(e));
    }
  };

  const browse = async () => {
    const picked = await transport.pickBrowserFile();
    if (picked) setPath(picked);
  };

  return (
    <Modal open onOpenChange={(o) => !o && void close()} title="JavaScript rendering needs a browser" description={message}>
      <div className="flex flex-col gap-3">
        {download ? (
          <p className="m-0 text-sm text-fg-2">
            SpiderLens can download a small headless browser (Chrome Headless Shell {download.version}, about {mb(download.bytes)} MB) from Google's Chrome for Testing. It is stored once and
            used only for crawling.
          </p>
        ) : (
          <Notice kind="info">No browser download is available for this computer. Choose an installed Chrome or Edge, or crawl raw HTML.</Notice>
        )}
        {offline && <Notice kind="warn" role="status">No internet connection: the browser can't be downloaded now</Notice>}
        {!offline && restored && <Notice kind="info" role="status">Connection restored</Notice>}
        {job !== null && progress && (
          <div className="flex flex-col gap-1" role="status">
            <span className="text-sm text-fg-2">{progressText(progress)}</span>
            <Bar className="w-full" value={progress.bytes_total > 0 ? progress.bytes_done / progress.bytes_total : 0} tone="accent" label="Download progress" />
          </div>
        )}
        {job !== null && !progress && <span className="text-sm text-fg-2">Starting the download</span>}
        {error && <Notice kind="bad" role="alert">{error}</Notice>}
        {choosing && (
          <div className="flex gap-2">
            <Input aria-label="Browser path" value={path} onChange={(e) => setPath(e.target.value)} placeholder="Path to Chrome or Edge" />
            <Button onClick={() => void browse()}>Browse…</Button>
            <Button onClick={() => void useBrowser()} disabled={!path.trim()}>Use this browser</Button>
          </div>
        )}
        <div className="flex flex-wrap justify-end gap-2">
          {job !== null ? (
            <Button onClick={() => void cancel()}>Cancel download</Button>
          ) : (
            <>
              {!choosing && <Button variant="quiet" onClick={() => setChoosing(true)}>Change browser</Button>}
              {onRaw && <Button onClick={onRaw}>Crawl raw HTML instead</Button>}
              <Button variant="primary" disabled={!download || offline} onClick={() => void install()}>
                {startLabel}
              </Button>
            </>
          )}
        </div>
      </div>
    </Modal>
  );
}
