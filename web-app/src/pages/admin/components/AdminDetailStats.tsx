import type { ComponentType, ReactNode } from "react";
import { cn } from "@/libs/shadcn/utils";

/**
 * Compact horizontal KPI strip pinned under the page header on every Admin
 * detail surface. Denser than `AdminStatCard` (used by overview hubs) and
 * intentionally NOT clickable — the strip is "vital signs at a glance,"
 * not navigation.
 *
 * Cells share an equal-width grid so the row reads as a single rhythm
 * across screen widths. Each cell carries a tiny uppercase label, a
 * large tabular-nums value, and an optional sub-line.
 */
export const AdminDetailStats = ({
  items,
  className
}: {
  items: Array<{
    label: string;
    value: ReactNode;
    /** Optional second line (e.g. "30 days ago" under "Last login"). */
    sub?: ReactNode;
    icon?: ComponentType<{ className?: string }>;
  }>;
  className?: string;
}) => (
  <div
    className={cn(
      "grid divide-x divide-border/60 overflow-hidden rounded-lg border border-border/60 bg-card",
      items.length <= 3 ? "grid-cols-1 sm:grid-cols-3" : "grid-cols-2 sm:grid-cols-4",
      className
    )}
  >
    {items.map((item, idx) => (
      <div key={`${item.label}-${idx}`} className='flex flex-col justify-center gap-1 px-4 py-3'>
        <div className='flex items-center gap-1.5 font-medium text-[10px] text-muted-foreground uppercase tracking-[0.14em]'>
          {item.icon ? <item.icon className='size-3' /> : null}
          {item.label}
        </div>
        <div className='font-semibold text-lg tabular-nums tracking-tight'>{item.value}</div>
        {item.sub ? (
          <div className='truncate text-muted-foreground text-xs tabular-nums'>{item.sub}</div>
        ) : null}
      </div>
    ))}
  </div>
);
