import { forwardRef, type ButtonHTMLAttributes } from "react";
import { cn } from "./cn";

export type ButtonVariant = "primary" | "secondary" | "quiet" | "destructive";

const variants: Record<ButtonVariant, string> = {
  primary: "bg-accent text-accent-fg hover:enabled:bg-accent-hover font-semibold",
  secondary: "bg-surface-2 text-fg hover:enabled:bg-surface-3 border border-line",
  quiet: "bg-transparent text-fg-2 hover:enabled:bg-surface-2 hover:enabled:text-fg",
  destructive: "bg-transparent text-bad border border-line hover:enabled:bg-bad-soft",
};

export interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: ButtonVariant;
  size?: "sm" | "md";
  /** Square button holding only an icon; give it an aria-label. */
  icon?: boolean;
}

/** FR-021/FR-022: one button for every screen, with all states defined. */
export const Button = forwardRef<HTMLButtonElement, ButtonProps>(function Button(
  { variant = "secondary", size = "md", icon, className, type = "button", ...rest },
  ref,
) {
  return (
    <button
      ref={ref}
      type={type}
      className={cn(
        "inline-flex shrink-0 items-center justify-center gap-1.5 rounded-md whitespace-nowrap select-none",
        "transition-colors duration-(--dur-fast) active:enabled:translate-y-px",
        "disabled:cursor-not-allowed disabled:opacity-45",
        size === "md" ? "h-control text-base" : "h-6 text-sm",
        icon ? (size === "md" ? "w-7" : "w-6") : size === "md" ? "px-3" : "px-2",
        variants[variant],
        className,
      )}
      {...rest}
    />
  );
});
