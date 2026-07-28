import { Info } from "lucide-react";
import type { ReactNode } from "react";
import { TooltipWrapper } from "@/components/ui/shadcn/utils/with-tooltip";
import { cn } from "@/libs/shadcn/utils";

/** Format a measure value for display: trim trailing float noise, cap decimals. */
export function formatMeasureValue(raw: string): string {
  const n = Number(raw);
  if (!Number.isFinite(n)) return raw;
  if (Number.isInteger(n)) return n.toLocaleString();
  // 7 significant digits to shed float noise, strip trailing zeros, then the
  // toLocaleString below caps display at 4 decimal places.
  const formatted = n.toPrecision(7).replace(/\.?0+$/, "");
  return Number(formatted).toLocaleString(undefined, { maximumFractionDigits: 4 });
}

export function SectionSpinner({ label = "loading…" }: { label?: string }) {
  return (
    <div className='flex items-center gap-2 py-2 font-mono text-[10px] text-muted-foreground'>
      <svg
        className='size-3 animate-spin'
        viewBox='0 0 24 24'
        fill='none'
        stroke='currentColor'
        strokeWidth={2.5}
      >
        <path d='M12 2v4M12 18v4M4.93 4.93l2.83 2.83M16.24 16.24l2.83 2.83M2 12h4M18 12h4M4.93 19.07l2.83-2.83M16.24 7.76l2.83-2.83' />
      </svg>
      {label}
    </div>
  );
}

export function SectionHeader({
  title,
  subtitle,
  color = "default"
}: {
  title: string;
  subtitle?: string;
  color?: "default" | "green" | "violet";
}) {
  return (
    <div className='flex items-baseline justify-between gap-2'>
      <span
        className={cn(
          "text-[9.5px] uppercase tracking-wider",
          color === "green" && "text-success",
          color === "violet" && "text-[color:var(--vis-purple)]",
          color === "default" && "text-foreground"
        )}
      >
        {title}
      </span>
      {subtitle && (
        <span className='shrink-0 truncate text-[9.5px] text-muted-foreground'>{subtitle}</span>
      )}
    </div>
  );
}

export function Row({
  children,
  onClick,
  className
}: {
  children: React.ReactNode;
  onClick?: () => void;
  className?: string;
}) {
  return (
    <div
      className={cn(
        "flex min-w-0 items-center gap-2 border border-border bg-background/40 px-2 py-1.5 font-mono text-xs",
        onClick && "cursor-pointer transition-colors hover:border-info/60",
        className
      )}
      onClick={onClick}
    >
      {children}
    </div>
  );
}

/**
 * Thin proportion bar for ranking dense rows at a glance — the eye should be
 * able to rank magnitudes without reading every number. `fraction` is clamped
 * to [0, 1]; the width is data-driven so it stays inline rather than an
 * arbitrary Tailwind size.
 */
export function MagnitudeBar({ fraction, className }: { fraction: number; className?: string }) {
  const pct = Math.max(0, Math.min(1, Number.isFinite(fraction) ? fraction : 0)) * 100;
  return (
    <div className={cn("h-0.5 w-full overflow-hidden rounded-full bg-muted", className)}>
      <div className='h-full rounded-full bg-info' style={{ width: `${pct}%` }} />
    </div>
  );
}

/** A hoverable ⓘ that moves always-on fine print into a tooltip. */
export function InfoTip({ content }: { content: string }) {
  return (
    <TooltipWrapper tooltip={content} className='max-w-64 text-[11px] leading-relaxed'>
      <span className='inline-flex cursor-help align-middle text-muted-foreground transition-colors hover:text-foreground'>
        <Info className='size-3' />
      </span>
    </TooltipWrapper>
  );
}

/** A terse metadata chip (e.g. `linear`, `high`), optionally explained on hover. */
export function MetaBadge({ children, tooltip }: { children: ReactNode; tooltip?: string }) {
  const badge = (
    <span className='rounded-sm border border-border px-1 py-px font-mono text-[9px] text-muted-foreground uppercase tracking-wide'>
      {children}
    </span>
  );
  return tooltip ? <TooltipWrapper tooltip={tooltip}>{badge}</TooltipWrapper> : badge;
}
