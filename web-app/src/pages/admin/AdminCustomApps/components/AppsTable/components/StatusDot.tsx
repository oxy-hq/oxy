import { cn } from "@/libs/shadcn/utils";

/**
 * The bare status LED — the cockpit's core status glyph. Filled brand dot with
 * a soft ring-glow = Live; hollow muted ring = Draft. Emerald stays reserved
 * for workflow-node success, so Live leans on the brand `primary` token (same
 * convention as `StatusPill`). Label-less by design: it rides beside a name in
 * the registry rail and the hover card where the "Live/Draft" word would be
 * redundant noise.
 */
export const StatusDot = ({
  isLive,
  className,
  decorative
}: {
  isLive: boolean;
  className?: string;
  /** Skip the a11y label when a text status sits right beside it (e.g. the
   *  fleet strip), so a screen reader doesn't announce "Live" twice. Elsewhere
   *  (cards, rail rows) the dot is the ONLY status signal, so it's labelled. */
  decorative?: boolean;
}) => {
  const label = isLive ? "Live" : "Draft";
  return (
    <span
      title={label}
      {...(decorative ? { "aria-hidden": true } : { role: "img", "aria-label": label })}
      className={cn(
        "size-2 shrink-0 rounded-full",
        isLive ? "bg-primary ring-2 ring-primary/25" : "border border-muted-foreground/50",
        className
      )}
    />
  );
};
