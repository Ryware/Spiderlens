// Search preview in the detail pane (spec 027 US A2, FR-A05, FR-A06).
//
// Draws the page's title, URL and description the way a search result
// shows them, cut where the desktop or mobile width runs out, and lets the
// user try other wording with the width updating as they type.

import { useEffect, useState } from "react";
import type { Snippet } from "../generated/Snippet";
import type { SerpStyle } from "../generated/SerpStyle";
import { Badge, Bar, Button, Segmented, Textarea } from "../components/ui";
import { LIMITS, measure } from "../serp/measure";

type Device = "desktop" | "mobile";

/** `example.com › blog › post`, as results show the URL. */
export function breadcrumb(url: string): string {
  try {
    const u = new URL(url);
    const parts = u.pathname.split("/").filter(Boolean).map((p) => {
      try {
        return decodeURIComponent(p);
      } catch {
        return p;
      }
    });
    return [u.hostname, ...parts].join(" › ");
  } catch {
    return url;
  }
}

function Meter({ label, text, style, device }: { label: string; text: string; style: SerpStyle; device: Device }) {
  const m = measure(text, style);
  const limit = LIMITS[style][device];
  const cut = device === "desktop" ? m.cut_desktop : m.cut_mobile;
  return (
    <div className="flex items-center gap-2 text-sm">
      <span className="w-20 shrink-0 text-fg-3">{label}</span>
      <Bar value={m.px / limit} tone={cut ? "warn" : "ok"} label={`${label} width`} className="w-16" />
      <span className="tabular text-fg-2">
        {m.px.toLocaleString()} / {limit} px · {m.chars} chars
      </span>
      {cut && (
        <Badge tone="warn" className="ml-auto">
          Cut
        </Badge>
      )}
    </div>
  );
}

export function SearchPreview({ url, snippet }: { url: string; snippet: Snippet }) {
  const [device, setDevice] = useState<Device>("desktop");
  const [editing, setEditing] = useState(false);
  const [title, setTitle] = useState(snippet.title ?? "");
  const [description, setDescription] = useState(snippet.description ?? "");

  // A new page starts from its own text.
  useEffect(() => {
    setTitle(snippet.title ?? "");
    setDescription(snippet.description ?? "");
    setEditing(false);
  }, [url, snippet.title, snippet.description]);

  const t = measure(title, "title");
  const d = measure(description, "description");
  const shownTitle = device === "desktop" ? t.display_desktop : t.display_mobile;
  const shownDesc = device === "desktop" ? d.display_desktop : d.display_mobile;
  const changed = title !== (snippet.title ?? "") || description !== (snippet.description ?? "");

  return (
    <div className="flex flex-col gap-2">
      <Segmented<Device>
        size="sm"
        label="Search preview device"
        value={device}
        onChange={setDevice}
        options={[
          { value: "desktop", label: "Desktop" },
          { value: "mobile", label: "Mobile" },
        ]}
      />
      <figure
        aria-label={`${device === "desktop" ? "Desktop" : "Mobile"} search result`}
        className="m-0 flex flex-col gap-0.5 rounded-md border border-line bg-surface-2 p-3"
        data-selectable
      >
        <span className="truncate text-xs text-fg-3">{breadcrumb(url)}</span>
        <span dir="auto" className="text-lg leading-snug text-accent-text" data-testid="preview-title">
          {shownTitle || <span className="text-fg-3">No title</span>}
        </span>
        <span dir="auto" className="text-sm leading-normal text-fg-2" data-testid="preview-description">
          {shownDesc || <span className="text-fg-3">No meta description: search engines will pick text from the page.</span>}
        </span>
      </figure>
      <Meter label="Title" text={title} style="title" device={device} />
      <Meter label="Description" text={description} style="description" device={device} />
      {editing ? (
        <div className="flex flex-col gap-1.5">
          <label className="flex flex-col gap-1 text-xs text-fg-3">
            Title
            <Textarea dir="auto" rows={2} value={title} onChange={(e) => setTitle(e.target.value)} className="font-sans" />
          </label>
          <label className="flex flex-col gap-1 text-xs text-fg-3">
            Description
            <Textarea
              dir="auto"
              rows={3}
              value={description}
              onChange={(e) => setDescription(e.target.value)}
              className="font-sans"
            />
          </label>
          <div className="flex items-center gap-2">
            <Button
              size="sm"
              variant="quiet"
              disabled={!changed}
              onClick={() => {
                setTitle(snippet.title ?? "");
                setDescription(snippet.description ?? "");
              }}
            >
              Reset to the page's text
            </Button>
            <span className="ml-auto text-xs text-fg-3">For trying wording; not saved.</span>
          </div>
        </div>
      ) : (
        <Button size="sm" variant="quiet" className="self-start" onClick={() => setEditing(true)}>
          Try other wording
        </Button>
      )}
    </div>
  );
}
