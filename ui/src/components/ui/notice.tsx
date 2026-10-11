import type { ReactNode } from "react";
import { AlertTriangle, CircleX, Info } from "lucide-react";
import { cn } from "./cn";

type Kind = "info" | "warn" | "bad";
const icons = { info: Info, warn: AlertTriangle, bad: CircleX };
const colors = { info: "text-info", warn: "text-warn", bad: "text-bad" };

/** One-line message with a meaning icon and color (never color alone). */
export function Notice({ kind = "info", children, className, role }: { kind?: Kind; children: ReactNode; className?: string; role?: string }) {
  const Icon = icons[kind];
  return (
    <div role={role} className={cn("flex items-start gap-1.5 text-sm", kind === "info" ? "text-fg-2" : colors[kind], className)}>
      <Icon aria-hidden className={cn("mt-px size-3.5 shrink-0", colors[kind])} />
      <div className="min-w-0">{children}</div>
    </div>
  );
}

export function EmptyState({ icon, title, children }: { icon?: ReactNode; title: string; children?: ReactNode }) {
  return (
    <div className="flex h-full flex-col items-center justify-center gap-2 p-6 text-center">
      {icon && <div className="text-fg-3">{icon}</div>}
      <div className="text-base font-semibold text-fg">{title}</div>
      {children && <div className="max-w-72 text-sm text-fg-3">{children}</div>}
    </div>
  );
}
