import { cn } from "@/libs/shadcn/utils";

/**
 * Operator-console-grade status indicator. Renders a 2-color dot + a short
 * label. Mid-density. Designed to be legible at a glance across a long
 * table of rows. Semantic tone via Tailwind theme tokens — no raw hex.
 *
 * Variants intentionally collapse "ok / needs-attention / critical" into a
 * small enumeration so cross-resource lists (orgs, users, workspaces) read
 * consistently. New resource statuses should map onto one of these tones
 * rather than introduce new ones.
 */
export type AdminStatusTone = "ok" | "info" | "warn" | "danger" | "muted";

const TONE: Record<AdminStatusTone, { dot: string; text: string; ring: string }> = {
  ok: {
    dot: "bg-emerald-500",
    text: "text-emerald-700 dark:text-emerald-400",
    ring: "ring-emerald-500/15"
  },
  info: { dot: "bg-primary", text: "text-primary", ring: "ring-primary/15" },
  warn: {
    dot: "bg-amber-500",
    text: "text-amber-700 dark:text-amber-400",
    ring: "ring-amber-500/15"
  },
  danger: { dot: "bg-destructive", text: "text-destructive", ring: "ring-destructive/15" },
  muted: { dot: "bg-muted-foreground/60", text: "text-muted-foreground", ring: "ring-border" }
};

export const AdminStatusPill = ({
  tone,
  label,
  className
}: {
  tone: AdminStatusTone;
  label: string;
  className?: string;
}) => {
  const v = TONE[tone];
  return (
    <span
      className={cn(
        "inline-flex items-center gap-1.5 rounded-full bg-muted/40 px-2 py-0.5 font-medium text-xs ring-1 ring-inset",
        v.text,
        v.ring,
        className
      )}
    >
      <span className={cn("inline-block size-1.5 rounded-full", v.dot)} aria-hidden />
      {label}
    </span>
  );
};
