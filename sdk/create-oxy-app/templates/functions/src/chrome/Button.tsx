import type { ButtonHTMLAttributes } from "react";
import { cn } from "../cn";

// Shared focus ring — reused by every interactive control so keyboard focus is
// consistent.
const RING =
  "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-info focus-visible:ring-offset-2 focus-visible:ring-offset-background";

// Class string for inputs / selects: hairline border, card fill, mono type,
// info-blue focus.
export const fieldClass = cn(
  "border border-border bg-card px-2 py-1 font-mono text-[12px] text-foreground",
  "hover:border-info/60",
  RING
);

// Two button recipes: `primary` is the solid info CTA (text in the bg color),
// `default` is the bordered secondary that tints to info on hover.
export function Button({
  variant = "default",
  className,
  type = "button",
  ...props
}: ButtonHTMLAttributes<HTMLButtonElement> & { variant?: "primary" | "default" }) {
  return (
    <button
      type={type}
      className={cn(
        "inline-flex items-center justify-center gap-1.5 border px-2.5 py-1 font-mono text-[11px] uppercase tracking-wider transition-colors",
        RING,
        "disabled:cursor-not-allowed disabled:opacity-40",
        variant === "primary"
          ? "border-info bg-info text-background hover:bg-info/90"
          : "border-border bg-card text-foreground hover:border-info hover:text-info",
        className
      )}
      {...props}
    />
  );
}
