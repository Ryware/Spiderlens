import { forwardRef, type InputHTMLAttributes, type SelectHTMLAttributes, type TextareaHTMLAttributes } from "react";
import { ChevronsUpDown } from "lucide-react";
import { Switch as BaseSwitch } from "@base-ui/react/switch";
import { cn } from "./cn";

const control =
  "rounded-md border border-control-border bg-surface text-fg placeholder:text-fg-3 " +
  "transition-colors duration-(--dur-fast) hover:enabled:border-fg-3 " +
  "focus-visible:outline-2 focus-visible:outline-accent focus-visible:-outline-offset-1 " +
  "disabled:cursor-not-allowed disabled:opacity-50 aria-invalid:border-bad";

export const Input = forwardRef<HTMLInputElement, InputHTMLAttributes<HTMLInputElement> & { mono?: boolean }>(
  function Input({ className, mono, ...rest }, ref) {
    return (
      <input
        ref={ref}
        className={cn(control, "h-control px-2 text-base", mono && "font-code text-sm", className)}
        {...rest}
      />
    );
  },
);

export const Textarea = forwardRef<HTMLTextAreaElement, TextareaHTMLAttributes<HTMLTextAreaElement>>(
  function Textarea({ className, ...rest }, ref) {
    return (
      <textarea
        ref={ref}
        className={cn(control, "min-h-16 w-full resize-y px-2 py-1.5 font-code text-sm leading-normal", className)}
        {...rest}
      />
    );
  },
);

/**
 * Native select, restyled. The open menu is the platform's own (an NSMenu on
 * macOS), which is the most native-feeling option a webview offers.
 */
export const Select = forwardRef<HTMLSelectElement, SelectHTMLAttributes<HTMLSelectElement>>(function Select(
  { className, children, ...rest },
  ref,
) {
  return (
    <span className={cn("relative inline-flex", className)}>
      <select ref={ref} className={cn(control, "h-control w-full appearance-none pr-7 pl-2 text-base")} {...rest}>
        {children}
      </select>
      <ChevronsUpDown aria-hidden className="pointer-events-none absolute top-1/2 right-2 size-3.5 -translate-y-1/2 text-fg-3" />
    </span>
  );
});

/** Number input where blank means `null` when `optional` is set. */
export function NumberInput({
  value,
  onChange,
  optional,
  className,
  ...rest
}: Omit<InputHTMLAttributes<HTMLInputElement>, "value" | "onChange" | "type"> & {
  value: number | null;
  onChange: (v: number | null) => void;
  optional?: boolean;
}) {
  return (
    <Input
      type="number"
      inputMode="decimal"
      className={cn("w-24 text-right tabular", className)}
      value={value === null || Number.isNaN(value) ? "" : value}
      onChange={(e) => onChange(e.target.value === "" ? (optional ? null : NaN) : Number(e.target.value))}
      {...rest}
    />
  );
}

/** On/off setting. Name it with `label`, or with `labelledBy` when a visible label exists. */
export function Switch({
  checked,
  onChange,
  label,
  labelledBy,
  disabled,
}: {
  checked: boolean;
  onChange: (v: boolean) => void;
  label?: string;
  labelledBy?: string;
  disabled?: boolean;
}) {
  return (
    <BaseSwitch.Root
      aria-label={labelledBy ? undefined : label}
      aria-labelledby={labelledBy}
      checked={checked}
      disabled={disabled}
      onCheckedChange={(v) => onChange(v)}
      className={cn(
        "relative inline-flex h-4.5 w-8 shrink-0 items-center rounded-full border border-control-border bg-surface-3 p-px",
        "transition-colors duration-(--dur) data-checked:border-accent data-checked:bg-accent",
        "data-disabled:cursor-not-allowed data-disabled:opacity-50",
      )}
    >
      <BaseSwitch.Thumb className="size-3.5 rounded-full bg-surface shadow-raised transition-transform duration-(--dur) ease-ui data-checked:translate-x-3.5" />
    </BaseSwitch.Root>
  );
}

export interface SegmentedOption<T extends string> {
  value: T;
  label: string;
  disabled?: boolean;
}

/** Single choice among a few options (radio group semantics, arrow keys move). */
export function Segmented<T extends string>({
  value,
  options,
  onChange,
  label,
  size = "md",
}: {
  value: T;
  options: SegmentedOption<T>[];
  onChange: (v: T) => void;
  label: string;
  size?: "sm" | "md";
}) {
  const move = (dir: number) => {
    const enabled = options.filter((o) => !o.disabled);
    const i = enabled.findIndex((o) => o.value === value);
    const next = enabled[(i + dir + enabled.length) % enabled.length];
    if (next) onChange(next.value);
  };
  return (
    <div
      role="radiogroup"
      aria-label={label}
      className={cn("inline-flex shrink-0 gap-0.5 rounded-md bg-surface-2 p-0.5", size === "md" ? "h-control" : "h-6")}
      onKeyDown={(e) => {
        if (e.key === "ArrowRight" || e.key === "ArrowDown") (e.preventDefault(), move(1));
        if (e.key === "ArrowLeft" || e.key === "ArrowUp") (e.preventDefault(), move(-1));
      }}
    >
      {options.map((o) => {
        const on = o.value === value;
        return (
          <button
            key={o.value}
            type="button"
            role="radio"
            aria-checked={on}
            aria-label={o.label}
            disabled={o.disabled}
            tabIndex={on ? 0 : -1}
            onClick={() => onChange(o.value)}
            className={cn(
              "rounded-sm px-2.5 text-sm transition-colors duration-(--dur-fast) disabled:opacity-45",
              on ? "bg-surface text-fg shadow-raised" : "text-fg-2 hover:enabled:text-fg",
            )}
          >
            {o.label}
          </button>
        );
      })}
    </div>
  );
}
