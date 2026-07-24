import type { ReactNode } from "react";
import { cn } from "../cn";

type Variant = "ok" | "warn" | "crit" | "info" | "neutral";

// Outline chip; the border inherits the text color via `border-current`, so a
// single `text-*` class drives both.
const VARIANT_CLASS: Record<Variant, string> = {
  ok: "text-status-success",
  warn: "text-status-warning",
  crit: "text-status-error",
  info: "text-info",
  neutral: "text-muted-foreground"
};

export function Pill({
  variant = "neutral",
  className,
  children
}: {
  variant?: Variant;
  className?: string;
  children: ReactNode;
}) {
  return (
    <span
      className={cn(
        "inline-block border border-current px-1.5 py-0 font-mono text-[9px] tracking-wider",
        VARIANT_CLASS[variant],
        className
      )}
    >
      {children}
    </span>
  );
}
